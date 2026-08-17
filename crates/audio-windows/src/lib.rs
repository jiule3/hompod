use anyhow::{anyhow, Result};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::sync::{atomic::{AtomicBool, Ordering}, Arc};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AudioEndpointSnapshot {
    pub endpoint_id: String,
    pub friendly_name: String,
    pub was_muted: bool,
}

#[derive(Debug, Clone)]
pub struct PcmFrame {
    pub samples: Vec<i16>,
    pub sample_rate: u32,
    pub channels: u8,
}

#[async_trait]
pub trait AudioRouter: Send + Sync {
    async fn current_default(&self) -> Result<AudioEndpointSnapshot>;
    async fn route_to_bridge_endpoint(&self) -> Result<()>;
    async fn restore_default(&self, snapshot: &AudioEndpointSnapshot) -> Result<()>;
}

#[async_trait]
pub trait CaptureSource: Send {
    async fn next_frame(&mut self) -> Result<PcmFrame>;
    async fn stop(&mut self) -> Result<()>;
}

#[async_trait]
pub trait CaptureFactory: Send + Sync {
    async fn start(&self) -> Result<Box<dyn CaptureSource>>;
}

/// AirSend's capture crate uses WASAPI loopback on Windows and requests the
/// 44.1 kHz / 16-bit / stereo format expected by the HomePod sender.
///
/// Important: standard WASAPI loopback is pre-endpoint-volume/mute unless the
/// client explicitly requests POST_VOLUME_LOOPBACK. We do not request that
/// option, so the local endpoint can be muted after capture is opened while the
/// bridge keeps receiving PCM.
#[derive(Debug, Default, Clone, Copy)]
pub struct AirSendLoopbackFactory;

struct AirSendLoopbackSource {
    capture: Option<Box<dyn audio_capture::Capture>>,
    rx: tokio::sync::mpsc::Receiver<PcmFrame>,
    stop: Arc<AtomicBool>,
    forwarder: Option<std::thread::JoinHandle<()>>,
}

#[async_trait]
impl CaptureFactory for AirSendLoopbackFactory {
    async fn start(&self) -> Result<Box<dyn CaptureSource>> {
        let (capture, raw_rx) = audio_capture::start_loopback(audio_capture::CaptureFormat::AIRPLAY_DEFAULT)
            .map_err(|e| anyhow!("WASAPI loopback start failed: {e}"))?;

        // Keep the async audio path free of per-frame spawn_blocking calls. A
        // single dedicated thread blocks on AirSend's crossbeam receiver and
        // forwards into a deliberately tiny async queue. If the UI/runtime
        // stalls, dropping a fresh frame is preferable to accumulating seconds
        // of stale audio latency.
        let (tx, rx) = tokio::sync::mpsc::channel::<PcmFrame>(4);
        let stop = Arc::new(AtomicBool::new(false));
        let forward_stop = stop.clone();
        let forwarder = std::thread::Builder::new()
            .name("loopback-forwarder".into())
            .spawn(move || {
                use crossbeam_channel::RecvTimeoutError;
                use tokio::sync::mpsc::error::TrySendError;
                use std::time::Duration;

                while !forward_stop.load(Ordering::Acquire) {
                    match raw_rx.recv_timeout(Duration::from_millis(20)) {
                        Ok(frame) => {
                            let pcm = PcmFrame {
                                samples: frame.samples,
                                sample_rate: frame.sample_rate,
                                channels: frame.channels as u8,
                            };
                            match tx.try_send(pcm) {
                                Ok(()) => {}
                                Err(TrySendError::Full(_)) => {
                                    // Bound latency: never wait for an already
                                    // stale queue to drain.
                                }
                                Err(TrySendError::Closed(_)) => break,
                            }
                        }
                        Err(RecvTimeoutError::Timeout) => continue,
                        Err(RecvTimeoutError::Disconnected) => break,
                    }
                }
            })
            .map_err(|e| anyhow!("loopback forwarder thread: {e}"))?;

        Ok(Box::new(AirSendLoopbackSource {
            capture: Some(capture),
            rx,
            stop,
            forwarder: Some(forwarder),
        }))
    }
}

#[async_trait]
impl CaptureSource for AirSendLoopbackSource {
    async fn next_frame(&mut self) -> Result<PcmFrame> {
        self.rx
            .recv()
            .await
            .ok_or_else(|| anyhow!("loopback stream closed"))
    }

    async fn stop(&mut self) -> Result<()> {
        self.stop.store(true, Ordering::Release);
        if let Some(capture) = self.capture.take() {
            capture.stop();
        }
        if let Some(forwarder) = self.forwarder.take() {
            tokio::task::spawn_blocking(move || forwarder.join())
                .await
                .map_err(|e| anyhow!("loopback forwarder join task: {e}"))?
                .map_err(|_| anyhow!("loopback forwarder panicked"))?;
        }
        Ok(())
    }
}

impl Drop for AirSendLoopbackSource {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(capture) = self.capture.take() {
            capture.stop();
        }
    }
}

/// Production router for the no-driver MVP. It does not change the Windows
/// default device. Instead it mutes the current local render endpoint *after*
/// loopback capture has been opened, then restores the exact endpoint's prior
/// mute state on disconnect.
#[derive(Debug, Default, Clone, Copy)]
pub struct SystemMuteRouter;

#[async_trait]
impl AudioRouter for SystemMuteRouter {
    async fn current_default(&self) -> Result<AudioEndpointSnapshot> {
        tokio::task::spawn_blocking(platform::snapshot_default_render)
            .await
            .map_err(|e| anyhow!("endpoint snapshot task failed: {e}"))?
    }

    async fn route_to_bridge_endpoint(&self) -> Result<()> {
        tokio::task::spawn_blocking(|| platform::set_default_render_muted(true))
            .await
            .map_err(|e| anyhow!("endpoint mute task failed: {e}"))?
    }

    async fn restore_default(&self, snapshot: &AudioEndpointSnapshot) -> Result<()> {
        let snapshot = snapshot.clone();
        tokio::task::spawn_blocking(move || platform::restore_render_mute(&snapshot))
            .await
            .map_err(|e| anyhow!("endpoint restore task failed: {e}"))?
    }
}

#[cfg(windows)]
mod platform {
    use super::AudioEndpointSnapshot;
    use anyhow::{anyhow, Result};
    use std::ffi::c_void;
    use windows::{
        core::{GUID, PCWSTR, PWSTR},
        Win32::{
            Media::Audio::{eConsole, eRender, IMMDevice, IMMDeviceEnumerator, MMDeviceEnumerator},
            Media::Audio::Endpoints::IAudioEndpointVolume,
            System::Com::{CoCreateInstance, CoInitializeEx, CoTaskMemFree, CoUninitialize, CLSCTX_ALL, COINIT_MULTITHREADED},
        },
    };

    struct CoOwnedPwstr(PWSTR);
    impl Drop for CoOwnedPwstr {
        fn drop(&mut self) {
            unsafe { CoTaskMemFree(Some(self.0.0 as *const c_void)); }
        }
    }

    struct ComApartmentGuard {
        should_uninitialize: bool,
    }

    impl Drop for ComApartmentGuard {
        fn drop(&mut self) {
            if self.should_uninitialize {
                unsafe { CoUninitialize(); }
            }
        }
    }

    fn init_com() -> ComApartmentGuard {
        // spawn_blocking workers are reused by Tokio. Balance every successful
        // CoInitializeEx (S_OK or S_FALSE) so the worker's COM refcount does not
        // grow after repeated connect/disconnect cycles. RPC_E_CHANGED_MODE is
        // deliberately left alone: the thread already has a usable apartment.
        let hr = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
        ComApartmentGuard {
            should_uninitialize: hr.is_ok(),
        }
    }

    unsafe fn enumerator() -> Result<IMMDeviceEnumerator> {
        CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)
            .map_err(|e| anyhow!("MMDeviceEnumerator: {e}"))
    }

    unsafe fn endpoint_volume(device: &IMMDevice) -> Result<IAudioEndpointVolume> {
        device
            .Activate(CLSCTX_ALL, None)
            .map_err(|e| anyhow!("IAudioEndpointVolume activation: {e}"))
    }

    unsafe fn device_id(device: &IMMDevice) -> Result<String> {
        let raw = device.GetId().map_err(|e| anyhow!("IMMDevice::GetId: {e}"))?;
        let guard = CoOwnedPwstr(raw);
        guard.0.to_string().map_err(|e| anyhow!("endpoint id UTF-16: {e}"))
    }

    unsafe fn get_device_by_id(id: &str) -> Result<IMMDevice> {
        let enumerator = enumerator()?;
        let wide: Vec<u16> = id.encode_utf16().chain(std::iter::once(0)).collect();
        enumerator
            .GetDevice(PCWSTR(wide.as_ptr()))
            .map_err(|e| anyhow!("IMMDeviceEnumerator::GetDevice: {e}"))
    }

    pub fn snapshot_default_render() -> Result<AudioEndpointSnapshot> {
        let _com = init_com();
        unsafe {
            let enumerator = enumerator()?;
            let device = enumerator
                .GetDefaultAudioEndpoint(eRender, eConsole)
                .map_err(|e| anyhow!("GetDefaultAudioEndpoint: {e}"))?;
            let endpoint = endpoint_volume(&device)?;
            let muted = endpoint.GetMute().map_err(|e| anyhow!("GetMute: {e}"))?;
            Ok(AudioEndpointSnapshot {
                endpoint_id: device_id(&device)?,
                friendly_name: "Windows default render endpoint".to_string(),
                was_muted: muted.as_bool(),
            })
        }
    }

    pub fn set_default_render_muted(muted: bool) -> Result<()> {
        let _com = init_com();
        unsafe {
            let enumerator = enumerator()?;
            let device = enumerator
                .GetDefaultAudioEndpoint(eRender, eConsole)
                .map_err(|e| anyhow!("GetDefaultAudioEndpoint: {e}"))?;
            let endpoint = endpoint_volume(&device)?;
            endpoint
                .SetMute(muted, std::ptr::null::<GUID>())
                .map_err(|e| anyhow!("SetMute: {e}"))
        }
    }

    pub fn restore_render_mute(snapshot: &AudioEndpointSnapshot) -> Result<()> {
        let _com = init_com();
        unsafe {
            let device = get_device_by_id(&snapshot.endpoint_id)?;
            let endpoint = endpoint_volume(&device)?;
            endpoint
                .SetMute(snapshot.was_muted, std::ptr::null::<GUID>())
                .map_err(|e| anyhow!("restore SetMute: {e}"))
        }
    }
}

#[cfg(not(windows))]
mod platform {
    use super::AudioEndpointSnapshot;
    use anyhow::{bail, Result};

    pub fn snapshot_default_render() -> Result<AudioEndpointSnapshot> {
        bail!("SystemMuteRouter is supported only on Windows")
    }
    pub fn set_default_render_muted(_muted: bool) -> Result<()> {
        bail!("SystemMuteRouter is supported only on Windows")
    }
    pub fn restore_render_mute(_snapshot: &AudioEndpointSnapshot) -> Result<()> {
        bail!("SystemMuteRouter is supported only on Windows")
    }
}
