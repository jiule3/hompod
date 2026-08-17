use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

use airplay_backend::AirSendBackend;
use audio_windows::{AirSendLoopbackFactory, SystemMuteRouter};
use bridge_core::{BridgeState, LatencyProfile, Receiver};
use bridge_service::BridgeService;
use tauri::{Manager, RunEvent, State};
use tokio::{sync::Mutex, task::JoinHandle};

type Service = BridgeService<AirSendBackend, SystemMuteRouter, AirSendLoopbackFactory>;

struct PumpControl {
    stop: Arc<AtomicBool>,
    task: JoinHandle<()>,
}

struct DesktopState {
    service: Arc<Service>,
    pump: Arc<Mutex<Option<PumpControl>>>,
    exit_in_progress: AtomicBool,
}

impl DesktopState {
    fn new() -> Self {
        Self {
            service: Arc::new(Service::new(
                Arc::new(AirSendBackend),
                Arc::new(SystemMuteRouter),
                Arc::new(AirSendLoopbackFactory),
            )),
            pump: Arc::new(Mutex::new(None)),
            exit_in_progress: AtomicBool::new(false),
        }
    }

    async fn stop_pump(&self) {
        if let Some(control) = self.pump.lock().await.take() {
            control.stop.store(true, Ordering::Release);
            control.task.abort();
        }
    }
}

#[tauri::command]
async fn scan_devices(state: State<'_, DesktopState>) -> Result<Vec<Receiver>, String> {
    state.service.discover().await.map_err(|e| e.to_string())
}

#[tauri::command]
async fn connect_device(
    state: State<'_, DesktopState>,
    receiver: Receiver,
    volume: f32,
    profile: LatencyProfile,
) -> Result<BridgeState, String> {
    state.stop_pump().await;
    let _ = state.service.disconnect().await;
    state.service.set_profile(profile).await;
    let volume = volume.clamp(0.0, 1.0);
    let pump_receiver = receiver.clone();
    state
        .service
        .connect(receiver, volume)
        .await
        .map_err(|e| e.to_string())?;

    let service = state.service.clone();
    let stop = Arc::new(AtomicBool::new(false));
    let pump_stop = stop.clone();
    let task = tauri::async_runtime::spawn(async move {
        // Do not immediately give up on a Wi-Fi hiccup. A failure streak is
        // cleared only after sustained good PCM flow, so repeated short-lived
        // reconnects eventually fall back Ultra -> Low -> Stable.
        let mut failure_streak: u8 = 0;
        let mut healthy_frames: u16 = 0;
        while !pump_stop.load(Ordering::Acquire) {
            match service.pump_once().await {
                Ok(()) => {
                    healthy_frames = healthy_frames.saturating_add(1);
                    if healthy_frames >= 125 {
                        failure_streak = 0;
                        healthy_frames = 0;
                    }
                }
                Err(err) => {
                    healthy_frames = 0;
                    failure_streak = failure_streak.saturating_add(1);
                    eprintln!("HomePod Bridge audio pump interrupted: {err}");

                    loop {
                        if pump_stop.load(Ordering::Acquire) {
                            return;
                        }
                        if failure_streak > service.max_reconnect_attempts() {
                            eprintln!("HomePod Bridge reconnect limit reached");
                            let _ = service.disconnect().await;
                            return;
                        }

                        let delay_ms = service.reconnect_delay_ms(failure_streak.saturating_sub(1));
                        tokio::time::sleep(std::time::Duration::from_millis(delay_ms)).await;
                        match service.recover_stream(&pump_receiver, failure_streak).await {
                            Ok(active_profile) => {
                                eprintln!(
                                    "HomePod Bridge reconnected on {:?} profile after {} failure(s)",
                                    active_profile, failure_streak
                                );
                                break;
                            }
                            Err(reconnect_err) => {
                                eprintln!("HomePod Bridge reconnect failed: {reconnect_err}");
                                failure_streak = failure_streak.saturating_add(1);
                            }
                        }
                    }
                }
            }
        }
    });
    *state.pump.lock().await = Some(PumpControl { stop, task });
    Ok(state.service.state().await)
}

#[tauri::command]
async fn disconnect_device(state: State<'_, DesktopState>) -> Result<BridgeState, String> {
    state.stop_pump().await;
    state.service.disconnect().await.map_err(|e| e.to_string())?;
    Ok(state.service.state().await)
}

#[tauri::command]
async fn set_volume(state: State<'_, DesktopState>, volume: f32) -> Result<(), String> {
    state
        .service
        .set_volume(volume.clamp(0.0, 1.0))
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn bridge_status(state: State<'_, DesktopState>) -> Result<BridgeState, String> {
    Ok(state.service.state().await)
}

#[tauri::command]
fn latency_profiles() -> Vec<(String, u32)> {
    [LatencyProfile::Ultra, LatencyProfile::Low, LatencyProfile::Stable]
        .into_iter()
        .map(|p| (format!("{:?}", p), p.tuning().sender_lead_ms))
        .collect()
}

fn main() {
    let app = tauri::Builder::default()
        .manage(DesktopState::new())
        .invoke_handler(tauri::generate_handler![
            scan_devices,
            connect_device,
            disconnect_device,
            set_volume,
            bridge_status,
            latency_profiles
        ])
        .build(tauri::generate_context!())
        .expect("HomePod Bridge failed to build");

    app.run(|app_handle, event| {
        if let RunEvent::ExitRequested { api, .. } = event {
            let state = app_handle.state::<DesktopState>();
            if !state.exit_in_progress.swap(true, Ordering::AcqRel) {
                api.prevent_exit();
                let service = state.service.clone();
                let pump = state.pump.clone();
                let app_handle = app_handle.clone();
                tauri::async_runtime::spawn(async move {
                    if let Some(control) = pump.lock().await.take() {
                        control.stop.store(true, Ordering::Release);
                        control.task.abort();
                    }
                    let _ = service.disconnect().await;
                    app_handle.exit(0);
                });
            }
        }
    });
}
