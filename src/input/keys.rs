//! The 100 bindable keys, in the original's ordinal order.
//!
//! PARITY: verbatim from `cDirectInput.cpp:9-108`. `g_nKeys = keyMiddleButton
//! + 1 = 100`. The 97 keyboard variants are named after their winit
//! `KeyCode` so both directions of the mapping come from one list; the
//! three mouse buttons (97-99) arrive via `MouseButton` instead.

use winit::event::MouseButton;
use winit::keyboard::{KeyCode, PhysicalKey};

macro_rules! keys {
    ($($key:ident),* ; $($mouse:ident),*) => {
        /// One of the 100 bindable physical keys/buttons.
        #[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
        #[repr(u8)]
        pub enum Key {
            $($key,)*
            $($mouse,)*
        }

        impl Key {
            /// Every key, in ordinal order.
            pub const ALL: [Key; KEY_COUNT] = [$(Key::$key,)* $(Key::$mouse,)*];

            /// The winit physical `KeyCode` for this key, or `None` for the
            /// three mouse buttons. PARITY: DirectInput used physical
            /// scancodes (`DIK_*`); winit `KeyCode` is likewise
            /// layout-independent.
            pub fn key_code(self) -> Option<KeyCode> {
                match self {
                    $(Key::$key => Some(KeyCode::$key),)*
                    $(Key::$mouse => None,)*
                }
            }

            /// Inverse of [`Key::key_code`].
            pub fn from_key_code(code: KeyCode) -> Option<Key> {
                match code {
                    $(KeyCode::$key => Some(Key::$key),)*
                    _ => None,
                }
            }
        }
    };
}

keys! {
    KeyA, KeyB, KeyC, KeyD, KeyE, KeyF, KeyG, KeyH, KeyI, KeyJ, KeyK, KeyL, KeyM,
    KeyN, KeyO, KeyP, KeyQ, KeyR, KeyS, KeyT, KeyU, KeyV, KeyW, KeyX, KeyY, KeyZ,
    Digit0, Digit1, Digit2, Digit3, Digit4, Digit5, Digit6, Digit7, Digit8, Digit9,
    Backquote, BracketLeft, BracketRight, Backslash, Semicolon, Quote, Comma, Period,
    Slash, Minus, Equal,
    Backspace, Enter, Tab, Escape, Insert, Delete, Home, End, PageUp, PageDown,
    NumpadAdd, NumpadSubtract, NumpadMultiply, NumpadDivide,
    Numpad0, Numpad1, Numpad2, Numpad3, Numpad4, Numpad5, Numpad6, Numpad7, Numpad8,
    Numpad9, NumpadDecimal, NumpadEnter,
    ArrowLeft, ArrowRight, ArrowUp, ArrowDown,
    Space,
    F1, F2, F3, F4, F5, F6, F7, F8, F9, F10, F11, F12,
    Pause,
    ControlLeft, ControlRight, AltLeft, AltRight, ShiftLeft, ShiftRight;
    MouseLeft, MouseRight, MouseMiddle
}

/// Total bindable keys — PARITY: `g_nKeys = keyMiddleButton + 1`.
pub const KEY_COUNT: usize = 100;

impl Key {
    #[inline]
    pub fn ordinal(self) -> u8 {
        self as u8
    }

    /// The winit `MouseButton` this key corresponds to, or `None` for every
    /// keyboard key.
    pub fn mouse_button(self) -> Option<MouseButton> {
        match self {
            Key::MouseLeft => Some(MouseButton::Left),
            Key::MouseRight => Some(MouseButton::Right),
            Key::MouseMiddle => Some(MouseButton::Middle),
            _ => None,
        }
    }

    /// Resolve a winit `PhysicalKey` to a bindable `Key` (only physical
    /// scancodes are honored — `PhysicalKey::Unidentified` never binds).
    pub fn from_physical_key(key: PhysicalKey) -> Option<Key> {
        match key {
            PhysicalKey::Code(code) => Key::from_key_code(code),
            PhysicalKey::Unidentified(_) => None,
        }
    }

    /// Inverse of [`Key::mouse_button`]. PARITY: only the three buttons the
    /// original tracked (`keyLeftButton`/`keyRightButton`/`keyMiddleButton`)
    /// bind; extra buttons (`Back`/`Forward`/`Other`) are ignored.
    pub fn from_mouse_button(button: MouseButton) -> Option<Key> {
        match button {
            MouseButton::Left => Some(Key::MouseLeft),
            MouseButton::Right => Some(Key::MouseRight),
            MouseButton::Middle => Some(Key::MouseMiddle),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ordinals_are_dense_and_pinned_to_the_original() {
        for (i, key) in Key::ALL.iter().enumerate() {
            assert_eq!(key.ordinal() as usize, i);
        }
        assert_eq!(Key::Digit0.ordinal(), 26);
        assert_eq!(Key::Backspace.ordinal(), 47);
        assert_eq!(Key::Space.ordinal(), 77);
        assert_eq!(Key::F1.ordinal(), 78);
        assert_eq!(Key::ShiftRight.ordinal(), 96);
        assert_eq!(Key::MouseMiddle.ordinal(), 99);
    }

    #[test]
    fn every_keyboard_key_round_trips_through_winit_key_code() {
        for key in Key::ALL {
            match key.key_code() {
                None => assert!(key.mouse_button().is_some()),
                Some(code) => {
                    assert_eq!(Key::from_key_code(code), Some(key));
                    assert_eq!(Key::from_physical_key(PhysicalKey::Code(code)), Some(key));
                }
            }
        }
        assert_eq!(Key::from_key_code(KeyCode::CapsLock), None);
    }

    #[test]
    fn exactly_three_keys_are_mouse_buttons() {
        let mouse: Vec<_> = Key::ALL.into_iter().filter(|k| k.mouse_button().is_some()).collect();
        assert_eq!(mouse, vec![Key::MouseLeft, Key::MouseRight, Key::MouseMiddle]);
    }

    #[test]
    fn mouse_button_round_trip() {
        assert_eq!(Key::from_mouse_button(MouseButton::Left), Some(Key::MouseLeft));
        assert_eq!(Key::from_mouse_button(MouseButton::Right), Some(Key::MouseRight));
        assert_eq!(Key::from_mouse_button(MouseButton::Middle), Some(Key::MouseMiddle));
        assert_eq!(Key::from_mouse_button(MouseButton::Forward), None);
        assert_eq!(Key::MouseLeft.mouse_button(), Some(MouseButton::Left));
    }

    #[test]
    fn unidentified_physical_key_does_not_bind() {
        use winit::keyboard::NativeKeyCode;
        assert_eq!(
            Key::from_physical_key(PhysicalKey::Unidentified(NativeKeyCode::Unidentified)),
            None
        );
    }
}
