use std::{
    fs, io,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, Sender},
    },
    thread,
};

use crate::operations::{ClipboardKind, FileClipboard, UndoAction};

#[derive(Debug, Clone)]
pub struct JobProgress {
    pub label: &'static str,
    pub completed: usize,
    pub total: usize,
    pub current: String,
}

#[derive(Debug)]
pub enum JobResult {
    Paste(UndoAction),
    Undo,
    Delete(usize),
    Cancelled,
}

#[derive(Debug)]
pub enum JobMessage {
    Progress(JobProgress),
    Finished(Result<JobResult, String>),
}

pub struct FileJobTask {
    pub receiver: Receiver<JobMessage>,
    cancel: Arc<AtomicBool>,
}

impl FileJobTask {
    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}

impl Drop for FileJobTask {
    fn drop(&mut self) {
        self.cancel();
    }
}

pub fn spawn_paste(clipboard: FileClipboard, destination: PathBuf) -> FileJobTask {
    spawn_job("ntiv-file-paste", move |sender, cancel| {
        paste_worker(&clipboard, &destination, &sender, &cancel)
    })
}

pub fn spawn_undo(action: UndoAction) -> FileJobTask {
    spawn_job("ntiv-file-undo", move |sender, cancel| {
        if cancel.load(Ordering::Relaxed) {
            return Ok(JobResult::Cancelled);
        }
        let _ = sender.send(JobMessage::Progress(JobProgress {
            label: "UNDO",
            completed: 0,
            total: 1,
            current: "Restoring last action".into(),
        }));
        crate::operations::undo(action).map_err(|error| error.to_string())?;
        Ok(JobResult::Undo)
    })
}

pub fn spawn_delete(paths: Vec<PathBuf>) -> FileJobTask {
    spawn_job("ntiv-file-delete", move |sender, cancel| {
        let total = paths.len();
        let mut completed = 0;
        for path in paths {
            if cancel.load(Ordering::Relaxed) {
                return Ok(JobResult::Cancelled);
            }
            let _ = sender.send(JobMessage::Progress(JobProgress {
                label: "TRASH",
                completed,
                total,
                current: display_name(&path),
            }));
            trash::delete(&path).map_err(|error| error.to_string())?;
            completed += 1;
        }
        Ok(JobResult::Delete(completed))
    })
}

fn spawn_job(
    name: &'static str,
    worker: impl FnOnce(Sender<JobMessage>, Arc<AtomicBool>) -> Result<JobResult, String>
    + Send
    + 'static,
) -> FileJobTask {
    let (sender, receiver) = mpsc::channel();
    let cancel = Arc::new(AtomicBool::new(false));
    let worker_cancel = Arc::clone(&cancel);
    thread::Builder::new()
        .name(name.into())
        .spawn(move || {
            let result = worker(sender.clone(), worker_cancel);
            let _ = sender.send(JobMessage::Finished(result));
        })
        .expect("failed to spawn file operation worker");
    FileJobTask { receiver, cancel }
}

fn paste_worker(
    clipboard: &FileClipboard,
    destination: &Path,
    sender: &Sender<JobMessage>,
    cancel: &AtomicBool,
) -> Result<JobResult, String> {
    let total = clipboard
        .paths
        .iter()
        .map(|path| count_units(path))
        .collect::<io::Result<Vec<_>>>()
        .map_err(|error| error.to_string())?
        .into_iter()
        .sum::<usize>()
        .max(1);
    let label = match clipboard.kind {
        ClipboardKind::Copy => "COPY",
        ClipboardKind::Cut => "MOVE",
    };
    let mut completed = 0;
    let mut created = Vec::new();
    let mut moved = Vec::new();

    for source in &clipboard.paths {
        if cancel.load(Ordering::Relaxed) {
            rollback(clipboard.kind, &created, &moved);
            return Ok(JobResult::Cancelled);
        }
        if !source.exists() {
            continue;
        }
        let Some(name) = source.file_name() else {
            continue;
        };
        let target = unique_destination(&destination.join(name));
        if source.is_dir() && target.starts_with(source) {
            rollback(clipboard.kind, &created, &moved);
            return Err("a folder cannot be copied or moved inside itself".into());
        }
        let units = count_units(source).map_err(|error| error.to_string())?;
        let result = match clipboard.kind {
            ClipboardKind::Copy => copy_path(
                source,
                &target,
                label,
                total,
                &mut completed,
                sender,
                cancel,
            ),
            ClipboardKind::Cut => match fs::rename(source, &target) {
                Ok(()) => {
                    completed = completed.saturating_add(units);
                    send_progress(sender, label, completed, total, source);
                    Ok(())
                }
                Err(_) => copy_path(
                    source,
                    &target,
                    label,
                    total,
                    &mut completed,
                    sender,
                    cancel,
                )
                .and_then(|()| remove_path(source)),
            },
        };
        if let Err(error) = result {
            let _ = remove_path(&target);
            rollback(clipboard.kind, &created, &moved);
            if error.kind() == io::ErrorKind::Interrupted {
                return Ok(JobResult::Cancelled);
            }
            return Err(error.to_string());
        }
        match clipboard.kind {
            ClipboardKind::Copy => created.push(target),
            ClipboardKind::Cut => moved.push((target, source.clone())),
        }
    }

    let undo = match clipboard.kind {
        ClipboardKind::Copy => UndoAction::RemoveCreated(created),
        ClipboardKind::Cut => UndoAction::MoveBack(moved),
    };
    Ok(JobResult::Paste(undo))
}

fn copy_path(
    source: &Path,
    target: &Path,
    label: &'static str,
    total: usize,
    completed: &mut usize,
    sender: &Sender<JobMessage>,
    cancel: &AtomicBool,
) -> io::Result<()> {
    if cancel.load(Ordering::Relaxed) {
        return Err(io::Error::new(io::ErrorKind::Interrupted, "cancelled"));
    }
    let metadata = fs::symlink_metadata(source)?;
    if metadata.file_type().is_symlink() {
        return Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "symbolic links and junctions are not copied",
        ));
    }
    if metadata.is_dir() {
        fs::create_dir_all(target)?;
        *completed += 1;
        send_progress(sender, label, *completed, total, source);
        for entry in fs::read_dir(source)? {
            let entry = entry?;
            copy_path(
                &entry.path(),
                &target.join(entry.file_name()),
                label,
                total,
                completed,
                sender,
                cancel,
            )?;
        }
    } else {
        fs::copy(source, target)?;
        *completed += 1;
        send_progress(sender, label, *completed, total, source);
    }
    Ok(())
}

fn count_units(path: &Path) -> io::Result<usize> {
    let metadata = fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink() {
        return Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "symbolic links and junctions are not copied",
        ));
    }
    if !metadata.is_dir() {
        return Ok(1);
    }
    let mut total = 1;
    for entry in fs::read_dir(path)? {
        total += count_units(&entry?.path())?;
    }
    Ok(total)
}

fn send_progress(
    sender: &Sender<JobMessage>,
    label: &'static str,
    completed: usize,
    total: usize,
    path: &Path,
) {
    let _ = sender.send(JobMessage::Progress(JobProgress {
        label,
        completed,
        total,
        current: display_name(path),
    }));
}

fn rollback(kind: ClipboardKind, created: &[PathBuf], moved: &[(PathBuf, PathBuf)]) {
    match kind {
        ClipboardKind::Copy => {
            for path in created.iter().rev() {
                let _ = remove_path(path);
            }
        }
        ClipboardKind::Cut => {
            for (current, original) in moved.iter().rev() {
                if current.exists() && !original.exists() {
                    let _ = fs::rename(current, original);
                }
            }
        }
    }
}

fn remove_path(path: &Path) -> io::Result<()> {
    if path.is_dir() {
        fs::remove_dir_all(path)
    } else if path.exists() {
        fs::remove_file(path)
    } else {
        Ok(())
    }
}

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

fn display_name(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string())
}

#[cfg(test)]
mod tests {
    use std::{fs, time::Duration};

    use tempfile::tempdir;

    use super::*;

    #[test]
    fn background_copy_reports_progress_and_returns_undo() {
        let temp = tempdir().unwrap();
        let source_dir = temp.path().join("source");
        let destination = temp.path().join("destination");
        fs::create_dir_all(&source_dir).unwrap();
        fs::create_dir_all(&destination).unwrap();
        let source = source_dir.join("notes.txt");
        fs::write(&source, "hello").unwrap();
        let task = spawn_paste(
            FileClipboard {
                kind: ClipboardKind::Copy,
                paths: vec![source],
            },
            destination.clone(),
        );
        let mut saw_progress = false;
        let undo = loop {
            match task.receiver.recv_timeout(Duration::from_secs(2)).unwrap() {
                JobMessage::Progress(progress) => {
                    saw_progress = true;
                    assert!(progress.completed <= progress.total);
                }
                JobMessage::Finished(Ok(JobResult::Paste(undo))) => break undo,
                JobMessage::Finished(other) => panic!("unexpected result: {other:?}"),
            }
        };
        assert!(saw_progress);
        assert!(destination.join("notes.txt").exists());
        crate::operations::undo(undo).unwrap();
        assert!(!destination.join("notes.txt").exists());
    }
}
