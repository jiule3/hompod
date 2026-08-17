use std::sync::Arc;

use airplay_backend::{AirPlayBackend, AirPlayStream, StreamConfig};
use anyhow::{anyhow, Result};
use audio_windows::{AudioEndpointSnapshot, AudioRouter, CaptureFactory, CaptureSource};
use bridge_core::{BridgeState, LatencyProfile, Receiver, StabilityPolicy};
use tokio::sync::Mutex;

pub struct BridgeService<B, R, C>
where
    B: AirPlayBackend,
    R: AudioRouter,
    C: CaptureFactory,
{
    backend: Arc<B>,
    router: Arc<R>,
    capture_factory: Arc<C>,
    state: Mutex<BridgeState>,
    requested_profile: Mutex<LatencyProfile>,
    current_volume: Mutex<f32>,
    original_endpoint: Mutex<Option<AudioEndpointSnapshot>>,
    stream: Mutex<Option<Box<dyn AirPlayStream>>>,
    capture: Mutex<Option<Box<dyn CaptureSource>>>,
    stability: StabilityPolicy,
}

impl<B, R, C> BridgeService<B, R, C>
where
    B: AirPlayBackend + 'static,
    R: AudioRouter + 'static,
    C: CaptureFactory + 'static,
{
    pub fn new(backend: Arc<B>, router: Arc<R>, capture_factory: Arc<C>) -> Self {
        Self {
            backend,
            router,
            capture_factory,
            state: Mutex::new(BridgeState::Idle),
            requested_profile: Mutex::new(LatencyProfile::Ultra),
            current_volume: Mutex::new(0.35),
            original_endpoint: Mutex::new(None),
            stream: Mutex::new(None),
            capture: Mutex::new(None),
            stability: StabilityPolicy::default(),
        }
    }

    pub async fn state(&self) -> BridgeState {
        self.state.lock().await.clone()
    }

    pub async fn discover(&self) -> Result<Vec<Receiver>> {
        let active = matches!(
            self.state.lock().await.clone(),
            BridgeState::Connecting { .. }
                | BridgeState::Streaming { .. }
                | BridgeState::Reconnecting { .. }
        );
        if !active {
            *self.state.lock().await = BridgeState::Discovering;
        }
        let result = self.backend.discover().await;
        if !active {
            *self.state.lock().await = BridgeState::Idle;
        }
        result
    }

    pub async fn set_profile(&self, profile: LatencyProfile) {
        *self.requested_profile.lock().await = profile;
    }

    pub async fn connect(&self, receiver: Receiver, volume: f32) -> Result<()> {
        let volume = volume.clamp(0.0, 1.0);
        *self.current_volume.lock().await = volume;
        *self.state.lock().await = BridgeState::Connecting { receiver_id: receiver.id.clone() };

        // Snapshot the user's current local mute state first, then open loopback
        // capture while the endpoint is still audible. Standard WASAPI loopback
        // taps pre-volume/pre-mute, so muting immediately afterward stops local
        // speaker output without removing PCM from the bridge.
        let original = self.router.current_default().await?;
        *self.original_endpoint.lock().await = Some(original);

        let capture = match self.capture_factory.start().await {
            Ok(c) => c,
            Err(err) => {
                *self.original_endpoint.lock().await = None;
                *self.state.lock().await = BridgeState::Error { message: err.to_string() };
                return Err(err);
            }
        };
        *self.capture.lock().await = Some(capture);

        if let Err(err) = self.router.route_to_bridge_endpoint().await {
            if let Some(mut capture) = self.capture.lock().await.take() {
                let _ = capture.stop().await;
            }
            self.restore_endpoint().await.ok();
            *self.state.lock().await = BridgeState::Error { message: err.to_string() };
            return Err(err);
        }

        let profile = *self.requested_profile.lock().await;
        let stream = match self.backend.open(&receiver, StreamConfig { profile, volume }).await {
            Ok(s) => s,
            Err(err) => {
                if let Some(mut capture) = self.capture.lock().await.take() {
                    let _ = capture.stop().await;
                }
                self.restore_endpoint().await.ok();
                *self.state.lock().await = BridgeState::Error { message: err.to_string() };
                return Err(err);
            }
        };

        *self.stream.lock().await = Some(stream);
        *self.state.lock().await = BridgeState::Streaming { receiver_id: receiver.id, profile };
        Ok(())
    }

    /// Rebuilds capture + AirPlay transport after a transient failure while
    /// keeping the user's physical render endpoint muted. The stability policy
    /// gradually moves Ultra -> Low -> Stable when repeated recovery attempts
    /// fail, but never restores local speaker audio during the retry window.
    pub async fn recover_stream(&self, receiver: &Receiver, failures: u8) -> Result<LatencyProfile> {
        let requested = *self.requested_profile.lock().await;
        let profile = self.stability.profile_for_attempt(requested, failures);
        *self.state.lock().await = BridgeState::Reconnecting {
            receiver_id: receiver.id.clone(),
            attempt: failures,
            profile,
        };

        if let Some(mut capture) = self.capture.lock().await.take() {
            let _ = capture.stop().await;
        }
        if let Some(mut stream) = self.stream.lock().await.take() {
            let _ = stream.close().await;
        }

        let volume = *self.current_volume.lock().await;
        let stream = self
            .backend
            .open(receiver, StreamConfig { profile, volume })
            .await?;

        let capture = match self.capture_factory.start().await {
            Ok(capture) => capture,
            Err(err) => {
                let mut stream = stream;
                let _ = stream.close().await;
                return Err(err);
            }
        };

        *self.stream.lock().await = Some(stream);
        *self.capture.lock().await = Some(capture);
        *self.state.lock().await = BridgeState::Streaming {
            receiver_id: receiver.id.clone(),
            profile,
        };
        Ok(profile)
    }

    pub fn max_reconnect_attempts(&self) -> u8 {
        self.stability.max_reconnect_attempts
    }

    pub fn reconnect_delay_ms(&self, attempt: u8) -> u64 {
        self.stability.reconnect_delay_ms(attempt)
    }

    pub async fn pump_once(&self) -> Result<()> {
        let frame = {
            let mut capture = self.capture.lock().await;
            let capture = capture.as_mut().ok_or_else(|| anyhow!("capture not active"))?;
            capture.next_frame().await?
        };
        let mut stream = self.stream.lock().await;
        let stream = stream.as_mut().ok_or_else(|| anyhow!("stream not active"))?;
        stream.send_pcm(frame.samples, frame.sample_rate, frame.channels).await
    }

    pub async fn set_volume(&self, volume: f32) -> Result<()> {
        let volume = volume.clamp(0.0, 1.0);
        let mut stream = self.stream.lock().await;
        let stream = stream.as_mut().ok_or_else(|| anyhow!("stream not active"))?;
        stream.set_volume(volume).await?;
        *self.current_volume.lock().await = volume;
        Ok(())
    }

    pub async fn disconnect(&self) -> Result<()> {
        if let Some(mut capture) = self.capture.lock().await.take() {
            let _ = capture.stop().await;
        }
        if let Some(mut stream) = self.stream.lock().await.take() {
            let _ = stream.close().await;
        }
        self.restore_endpoint().await?;
        *self.state.lock().await = BridgeState::Idle;
        Ok(())
    }

    async fn restore_endpoint(&self) -> Result<()> {
        if let Some(snapshot) = self.original_endpoint.lock().await.take() {
            self.router.restore_default(&snapshot).await?;
        }
        Ok(())
    }

    pub fn fallback_profile_after(&self, failures: u8, requested: LatencyProfile) -> LatencyProfile {
        self.stability.profile_for_attempt(requested, failures)
    }
}
