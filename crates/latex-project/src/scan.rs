//! Directory scanning and file classification.

use anyhow::Result;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileKind {
    Source,
    Bibliography,
    Style,
    Asset,
    Other,
}

#[derive(Debug, Clone)]
pub struct FileEntry {
    /// Path relative to the project root.
    pub path: PathBuf,
    pub kind: FileKind,
    pub size: u64,
}

#[derive(Debug, Clone, Default)]
pub struct FileIndex {
    pub files: Vec<FileEntry>,
}

impl FileIndex {
    pub fn sources(&self) -> impl Iterator<Item = &FileEntry> {
        self.files.iter().filter(|f| f.kind == FileKind::Source)
    }
    pub fn bibliographies(&self) -> impl Iterator<Item = &FileEntry> {
        self.files.iter().filter(|f| f.kind == FileKind::Bibliography)
    }
    pub fn by_kind(&self, kind: FileKind) -> impl Iterator<Item = &FileEntry> {
        self.files.iter().filter(move |f| f.kind == kind)
    }
}

const IGNORED_DIRS: &[&str] = &[
    ".git",
    ".hg",
    ".svn",
    ".latex-studio-build",
    "target",
    "node_modules",
    ".dart_tool",
    "build",
];

pub fn scan(root: &Path) -> Result<FileIndex> {
    let mut files = Vec::new();
    walk(root, root, &mut files)?;
    files.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(FileIndex { files })
}

fn walk(root: &Path, dir: &Path, out: &mut Vec<FileEntry>) -> Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        let name = path.file_name().and_then(|s| s.to_str()).unwrap_or("");
        if name.starts_with('.') {
            continue;
        }
        let meta = std::fs::symlink_metadata(&path)?;
        if meta.file_type().is_symlink() {
            // Skip symlinks: avoids directory loops and paths that escape
            // the project root.
            continue;
        }
        if meta.is_dir() {
            if IGNORED_DIRS.contains(&name) {
                continue;
            }
            walk(root, &path, out)?;
        } else if meta.is_file() {
            let rel = path
                .strip_prefix(root)
                .map(|p| p.to_path_buf())
                .unwrap_or_else(|_| path.clone());
            out.push(FileEntry {
                kind: classify(&path),
                path: rel,
                size: meta.len(),
            });
        }
    }
    Ok(())
}

fn classify(path: &Path) -> FileKind {
    match path
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_ascii_lowercase()
        .as_str()
    {
        "tex" | "ltx" => FileKind::Source,
        "bib" => FileKind::Bibliography,
        "sty" | "cls" | "bst" | "def" => FileKind::Style,
        "png" | "jpg" | "jpeg" | "pdf" | "eps" | "svg" | "gif" | "tif" | "tiff" | "bmp" => {
            FileKind::Asset
        }
        _ => FileKind::Other,
    }
}
