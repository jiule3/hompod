# Third-party notices

HomePod Bridge links to third-party AirPlay components and keeps their licensing requirements intact.

- **AirSend / cap-core / audio-capture** — pinned to commit `07975d74ab6fd2b6150a93d5b655ebb482dab0ea` from `Pabldi08/AirSend`. The upstream Cargo workspace declares `GPL-2.0`.
- **airplay2-rs** — pulled transitively by the pinned AirSend `cap-core` through the AirSend Windows-compatible fork. The upstream workspace declares `GPL-2.0`.
- Other Rust/Tauri dependencies retain their respective licenses as resolved by Cargo.

The HomePod Bridge workspace is therefore distributed as **GPL-2.0-only** for this source MVP. Before redistributing a binary, preserve the corresponding source and all required third-party notices.
