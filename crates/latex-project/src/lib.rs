//! Project-level model for LaTeX sources.
//!
//! The project root is a directory the user picked. Files are described
//! relative to the root; nothing in this crate writes to disk.

mod graph;
mod save;
mod scan;

pub use graph::{IncludeEdge, IncludeGraph, UnresolvedInclude};
pub use save::{atomic_write, autosave_clear, autosave_list, autosave_read, autosave_write, Recoverable};
pub use scan::{FileEntry, FileIndex, FileKind};

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

pub struct Project {
    root: PathBuf,
}

impl Project {
    /// Open a project rooted at `root`. The path is canonicalized so that
    /// later `strip_prefix` calls are reliable.
    pub fn open(root: impl AsRef<Path>) -> Result<Self> {
        let root = root
            .as_ref()
            .canonicalize()
            .with_context(|| format!("cannot resolve project root {:?}", root.as_ref()))?;
        anyhow::ensure!(root.is_dir(), "project root is not a directory: {:?}", root);
        Ok(Self { root })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Scan the project tree. Ignores VCS directories, build outputs,
    /// hidden entries, and symlinks.
    pub fn index(&self) -> Result<FileIndex> {
        scan::scan(&self.root)
    }

    /// Pick the entry file from an index.
    ///
    /// A candidate is any `.tex` file containing `\documentclass`. When
    /// multiple candidates exist, rank by filename:
    ///
    ///   main.tex > document.tex > paper.tex > thesis.tex > root.tex > rest.
    ///
    /// Returns `None` when no candidate exists.
    pub fn entry(&self, index: &FileIndex) -> Option<PathBuf> {
        let mut candidates: Vec<(u8, PathBuf)> = Vec::new();
        for file in index.sources() {
            let abs = self.root.join(&file.path);
            let Ok(text) = std::fs::read_to_string(&abs) else {
                continue;
            };
            if !text.contains("\\documentclass") {
                continue;
            }
            candidates.push((preferred_name_rank(&file.path), file.path.clone()));
        }
        candidates.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1)));
        candidates.into_iter().next().map(|(_, p)| p)
    }

    /// Build the include graph rooted at `entry`. `entry` is relative to
    /// the project root.
    pub fn include_graph(&self, entry: &Path) -> Result<IncludeGraph> {
        graph::build(&self.root, entry)
    }

    /// Read a project file as UTF-8. `rel` is relative to the root.
    pub fn read(&self, rel: &Path) -> Result<String> {
        let abs = self.root.join(rel);
        std::fs::read_to_string(&abs).with_context(|| format!("cannot read {:?}", abs))
    }

    /// Absolute path for a project-relative path.
    pub fn abs(&self, rel: &Path) -> PathBuf {
        self.root.join(rel)
    }
}

fn preferred_name_rank(p: &Path) -> u8 {
    match p.file_name().and_then(|s| s.to_str()).unwrap_or("") {
        "main.tex" => 0,
        "document.tex" => 1,
        "paper.tex" => 2,
        "thesis.tex" => 3,
        "root.tex" => 4,
        _ => 10,
    }
}
