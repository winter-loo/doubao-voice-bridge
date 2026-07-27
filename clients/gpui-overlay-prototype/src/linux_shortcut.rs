pub const VOICE_SHORTCUT_ENV: &str = "DOUBAO_VOICE_SHORTCUT";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LinuxShortcut {
    trigger: String,
    key_name: String,
    modifiers: ShortcutModifiers,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ShortcutModifiers {
    pub control: bool,
    pub alt: bool,
    pub shift: bool,
    pub num: bool,
    pub logo: bool,
}

impl LinuxShortcut {
    pub fn parse(value: &str) -> Result<Self, String> {
        let value = value.trim();
        let mut parts = value.split('+').collect::<Vec<_>>();
        let key_name = parts
            .pop()
            .filter(|key| !key.is_empty())
            .ok_or_else(|| format!("invalid {VOICE_SHORTCUT_ENV}: a key is required"))?;
        let key_name = if key_name.len() == 1 {
            key_name.to_ascii_lowercase()
        } else {
            key_name.to_string()
        };
        if !key_name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
        {
            return Err(format!(
                "invalid {VOICE_SHORTCUT_ENV}: key names may only contain letters, digits, and _"
            ));
        }

        let mut modifiers = ShortcutModifiers::default();
        for modifier in parts {
            let slot = match modifier.to_ascii_uppercase().as_str() {
                "CTRL" => &mut modifiers.control,
                "ALT" => &mut modifiers.alt,
                "SHIFT" => &mut modifiers.shift,
                "NUM" => &mut modifiers.num,
                "LOGO" => &mut modifiers.logo,
                _ => {
                    return Err(format!(
                        "invalid {VOICE_SHORTCUT_ENV}: unknown modifier {modifier:?}"
                    ));
                }
            };
            if std::mem::replace(slot, true) {
                return Err(format!(
                    "invalid {VOICE_SHORTCUT_ENV}: duplicate modifier {modifier:?}"
                ));
            }
        }

        let mut trigger_parts = Vec::new();
        for (enabled, name) in [
            (modifiers.control, "CTRL"),
            (modifiers.alt, "ALT"),
            (modifiers.shift, "SHIFT"),
            (modifiers.num, "NUM"),
            (modifiers.logo, "LOGO"),
        ] {
            if enabled {
                trigger_parts.push(name);
            }
        }
        trigger_parts.push(&key_name);

        Ok(Self {
            trigger: trigger_parts.join("+"),
            key_name,
            modifiers,
        })
    }

    pub fn trigger(&self) -> &str {
        &self.trigger
    }

    pub fn key_name(&self) -> &str {
        &self.key_name
    }

    pub fn modifiers(&self) -> ShortcutModifiers {
        self.modifiers
    }
}

#[cfg(target_os = "linux")]
impl LinuxShortcut {
    pub fn from_environment() -> Result<Self, String> {
        let value = match std::env::var(VOICE_SHORTCUT_ENV) {
            Ok(value) => value,
            Err(std::env::VarError::NotPresent) => {
                crate::client_settings::ClientSettings::load()?.voice_shortcut
            }
            Err(std::env::VarError::NotUnicode(_)) => {
                return Err(format!("{VOICE_SHORTCUT_ENV} must be valid UTF-8"));
            }
        };
        let shortcut = Self::parse(&value)?;
        shortcut.keysym()?;
        Ok(shortcut)
    }

    pub fn keysym(&self) -> Result<u32, String> {
        use xkbcommon::xkb;

        let mut keysym = xkb::keysym_from_name(&self.key_name, xkb::KEYSYM_NO_FLAGS);
        if keysym.raw() == xkb::keysyms::KEY_NoSymbol {
            keysym = xkb::keysym_from_name(&self.key_name, xkb::KEYSYM_CASE_INSENSITIVE);
        }
        (keysym.raw() != xkb::keysyms::KEY_NoSymbol)
            .then_some(keysym.raw())
            .ok_or_else(|| format!("unknown XKB key name {:?}", self.key_name))
    }
}

#[cfg(test)]
mod tests {
    use super::{LinuxShortcut, ShortcutModifiers};

    #[test]
    fn parses_xdg_modifier_shortcut() {
        let shortcut = LinuxShortcut::parse("ALT+CTRL+V").unwrap();

        assert_eq!(shortcut.trigger(), "CTRL+ALT+v");
        assert_eq!(shortcut.key_name(), "v");
        assert_eq!(
            shortcut.modifiers(),
            ShortcutModifiers {
                control: true,
                alt: true,
                ..ShortcutModifiers::default()
            }
        );
    }

    #[test]
    fn accepts_key_only_shortcut() {
        let shortcut = LinuxShortcut::parse("F8").unwrap();

        assert_eq!(shortcut.trigger(), "F8");
        assert_eq!(shortcut.key_name(), "F8");
        assert_eq!(shortcut.modifiers(), ShortcutModifiers::default());
    }

    #[test]
    fn rejects_unknown_or_duplicate_modifiers() {
        assert!(LinuxShortcut::parse("META+v").is_err());
        assert!(LinuxShortcut::parse("CTRL+CTRL+v").is_err());
    }

    #[test]
    fn rejects_missing_or_non_xdg_key_names() {
        assert!(LinuxShortcut::parse("").is_err());
        assert!(LinuxShortcut::parse("CTRL+").is_err());
        assert!(LinuxShortcut::parse("CTRL+Page Down").is_err());
    }
}
