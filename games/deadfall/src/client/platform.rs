//! Application-owned foreground check and console hookup. Keeps OS calls out of the engine library.
#[cfg(windows)]
pub fn focused() -> bool {
    // Read only the foreground process ID; no window is changed.
    unsafe {
        let mut pid = 0;
        windows_sys::Win32::UI::WindowsAndMessaging::GetWindowThreadProcessId(windows_sys::Win32::UI::WindowsAndMessaging::GetForegroundWindow(), &mut pid);
        pid == windows_sys::Win32::System::Threading::GetCurrentProcessId()
    }
}
#[cfg(not(windows))]
pub fn focused() -> bool {
    true
} // GameShell additionally handles minimization.

/// A windowed release build has no console of its own, so a game started from Explorer opens no stray console
/// window. Reattach to the launching terminal, if any, so `--capture` output stays visible.
pub fn attach_console() {
    #[cfg(windows)]
    // Attaching only changes where this process's stdout goes; failure (no parent console) is harmless.
    unsafe {
        windows_sys::Win32::System::Console::AttachConsole(windows_sys::Win32::System::Console::ATTACH_PARENT_PROCESS);
    }
}
