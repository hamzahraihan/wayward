use iced::keyboard::{self, Key};
use iced::mouse;

use wayward_core::MouseButton;

/// Map iced's mouse button to the core-local enum at the app boundary.
/// Click routing lands in M5/M8; this pure mapper exists now to keep iced types out of core.
#[allow(dead_code, reason = "click routing arrives in M5/M8")]
pub fn map_button(button: mouse::Button) -> MouseButton {
    match button {
        mouse::Button::Left => MouseButton::Left,
        mouse::Button::Right => MouseButton::Right,
        mouse::Button::Middle => MouseButton::Middle,
        mouse::Button::Back => MouseButton::Back,
        mouse::Button::Forward => MouseButton::Forward,
        mouse::Button::Other(code) => MouseButton::Other(code),
    }
}

/// A key action inside the AI usage popup.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PopupKey {
    Refresh,
    ToggleLeftUsed,
    Close,
}

/// Plain fn for `iced::event::listen_with` — must not capture.
pub fn popup_key(
    event: iced::Event,
    _status: iced::event::Status,
    _id: iced::window::Id,
) -> Option<PopupKey> {
    let iced::Event::Keyboard(keyboard::Event::KeyPressed { key, .. }) = event else {
        return None;
    };

    match key.as_ref() {
        Key::Character(c) if c.eq_ignore_ascii_case("r") => Some(PopupKey::Refresh),
        Key::Character(c) if c.eq_ignore_ascii_case("l") || c.eq_ignore_ascii_case("u") => {
            Some(PopupKey::ToggleLeftUsed)
        }
        Key::Named(keyboard::key::Named::Escape) => Some(PopupKey::Close),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use iced::keyboard::{key, Key, Location, Modifiers};
    use iced::window;

    fn key_pressed(key: Key) -> iced::Event {
        iced::Event::Keyboard(keyboard::Event::KeyPressed {
            key,
            modified_key: Key::Unidentified,
            physical_key: key::Physical::Unidentified(key::NativeCode::Unidentified),
            location: Location::Standard,
            modifiers: Modifiers::NONE,
            text: None,
            repeat: false,
        })
    }

    fn map(key: Key) -> Option<PopupKey> {
        popup_key(
            key_pressed(key),
            iced::event::Status::Ignored,
            window::Id::unique(),
        )
    }

    #[test]
    fn maps_refresh_keys() {
        assert_eq!(map(Key::Character("r".into())), Some(PopupKey::Refresh));
        assert_eq!(map(Key::Character("R".into())), Some(PopupKey::Refresh));
    }

    #[test]
    fn maps_toggle_keys() {
        assert_eq!(
            map(Key::Character("l".into())),
            Some(PopupKey::ToggleLeftUsed)
        );
        assert_eq!(
            map(Key::Character("L".into())),
            Some(PopupKey::ToggleLeftUsed)
        );
        assert_eq!(
            map(Key::Character("u".into())),
            Some(PopupKey::ToggleLeftUsed)
        );
        assert_eq!(
            map(Key::Character("U".into())),
            Some(PopupKey::ToggleLeftUsed)
        );
    }

    #[test]
    fn maps_escape_to_close() {
        assert_eq!(map(Key::Named(key::Named::Escape)), Some(PopupKey::Close));
    }

    #[test]
    fn ignores_other_keys() {
        assert_eq!(map(Key::Character("x".into())), None);
        assert_eq!(map(Key::Named(key::Named::Enter)), None);
        assert_eq!(map(Key::Unidentified), None);
    }

    #[test]
    fn ignores_non_keyboard_events() {
        let event = iced::Event::Window(iced::window::Event::Focused);
        assert_eq!(
            popup_key(event, iced::event::Status::Ignored, window::Id::unique()),
            None
        );
    }

    #[test]
    fn maps_all_variants() {
        assert_eq!(map_button(mouse::Button::Left), MouseButton::Left);
        assert_eq!(map_button(mouse::Button::Right), MouseButton::Right);
        assert_eq!(map_button(mouse::Button::Middle), MouseButton::Middle);
        assert_eq!(map_button(mouse::Button::Back), MouseButton::Back);
        assert_eq!(map_button(mouse::Button::Forward), MouseButton::Forward);
        assert_eq!(map_button(mouse::Button::Other(7)), MouseButton::Other(7));
    }
}
