$ErrorActionPreference = "Stop"
Set-Location (Split-Path -Parent $PSScriptRoot)
cargo run --release --locked -- --host
if ($LASTEXITCODE -ne 0) { throw "Game launch failed ($LASTEXITCODE)" }
