use std::{
    fs::{self, File},
    io,
    path::{Path, PathBuf},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClipboardKind {
    Copy,
    Cut,
}

#[derive(Debug, Clone)]
pub struct FileClipboard {
    pub kind: ClipboardKind,
    pub paths: Vec<PathBuf>,
}

#[derive(Debug, Clone)]
pub enum UndoAction {
    RemoveCreated(Vec<PathBuf>),
    MoveBack(Vec<(PathBuf, PathBuf)>),
    Rename { from: PathBuf, to: PathBuf },
    RemoveNew(PathBuf),
}

#[cfg(test)]
pub fn paste(clipboard: &FileClipboard, destination: &Path) -> io::Result<UndoAction> {
    let mut created = Vec::new();
    let mut moved = Vec::new();

    for source in &clipboard.paths {
        if !source.exists() {
            continue;
        }
        let Some(name) = source.file_name() else {
            continue;
        };
        let target = unique_destination(&destination.join(name));
        match clipboard.kind {
            ClipboardKind::Copy => {
                copy_recursively(source, &target)?;
                created.push(target);
            }
            ClipboardKind::Cut => {
                move_path(source, &target)?;
                moved.push((target, source.clone()));
            }
        }
    }

    Ok(match clipboard.kind {
        ClipboardKind::Copy => UndoAction::RemoveCreated(created),
        ClipboardKind::Cut => UndoAction::MoveBack(moved),
    })
}

pub fn create(
    directory: &Path,
    name: &str,
    is_directory: bool,
) -> io::Result<(PathBuf, UndoAction)> {
    let path = directory.join(name);
    if path.exists() {
        return Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "an item with that name already exists",
        ));
    }
    if is_directory {
        fs::create_dir(&path)?;
    } else {
        File::create(&path)?;
    }
    Ok((path.clone(), UndoAction::RemoveNew(path)))
}

pub fn undo(action: UndoAction) -> io::Result<()> {
    match action {
        UndoAction::RemoveCreated(paths) => {
            for path in paths.into_iter().rev() {
                remove_created(&path)?;
            }
        }
        UndoAction::MoveBack(moves) => {
            for (current, original) in moves.into_iter().rev() {
                if current.exists() && !original.exists() {
                    move_path(&current, &original)?;
                }
            }
        }
        UndoAction::Rename { from, to } => {
            if from.exists() && !to.exists() {
                fs::rename(from, to)?;
            }
        }
        UndoAction::RemoveNew(path) => remove_created(&path)?,
    }
    Ok(())
}

fn move_path(source: &Path, target: &Path) -> io::Result<()> {
    match fs::rename(source, target) {
        Ok(()) => Ok(()),
        Err(_) => {
            copy_recursively(source, target)?;
            remove_created(source)
        }
    }
}

fn copy_recursively(source: &Path, target: &Path) -> io::Result<()> {
    let metadata = fs::symlink_metadata(source)?;
    if metadata.file_type().is_symlink() {
        return Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "symbolic links and junctions are not copied",
        ));
    }
    if metadata.is_dir() {
        fs::create_dir_all(target)?;
        for entry in fs::read_dir(source)? {
            let entry = entry?;
            copy_recursively(&entry.path(), &target.join(entry.file_name()))?;
        }
    } else {
        fs::copy(source, target)?;
    }
    Ok(())
}

fn remove_created(path: &Path) -> io::Result<()> {
    if path.is_dir() {
        fs::remove_dir_all(path)
    } else if path.exists() {
        fs::remove_file(path)
    } else {
        Ok(())
    }
}

#[cfg(test)]
fn unique_destination(requested: &Path) -> PathBuf {
    if !requested.exists() {
        return requested.to_path_buf();
    }
    let parent = requested.parent().unwrap_or_else(|| Path::new("."));
    let stem = requested
        .file_stem()
        .map(|value| value.to_string_lossy().into_owned())
        .unwrap_or_else(|| "item".into());
    let extension = requested.extension().map(|value| value.to_string_lossy());

    for number in 2..10_000 {
        let mut name = format!("{stem} (copy {number})");
        if let Some(extension) = &extension {
            name.push('.');
            name.push_str(extension);
        }
        let candidate = parent.join(name);
        if !candidate.exists() {
            return candidate;
        }
    }
    requested.to_path_buf()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn copy_uses_a_safe_name_on_conflict_and_can_be_undone() {
        let temp = tempdir().unwrap();
        let source_dir = temp.path().join("source");
        let destination = temp.path().join("destination");
        fs::create_dir_all(&source_dir).unwrap();
        fs::create_dir_all(&destination).unwrap();
        let source = source_dir.join("note.txt");
        fs::write(&source, "hello").unwrap();
        fs::write(destination.join("note.txt"), "existing").unwrap();

        let action = paste(
            &FileClipboard {
                kind: ClipboardKind::Copy,
                paths: vec![source],
            },
            &destination,
        )
        .unwrap();
        let copied = destination.join("note (copy 2).txt");
        assert!(copied.exists());
        undo(action).unwrap();
        assert!(!copied.exists());
    }

    #[test]
    fn cut_and_undo_restore_the_original_path() {
        let temp = tempdir().unwrap();
        let source_dir = temp.path().join("source");
        let destination = temp.path().join("destination");
        fs::create_dir_all(&source_dir).unwrap();
        fs::create_dir_all(&destination).unwrap();
        let source = source_dir.join("move.txt");
        fs::write(&source, "hello").unwrap();

        let action = paste(
            &FileClipboard {
                kind: ClipboardKind::Cut,
                paths: vec![source.clone()],
            },
            &destination,
        )
        .unwrap();
        assert!(!source.exists());
        undo(action).unwrap();
        assert!(source.exists());
    }
}
