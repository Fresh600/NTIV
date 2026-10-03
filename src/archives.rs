use std::{
    fs::{self, File},
    io::{self, Write},
    path::{Path, PathBuf},
    time::UNIX_EPOCH,
};

use sha2::{Digest, Sha256};
use zip::ZipArchive;

const MAX_ARCHIVE_ENTRIES: usize = 100_000;
const MAX_TOTAL_BYTES: u64 = 2 * 1024 * 1024 * 1024;
const MAX_FILE_BYTES: u64 = 512 * 1024 * 1024;

pub fn extract_for_browsing(path: &Path) -> io::Result<PathBuf> {
    let file = File::open(path)?;
    let mut archive = ZipArchive::new(file).map_err(zip_error)?;
    if archive.len() > MAX_ARCHIVE_ENTRIES {
        return Err(io::Error::other("ZIP contains too many entries"));
    }

    let root = extraction_root(path)?;
    fs::create_dir_all(&root)?;
    let mut total = 0_u64;

    for index in 0..archive.len() {
        let mut entry = archive.by_index(index).map_err(zip_error)?;
        let Some(relative) = entry.enclosed_name() else {
            continue;
        };
        if entry
            .unix_mode()
            .is_some_and(|mode| mode & 0o170000 == 0o120000)
        {
            continue;
        }
        if entry.size() > MAX_FILE_BYTES {
            return Err(io::Error::other("ZIP contains a file larger than 512 MB"));
        }
        total = total.saturating_add(entry.size());
        if total > MAX_TOTAL_BYTES {
            return Err(io::Error::other("ZIP expands beyond the 2 GB safety limit"));
        }

        let output = root.join(relative);
        if entry.is_dir() {
            fs::create_dir_all(&output)?;
            continue;
        }
        if let Some(parent) = output.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut target = File::create(output)?;
        io::copy(&mut entry, &mut target)?;
        target.flush()?;
    }

    Ok(root)
}

fn extraction_root(path: &Path) -> io::Result<PathBuf> {
    let metadata = fs::metadata(path)?;
    let modified = metadata
        .modified()
        .ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map_or(0, |duration| duration.as_secs());
    let mut hasher = Sha256::new();
    hasher.update(path.as_os_str().to_string_lossy().as_bytes());
    hasher.update(metadata.len().to_le_bytes());
    hasher.update(modified.to_le_bytes());
    let key = format!("{:x}", hasher.finalize());
    Ok(std::env::temp_dir().join("NTIV").join("archives").join(key))
}

fn zip_error(error: zip::result::ZipError) -> io::Error {
    io::Error::other(error)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;
    use zip::write::SimpleFileOptions;

    #[test]
    fn extracts_nested_zip_for_browsing() {
        let temp = tempdir().unwrap();
        let archive_path = temp.path().join("sample.zip");
        let file = File::create(&archive_path).unwrap();
        let mut writer = zip::ZipWriter::new(file);
        writer
            .start_file("folder/hello.txt", SimpleFileOptions::default())
            .unwrap();
        writer.write_all(b"hello from zip").unwrap();
        writer.finish().unwrap();

        let extracted = extract_for_browsing(&archive_path).unwrap();
        assert_eq!(
            fs::read_to_string(extracted.join("folder/hello.txt")).unwrap(),
            "hello from zip"
        );
    }
}
