// Copyright Rob Gage 2026

/// A keyboard key
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub struct Key(winit::keyboard::KeyCode);

impl Key {
    /// The `A` key
    pub const A: Self = Self(winit::keyboard::KeyCode::KeyA);

    /// The `B` key
    pub const B: Self = Self(winit::keyboard::KeyCode::KeyB);

    /// The `C` key
    pub const C: Self = Self(winit::keyboard::KeyCode::KeyC);

    /// The `D` key
    pub const D: Self = Self(winit::keyboard::KeyCode::KeyD);

    /// The `E` key
    pub const E: Self = Self(winit::keyboard::KeyCode::KeyE);

    /// The `F` key
    pub const F: Self = Self(winit::keyboard::KeyCode::KeyF);

    /// The `G` key
    pub const G: Self = Self(winit::keyboard::KeyCode::KeyG);

    /// The `H` key
    pub const H: Self = Self(winit::keyboard::KeyCode::KeyH);

    /// The `I` key
    pub const I: Self = Self(winit::keyboard::KeyCode::KeyI);

    /// The `J` key
    pub const J: Self = Self(winit::keyboard::KeyCode::KeyJ);

    /// The `K` key
    pub const K: Self = Self(winit::keyboard::KeyCode::KeyK);

    /// The `L` key
    pub const L: Self = Self(winit::keyboard::KeyCode::KeyL);

    /// The `M` key
    pub const M: Self = Self(winit::keyboard::KeyCode::KeyM);

    /// The `N` key
    pub const N: Self = Self(winit::keyboard::KeyCode::KeyN);

    /// The `O` key
    pub const O: Self = Self(winit::keyboard::KeyCode::KeyO);

    /// The `P` key
    pub const P: Self = Self(winit::keyboard::KeyCode::KeyP);

    /// The `Q` key
    pub const Q: Self = Self(winit::keyboard::KeyCode::KeyQ);

    /// The `R` key
    pub const R: Self = Self(winit::keyboard::KeyCode::KeyR);

    /// The `S` key
    pub const S: Self = Self(winit::keyboard::KeyCode::KeyS);

    /// The `T` key
    pub const T: Self = Self(winit::keyboard::KeyCode::KeyT);

    /// The `U` key
    pub const U: Self = Self(winit::keyboard::KeyCode::KeyU);

    /// The `V` key
    pub const V: Self = Self(winit::keyboard::KeyCode::KeyV);

    /// The `W` key
    pub const W: Self = Self(winit::keyboard::KeyCode::KeyW);

    /// The `X` key
    pub const X: Self = Self(winit::keyboard::KeyCode::KeyX);

    /// The `Y` key
    pub const Y: Self = Self(winit::keyboard::KeyCode::KeyY);

    /// The `Z` key
    pub const Z: Self = Self(winit::keyboard::KeyCode::KeyZ);

    /// The `0` key
    pub const DIGIT_0: Self = Self(winit::keyboard::KeyCode::Digit0);

    /// The `1` key
    pub const DIGIT_1: Self = Self(winit::keyboard::KeyCode::Digit1);

    /// The `2` key
    pub const DIGIT_2: Self = Self(winit::keyboard::KeyCode::Digit2);

    /// The `3` key
    pub const DIGIT_3: Self = Self(winit::keyboard::KeyCode::Digit3);

    /// The `4` key
    pub const DIGIT_4: Self = Self(winit::keyboard::KeyCode::Digit4);

    /// The `5` key
    pub const DIGIT_5: Self = Self(winit::keyboard::KeyCode::Digit5);

    /// The `6` key
    ///
    pub const DIGIT_6: Self = Self(winit::keyboard::KeyCode::Digit6);

    /// The `7` key
    pub const DIGIT_7: Self = Self(winit::keyboard::KeyCode::Digit7);

    /// The `8` key
    pub const DIGIT_8: Self = Self(winit::keyboard::KeyCode::Digit8);

    /// The `9` key
    pub const DIGIT_9: Self = Self(winit::keyboard::KeyCode::Digit9);

    /// The space key
    pub const SPACE: Self = Self(winit::keyboard::KeyCode::Space);

    /// The escape key
    pub const ESCAPE: Self = Self(winit::keyboard::KeyCode::Escape);

    /// The tilde/grave key
    pub const TILDE: Self = Self(winit::keyboard::KeyCode::Backquote);

    /// The tab key
    pub const TAB: Self = Self(winit::keyboard::KeyCode::Tab);

    /// The `F1` key
    pub const F1: Self = Self(winit::keyboard::KeyCode::F1);

    /// The `F2` key
    pub const F2: Self = Self(winit::keyboard::KeyCode::F2);

    /// The `F3` key
    pub const F3: Self = Self(winit::keyboard::KeyCode::F3);

    /// The `F4` key
    pub const F4: Self = Self(winit::keyboard::KeyCode::F4);

    /// The `F5` key
    pub const F5: Self = Self(winit::keyboard::KeyCode::F5);

    /// The `F6` key
    pub const F6: Self = Self(winit::keyboard::KeyCode::F6);

    /// The `F7` key
    pub const F7: Self = Self(winit::keyboard::KeyCode::F7);

    /// The `F8` key
    pub const F8: Self = Self(winit::keyboard::KeyCode::F8);

    /// The `F9` key
    pub const F9: Self = Self(winit::keyboard::KeyCode::F9);

    /// The `F10` key
    pub const F10: Self = Self(winit::keyboard::KeyCode::F10);

    /// The `F11` key
    pub const F11: Self = Self(winit::keyboard::KeyCode::F11);

    /// The `F12` key
    pub const F12: Self = Self(winit::keyboard::KeyCode::F12);

    /// The up arrow key
    pub const ARROW_UP: Self = Self(winit::keyboard::KeyCode::ArrowUp);

    /// The down arrow key
    pub const ARROW_DOWN: Self = Self(winit::keyboard::KeyCode::ArrowDown);

    /// The left arrow key
    pub const ARROW_LEFT: Self = Self(winit::keyboard::KeyCode::ArrowLeft);

    /// The right arrow key
    pub const ARROW_RIGHT: Self = Self(winit::keyboard::KeyCode::ArrowRight);

    /// Creates a `Key` from a winit physical key code
    pub(super) const fn from_winit(key_code: winit::keyboard::KeyCode) -> Self {
        Self(key_code)
    }
}
