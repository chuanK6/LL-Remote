use crate::error::{AppError, ErrorCode, Result};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ShortcutAction {
    Mouse,
    Disconnect,
    Display,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct Shortcut {
    key: String,
    ctrl: bool,
    alt: bool,
    shift: bool,
}

impl Shortcut {
    fn default_key(key: &str) -> Self {
        Self {
            key: key.into(),
            ctrl: true,
            alt: true,
            shift: true,
        }
    }

    fn valid(&self) -> bool {
        let key = self.key.as_bytes();
        let letter_or_digit = matches!(key, [b'A'..=b'Z' | b'0'..=b'9']);
        let function = matches!(
            self.key.as_str(),
            "F1" | "F2" | "F3" | "F4" | "F5" | "F6" | "F7" | "F8" | "F9" | "F10" | "F11" | "F12"
        );
        (self.ctrl || self.alt || self.shift) && (letter_or_digit || function)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(default)]
pub struct Shortcuts {
    mouse: Shortcut,
    disconnect: Shortcut,
    display: Shortcut,
}

impl Default for Shortcuts {
    fn default() -> Self {
        Self {
            mouse: Shortcut::default_key("Z"),
            disconnect: Shortcut::default_key("D"),
            display: Shortcut::default_key("L"),
        }
    }
}

impl Shortcuts {
    pub fn validate(&self) -> Result<()> {
        if ![&self.mouse, &self.disconnect, &self.display]
            .into_iter()
            .all(Shortcut::valid)
        {
            return Err(AppError::new(
                ErrorCode::InvalidInput,
                "请使用 Ctrl、Alt 或 Shift 搭配字母、数字或 F1–F12。",
            ));
        }
        if self.mouse == self.disconnect
            || self.mouse == self.display
            || self.disconnect == self.display
        {
            return Err(AppError::new(
                ErrorCode::InvalidInput,
                "三项动作需要使用不同的快捷键。",
            ));
        }
        Ok(())
    }
}
