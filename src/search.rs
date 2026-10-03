use std::{
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver},
    },
    thread,
};

use ignore::WalkBuilder;

const BATCH_SIZE: usize = 64;

#[derive(Debug)]
pub enum SearchMessage {
    Batch(Vec<PathBuf>),
    Done { skipped: usize },
}

pub struct SearchTask {
    pub receiver: Receiver<SearchMessage>,
    cancel: Arc<AtomicBool>,
}

impl SearchTask {
    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}

impl Drop for SearchTask {
    fn drop(&mut self) {
        self.cancel();
    }
}

pub fn spawn_search(root: PathBuf) -> SearchTask {
    let (sender, receiver) = mpsc::channel();
    let cancel = Arc::new(AtomicBool::new(false));
    let worker_cancel = Arc::clone(&cancel);

    thread::Builder::new()
        .name("ntiv-file-search".into())
        .spawn(move || {
            let mut builder = WalkBuilder::new(&root);
            builder
                .hidden(false)
                .parents(false)
                .ignore(false)
                .git_ignore(false)
                .git_global(false)
                .git_exclude(false)
                .follow_links(false);

            let mut batch = Vec::with_capacity(BATCH_SIZE);
            let mut skipped = 0;
            for result in builder.build() {
                if worker_cancel.load(Ordering::Relaxed) {
                    return;
                }
                match result {
                    Ok(entry) if entry.file_type().is_some_and(|kind| kind.is_file()) => {
                        batch.push(entry.into_path());
                        if batch.len() == BATCH_SIZE {
                            if sender.send(SearchMessage::Batch(batch)).is_err() {
                                return;
                            }
                            batch = Vec::with_capacity(BATCH_SIZE);
                        }
                    }
                    Ok(_) => {}
                    Err(_) => skipped += 1,
                }
            }
            if !batch.is_empty() && sender.send(SearchMessage::Batch(batch)).is_err() {
                return;
            }
            let _ = sender.send(SearchMessage::Done { skipped });
        })
        .expect("failed to spawn file search worker");

    SearchTask { receiver, cancel }
}

pub fn rank_paths(paths: &[PathBuf], root: &Path, query: &str, limit: usize) -> Vec<PathBuf> {
    let mut ranked: Vec<(i64, String, PathBuf)> = paths
        .iter()
        .filter_map(|path| {
            let display = path
                .strip_prefix(root)
                .unwrap_or(path)
                .to_string_lossy()
                .into_owned();
            fuzzy_score(&display, query).map(|score| (score, display.to_lowercase(), path.clone()))
        })
        .collect();

    ranked.sort_by(|left, right| right.0.cmp(&left.0).then_with(|| left.1.cmp(&right.1)));
    ranked
        .into_iter()
        .take(limit)
        .map(|(_, _, path)| path)
        .collect()
}

pub fn fuzzy_score(candidate: &str, query: &str) -> Option<i64> {
    let candidate = candidate.to_lowercase();
    let query = query.trim().to_lowercase();
    if query.is_empty() {
        return Some(0);
    }
    if let Some(position) = candidate.find(&query) {
        return Some(10_000 - position as i64 * 10 - candidate.len() as i64);
    }

    let mut query_chars = query.chars();
    let mut wanted = query_chars.next()?;
    let mut score = 0_i64;
    let mut previous_match = None;
    let mut matched = 0;

    for (index, character) in candidate.chars().enumerate() {
        if character != wanted {
            continue;
        }
        matched += 1;
        score += 50;
        if previous_match.is_some_and(|previous| previous + 1 == index) {
            score += 25;
        }
        if index == 0
            || candidate
                .chars()
                .nth(index.saturating_sub(1))
                .is_some_and(|previous| matches!(previous, '\\' | '/' | '-' | '_' | ' '))
        {
            score += 20;
        }
        previous_match = Some(index);
        match query_chars.next() {
            Some(next) => wanted = next,
            None => return Some(score - candidate.len() as i64 + matched * 5),
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use std::{fs, fs::File, time::Duration};

    use tempfile::tempdir;

    use super::*;

    #[test]
    fn fuzzy_matching_rewards_contiguous_text() {
        let contiguous = fuzzy_score("src/preview.rs", "pre").unwrap();
        let scattered = fuzzy_score("project_reader.exe", "pre").unwrap();
        assert!(contiguous > scattered);
        assert_eq!(fuzzy_score("notes.txt", "xyz"), None);
    }

    #[test]
    fn empty_query_matches_everything() {
        assert_eq!(fuzzy_score("anything", ""), Some(0));
    }

    #[test]
    fn background_search_finds_nested_unicode_file() {
        let temp = tempdir().unwrap();
        let nested = temp.path().join("תיקייה");
        fs::create_dir(&nested).unwrap();
        let wanted = nested.join("שלום.txt");
        File::create(&wanted).unwrap();

        let task = spawn_search(temp.path().to_path_buf());
        let mut found = Vec::new();
        while let SearchMessage::Batch(paths) =
            task.receiver.recv_timeout(Duration::from_secs(2)).unwrap()
        {
            found.extend(paths);
        }
        assert!(found.contains(&wanted));
    }
}
