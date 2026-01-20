//! Configuration loading for BoxLite CLI.
//!
//! Handles loading configuration from JSON and YAML files.

use boxlite::runtime::options::BoxliteOptions;
use std::path::Path;
use tracing::warn;

const CONFIG_FILE_JSON: &str = "config.json";

/// Load BoxliteOptions from configuration files in the given home directory.
///
/// Tries to load from config.json.
///
/// Returns options with defaults if no config file is found.
pub fn load_config(home_dir: &Path) -> BoxliteOptions {
    let mut options = BoxliteOptions {
        home_dir: home_dir.to_path_buf(),
        ..BoxliteOptions::default()
    };

    if let Some(config) = try_load_json(home_dir) {
        // Merge loaded config into defaults
        // Currently we only care about image_registries from the config file
        if !config.image_registries.is_empty() {
            options.image_registries = config.image_registries;
        }
    }

    options
}

fn try_load_json(home_dir: &Path) -> Option<BoxliteOptions> {
    let config_path = home_dir.join(CONFIG_FILE_JSON);
    if !config_path.exists() {
        return None;
    }

    let content = match std::fs::read_to_string(&config_path) {
        Ok(c) => c,
        Err(e) => {
            warn!(
                "Failed to read config file {}: {}",
                config_path.display(),
                e
            );
            return None;
        }
    };

    match serde_json::from_str::<BoxliteOptions>(&content) {
        Ok(config) => Some(config),
        Err(e) => {
            warn!(
                "Failed to parse config file {}: {}",
                config_path.display(),
                e
            );
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_load_json_config() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("config.json");
        let config_content = r#"{"image_registries": ["ghcr.io", "docker.io"]}"#;
        fs::write(&config_path, config_content).unwrap();

        let options = load_config(temp_dir.path());
        assert_eq!(options.image_registries, vec!["ghcr.io", "docker.io"]);
    }

    #[test]
    fn test_load_config_with_home_dir() {
        let temp_dir = TempDir::new().unwrap();
        let options = load_config(temp_dir.path());
        assert_eq!(options.home_dir, temp_dir.path());
    }

    #[test]
    fn test_invalid_json_warns_and_returns_defaults() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("config.json");
        let config_content = r#"{"image_registries": ["invalid"#; // Truncated JSON
        fs::write(&config_path, config_content).unwrap();

        let options = load_config(temp_dir.path());
        assert!(options.image_registries.is_empty());
    }
}
