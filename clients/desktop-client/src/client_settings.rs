#[cfg(any(target_os = "windows", target_os = "linux"))]
use std::{fs, path::PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default)]
pub struct ClientSettings {
    pub server: String,
    pub audio_port: u16,
    pub token: Option<String>,
    pub input_device_id: Option<String>,
    pub voice_shortcut: String,
    pub start_with_windows: bool,
    pub setup_completed: bool,
}

impl Default for ClientSettings {
    fn default() -> Self {
        Self {
            server: "100.116.241.81:4387".to_string(),
            audio_port: 5004,
            token: None,
            input_device_id: None,
            voice_shortcut: "F13".to_string(),
            start_with_windows: true,
            setup_completed: false,
        }
    }
}

#[cfg(any(target_os = "windows", target_os = "linux"))]
impl ClientSettings {
    pub fn load() -> Result<Self, String> {
        let path = settings_path()?;
        if !path.is_file() {
            return Ok(Self::default());
        }
        let bytes = fs::read(&path)
            .map_err(|error| format!("could not read {}: {error}", path.display()))?;
        serde_json::from_slice(&bytes)
            .map_err(|error| format!("invalid settings in {}: {error}", path.display()))
    }

    #[cfg(any(target_os = "windows", target_os = "linux"))]
    pub fn save(&self) -> Result<(), String> {
        let path = settings_path()?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .map_err(|error| format!("could not create {}: {error}", parent.display()))?;
        }
        let bytes = serde_json::to_vec_pretty(self)
            .map_err(|error| format!("could not encode settings: {error}"))?;
        fs::write(&path, bytes)
            .map_err(|error| format!("could not write {}: {error}", path.display()))
    }
}

#[cfg(target_os = "windows")]
pub fn settings_path() -> Result<PathBuf, String> {
    let local_app_data = std::env::var_os("LOCALAPPDATA")
        .ok_or_else(|| "LOCALAPPDATA is not available".to_string())?;
    Ok(PathBuf::from(local_app_data)
        .join("DoubaoVoiceBridge")
        .join("client.json"))
}

#[cfg(target_os = "linux")]
pub fn settings_path() -> Result<PathBuf, String> {
    let config_home = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))
        .ok_or_else(|| "neither XDG_CONFIG_HOME nor HOME is available".to_string())?;
    Ok(config_home.join("DoubaoVoiceBridge").join("client.json"))
}

#[cfg(test)]
mod tests {
    use super::ClientSettings;

    #[test]
    fn missing_fields_keep_consumer_defaults() {
        let settings: ClientSettings = serde_json::from_str(r#"{"server":"mac:4387"}"#).unwrap();

        assert_eq!(settings.server, "mac:4387");
        assert_eq!(settings.audio_port, 5004);
        assert_eq!(settings.voice_shortcut, "F13");
        assert!(settings.start_with_windows);
        assert!(!settings.setup_completed);
    }
}
