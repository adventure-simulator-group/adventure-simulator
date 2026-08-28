use fabelgeist_math::Vec2;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ButtonState {
    Normal,
    Hovered,
    Pressed,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct Mouse {
    pub position: Vec2,
    pub is_pressed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Button {
    pub name: String,
    pub position: Vec2,
    pub size: Vec2,
}

impl Button {
    pub fn new(name: String, position: Vec2, size: Vec2) -> Self {
        Self {
            name,
            position,
            size,
        }
    }

    pub fn input(&self, mouse: &Mouse) -> ButtonState {
        let half_width = self.size.x / 2.0;
        let half_height = self.size.y / 2.0;

        let min_x = self.position.x - half_width;
        let max_x = self.position.x + half_width;
        let min_y = self.position.y - half_height;
        let max_y = self.position.y + half_height;

        let inside = mouse.position.x >= min_x
            && mouse.position.x <= max_x
            && mouse.position.y >= min_y
            && mouse.position.y <= max_y;

        if inside {
            if mouse.is_pressed {
                ButtonState::Pressed
            } else {
                ButtonState::Hovered
            }
        } else {
            ButtonState::Normal
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_button_input() {
        let button = Button::new(
            "Test".to_string(),
            Vec2::new(100.0, 50.0),
            Vec2::new(80.0, 40.0),
        );

        // Hover inside
        let mouse_hover = Mouse {
            position: Vec2::new(110.0, 45.0),
            is_pressed: false,
        };
        assert_eq!(button.input(&mouse_hover), ButtonState::Hovered);

        // Press inside
        let mouse_press = Mouse {
            position: Vec2::new(90.0, 55.0),
            is_pressed: true,
        };
        assert_eq!(button.input(&mouse_press), ButtonState::Pressed);

        // Outside
        let mouse_outside = Mouse {
            position: Vec2::new(200.0, 50.0),
            is_pressed: false,
        };
        assert_eq!(button.input(&mouse_outside), ButtonState::Normal);
    }
}
