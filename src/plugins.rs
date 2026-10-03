use std::{fs, path::PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginDefinition {
    pub name: String,
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
}

pub fn load() -> Vec<PluginDefinition> {
    let Some(path) = config_path() else {
        return Vec::new();
    };
    fs::read(path)
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
}

pub fn config_path() -> Option<PathBuf> {
    dirs::config_dir().map(|directory| directory.join("NTIV").join("plugins.json"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plugin_definition_parses_with_optional_arguments() {
        let json = br#"[{"name":"Echo","command":"cmd.exe"}]"#;
        let plugins: Vec<PluginDefinition> = serde_json::from_slice(json).unwrap();
        assert_eq!(plugins[0].name, "Echo");
        assert!(plugins[0].args.is_empty());
    }
}
