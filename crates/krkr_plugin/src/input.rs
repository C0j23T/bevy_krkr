//! Host input → Kirikiri input events.
//!
//! Kirikiroid2 expects the platform to feed it input the way a native window
//! manager would: the host posts `tTVPOn*InputEvent`s and the engine routes them
//! to the TJS handlers of the front-most `Window` (`onMouseMove`,
//! `onMouseDown`, `onMouseUp`, `onKeyDown`, `onKeyUp`). That is the surface a
//! game's own UI listens on, so without it its menus and its "click to
//! advance" never react.
//!
//! Positions arrive in Bevy's window space and are converted to the Kirikiri
//! window's pixels through the camera and the screen sprite
//! ([`ScreenPlacement`]), so a moved or scaled screen still gets the right
//! coordinates. Touches are reported as the left mouse button, which is what a
//! game written for a desktop expects.
//!
//! Nothing is forwarded while no instance runs, and the engine ignores input
//! while no `Window` of the game is visible.

use std::ffi::c_int;

use bevy::input::mouse::MouseButton;
use bevy::input::touch::Touches;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use krkr_sys as sys;

use crate::render::{KirikiriScreen, KirikiriScreenImage};
use crate::{KirikiriCamera, KirikiriState};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ScreenPlacement {
    pub center: Vec2,
    pub scale: Vec2,
    pub size: UVec2,
}

impl ScreenPlacement {
    pub fn new(transform: &GlobalTransform, size: UVec2) -> Self {
        // The rotation is ignored: a screen tilted away from the camera would
        // also need a different hit test, and no host does that.
        Self {
            center: transform.translation().truncate(),
            scale: transform.scale().truncate(),
            size,
        }
    }

    pub fn pixel_at(self, world: Vec2) -> Option<UVec2> {
        if self.size.x == 0 || self.size.y == 0 || self.scale.x == 0.0 || self.scale.y == 0.0 {
            return None;
        }
        // Kirikiri pixels relative to the middle of the screen, y up.
        let local = (world - self.center) / self.scale;
        let x = local.x + self.size.x as f32 * 0.5;
        let y = self.size.y as f32 * 0.5 - local.y;
        if !(x >= 0.0 && y >= 0.0) {
            return None;
        }
        let (x, y) = (x as u32, y as u32);
        (x < self.size.x && y < self.size.y).then_some(UVec2::new(x, y))
    }
}

pub fn shift_state(keys: &ButtonInput<KeyCode>) -> u32 {
    let mut state = 0;
    if keys.any_pressed([KeyCode::ShiftLeft, KeyCode::ShiftRight]) {
        state |= 0x1;
    }
    if keys.any_pressed([KeyCode::AltLeft, KeyCode::AltRight]) {
        state |= 0x2;
    }
    if keys.any_pressed([KeyCode::ControlLeft, KeyCode::ControlRight]) {
        state |= 0x4;
    }
    state
}

pub fn virtual_key(key: KeyCode) -> Option<u8> {
    Some(match key {
        KeyCode::KeyA => 0x41,
        KeyCode::KeyB => 0x42,
        KeyCode::KeyC => 0x43,
        KeyCode::KeyD => 0x44,
        KeyCode::KeyE => 0x45,
        KeyCode::KeyF => 0x46,
        KeyCode::KeyG => 0x47,
        KeyCode::KeyH => 0x48,
        KeyCode::KeyI => 0x49,
        KeyCode::KeyJ => 0x4A,
        KeyCode::KeyK => 0x4B,
        KeyCode::KeyL => 0x4C,
        KeyCode::KeyM => 0x4D,
        KeyCode::KeyN => 0x4E,
        KeyCode::KeyO => 0x4F,
        KeyCode::KeyP => 0x50,
        KeyCode::KeyQ => 0x51,
        KeyCode::KeyR => 0x52,
        KeyCode::KeyS => 0x53,
        KeyCode::KeyT => 0x54,
        KeyCode::KeyU => 0x55,
        KeyCode::KeyV => 0x56,
        KeyCode::KeyW => 0x57,
        KeyCode::KeyX => 0x58,
        KeyCode::KeyY => 0x59,
        KeyCode::KeyZ => 0x5A,

        KeyCode::Digit0 => 0x30,
        KeyCode::Digit1 => 0x31,
        KeyCode::Digit2 => 0x32,
        KeyCode::Digit3 => 0x33,
        KeyCode::Digit4 => 0x34,
        KeyCode::Digit5 => 0x35,
        KeyCode::Digit6 => 0x36,
        KeyCode::Digit7 => 0x37,
        KeyCode::Digit8 => 0x38,
        KeyCode::Digit9 => 0x39,

        KeyCode::Numpad0 => 0x60,
        KeyCode::Numpad1 => 0x61,
        KeyCode::Numpad2 => 0x62,
        KeyCode::Numpad3 => 0x63,
        KeyCode::Numpad4 => 0x64,
        KeyCode::Numpad5 => 0x65,
        KeyCode::Numpad6 => 0x66,
        KeyCode::Numpad7 => 0x67,
        KeyCode::Numpad8 => 0x68,
        KeyCode::Numpad9 => 0x69,
        KeyCode::NumpadAdd => 0x6B,
        KeyCode::NumpadSubtract => 0x6D,
        KeyCode::NumpadMultiply => 0x6A,
        KeyCode::NumpadDivide => 0x6F,
        KeyCode::NumpadDecimal => 0x6E,
        KeyCode::NumpadEnter => 0x0D,

        KeyCode::Enter => 0x0D,
        KeyCode::Space => 0x20,
        KeyCode::Tab => 0x09,
        KeyCode::Backspace => 0x08,
        KeyCode::Escape => 0x1B,
        KeyCode::Delete => 0x2E,
        KeyCode::Insert => 0x2D,
        KeyCode::Home => 0x24,
        KeyCode::End => 0x23,
        KeyCode::PageUp => 0x21,
        KeyCode::PageDown => 0x22,
        KeyCode::ArrowLeft => 0x25,
        KeyCode::ArrowUp => 0x26,
        KeyCode::ArrowRight => 0x27,
        KeyCode::ArrowDown => 0x28,

        KeyCode::ShiftLeft | KeyCode::ShiftRight => 0x10,
        KeyCode::ControlLeft | KeyCode::ControlRight => 0x11,
        KeyCode::AltLeft | KeyCode::AltRight => 0x12,
        KeyCode::SuperLeft | KeyCode::SuperRight => 0x5B,
        KeyCode::CapsLock => 0x14,
        KeyCode::NumLock => 0x90,
        KeyCode::ScrollLock => 0x91,
        KeyCode::PrintScreen => 0x2C,
        KeyCode::Pause => 0x13,
        KeyCode::ContextMenu => 0x5D,

        KeyCode::Semicolon => 0xBA,
        KeyCode::Equal => 0xBB,
        KeyCode::Comma => 0xBC,
        KeyCode::Minus => 0xBD,
        KeyCode::Period => 0xBE,
        KeyCode::Slash => 0xBF,
        KeyCode::Backquote => 0xC0,
        KeyCode::BracketLeft => 0xDB,
        KeyCode::Backslash => 0xDC,
        KeyCode::BracketRight => 0xDD,
        KeyCode::Quote => 0xDE,

        KeyCode::F1 => 0x70,
        KeyCode::F2 => 0x71,
        KeyCode::F3 => 0x72,
        KeyCode::F4 => 0x73,
        KeyCode::F5 => 0x74,
        KeyCode::F6 => 0x75,
        KeyCode::F7 => 0x76,
        KeyCode::F8 => 0x77,
        KeyCode::F9 => 0x78,
        KeyCode::F10 => 0x79,
        KeyCode::F11 => 0x7A,
        KeyCode::F12 => 0x7B,
        KeyCode::F13 => 0x7C,
        KeyCode::F14 => 0x7D,
        KeyCode::F15 => 0x7E,
        KeyCode::F16 => 0x7F,
        KeyCode::F17 => 0x80,
        KeyCode::F18 => 0x81,
        KeyCode::F19 => 0x82,
        KeyCode::F20 => 0x83,
        KeyCode::F21 => 0x84,
        KeyCode::F22 => 0x85,
        KeyCode::F23 => 0x86,
        KeyCode::F24 => 0x87,

        KeyCode::AudioVolumeMute => 0xAD,
        KeyCode::AudioVolumeDown => 0xAE,
        KeyCode::AudioVolumeUp => 0xAF,
        KeyCode::MediaStop => 0xB2,
        KeyCode::MediaPlayPause => 0xB3,
        KeyCode::MediaTrackNext => 0xB0,
        KeyCode::MediaTrackPrevious => 0xB1,

        _ => return None,
    })
}

pub fn mouse_button_number(button: MouseButton) -> Option<c_int> {
    Some(match button {
        MouseButton::Left => 0,
        MouseButton::Right => 1,
        MouseButton::Middle => 2,
        _ => return None,
    })
}

fn post_move(pixel: UVec2) {
    unsafe { sys::krkr_window_mouse_move(pixel.x as c_int, pixel.y as c_int) };
}

fn post_button(pixel: UVec2, number: c_int, down: bool) {
    unsafe {
        if down {
            sys::krkr_window_mouse_down(pixel.x as c_int, pixel.y as c_int, number);
        } else {
            sys::krkr_window_mouse_up(pixel.x as c_int, pixel.y as c_int, number);
        }
    }
}

pub fn forward_keys(state: Res<KirikiriState>, keys: Option<Res<ButtonInput<KeyCode>>>) {
    let Some(keys) = keys else { return };
    if !matches!(*state, KirikiriState::Running { .. }) {
        return;
    }
    let shift = shift_state(&keys);
    for key in keys.get_just_pressed() {
        if let Some(vk) = virtual_key(*key) {
            // SAFETY: main thread, instance running.
            unsafe { sys::krkr_window_key_down(vk as c_int, shift) };
        }
    }
    for key in keys.get_just_released() {
        if let Some(vk) = virtual_key(*key) {
            // SAFETY: as above.
            unsafe { sys::krkr_window_key_up(vk as c_int, shift) };
        }
    }
}

pub fn forward_pointer(
    state: Res<KirikiriState>,
    window: Query<&Window, With<PrimaryWindow>>,
    cameras: Query<(&Camera, &GlobalTransform), With<KirikiriCamera>>,
    screens: Query<&GlobalTransform, With<KirikiriScreen>>,
    screen: Res<KirikiriScreenImage>,
    mouse: Option<Res<ButtonInput<MouseButton>>>,
    touches: Option<Res<Touches>>,
    // Position of the pointer the last move event reported.
    mut last: Local<Option<UVec2>>,
) {
    let (Some(mouse), Some(touches)) = (mouse, touches) else {
        return;
    };
    if !matches!(*state, KirikiriState::Running { .. }) {
        *last = None;
        return;
    }
    let (Ok(window), Ok((camera, camera_transform)), Ok(sprite)) =
        (window.single(), cameras.single(), screens.single())
    else {
        *last = None;
        return;
    };
    let placement = ScreenPlacement::new(sprite, screen.size);
    let to_pixel = |position: Vec2| -> Option<UVec2> {
        let world = camera
            .viewport_to_world_2d(camera_transform, position)
            .ok()?;
        placement.pixel_at(world)
    };

    let mut from_touch = false;
    for touch in touches.iter() {
        let Some(pixel) = to_pixel(touch.position()) else {
            continue;
        };
        from_touch = true;
        if touches.just_pressed(touch.id()) {
            post_move(pixel);
            post_button(pixel, 0, true);
        } else if *last != Some(pixel) {
            post_move(pixel);
        }
        *last = Some(pixel);
    }
    for touch in touches
        .iter_just_released()
        .chain(touches.iter_just_canceled())
    {
        if let Some(pixel) = to_pixel(touch.position()) {
            post_button(pixel, 0, false);
            *last = Some(pixel);
        }
    }

    let mut pointer = None;
    if !from_touch {
        match window.cursor_position().and_then(to_pixel) {
            Some(pixel) => {
                if *last != Some(pixel) {
                    post_move(pixel);
                }
                *last = Some(pixel);
                pointer = Some(pixel);
            }
            None => *last = None,
        }
    }
    let Some(pixel) = pointer.or(*last) else {
        return;
    };
    for button in [MouseButton::Left, MouseButton::Right, MouseButton::Middle] {
        let Some(number) = mouse_button_number(button) else {
            continue;
        };
        if mouse.just_pressed(button) {
            post_button(pixel, number, true);
        }
        if mouse.just_released(button) {
            post_button(pixel, number, false);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An unscaled screen sprite of `size` centred on the origin, which is what
    /// `sync_screen` spawns.
    fn centred(size: UVec2) -> ScreenPlacement {
        ScreenPlacement {
            center: Vec2::ZERO,
            scale: Vec2::ONE,
            size,
        }
    }

    #[test]
    fn corners_map_to_the_corners() {
        let screen = centred(UVec2::new(800, 600));
        // World y grows upwards, Kirikiri's y downwards.
        assert_eq!(screen.pixel_at(Vec2::new(-400.0, 300.0)), Some(UVec2::ZERO));
        assert_eq!(
            screen.pixel_at(Vec2::new(399.0, -299.0)),
            Some(UVec2::new(799, 599))
        );
        assert_eq!(screen.pixel_at(Vec2::ZERO), Some(UVec2::new(400, 300)));
    }

    #[test]
    fn outside_the_screen_is_none() {
        let screen = centred(UVec2::new(800, 600));
        assert_eq!(screen.pixel_at(Vec2::new(-401.0, 0.0)), None);
        assert_eq!(screen.pixel_at(Vec2::new(400.0, 0.0)), None);
        // The top edge itself is the first row, so only above it is outside.
        assert_eq!(
            screen.pixel_at(Vec2::new(0.0, 300.0)),
            Some(UVec2::new(400, 0))
        );
        assert_eq!(screen.pixel_at(Vec2::new(0.0, 301.0)), None);
        assert_eq!(screen.pixel_at(Vec2::new(0.0, -300.0)), None);
    }

    #[test]
    fn a_scaled_and_moved_screen_still_maps() {
        // Half size, shifted right and down: one Kirikiri pixel is half a world
        // unit, and the screen now spans 400x300 world units around (100, 0).
        let screen = ScreenPlacement {
            center: Vec2::new(100.0, 0.0),
            scale: Vec2::splat(0.5),
            size: UVec2::new(800, 600),
        };
        assert_eq!(
            screen.pixel_at(Vec2::new(100.0 - 200.0, 150.0)),
            Some(UVec2::ZERO)
        );
        assert_eq!(
            screen.pixel_at(Vec2::new(100.0, 0.0)),
            Some(UVec2::new(400, 300))
        );
    }

    #[test]
    fn empty_screen_has_no_pixels() {
        assert_eq!(centred(UVec2::ZERO).pixel_at(Vec2::ZERO), None);
        let flat = ScreenPlacement {
            center: Vec2::ZERO,
            scale: Vec2::ZERO,
            size: UVec2::new(80, 60),
        };
        assert_eq!(flat.pixel_at(Vec2::ZERO), None);
    }

    #[test]
    fn keys_use_win32_virtual_key_codes() {
        assert_eq!(virtual_key(KeyCode::Enter), Some(0x0D));
        assert_eq!(virtual_key(KeyCode::Space), Some(0x20));
        assert_eq!(virtual_key(KeyCode::Escape), Some(0x1B));
        assert_eq!(virtual_key(KeyCode::KeyA), Some(0x41));
        assert_eq!(virtual_key(KeyCode::Digit1), Some(0x31));
        assert_eq!(virtual_key(KeyCode::ArrowDown), Some(0x28));
        assert_eq!(virtual_key(KeyCode::F12), Some(0x7B));
        assert_eq!(virtual_key(KeyCode::ShiftLeft), Some(0x10));
        // A key no Kirikiri script has a name for.
        assert_eq!(virtual_key(KeyCode::BrowserBack), None);
    }

    #[test]
    fn shift_state_reports_the_ss_bits() {
        let mut keys = ButtonInput::<KeyCode>::default();
        assert_eq!(shift_state(&keys), 0);
        keys.press(KeyCode::ShiftRight);
        keys.press(KeyCode::ControlLeft);
        // ssShift | ssCtrl
        assert_eq!(shift_state(&keys), 0x1 | 0x4);
        keys.release(KeyCode::ControlLeft);
        keys.press(KeyCode::AltRight);
        // ssShift | ssAlt
        assert_eq!(shift_state(&keys), 0x1 | 0x2);
    }

    #[test]
    fn mouse_buttons_use_the_kirikiri_numbers() {
        assert_eq!(mouse_button_number(MouseButton::Left), Some(0));
        assert_eq!(mouse_button_number(MouseButton::Right), Some(1));
        assert_eq!(mouse_button_number(MouseButton::Middle), Some(2));
    }
}
