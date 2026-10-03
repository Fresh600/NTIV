use std::{
    fs,
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver},
    },
    thread,
};

use ignore::WalkBuilder;

const MAX_SEARCH_FILE_BYTES: u64 = 2 * 1024 * 1024;
const BATCH_SIZE: usize = 32;
const RESULT_LIMIT: usize = 2_000;

#[derive(Debug, Clone)]
pub struct ContentMatch {
    pub path: PathBuf,
    pub line: usize,
    pub column: usize,
    pub text: String,
}

#[derive(Debug)]
pub enum ContentSearchMessage {
    Batch(Vec<ContentMatch>),
    Done { skipped: usize, limited: bool },
}

pub struct ContentSearchTask {
    pub receiver: Receiver<ContentSearchMessage>,
    cancel: Arc<AtomicBool>,
}

impl ContentSearchTask {
    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}

impl Drop for ContentSearchTask {
    fn drop(&mut self) {
        self.cancel();
    }
}

pub fn spawn_content_search(root: PathBuf, query: String) -> ContentSearchTask {
    let (sender, receiver) = mpsc::channel();
    let cancel = Arc::new(AtomicBool::new(false));
    let worker_cancel = Arc::clone(&cancel);
    thread::Builder::new()
        .name("ntiv-content-search".into())
        .spawn(move || {
            let needle = query.to_lowercase();
            let mut builder = WalkBuilder::new(root);
            builder.hidden(false).follow_links(false);
            let mut batch = Vec::with_capacity(BATCH_SIZE);
            let mut skipped = 0;
            let mut found = 0;
            let mut limited = false;

            'walk: for result in builder.build() {
                if worker_cancel.load(Ordering::Relaxed) {
                    return;
                }
                let entry = match result {
                    Ok(entry) if entry.file_type().is_some_and(|kind| kind.is_file()) => entry,
                    Ok(_) => continue,
                    Err(_) => {
                        skipped += 1;
                        continue;
                    }
                };
                let Ok(metadata) = entry.metadata() else {
                    skipped += 1;
                    continue;
                };
                if metadata.len() > MAX_SEARCH_FILE_BYTES {
                    skipped += 1;
                    continue;
                }
                let Ok(bytes) = fs::read(entry.path()) else {
                    skipped += 1;
                    continue;
                };
                if bytes.iter().take(8_192).any(|byte| *byte == 0) {
                    continue;
                }
                let text = String::from_utf8_lossy(&bytes);
                for (line_index, line) in text.lines().enumerate() {
                    if worker_cancel.load(Ordering::Relaxed) {
                        return;
                    }
                    let lowercase = line.to_lowercase();
                    let Some(column) = lowercase.find(&needle) else {
                        continue;
                    };
                    batch.push(ContentMatch {
                        path: entry.path().to_path_buf(),
                        line: line_index + 1,
                        column: column + 1,
                        text: line.trim().chars().take(180).collect(),
                    });
                    found += 1;
                    if batch.len() == BATCH_SIZE
                        && sender
                            .send(ContentSearchMessage::Batch(std::mem::take(&mut batch)))
                            .is_err()
                    {
                        return;
                    }
                    if found >= RESULT_LIMIT {
                        limited = true;
                        break 'walk;
                    }
                }
            }
            if !batch.is_empty() && sender.send(ContentSearchMessage::Batch(batch)).is_err() {
                return;
            }
            let _ = sender.send(ContentSearchMessage::Done { skipped, limited });
        })
        .expect("failed to spawn content search worker");
    ContentSearchTask { receiver, cancel }
}

#[cfg(test)]
mod tests {
    use std::{fs, time::Duration};

    use tempfile::tempdir;

    use super::*;

    #[test]
    fn finds_text_with_line_and_column_and_skips_binary_files() {
        let temp = tempdir().unwrap();
        fs::write(temp.path().join("notes.txt"), "first\nhello NTIV world\n").unwrap();
        fs::write(temp.path().join("binary.bin"), b"hello\0NTIV").unwrap();
        let task = spawn_content_search(temp.path().to_path_buf(), "ntiv".into());
        let mut results = Vec::new();
        while let ContentSearchMessage::Batch(batch) =
            task.receiver.recv_timeout(Duration::from_secs(2)).unwrap()
        {
            results.extend(batch);
        }
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].line, 2);
        assert_eq!(results[0].column, 7);
        assert_eq!(results[0].text, "hello NTIV world");
    }
}
