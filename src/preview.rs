use std::{
    fs::{self, File},
    io::{Read, Take},
    path::{Path, PathBuf},
    sync::mpsc::Sender,
    thread,
};

use chrono::{DateTime, Local};

use crate::filesystem::{display_name, human_size};

const MAX_PREVIEW_BYTES: u64 = 256 * 1024;
const MAX_PREVIEW_LINES: usize = 4_000;

#[derive(Debug, Clone)]
pub enum PreviewBody {
    Loading,
    Text(Vec<String>),
    Metadata(Vec<String>),
    Error(String),
}

#[derive(Debug, Clone)]
pub struct PreviewResponse {
    pub generation: u64,
    pub path: PathBuf,
    pub title: String,
    pub body: PreviewBody,
}

pub fn spawn_preview(path: PathBuf, generation: u64, sender: Sender<PreviewResponse>) {
    let _ = thread::Builder::new()
        .name("ntiv-preview".into())
        .spawn(move || {
            let response = match build_preview(&path, generation) {
                Ok(response) => response,
                Err(error) => PreviewResponse {
                    generation,
                    title: display_name(&path),
                    path,
                    body: PreviewBody::Error(error.to_string()),
                },
            };
            let _ = sender.send(response);
        });
}

fn build_preview(path: &Path, generation: u64) -> std::io::Result<PreviewResponse> {
    let metadata = fs::metadata(path)?;
    let title = display_name(path);
    if metadata.is_dir() {
        let mut directories = 0_usize;
        let mut files = 0_usize;
        let mut unreadable = false;
        match fs::read_dir(path) {
            Ok(items) => {
                for item in items.flatten() {
                    match item.file_type() {
                        Ok(kind) if kind.is_dir() => directories += 1,
                        Ok(_) => files += 1,
                        Err(_) => unreadable = true,
                    }
                }
            }
            Err(_) => unreadable = true,
        }
        let mut lines = vec![
            "Directory".to_string(),
            String::new(),
            format!("Folders: {directories}"),
            format!("Files:   {files}"),
        ];
        if unreadable {
            lines.push("Some entries could not be read.".into());
        }
        push_modified(&mut lines, metadata.modified().ok());
        return Ok(PreviewResponse {
            generation,
            path: path.to_path_buf(),
            title,
            body: PreviewBody::Metadata(lines),
        });
    }

    if is_image(path) {
        let mut response = metadata_response(path, generation, metadata, "Image");
        if let Ok(size) = imagesize::size(path)
            && let PreviewBody::Metadata(lines) = &mut response.body
        {
            lines.insert(
                2,
                format!("Dimensions: {} × {} px", size.width, size.height),
            );
        }
        return Ok(response);
    }

    if path
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("pdf"))
    {
        let mut response = metadata_response(path, generation, metadata, "PDF document");
        if let Ok(bytes) = fs::read(path)
            && let PreviewBody::Metadata(lines) = &mut response.body
        {
            let marker = b"/Type /Page";
            let pages = bytes
                .windows(marker.len())
                .filter(|window| *window == marker)
                .count();
            if pages > 0 {
                lines.insert(2, format!("Pages: {pages}"));
            }
        }
        return Ok(response);
    }

    if is_known_binary(path) {
        return Ok(metadata_response(path, generation, metadata, "Binary file"));
    }

    let file = File::open(path)?;
    let mut buffer = Vec::new();
    let mut limited: Take<File> = file.take(MAX_PREVIEW_BYTES + 1);
    limited.read_to_end(&mut buffer)?;
    let truncated = buffer.len() as u64 > MAX_PREVIEW_BYTES;
    buffer.truncate(MAX_PREVIEW_BYTES as usize);

    if buffer.iter().take(8_192).any(|byte| *byte == 0) {
        return Ok(metadata_response(path, generation, metadata, "Binary file"));
    }

    let text = String::from_utf8_lossy(&buffer);
    let mut lines: Vec<String> = text
        .lines()
        .take(MAX_PREVIEW_LINES)
        .map(|line| line.replace('\t', "    "))
        .collect();
    if truncated || text.lines().count() > MAX_PREVIEW_LINES {
        lines.push(String::new());
        lines.push("… preview truncated …".into());
    }
    if lines.is_empty() {
        lines.push("(empty file)".into());
    }

    Ok(PreviewResponse {
        generation,
        path: path.to_path_buf(),
        title,
        body: PreviewBody::Text(lines),
    })
}

fn metadata_response(
    path: &Path,
    generation: u64,
    metadata: fs::Metadata,
    kind: &str,
) -> PreviewResponse {
    let mut lines = vec![
        kind.to_string(),
        String::new(),
        format!("Size: {}", human_size(metadata.len())),
        format!(
            "Type: {}",
            path.extension()
                .map(|extension| extension.to_string_lossy().to_uppercase())
                .unwrap_or_else(|| "Unknown".into())
        ),
    ];
    push_modified(&mut lines, metadata.modified().ok());
    PreviewResponse {
        generation,
        path: path.to_path_buf(),
        title: display_name(path),
        body: PreviewBody::Metadata(lines),
    }
}

fn push_modified(lines: &mut Vec<String>, modified: Option<std::time::SystemTime>) {
    if let Some(modified) = modified {
        let modified: DateTime<Local> = modified.into();
        lines.push(format!("Modified: {}", modified.format("%Y-%m-%d %H:%M")));
    }
}

fn is_known_binary(path: &Path) -> bool {
    let extension = path
        .extension()
        .map(|extension| extension.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    matches!(
        extension.as_str(),
        "png"
            | "jpg"
            | "jpeg"
            | "gif"
            | "webp"
            | "bmp"
            | "ico"
            | "pdf"
            | "zip"
            | "7z"
            | "rar"
            | "exe"
            | "dll"
            | "pdb"
            | "mp3"
            | "wav"
            | "mp4"
            | "mov"
            | "avi"
            | "docx"
            | "xlsx"
            | "pptx"
    )
}

fn is_image(path: &Path) -> bool {
    path.extension()
        .map(|extension| extension.to_string_lossy().to_ascii_lowercase())
        .is_some_and(|extension| {
            matches!(
                extension.as_str(),
                "png" | "jpg" | "jpeg" | "gif" | "webp" | "bmp" | "ico"
            )
        })
}

#[cfg(test)]
mod tests {
    use std::{fs::File, io::Write};

    use tempfile::tempdir;

    use super::*;

    #[test]
    fn text_preview_preserves_unicode() {
        let temp = tempdir().unwrap();
        let path = temp.path().join("שלום.txt");
        let mut file = File::create(&path).unwrap();
        writeln!(file, "hello שלום").unwrap();

        let preview = build_preview(&path, 7).unwrap();
        assert_eq!(preview.generation, 7);
        match preview.body {
            PreviewBody::Text(lines) => assert!(lines[0].contains("שלום")),
            _ => panic!("expected text preview"),
        }
    }

    #[test]
    fn large_text_preview_is_truncated_safely() {
        let temp = tempdir().unwrap();
        let path = temp.path().join("large.txt");
        let mut file = File::create(&path).unwrap();
        file.write_all(&vec![b'a'; MAX_PREVIEW_BYTES as usize + 32])
            .unwrap();

        let preview = build_preview(&path, 1).unwrap();
        match preview.body {
            PreviewBody::Text(lines) => {
                assert_eq!(
                    lines.last().map(String::as_str),
                    Some("… preview truncated …")
                );
            }
            _ => panic!("expected text preview"),
        }
    }

    #[test]
    fn image_preview_includes_dimensions() {
        let temp = tempdir().unwrap();
        let path = temp.path().join("pixel.png");
        let png = [
            0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 0, 0, 0, 13, b'I', b'H', b'D', b'R', 0,
            0, 0, 2, 0, 0, 0, 3, 8, 6, 0, 0, 0,
        ];
        fs::write(&path, png).unwrap();
        let preview = build_preview(&path, 1).unwrap();
        match preview.body {
            PreviewBody::Metadata(lines) => {
                assert!(lines.iter().any(|line| line.contains("2 × 3 px")))
            }
            _ => panic!("expected image metadata"),
        }
    }
}
