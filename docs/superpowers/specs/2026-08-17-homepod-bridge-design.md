# HomePod Bridge MVP Design

## Goal
Build a Windows 10/11 x64 desktop application that routes Windows system audio to a single HomePod target (including a HomePod stereo pair represented as one AirPlay target) with the lowest stable latency possible.

## User-visible behavior
- Automatically discover HomePod/AirPlay 2 receivers on the local network.
- One-click connect and disconnect.
- When connected, Windows system audio is routed to HomePod; the physical PC speakers should not play the mirrored stream.
- Restore the previous default Windows audio endpoint when disconnected or when the app exits cleanly.
- Show connection state, receiver name, volume, estimated configured latency profile, and transport errors.
- Automatically reconnect the active target after transient stream failures, with bounded fallback from Ultra to Low to Stable.

## Latency policy
True zero-latency is impossible over AirPlay 2/HomePod because sender buffering, network transport, receiver buffering, and clock synchronization are mandatory. The product therefore exposes an `Ultra` profile that minimizes sender-side buffering and packet queueing while preserving stream stability. The default is `Ultra`, with automatic fallback to `Low` and then `Stable` after repeated underruns or disconnects.

Profiles:
- Ultra: maps to AirSend `Gaming` (250 ms negotiated buffer floor target).
- Low: maps to AirSend `Video` (500 ms floor target in this product policy).
- Stable: maps to AirSend `Music` (1000 ms floor target in this product policy).

The exact receiver-side latency is device/firmware dependent and is not represented as 0 ms.

## Architecture
A Rust workspace contains four bounded units:
1. `bridge-core`: state machine, latency profiles, reconnect policy, endpoint restore policy; platform-independent and unit tested.
2. `audio-windows`: Windows audio capture/routing layer. It opens pre-volume WASAPI loopback, snapshots the default endpoint ID + mute state, mutes local playback while connected, and restores the prior mute state on disconnect.
3. `airplay-backend`: receiver discovery/session/streaming abstraction. The concrete implementation is feature-gated and intended to wrap a proven AirPlay 2 implementation.
4. `desktop`: Tauri UI and command layer.

## Audio data flow
Windows app audio -> current Windows render endpoint -> pre-volume WASAPI loopback capture -> local endpoint mute -> PCM frame queue -> AirPlay backend encoder/session -> encrypted RTP/RTSP transport -> HomePod.

The MVP deliberately avoids a virtual audio driver. Standard WASAPI loopback is tapped before endpoint volume/mute unless POST_VOLUME_LOOPBACK is explicitly requested, so HomePod Bridge opens loopback first, snapshots the user's existing mute state, mutes the local render endpoint, and restores that exact endpoint state on disconnect. A virtual endpoint remains a future fallback only for hardware/driver combinations that violate this path.

## Error handling
- Discovery failures do not terminate the app; UI reports them and retries.
- Streaming failure transitions to `Reconnecting` with bounded exponential backoff.
- Ultra profile falls back after repeated instability, but the user can force it again.
- Endpoint switching is transactional: capture the previous endpoint before switching, and attempt restoration on disconnect and shutdown.
- No secrets are logged.

## Compatibility target
- Windows 10 22H2 x64 and Windows 11 x64.
- HomePod (1st gen), HomePod (2nd gen), HomePod mini.
- HomePod stereo pair when advertised by the network as one routable AirPlay 2 target.

## Licensing
The HomePod Bridge source MVP is GPL-2.0-only to stay compatible with the directly linked AirSend and airplay2-rs workspaces, both of which declare GPL-2.0. Third-party components retain their own attribution and notices.
