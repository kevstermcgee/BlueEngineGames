fn main() {
    #[cfg(all(windows, feature = "desktop"))]
    tauri_build::build();
}
