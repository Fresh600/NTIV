use std::{
    cmp::Ordering,
    fs, io,
    path::{Path, PathBuf},
};

#[derive(Debug, Clone)]
pub struct FileEntry {
    pub path: PathBuf,
    pub name: String,
    pub is_dir: bool,
    pub len: u64,
}

pub fn read_entries(directory: &Path) -> io::Result<Vec<FileEntry>> {
    let mut entries = Vec::new();
    for item in fs::read_dir(directory)? {
        let item = match item {
            Ok(item) => item,
            Err(_) => continue,
        };
        let file_type = match item.file_type() {
            Ok(file_type) => file_type,
            Err(_) => continue,
        };
        let metadata = item.metadata().ok();
        entries.push(FileEntry {
            path: item.path(),
            name: item.file_name().to_string_lossy().into_owned(),
            is_dir: file_type.is_dir(),
            len: metadata.as_ref().map_or(0, fs::Metadata::len),
        });
    }
    entries.sort_by(compare_entries);
    Ok(entries)
}

pub fn read_child_directories(directory: &Path) -> io::Result<Vec<PathBuf>> {
    let mut directories = Vec::new();
    for item in fs::read_dir(directory)? {
        let item = match item {
            Ok(item) => item,
            Err(_) => continue,
        };
        if item.file_type().is_ok_and(|kind| kind.is_dir()) {
            directories.push(item.path());
        }
    }
    directories.sort_by(|left, right| {
        display_name(left)
            .to_lowercase()
            .cmp(&display_name(right).to_lowercase())
    });
    Ok(directories)
}

fn compare_entries(left: &FileEntry, right: &FileEntry) -> Ordering {
    right
        .is_dir
        .cmp(&left.is_dir)
        .then_with(|| left.name.to_lowercase().cmp(&right.name.to_lowercase()))
}

pub fn display_name(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| path.display().to_string())
}

pub fn human_size(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} {}", UNITS[unit])
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

#[cfg(test)]
mod tests {
    use std::fs::{self, File};

    use tempfile::tempdir;

    use super::*;

    #[test]
    fn directories_sort_before_files_and_unicode_is_preserved() {
        let temp = tempdir().unwrap();
        File::create(temp.path().join("z.txt")).unwrap();
        fs::create_dir(temp.path().join("תיקייה")).unwrap();

        let entries = read_entries(temp.path()).unwrap();
        assert!(entries[0].is_dir);
        assert_eq!(entries[0].name, "תיקייה");
        assert_eq!(entries[1].name, "z.txt");
    }

    #[test]
    fn formats_sizes() {
        assert_eq!(human_size(42), "42 B");
        assert_eq!(human_size(1536), "1.5 KB");
    }
}
