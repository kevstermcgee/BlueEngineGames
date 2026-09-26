param(
    [string]$Listen = "0.0.0.0:4200",
    [string]$Key = "riftwake-party"
)
$ErrorActionPreference = "Stop"
Set-Location (Split-Path -Parent $PSScriptRoot)
cargo run --release --locked --bin riftwake-server -- --listen $Listen --key $Key
