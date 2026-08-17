$ErrorActionPreference = "Stop"
Write-Host "[1/3] Running workspace tests"
cargo test --workspace
Write-Host "[2/3] Building release"
cargo build --workspace --release
Write-Host "[3/3] Building Tauri installer"
Push-Location "$PSScriptRoot\..\apps\desktop\src-tauri"
cargo tauri build
Pop-Location
