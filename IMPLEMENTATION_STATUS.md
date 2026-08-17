# HomePod Bridge 0.1.0 — Implementation Status

## Implemented
- HomePod discovery through AirSend `cap-core`.
- HomePod transient pairing and live AirPlay 2 audio stream through AirSend.
- Windows system audio capture through AirSend WASAPI loopback.
- No-driver local-silence strategy: snapshot render endpoint mute state, open loopback, mute local endpoint, restore exactly on disconnect/clean exit.
- Ultra/Low/Stable latency profiles; Ultra maps to AirSend Gaming.
- Automatic reconnect with exponential backoff and profile fallback.
- Dedicated four-frame loopback-forwarder queue to bound local scheduling latency.
- Chinese Tauri UI: scan, select, connect/disconnect, volume, latency profile, live connection/reconnect status.
- Windows NSIS build script and GitHub Actions workflow.

## Not yet hardware-validated here
- Windows 10/11 compile/runtime (the current container has no Rust/MSVC toolchain).
- HomePod 1st gen, 2nd gen and mini model-by-model behavior.
- HomePod stereo-pair group endpoint behavior. The transport intentionally treats a pair as one AirPlay destination; it does not target left/right members separately.
- Real end-to-end latency. Ultra requests the lowest practical upstream AirSend profile but HomePod firmware ultimately controls receiver buffering.

## Deferred after first Windows smoke test
- Tray controls and start-with-Windows.
- Remember/reconnect last receiver across app restarts.
- Installer signing and release auto-update.
- Optional experimental buffer values below the AirSend Gaming floor, only if real hardware testing proves stable.
