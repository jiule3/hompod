# HomePod Bridge

Windows 10/11 system-audio bridge for HomePod over AirPlay 2, optimized for the lowest stable AirPlay buffering profile that HomePod will accept.

## Current MVP
- Default `Ultra` profile: AirSend `Gaming` profile, whose advertised/negotiated buffer floor target is 250 ms.
- Automatic reconnect with bounded backoff; repeated failures fall back `Ultra -> Low -> Stable`.
- `Low` (500 ms floor target) and `Stable` (1000 ms floor target) fallbacks.
- Receiver/streaming contracts separated from Windows audio routing.
- AirPlay profile mapping targets the proven AirSend `Gaming` / `Video` / `Music` profiles.
- Dedicated loopback-forwarder thread with a four-frame async queue to avoid per-frame runtime scheduling overhead.
- Chinese-first Tauri shell, connection-state polling, and Windows CI build recipe.

## Important latency note
HomePod does not provide true zero-latency network audio. `250 ms` is the lower buffer target selected through AirSend's Gaming profile, not an end-to-end latency guarantee; HomePod firmware can negotiate/add receiver and network buffering.

## No-driver audio routing
The MVP does **not** require a virtual sound-card driver. It opens standard WASAPI loopback on the current render endpoint, then mutes that local endpoint. Windows documents standard loopback as pre-volume/pre-mute unless POST_VOLUME_LOOPBACK is explicitly requested, so the bridge continues receiving PCM while the PC speakers are silent. On disconnect, the original endpoint mute state is restored.

This is substantially simpler to install than a signed virtual driver. A virtual endpoint remains a fallback option only if specific hardware/driver combinations prove incompatible in testing.

## Build on Windows
Prerequisites: Visual Studio Build Tools with Desktop C++, Rust stable, WebView2 runtime, and Tauri CLI.

```powershell
cargo install tauri-cli --version "^2"
.\scripts\build-windows.ps1
```

The installer is emitted under `apps/desktop/src-tauri/target/release/bundle/nsis/`. The repository also includes a GitHub Actions Windows build that uploads the NSIS installer as an artifact.

## AirPlay dependency
The MVP adapter maps to the AirSend `cap-core` crate pinned to the v0.1.5 release commit. AirSend in turn uses a Windows-patched fork of `airplay2-rs` for HomePod AirPlay 2 pairing and realtime audio transport.


## Verification status for this source handoff

- Source-contract suite: 19 checks currently pass in the provided environment.
- TOML/JSON configuration parsing and `git diff --check` pass.
- The `windows-rs 0.58` Core Audio binding shape was checked against the 0.58.0 generated sources (`IAudioEndpointVolume` lives under the `Endpoints` feature and `GetMute()` returns `BOOL`).
- This Linux execution environment does **not** contain Rust/MSVC, so a real Windows binary was not compiled here. Use the included Windows build script or GitHub Actions workflow for the first executable build.
- Single-HomePod routing is the implemented transport path. Stereo-pair behavior relies on the AirPlay group endpoint advertised by the HomePod pair and still needs real-hardware validation; the UI does not split stereo members.
