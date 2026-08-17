from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def read(rel: str) -> str:
    return (ROOT / rel).read_text(encoding="utf-8")


def test_core_defaults_to_ultra_250ms():
    src = read("crates/bridge-core/src/lib.rs")
    assert "Ultra" in src
    assert "sender_lead_ms: 250" in src
    assert "impl Default for LatencyProfile" in src


def test_core_has_bounded_fallback_policy():
    src = read("crates/bridge-core/src/lib.rs")
    assert "fallback_after_failures" in src
    assert "LatencyProfile::Ultra => LatencyProfile::Low" in src
    assert "LatencyProfile::Low => LatencyProfile::Stable" in src


def test_airplay_maps_ultra_to_upstream_gaming_profile():
    src = read("crates/airplay-backend/src/lib.rs")
    assert "LatencyProfile::Ultra => cap_core::streaming::LatencyProfile::Gaming" in src


def test_desktop_does_not_claim_zero_latency():
    html = read("apps/desktop/ui/index.html")
    import re
    assert re.search(r"(?<!\d)0\s*ms", html.lower()) is None
    assert "极速" in html
    assert "250 ms" in html


def test_windows_build_workflow_exists():
    wf = read(".github/workflows/windows-build.yml")
    assert "windows-latest" in wf
    assert "cargo test --workspace" in wf
    assert "cargo build --workspace --release" in wf


def test_windows_audio_uses_pre_volume_loopback_then_local_mute():
    src = read("crates/audio-windows/src/lib.rs")
    assert "AirSendLoopbackFactory" in src
    assert "SystemMuteRouter" in src
    assert "set_default_render_muted(true)" in src
    assert "audio_capture::start_loopback" in src


def test_service_starts_capture_before_muting_local_endpoint():
    src = read("crates/bridge-service/src/lib.rs")
    start = src.index("self.capture_factory.start().await")
    mute = src.index("self.router.route_to_bridge_endpoint().await")
    assert start < mute


def test_real_airsend_backend_is_present():
    src = read("crates/airplay-backend/src/lib.rs")
    assert "pub struct AirSendBackend" in src
    assert "cap_core::browse_once" in src
    assert "cap_core::streaming::open_live_stream" in src


def test_desktop_exposes_real_bridge_commands():
    src = read("apps/desktop/src-tauri/src/main.rs")
    for command in ["scan_devices", "connect_device", "disconnect_device", "set_volume", "bridge_status"]:
        assert f"fn {command}" in src or f"async fn {command}" in src
    assert "BridgeService<AirSendBackend, SystemMuteRouter, AirSendLoopbackFactory>" in src
    assert "pump_once" in src


def test_ui_invokes_scan_connect_disconnect_and_volume():
    html = read("apps/desktop/ui/index.html")
    for command in ["scan_devices", "connect_device", "disconnect_device", "set_volume"]:
        assert f'"{command}"' in html
    assert "window.__TAURI__.core.invoke" in html


def test_normal_exit_restores_local_endpoint():
    src = read("apps/desktop/src-tauri/src/main.rs")
    assert "RunEvent::ExitRequested" in src
    assert "api.prevent_exit()" in src
    assert "service.disconnect().await" in src
    assert "app_handle.exit(0)" in src


def test_windows_rs_058_endpoint_volume_binding_shape():
    cargo = Path('crates/audio-windows/Cargo.toml').read_text(encoding='utf-8')
    src = Path('crates/audio-windows/src/lib.rs').read_text(encoding='utf-8')
    assert 'Win32_Media_Audio_Endpoints' in cargo
    assert 'Media::Audio::Endpoints::IAudioEndpointVolume' in src
    assert 'endpoint.GetMute()' in src
    assert 'GetMute(&mut' not in src


def test_windows_com_lifetime_is_balanced_on_blocking_workers():
    src = Path('crates/audio-windows/src/lib.rs').read_text(encoding='utf-8')
    assert 'CoUninitialize' in src
    assert 'struct ComApartmentGuard' in src


def test_service_can_recover_stream_without_unmuting_local_endpoint():
    src = Path('crates/bridge-service/src/lib.rs').read_text(encoding='utf-8')
    assert 'pub async fn recover_stream' in src
    assert 'BridgeState::Reconnecting' in src
    assert 'self.stability.profile_for_attempt' in src
    recover = src.split('pub async fn recover_stream', 1)[1].split('pub async fn', 1)[0]
    assert 'restore_endpoint' not in recover


def test_desktop_pump_retries_and_falls_back_before_disconnect():
    src = Path('apps/desktop/src-tauri/src/main.rs').read_text(encoding='utf-8')
    assert 'recover_stream' in src
    assert 'max_reconnect_attempts' in src
    assert 'reconnect_delay_ms' in src
    assert 'tokio::time::sleep' in src


def test_ui_labels_latency_as_protocol_buffer_floor_and_polls_service_state():
    ui = Path('apps/desktop/ui/index.html').read_text(encoding='utf-8')
    assert '250 ms 协议缓冲下限目标' in ui
    assert '250 ms 发送端目标' not in ui
    assert 'bridge_status' in ui
    assert 'setInterval' in ui


def test_loopback_uses_dedicated_forwarder_not_spawn_blocking_per_frame():
    src = Path('crates/audio-windows/src/lib.rs').read_text(encoding='utf-8')
    assert 'loopback-forwarder' in src
    capture_impl = src.split('impl CaptureSource for AirSendLoopbackSource', 1)[1].split('async fn stop', 1)[0]
    assert 'spawn_blocking' not in capture_impl
    assert 'tokio::sync::mpsc::Receiver<PcmFrame>' in src


def test_discovery_does_not_clobber_active_stream_state():
    src = Path('crates/bridge-service/src/lib.rs').read_text(encoding='utf-8')
    discover = src.split('pub async fn discover', 1)[1].split('pub async fn set_profile', 1)[0]
    assert 'BridgeState::Streaming' in discover
    assert 'BridgeState::Reconnecting' in discover
    assert 'if !active' in discover


def test_project_license_matches_gpl2_airplay_dependencies():
    root = Path('Cargo.toml').read_text(encoding='utf-8')
    assert 'license = "GPL-2.0-only"' in root
    assert Path('LICENSE').exists()
    assert Path('THIRD_PARTY_NOTICES.md').exists()
