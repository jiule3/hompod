# HomePod Bridge MVP Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [x]`) syntax for tracking.

**Goal:** Produce a Windows-focused source MVP for discovering a HomePod target, managing connection/reconnect state, selecting an ultra-low-latency sender profile, and routing/capturing system audio through isolated platform backends.

**Architecture:** A Rust workspace isolates platform-independent policy from Windows audio and AirPlay transport. The MVP uses standard pre-volume WASAPI loopback plus reversible endpoint mute, so no virtual audio driver is required. A lightweight Tauri desktop shell consumes the service interfaces.

**Tech Stack:** Rust 2021, Tokio, Serde, Tauri 2, Windows Core Audio/WASAPI abstraction, AirPlay 2 backend adapter.

## Global Constraints
- Windows 10 22H2 x64 and Windows 11 x64.
- Default latency profile is `Ultra`, mapped to AirSend Gaming with a 250 ms protocol buffer-floor target.
- Never display or claim true 0 ms AirPlay latency.
- One active receiver at a time; a stereo pair is treated as one receiver.
- Restore the prior Windows render endpoint mute state on disconnect/shutdown.

---

### Task 1: Core state and latency policy
**Files:** create `crates/bridge-core/src/lib.rs`, `crates/bridge-core/Cargo.toml`.
- [x] Add latency profile types and exact defaults.
- [x] Add bridge state machine and reconnect/fallback policy.
- [x] Add unit tests for profile values and state transitions.

### Task 2: Audio backend contract
**Files:** create `crates/audio-windows/src/lib.rs`, `crates/audio-windows/Cargo.toml`.
- [x] Define endpoint snapshot, routing transaction, capture frame, and capture source interfaces.
- [x] Provide non-Windows stub and Windows facade boundaries.
- [x] Add tests for transactional restore semantics using a mock provider.

### Task 3: AirPlay backend contract
**Files:** create `crates/airplay-backend/src/lib.rs`, `crates/airplay-backend/Cargo.toml`.
- [x] Define receiver discovery and streaming contracts.
- [x] Map core latency profiles to transport configuration.
- [x] Add a deterministic mock backend for tests and UI development.

### Task 4: Bridge service orchestration
**Files:** create `crates/bridge-service/src/lib.rs`, `crates/bridge-service/Cargo.toml`.
- [x] Orchestrate discovery, connect, PCM pumping, reconnect, and endpoint restoration.
- [x] Add tests using mock audio and mock AirPlay backends.

### Task 5: Desktop shell
**Files:** create `apps/desktop/*`.
- [x] Add Tauri commands for scan/connect/disconnect/volume/profile/status.
- [x] Add compact Chinese-first UI showing receivers and explicit latency wording.
- [ ] Add tray behavior and saved last-target preferences (deferred; not required for the first functional audio build).

### Task 6: Windows build and verification tooling
**Files:** create `.github/workflows/windows-build.yml`, `scripts/build-windows.ps1`, `README.md`.
- [x] Configure Windows build checks and artifact output.
- [x] Document driver-signing limitation and development loopback fallback.
- [x] Package the source tree for handoff.
