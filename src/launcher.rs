use std::{env, fs, path::PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LauncherApp {
    pub name: String,
    pub path: PathBuf,
}

pub fn discover() -> Vec<LauncherApp> {
    let mut roots = Vec::new();
    if let Ok(app_data) = env::var("APPDATA") {
        roots.push(PathBuf::from(app_data).join(r"Microsoft\Windows\Start Menu\Programs"));
    }
    if let Ok(program_data) = env::var("PROGRAMDATA") {
        roots.push(PathBuf::from(program_data).join(r"Microsoft\Windows\Start Menu\Programs"));
    }

    let mut apps = Vec::new();
    for root in roots {
        collect_apps(&root, 0, &mut apps);
    }
    apps.sort_by_key(|app| app.name.to_lowercase());
    apps.dedup_by(|left, right| left.name.eq_ignore_ascii_case(&right.name));
    apps
}

fn collect_apps(directory: &std::path::Path, depth: usize, apps: &mut Vec<LauncherApp>) {
    if depth > 8 {
        return;
    }
    let Ok(entries) = fs::read_dir(directory) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_apps(&path, depth + 1, apps);
            continue;
        }
        let extension = path
            .extension()
            .map(|value| value.to_string_lossy().to_ascii_lowercase())
            .unwrap_or_default();
        if !matches!(extension.as_str(), "lnk" | "url" | "exe") {
            continue;
        }
        let Some(stem) = path.file_stem() else {
            continue;
        };
        apps.push(LauncherApp {
            name: stem.to_string_lossy().into_owned(),
            path,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn discovers_shortcuts_recursively() {
        let temp = tempdir().unwrap();
        let nested = temp.path().join("Tools");
        fs::create_dir(&nested).unwrap();
        fs::write(nested.join("My App.lnk"), "shortcut").unwrap();
        fs::write(nested.join("ignore.txt"), "text").unwrap();
        let mut apps = Vec::new();
        collect_apps(temp.path(), 0, &mut apps);
        assert_eq!(apps.len(), 1);
        assert_eq!(apps[0].name, "My App");
    }
}
