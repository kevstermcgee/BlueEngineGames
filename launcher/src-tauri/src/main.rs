#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    #[cfg(all(windows, feature = "desktop"))]
    blueengine_launcher::desktop::run();
    #[cfg(not(all(windows, feature = "desktop")))]
    eprintln!(
        "BlueEngine Games is a Windows application. Its library core can be tested on this host."
    );
}
