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

#[cfg(test)]
mod tests {
    use super::*;

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
