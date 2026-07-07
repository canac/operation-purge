//! The 100-entry key table — ordinal order AND display-name strings are
//! the serialization ABI (design §1.5.1, §5.2; libs-map §5.5/§6 item 6).
//!
//! PARITY: verbatim from `cDirectInput.cpp:9-108`. `g_nKeys = keyMiddleButton
//! + 1 = 100` (**not 99** — source-map's "99-entry" note undercounts by
//! one; the corrected count is pinned by a unit test below).
//! `keyLast = keyRShift = 96` is the default `tCommand` key; the three
//! mouse buttons (97-99) are valid binding targets beyond `keyLast`.
//!
//! `GameData.toml` `Key` values deserialize from these exact display-name
//! strings (design §4.1) — renaming a variant or reordering the table
//! changes the on-disk format.
//!
//! winit `KeyCode`/`MouseButton` mapping is wired up at M3 (task
//! instructions: keep winit APIs out of M0 module stubs); variant names
//! below already match winit's `KeyCode` naming so that mapping is
//! mechanical.

use winit::event::MouseButton;
use winit::keyboard::{KeyCode, PhysicalKey};

/// One of the 100 bindable physical keys/buttons.
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
#[repr(u8)]
pub enum Key {
    KeyA = 0,
    KeyB,
    KeyC,
    KeyD,
    KeyE,
    KeyF,
    KeyG,
    KeyH,
    KeyI,
    KeyJ,
    KeyK,
    KeyL,
    KeyM,
    KeyN,
    KeyO,
    KeyP,
    KeyQ,
    KeyR,
    KeyS,
    KeyT,
    KeyU,
    KeyV,
    KeyW,
    KeyX,
    KeyY,
    KeyZ, // 25
    Digit0, // 26
    Digit1,
    Digit2,
    Digit3,
    Digit4,
    Digit5,
    Digit6,
    Digit7,
    Digit8,
    Digit9, // 35
    Backquote,    // 36
    BracketLeft,  // 37
    BracketRight, // 38
    Backslash,    // 39
    Semicolon,    // 40
    Quote,        // 41
    Comma,        // 42
    Period,       // 43
    Slash,        // 44
    Minus,        // 45
    Equal,        // 46
    Backspace,  // 47
    Enter,      // 48
    Tab,        // 49
    Escape,     // 50
    Insert,     // 51
    Delete,     // 52
    Home,       // 53
    End,        // 54
    PageUp,     // 55
    PageDown,   // 56
    NumpadAdd,      // 57
    NumpadSubtract, // 58
    NumpadMultiply, // 59
    NumpadDivide,   // 60
    Numpad0, // 61
    Numpad1,
    Numpad2,
    Numpad3,
    Numpad4,
    Numpad5,
    Numpad6,
    Numpad7,
    Numpad8,
    Numpad9, // 70
    NumpadDecimal, // 71
    NumpadEnter,   // 72
    ArrowLeft,  // 73
    ArrowRight, // 74
    ArrowUp,    // 75
    ArrowDown,  // 76
    Space, // 77
    F1, // 78
    F2,
    F3,
    F4,
    F5,
    F6,
    F7,
    F8,
    F9,
    F10,
    F11,
    F12, // 89
    Pause, // 90
    ControlLeft,  // 91
    ControlRight, // 92
    AltLeft,      // 93
    AltRight,     // 94
    ShiftLeft,    // 95
    ShiftRight,   // 96 == keyLast
    MouseLeft,   // 97
    MouseRight,  // 98
    MouseMiddle, // 99
}

/// Total bindable keys — PARITY: `g_nKeys = keyMiddleButton + 1`.
pub const KEY_COUNT: usize = 100;

/// `keyLast` — the default `tCommand` key (`cGameData.cpp:1045`).
pub const KEY_LAST: Key = Key::ShiftRight;

/// `(ordinal, display name)` in the exact order/casing from
/// `cDirectInput.cpp:9-108`. Casing is intentionally inconsistent
/// ("Left control" vs "Left Shift") — PARITY, preserve verbatim.
pub const KEY_TABLE: [(Key, &str); KEY_COUNT] = [
    (Key::KeyA, "A"),
    (Key::KeyB, "B"),
    (Key::KeyC, "C"),
    (Key::KeyD, "D"),
    (Key::KeyE, "E"),
    (Key::KeyF, "F"),
    (Key::KeyG, "G"),
    (Key::KeyH, "H"),
    (Key::KeyI, "I"),
    (Key::KeyJ, "J"),
    (Key::KeyK, "K"),
    (Key::KeyL, "L"),
    (Key::KeyM, "M"),
    (Key::KeyN, "N"),
    (Key::KeyO, "O"),
    (Key::KeyP, "P"),
    (Key::KeyQ, "Q"),
    (Key::KeyR, "R"),
    (Key::KeyS, "S"),
    (Key::KeyT, "T"),
    (Key::KeyU, "U"),
    (Key::KeyV, "V"),
    (Key::KeyW, "W"),
    (Key::KeyX, "X"),
    (Key::KeyY, "Y"),
    (Key::KeyZ, "Z"),
    (Key::Digit0, "0"),
    (Key::Digit1, "1"),
    (Key::Digit2, "2"),
    (Key::Digit3, "3"),
    (Key::Digit4, "4"),
    (Key::Digit5, "5"),
    (Key::Digit6, "6"),
    (Key::Digit7, "7"),
    (Key::Digit8, "8"),
    (Key::Digit9, "9"),
    (Key::Backquote, "`"),
    (Key::BracketLeft, "["),
    (Key::BracketRight, "]"),
    (Key::Backslash, "\\"),
    (Key::Semicolon, ";"),
    (Key::Quote, "'"),
    (Key::Comma, ","),
    (Key::Period, "."),
    (Key::Slash, "/"),
    (Key::Minus, "-"),
    (Key::Equal, "="),
    (Key::Backspace, "Backspace"),
    (Key::Enter, "Enter"),
    (Key::Tab, "Tab"),
    (Key::Escape, "Escape"),
    (Key::Insert, "Insert"),
    (Key::Delete, "Delete"),
    (Key::Home, "Home"),
    (Key::End, "End"),
    (Key::PageUp, "Page up"),
    (Key::PageDown, "Page down"),
    (Key::NumpadAdd, "Num +"),
    (Key::NumpadSubtract, "Num -"),
    (Key::NumpadMultiply, "Num *"),
    (Key::NumpadDivide, "Num /"),
    (Key::Numpad0, "Num 0"),
    (Key::Numpad1, "Num 1"),
    (Key::Numpad2, "Num 2"),
    (Key::Numpad3, "Num 3"),
    (Key::Numpad4, "Num 4"),
    (Key::Numpad5, "Num 5"),
    (Key::Numpad6, "Num 6"),
    (Key::Numpad7, "Num 7"),
    (Key::Numpad8, "Num 8"),
    (Key::Numpad9, "Num 9"),
    (Key::NumpadDecimal, "Num ."),
    (Key::NumpadEnter, "Num Enter"),
    (Key::ArrowLeft, "Left"),
    (Key::ArrowRight, "Right"),
    (Key::ArrowUp, "Up"),
    (Key::ArrowDown, "Down"),
    (Key::Space, "Space"),
    (Key::F1, "F1"),
    (Key::F2, "F2"),
    (Key::F3, "F3"),
    (Key::F4, "F4"),
    (Key::F5, "F5"),
    (Key::F6, "F6"),
    (Key::F7, "F7"),
    (Key::F8, "F8"),
    (Key::F9, "F9"),
    (Key::F10, "F10"),
    (Key::F11, "F11"),
    (Key::F12, "F12"),
    (Key::Pause, "Pause"),
    (Key::ControlLeft, "Left control"),
    (Key::ControlRight, "Right control"),
    (Key::AltLeft, "Left alt"),
    (Key::AltRight, "Right alt"),
    (Key::ShiftLeft, "Left Shift"),
    (Key::ShiftRight, "Right Shift"),
    (Key::MouseLeft, "Left mouse button"),
    (Key::MouseRight, "Right mouse button"),
    (Key::MouseMiddle, "Middle mouse button"),
];

impl Key {
    #[inline]
    pub fn ordinal(self) -> u8 {
        self as u8
    }

    pub fn from_ordinal(ordinal: u8) -> Option<Key> {
        KEY_TABLE.get(ordinal as usize).map(|(k, _)| *k)
    }

    pub fn display_name(self) -> &'static str {
        KEY_TABLE[self.ordinal() as usize].1
    }

    pub fn from_display_name(name: &str) -> Option<Key> {
        KEY_TABLE.iter().find(|(_, n)| *n == name).map(|(k, _)| *k)
    }

    /// The winit physical `KeyCode` this key corresponds to, or `None` for
    /// the three mouse buttons (which arrive via `MouseButton`, not the
    /// keyboard, design §5.2). PARITY: DirectInput used physical scancodes
    /// (`DIK_*`); winit `KeyCode` is likewise physical/layout-independent,
    /// and our variant names were chosen to match winit's `KeyCode` naming
    /// 1:1 so this mapping is mechanical (`cDirectInput.cpp:9-108`).
    pub fn key_code(self) -> Option<KeyCode> {
        use Key::*;
        Some(match self {
            KeyA => KeyCode::KeyA,
            KeyB => KeyCode::KeyB,
            KeyC => KeyCode::KeyC,
            KeyD => KeyCode::KeyD,
            KeyE => KeyCode::KeyE,
            KeyF => KeyCode::KeyF,
            KeyG => KeyCode::KeyG,
            KeyH => KeyCode::KeyH,
            KeyI => KeyCode::KeyI,
            KeyJ => KeyCode::KeyJ,
            KeyK => KeyCode::KeyK,
            KeyL => KeyCode::KeyL,
            KeyM => KeyCode::KeyM,
            KeyN => KeyCode::KeyN,
            KeyO => KeyCode::KeyO,
            KeyP => KeyCode::KeyP,
            KeyQ => KeyCode::KeyQ,
            KeyR => KeyCode::KeyR,
            KeyS => KeyCode::KeyS,
            KeyT => KeyCode::KeyT,
            KeyU => KeyCode::KeyU,
            KeyV => KeyCode::KeyV,
            KeyW => KeyCode::KeyW,
            KeyX => KeyCode::KeyX,
            KeyY => KeyCode::KeyY,
            KeyZ => KeyCode::KeyZ,
            Digit0 => KeyCode::Digit0,
            Digit1 => KeyCode::Digit1,
            Digit2 => KeyCode::Digit2,
            Digit3 => KeyCode::Digit3,
            Digit4 => KeyCode::Digit4,
            Digit5 => KeyCode::Digit5,
            Digit6 => KeyCode::Digit6,
            Digit7 => KeyCode::Digit7,
            Digit8 => KeyCode::Digit8,
            Digit9 => KeyCode::Digit9,
            Backquote => KeyCode::Backquote,
            BracketLeft => KeyCode::BracketLeft,
            BracketRight => KeyCode::BracketRight,
            Backslash => KeyCode::Backslash,
            Semicolon => KeyCode::Semicolon,
            Quote => KeyCode::Quote,
            Comma => KeyCode::Comma,
            Period => KeyCode::Period,
            Slash => KeyCode::Slash,
            Minus => KeyCode::Minus,
            Equal => KeyCode::Equal,
            Backspace => KeyCode::Backspace,
            Enter => KeyCode::Enter,
            Tab => KeyCode::Tab,
            Escape => KeyCode::Escape,
            Insert => KeyCode::Insert,
            Delete => KeyCode::Delete,
            Home => KeyCode::Home,
            End => KeyCode::End,
            PageUp => KeyCode::PageUp,
            PageDown => KeyCode::PageDown,
            NumpadAdd => KeyCode::NumpadAdd,
            NumpadSubtract => KeyCode::NumpadSubtract,
            NumpadMultiply => KeyCode::NumpadMultiply,
            NumpadDivide => KeyCode::NumpadDivide,
            Numpad0 => KeyCode::Numpad0,
            Numpad1 => KeyCode::Numpad1,
            Numpad2 => KeyCode::Numpad2,
            Numpad3 => KeyCode::Numpad3,
            Numpad4 => KeyCode::Numpad4,
            Numpad5 => KeyCode::Numpad5,
            Numpad6 => KeyCode::Numpad6,
            Numpad7 => KeyCode::Numpad7,
            Numpad8 => KeyCode::Numpad8,
            Numpad9 => KeyCode::Numpad9,
            NumpadDecimal => KeyCode::NumpadDecimal,
            NumpadEnter => KeyCode::NumpadEnter,
            ArrowLeft => KeyCode::ArrowLeft,
            ArrowRight => KeyCode::ArrowRight,
            ArrowUp => KeyCode::ArrowUp,
            ArrowDown => KeyCode::ArrowDown,
            Space => KeyCode::Space,
            F1 => KeyCode::F1,
            F2 => KeyCode::F2,
            F3 => KeyCode::F3,
            F4 => KeyCode::F4,
            F5 => KeyCode::F5,
            F6 => KeyCode::F6,
            F7 => KeyCode::F7,
            F8 => KeyCode::F8,
            F9 => KeyCode::F9,
            F10 => KeyCode::F10,
            F11 => KeyCode::F11,
            F12 => KeyCode::F12,
            Pause => KeyCode::Pause,
            ControlLeft => KeyCode::ControlLeft,
            ControlRight => KeyCode::ControlRight,
            AltLeft => KeyCode::AltLeft,
            AltRight => KeyCode::AltRight,
            ShiftLeft => KeyCode::ShiftLeft,
            ShiftRight => KeyCode::ShiftRight,
            MouseLeft | MouseRight | MouseMiddle => return None,
        })
    }

    /// The winit `MouseButton` this key corresponds to, or `None` for every
    /// keyboard key (ordinals 0..=96, design §5.2).
    pub fn mouse_button(self) -> Option<MouseButton> {
        match self {
            Key::MouseLeft => Some(MouseButton::Left),
            Key::MouseRight => Some(MouseButton::Right),
            Key::MouseMiddle => Some(MouseButton::Middle),
            _ => None,
        }
    }

    /// Inverse of [`Key::key_code`].
    pub fn from_key_code(code: KeyCode) -> Option<Key> {
        KEY_TABLE
            .iter()
            .find(|(k, _)| k.key_code() == Some(code))
            .map(|(k, _)| *k)
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
    fn table_has_exactly_100_entries() {
        assert_eq!(KEY_TABLE.len(), KEY_COUNT);
        assert_eq!(KEY_COUNT, 100);
    }

    #[test]
    fn ordinals_are_dense_and_match_index() {
        for (i, (key, _)) in KEY_TABLE.iter().enumerate() {
            assert_eq!(key.ordinal() as usize, i);
        }
    }

    #[test]
    fn key_last_is_right_shift_at_ordinal_96() {
        assert_eq!(KEY_LAST, Key::ShiftRight);
        assert_eq!(KEY_LAST.ordinal(), 96);
        assert_eq!(KEY_LAST.display_name(), "Right Shift");
    }

    #[test]
    fn names_round_trip() {
        for (key, name) in KEY_TABLE.iter() {
            assert_eq!(Key::from_display_name(name), Some(*key));
            assert_eq!(key.display_name(), *name);
        }
    }

    #[test]
    fn mouse_buttons_are_beyond_key_last() {
        assert!(Key::MouseLeft.ordinal() > KEY_LAST.ordinal());
        assert!(Key::MouseRight.ordinal() > KEY_LAST.ordinal());
        assert!(Key::MouseMiddle.ordinal() > KEY_LAST.ordinal());
    }

    #[test]
    fn every_keyboard_key_round_trips_through_winit_key_code() {
        for (key, _) in KEY_TABLE.iter() {
            match key.mouse_button() {
                Some(_) => {
                    // Mouse buttons have no KeyCode.
                    assert_eq!(key.key_code(), None);
                }
                None => {
                    let code = key
                        .key_code()
                        .expect("every non-mouse key maps to a KeyCode");
                    assert_eq!(Key::from_key_code(code), Some(*key));
                    assert_eq!(
                        Key::from_physical_key(PhysicalKey::Code(code)),
                        Some(*key)
                    );
                }
            }
        }
    }

    #[test]
    fn exactly_three_keys_are_mouse_buttons() {
        let mouse: Vec<_> = KEY_TABLE
            .iter()
            .filter(|(k, _)| k.mouse_button().is_some())
            .map(|(k, _)| *k)
            .collect();
        assert_eq!(mouse, vec![Key::MouseLeft, Key::MouseRight, Key::MouseMiddle]);
    }

    #[test]
    fn mouse_button_round_trip() {
        assert_eq!(Key::from_mouse_button(MouseButton::Left), Some(Key::MouseLeft));
        assert_eq!(
            Key::from_mouse_button(MouseButton::Right),
            Some(Key::MouseRight)
        );
        assert_eq!(
            Key::from_mouse_button(MouseButton::Middle),
            Some(Key::MouseMiddle)
        );
        // PARITY: extra buttons don't bind (only 3 tracked in the original).
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
