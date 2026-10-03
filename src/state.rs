use std::{
    fs, io,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

const MAX_RECENT: usize = 20;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SavedState {
    #[serde(default)]
    pub favorites: Vec<PathBuf>,
    #[serde(default)]
    pub recent: Vec<PathBuf>,
    #[serde(default)]
    pub theme_index: usize,
}

impl SavedState {
    pub fn record_recent(&mut self, path: PathBuf) {
        self.recent.retain(|existing| existing != &path);
        self.recent.insert(0, path);
        self.recent.truncate(MAX_RECENT);
    }

    pub fn toggle_favorite(&mut self, path: PathBuf) -> bool {
        if let Some(index) = self.favorites.iter().position(|item| item == &path) {
            self.favorites.remove(index);
            false
        } else {
            self.favorites.push(path);
            self.favorites.sort();
            true
        }
    }
}

pub fn load() -> SavedState {
    state_path()
        .and_then(|path| load_from(&path).ok())
        .unwrap_or_default()
}

pub fn save(state: &SavedState) -> io::Result<()> {
    let path = state_path().ok_or_else(|| io::Error::other("config directory unavailable"))?;
    save_to(&path, state)
}

fn state_path() -> Option<PathBuf> {
    dirs::config_dir().map(|directory| directory.join("NTIV").join("state.json"))
}

fn load_from(path: &Path) -> io::Result<SavedState> {
    let bytes = fs::read(path)?;
    serde_json::from_slice(&bytes).map_err(io::Error::other)
}

fn save_to(path: &Path, state: &SavedState) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let temporary = path.with_extension("json.tmp");
    let json = serde_json::to_vec_pretty(state).map_err(io::Error::other)?;
    fs::write(&temporary, json)?;
    if path.exists() {
        fs::remove_file(path)?;
    }
    fs::rename(temporary, path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn favorites_and_recent_round_trip() {
        let temp = tempdir().unwrap();
        let path = temp.path().join("state.json");
        let favorite = PathBuf::from(r"C:\Projects\NTIV");
        let mut state = SavedState::default();
        assert!(state.toggle_favorite(favorite.clone()));
        state.record_recent(favorite.clone());
        save_to(&path, &state).unwrap();

        let loaded = load_from(&path).unwrap();
        assert_eq!(loaded.favorites, vec![favorite.clone()]);
        assert_eq!(loaded.recent, vec![favorite]);
    }

    #[test]
    fn recent_paths_are_unique_and_bounded() {
        let mut state = SavedState::default();
        for index in 0..25 {
            state.record_recent(PathBuf::from(format!(r"C:\Folder{index}")));
        }
        state.record_recent(PathBuf::from(r"C:\Folder20"));
        assert_eq!(state.recent.len(), MAX_RECENT);
        assert_eq!(state.recent[0], PathBuf::from(r"C:\Folder20"));
    }
}
