//! The 36-entry command enum, in `eCommand` order (design §5.2; source-map
//! §4.1 "Command index enum"), plus the per-projectile-type fire bindings.
//!
//! The key bindings themselves are **compiled in** ([`COMMAND_BINDINGS`] +
//! [`FIRE_COMMANDS`]), transcribed from the original `GameData.xml`, rather
//! than read from `GameData.toml` (user decision — same rationale as the
//! hardcoded assets / `Objects` / `LightData`). PARITY: the enum order is the
//! index space the original bound against; reordering it re-maps which key
//! each [`COMMAND_BINDINGS`] slot drives.

/// One of the 36 fixed (non-per-projectile) commands.
///
/// PARITY: variant order matches `cGameData::s_aszCommandsNames`
/// (GameEditor/cGameData.cpp:17-54) exactly, so `Command as usize` is a
/// valid index into `COMMAND_ORDER` / the `[ui].commands` array.
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub enum Command {
    Restart,
    EndGame,
    ToggleMap,
    ToggleProfilerRunning,
    ToggleProfilerVisible,
    ToggleProfilerInteraction,
    IncreaseBuildingWidth,
    IncreaseBuildingHeight,
    DecreaseBuildingWidth,
    DecreaseBuildingHeight,
    MoveBuildingRight,
    MoveBuildingDown,
    MoveBuildingLeft,
    MoveBuildingUp,
    Info,
    SelectObserverRobot,
    SelectObserverPosition,
    Enter2DMode,
    Exit2DMode,
    SetObserver,
    MoveObserverForwards,
    MoveObserverBackwards,
    MoveObserverRight,
    MoveObserverLeft,
    MoveObserverUp,
    MoveObserverDown,
    MoveObserverSpeed1,
    MoveObserverSpeed2,
    MoveObserverSpeed3,
    IncreaseGameSpeed,
    DecreaseGameSpeed,
    Pause,
    ToggleAIMode,
    Accelerate,
    Decelerate,
    FireProjectiles,
}

/// Fixed iteration order — the same order `[ui].commands` in
/// `GameData.toml` lists its 36 entries in (design §4.1). PARITY:
/// verbatim `s_aszCommandsNames` order (cGameData.cpp:17-54).
pub const COMMAND_ORDER: [Command; 36] = [
    Command::Restart,
    Command::EndGame,
    Command::ToggleMap,
    Command::ToggleProfilerRunning,
    Command::ToggleProfilerVisible,
    Command::ToggleProfilerInteraction,
    Command::IncreaseBuildingWidth,
    Command::IncreaseBuildingHeight,
    Command::DecreaseBuildingWidth,
    Command::DecreaseBuildingHeight,
    Command::MoveBuildingRight,
    Command::MoveBuildingDown,
    Command::MoveBuildingLeft,
    Command::MoveBuildingUp,
    Command::Info,
    Command::SelectObserverRobot,
    Command::SelectObserverPosition,
    Command::Enter2DMode,
    Command::Exit2DMode,
    Command::SetObserver,
    Command::MoveObserverForwards,
    Command::MoveObserverBackwards,
    Command::MoveObserverRight,
    Command::MoveObserverLeft,
    Command::MoveObserverUp,
    Command::MoveObserverDown,
    Command::MoveObserverSpeed1,
    Command::MoveObserverSpeed2,
    Command::MoveObserverSpeed3,
    Command::IncreaseGameSpeed,
    Command::DecreaseGameSpeed,
    Command::Pause,
    Command::ToggleAIMode,
    Command::Accelerate,
    Command::Decelerate,
    Command::FireProjectiles,
];

impl Command {
    /// Position of this command in the fixed `eCommand` index space
    /// (`COMMAND_ORDER`) — the same index `[ui].commands` binds against.
    pub fn index(self) -> usize {
        COMMAND_ORDER
            .iter()
            .position(|&c| c == self)
            .expect("every Command variant is in COMMAND_ORDER")
    }
}

/// A binding: key + modifier requirements. PARITY: modifiers test
/// *either*-side keys (design §5.2 "modifier flags bControl/bAlt/bShift
/// test either-side keys").
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub struct CommandBinding {
    pub key: crate::input::keys::Key,
    pub control: bool,
    pub alt: bool,
    pub shift: bool,
}

/// Compact const constructor for the binding tables below.
const fn cb(key: crate::input::keys::Key, control: bool, alt: bool, shift: bool) -> CommandBinding {
    CommandBinding {
        key,
        control,
        alt,
        shift,
    }
}

/// The 36 fixed command bindings, in `COMMAND_ORDER` index space
/// (`commands[Command::index()]`). PARITY: transcribed verbatim from the
/// original `GameData.xml` `[ui].commands` (Ctrl+arrows resize the building,
/// Ctrl+Left-mouse drops the observer point, etc.).
pub const COMMAND_BINDINGS: [CommandBinding; 36] = {
    use crate::input::keys::Key;
    [
        cb(Key::F4, false, false, false),          // 0  Restart
        cb(Key::Escape, false, false, false),      // 1  EndGame
        cb(Key::F1, false, false, false),          // 2  ToggleMap
        cb(Key::KeyP, false, false, false),        // 3  ToggleProfilerRunning
        cb(Key::BracketLeft, false, false, false), // 4  ToggleProfilerVisible
        cb(Key::BracketRight, false, false, false), // 5 ToggleProfilerInteraction
        cb(Key::ArrowRight, true, false, false),   // 6  IncreaseBuildingWidth
        cb(Key::ArrowUp, true, false, false),      // 7  IncreaseBuildingHeight
        cb(Key::ArrowLeft, true, false, false),    // 8  DecreaseBuildingWidth
        cb(Key::ArrowDown, true, false, false),    // 9  DecreaseBuildingHeight
        cb(Key::ArrowRight, false, false, false),  // 10 MoveBuildingRight
        cb(Key::ArrowUp, false, false, false),     // 11 MoveBuildingDown
        cb(Key::ArrowLeft, false, false, false),   // 12 MoveBuildingLeft
        cb(Key::ArrowDown, false, false, false),   // 13 MoveBuildingUp
        cb(Key::F2, false, false, false),          // 14 Info
        cb(Key::MouseLeft, false, false, false),   // 15 SelectObserverRobot
        cb(Key::MouseLeft, true, false, false),    // 16 SelectObserverPosition
        cb(Key::KeyA, false, false, false),        // 17 Enter2DMode
        cb(Key::KeyS, false, false, false),        // 18 Exit2DMode
        cb(Key::KeyF, false, false, false),        // 19 SetObserver
        cb(Key::KeyE, false, false, false),        // 20 MoveObserverForwards
        cb(Key::KeyD, false, false, false),        // 21 MoveObserverBackwards
        cb(Key::KeyF, false, false, false),        // 22 MoveObserverRight
        cb(Key::KeyS, false, false, false),        // 23 MoveObserverLeft
        cb(Key::KeyR, false, false, false),        // 24 MoveObserverUp
        cb(Key::KeyV, false, false, false),        // 25 MoveObserverDown
        cb(Key::Digit1, false, false, false),      // 26 MoveObserverSpeed1
        cb(Key::Digit2, false, false, false),      // 27 MoveObserverSpeed2
        cb(Key::Digit3, false, false, false),      // 28 MoveObserverSpeed3
        cb(Key::KeyT, false, false, false),        // 29 IncreaseGameSpeed
        cb(Key::KeyG, false, false, false),        // 30 DecreaseGameSpeed
        cb(Key::KeyQ, false, false, false),        // 31 Pause
        cb(Key::KeyV, false, false, false),        // 32 ToggleAIMode
        cb(Key::KeyE, false, false, false),        // 33 Accelerate
        cb(Key::KeyD, false, false, false),        // 34 Decelerate
        cb(Key::Space, false, false, false),       // 35 FireProjectiles
    ]
};

/// Per-projectile-type fire bindings, in `GameData.projectile_types` order
/// (Bullet → Left mouse, Rocket → Right mouse, Better Bullet → Left mouse).
/// PARITY: transcribed from the original `GameData.xml` `fire_command`s.
pub const FIRE_COMMANDS: [CommandBinding; 3] = {
    use crate::input::keys::Key;
    [
        cb(Key::MouseLeft, false, false, false),  // Bullet
        cb(Key::MouseRight, false, false, false), // Rocket
        cb(Key::MouseLeft, false, false, false),  // Better Bullet
    ]
};

impl CommandBinding {
    /// PARITY (`cOperationPurge::GetCommandState`, cOperationPurge.cpp:3116-3144
    /// with the verified `bExactModifiers = false`): a *required* modifier
    /// must be held (either side); a *non-required* modifier is ignored
    /// (holding Shift never suppresses a command that doesn't ask for
    /// Shift). Formally, for each modifier m: `!required_m || down_m`.
    pub fn modifiers_satisfied(&self, input: &super::Input) -> bool {
        (!self.control || input.control_down())
            && (!self.alt || input.alt_down())
            && (!self.shift || input.shift_down())
    }

    /// `bDown`: the bound key is down AND the required modifiers are held.
    pub fn is_down(&self, input: &super::Input) -> bool {
        input.is_down(self.key) && self.modifiers_satisfied(input)
    }

    /// `bPressed`: the bound key edged down this frame AND modifiers hold.
    pub fn just_pressed(&self, input: &super::Input) -> bool {
        input.just_pressed(self.key) && self.modifiers_satisfied(input)
    }

    /// `bReleased`: the bound key edged up this frame AND modifiers hold.
    pub fn just_released(&self, input: &super::Input) -> bool {
        input.just_released(self.key) && self.modifiers_satisfied(input)
    }
}

/// The full resolved binding set: the 36 fixed commands (in `COMMAND_ORDER`
/// index space) plus one fire binding per projectile type. All compiled in
/// (design §5.2) — see [`COMMAND_BINDINGS`] / [`FIRE_COMMANDS`].
#[derive(Copy, Clone, Debug)]
pub struct CommandBindings {
    /// Indexed by `Command::index()` / `COMMAND_ORDER` position.
    pub commands: [CommandBinding; 36],
    /// One per `GameData.projectile_types` entry, in array order (the index
    /// space `ProjectileType` refs point into).
    pub fire_commands: &'static [CommandBinding],
}

/// The single, compiled-in binding set (design §5.2, user decision).
pub const BINDINGS: CommandBindings = CommandBindings {
    commands: COMMAND_BINDINGS,
    fire_commands: &FIRE_COMMANDS,
};

impl CommandBindings {
    /// The hardcoded bindings (no data path — keys are compiled in).
    pub fn hardcoded() -> Self {
        BINDINGS
    }

    /// The binding for a fixed command.
    pub fn binding(&self, command: Command) -> &CommandBinding {
        &self.commands[command.index()]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_order_has_exactly_36_entries() {
        assert_eq!(COMMAND_ORDER.len(), 36);
    }

    // PARITY: pins the eCommand ABI (cGameData.cpp:17-54). Index N here
    // must equal s_aszCommandsNames[N] or GameData.toml [ui].commands
    // bindings resolve to the wrong command.
    #[test]
    fn command_order_matches_ecommand_abi() {
        assert_eq!(COMMAND_ORDER[0], Command::Restart);
        assert_eq!(COMMAND_ORDER[1], Command::EndGame);
        assert_eq!(COMMAND_ORDER[2], Command::ToggleMap);
        assert_eq!(COMMAND_ORDER[14], Command::Info);
        assert_eq!(COMMAND_ORDER[17], Command::Enter2DMode);
        assert_eq!(COMMAND_ORDER[35], Command::FireProjectiles);
    }

    #[test]
    fn command_index_is_the_inverse_of_command_order() {
        for (i, &cmd) in COMMAND_ORDER.iter().enumerate() {
            assert_eq!(cmd.index(), i);
        }
    }

    use crate::input::keys::Key;
    use crate::input::Input;

    fn binding(key: Key, control: bool, alt: bool, shift: bool) -> CommandBinding {
        CommandBinding {
            key,
            control,
            alt,
            shift,
        }
    }

    #[test]
    fn plain_binding_ignores_extra_held_modifiers() {
        // PARITY: bExactModifiers = false — a command with no required
        // modifier still fires while Shift/Ctrl/Alt happen to be down.
        let b = binding(Key::KeyF, false, false, false);
        let mut input = Input::new();
        input.on_key_event(Key::KeyF, true);
        input.on_key_event(Key::ShiftLeft, true);
        input.on_key_event(Key::ControlRight, true);
        input.begin_frame();
        assert!(b.is_down(&input));
    }

    #[test]
    fn required_modifier_must_be_held_either_side() {
        let b = binding(Key::KeyR, true, false, false); // Ctrl+R
        let mut input = Input::new();
        input.on_key_event(Key::KeyR, true);
        input.begin_frame();
        assert!(!b.is_down(&input), "no Ctrl held ⇒ inactive");

        // Right control satisfies a Ctrl requirement (either-side).
        input.on_key_event(Key::ControlRight, true);
        input.begin_frame();
        assert!(b.is_down(&input));
    }

    #[test]
    fn pressed_and_released_edges_respect_modifiers() {
        let b = binding(Key::KeyP, false, false, true); // Shift+P
        let mut input = Input::new();
        input.begin_frame(); // consume the first-frame edge suppression (FIX)
        input.on_key_event(Key::ShiftLeft, true);
        input.on_key_event(Key::KeyP, true);
        input.begin_frame();
        assert!(b.just_pressed(&input));
        assert!(b.is_down(&input));
        // Hold: no repeated press edge.
        input.begin_frame();
        assert!(!b.just_pressed(&input));
        // Release the key.
        input.on_key_event(Key::KeyP, false);
        input.begin_frame();
        assert!(b.just_released(&input));
    }

    #[test]
    fn hardcoded_bindings_have_the_expected_shape_and_anchors() {
        let bindings = CommandBindings::hardcoded();
        assert_eq!(bindings.commands.len(), 36);
        // One fire binding per shipped projectile type (Bullet/Rocket/Better).
        assert_eq!(bindings.fire_commands.len(), 3);

        // Anchor a few bindings against the original GameData.xml values.
        assert_eq!(bindings.binding(Command::Restart).key, Key::F4);
        assert_eq!(bindings.binding(Command::EndGame).key, Key::Escape);
        assert_eq!(bindings.binding(Command::FireProjectiles).key, Key::Space);
        // Ctrl+Right increases building width; Ctrl+Left-mouse drops the point.
        let w = bindings.binding(Command::IncreaseBuildingWidth);
        assert_eq!((w.key, w.control), (Key::ArrowRight, true));
        let p = bindings.binding(Command::SelectObserverPosition);
        assert_eq!((p.key, p.control), (Key::MouseLeft, true));
        // Fire bindings: Bullet=Left, Rocket=Right mouse.
        assert_eq!(bindings.fire_commands[0].key, Key::MouseLeft);
        assert_eq!(bindings.fire_commands[1].key, Key::MouseRight);
    }
}
