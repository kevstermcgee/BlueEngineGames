//! Own-process Windows keyboard fallback for scan-code-free injected events.
use macroquad::prelude::{KeyCode, MouseButton};
use std::{cell::RefCell, collections::HashSet};
#[derive(Default)]
struct State {
    down: HashSet<KeyCode>,
    pressed: HashSet<KeyCode>,
    mouse_down: HashSet<MouseButton>,
    mouse_pressed: HashSet<MouseButton>,
}
thread_local! { static KEYS: RefCell<State> = RefCell::new(State::default()); }
pub fn poll(focused: bool) {
    KEYS.with(|cell| {
        let mut state = cell.borrow_mut();
        state.pressed.clear();
        #[cfg(windows)]
        for (key, vk) in [
            (KeyCode::W, 0x57),
            (KeyCode::A, 0x41),
            (KeyCode::S, 0x53),
            (KeyCode::D, 0x44),
            (KeyCode::Up, 0x26),
            (KeyCode::Down, 0x28),
            (KeyCode::Left, 0x25),
            (KeyCode::Right, 0x27),
            (KeyCode::Space, 0x20),
            (KeyCode::LeftShift, 0xA0),
            (KeyCode::LeftControl, 0xA2),
            (KeyCode::Escape, 0x1B),
            (KeyCode::F, 0x46),
            (KeyCode::F11, 0x7A),
            (KeyCode::F3, 0x72),
            (KeyCode::Enter, 0x0D),
            (KeyCode::Tab, 0x09),
            (KeyCode::LeftAlt, 0xA4),
            (KeyCode::R, 0x52),
            (KeyCode::Key1, 0x31),
            (KeyCode::Key2, 0x32),
            (KeyCode::Key3, 0x33),
            (KeyCode::Key4, 0x34),
            (KeyCode::Key5, 0x35),
            (KeyCode::Key6, 0x36),
            (KeyCode::Key7, 0x37),
            (KeyCode::Key8, 0x38),
            (KeyCode::Key9, 0x39),
            (KeyCode::Key0, 0x30),
        ] {
            // Read-only Win32 query takes a virtual-key integer, no pointers.
            let bits =
                unsafe { windows_sys::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState(vk) }
                    as u16;
            let held = bits & 0x8000 != 0;
            if focused && !state.down.contains(&key) && (held || bits & 1 != 0) {
                state.pressed.insert(key);
            }
            if focused && held {
                state.down.insert(key);
            } else {
                state.down.remove(&key);
            }
        }
        state.mouse_pressed.clear();
        #[cfg(windows)]
        for (button, vk) in [(MouseButton::Left, 1), (MouseButton::Right, 2)] {
            // Read-only own-input query; the foreground gate excludes other apps.
            let held =
                unsafe { windows_sys::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState(vk) }
                    as u16
                    & 0x8000
                    != 0;
            if focused && held && !state.mouse_down.contains(&button) {
                state.mouse_pressed.insert(button);
            }
            if focused && held {
                state.mouse_down.insert(button);
            } else {
                state.mouse_down.remove(&button);
            }
        }
        if !focused {
            state.down.clear();
            state.pressed.clear();
        }
    });
}
pub fn is_key_down(key: KeyCode) -> bool {
    #[cfg(windows)]
    {
        KEYS.with(|s| s.borrow().down.contains(&key))
    }
    #[cfg(not(windows))]
    {
        macroquad::input::is_key_down(key)
    }
}
pub fn is_key_pressed(key: KeyCode) -> bool {
    #[cfg(windows)]
    {
        KEYS.with(|s| s.borrow().pressed.contains(&key))
    }
    #[cfg(not(windows))]
    {
        macroquad::input::is_key_pressed(key)
    }
}
pub fn axes() -> (f32, f32) {
    let held = |a, b| {
        if is_key_down(a) || is_key_down(b) {
            1.
        } else {
            0.
        }
    };
    (
        held(KeyCode::W, KeyCode::Up) - held(KeyCode::S, KeyCode::Down),
        held(KeyCode::D, KeyCode::Right) - held(KeyCode::A, KeyCode::Left),
    )
}

pub fn is_mouse_button_down(button: MouseButton) -> bool {
    KEYS.with(|s| s.borrow().mouse_down.contains(&button))
        || macroquad::input::is_mouse_button_down(button)
}
pub fn is_mouse_button_pressed(button: MouseButton) -> bool {
    KEYS.with(|s| s.borrow().mouse_pressed.contains(&button))
        || macroquad::input::is_mouse_button_pressed(button)
}
