//! Atomic file writes and autosave/recovery support.
//!
//! Every real save goes through `atomic_write`: write to a temp file in the
//! same directory, `fsync` it, then `rename` over the target. A crash at any
//! point leaves either the old file or the new one, never a truncated one.
//!
//! Autosaves live under `.latex-studio-build/autosave/<rel_path>`, mirroring
//! the project tree. They are cleared on successful real save. On load, any
//! autosave whose mtime is newer than the corresponding real file is reported
//! as recoverable.

use anyhow::{anyhow, bail, Context, Result};
use std::fs::{self, File};
use std::io::Write;
use std::path::{Component, Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

pub const AUTOSAVE_DIR: &str = ".latex-studio-build/autosave";

#[derive(Debug, Clone, serde::Serialize)]
pub struct Recoverable {
    /// Path relative to the project root.
    pub path: String,
    /// Autosave mtime, milliseconds since the Unix epoch.
    pub autosave_mtime_ms: u128,
    /// Real file mtime, or 0 if the real file does not exist.
    pub original_mtime_ms: u128,
}

/// Write `content` to `target` atomically. Creates parent directories.
pub fn atomic_write(target: &Path, content: &str) -> Result<()> {
    let parent = target
        .parent()
        .ok_or_else(|| anyhow!("no parent directory for {target:?}"))?;
    fs::create_dir_all(parent)
        .with_context(|| format!("create_dir_all {parent:?}"))?;

    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let pid = std::process::id();
    let basename = target
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("file");
    let tmp = parent.join(format!(".{basename}.tmp-{pid}-{nanos}"));

    {
        let mut f = File::create(&tmp)
            .with_context(|| format!("create temp {tmp:?}"))?;
        f.write_all(content.as_bytes())?;
        f.sync_all()?;
    }

    // On Windows, rename over an existing file fails.
    #[cfg(windows)]
    {
        let _ = fs::remove_file(target);
    }

    fs::rename(&tmp, target)
        .with_context(|| format!("rename {tmp:?} -> {target:?}"))?;

    // Durability of the rename itself on Unix: fsync the parent directory.
    #[cfg(unix)]
    {
        if let Ok(dir) = File::open(parent) {
            let _ = dir.sync_all();
        }
    }

    Ok(())
}

/// Write an autosave for `rel` under the project's autosave directory.
pub fn autosave_write(root: &Path, rel: &Path, content: &str) -> Result<()> {
    let target = autosave_path(root, rel)?;
    atomic_write(&target, content)
}

/// Clear any autosave for `rel`. No-op if none exists.
pub fn autosave_clear(root: &Path, rel: &Path) -> Result<()> {
    let target = autosave_path(root, rel)?;
    if target.exists() {
        fs::remove_file(&target)
            .with_context(|| format!("remove {target:?}"))?;
    }
    Ok(())
}

/// Read an autosave for `rel`.
pub fn autosave_read(root: &Path, rel: &Path) -> Result<String> {
    let target = autosave_path(root, rel)?;
    fs::read_to_string(&target)
        .with_context(|| format!("read {target:?}"))
}

/// List autosaves newer than their real counterparts.
pub fn autosave_list(root: &Path) -> Result<Vec<Recoverable>> {
    let base = root.join(AUTOSAVE_DIR);
    if !base.is_dir() {
        return Ok(Vec::new());
    }
    let mut out = Vec::new();
    walk(&base, &base, root, &mut out)?;
    out.sort_by(|a, b| b.autosave_mtime_ms.cmp(&a.autosave_mtime_ms));
    Ok(out)
}

fn autosave_path(root: &Path, rel: &Path) -> Result<PathBuf> {
    let base = root.join(AUTOSAVE_DIR);
    let mut out = base;
    for comp in rel.components() {
        match comp {
            Component::Normal(c) => out.push(c),
            Component::CurDir => {}
            _ => bail!("autosave path escapes project root: {rel:?}"),
        }
    }
    Ok(out)
}

fn walk(base: &Path, cur: &Path, root: &Path, out: &mut Vec<Recoverable>) -> Result<()> {
    for entry in fs::read_dir(cur)? {
        let entry = entry?;
        let p = entry.path();
        let ft = entry.file_type()?;
        if ft.is_dir() {
            walk(base, &p, root, out)?;
        } else if ft.is_file() {
            let rel = match p.strip_prefix(base) {
                Ok(r) => r.to_path_buf(),
                Err(_) => continue,
            };
            let real = root.join(&rel);
            let autosave_mtime = mtime_ms(&p);
            let original_mtime = if real.exists() { mtime_ms(&real) } else { 0 };

            // Recoverable if the real file is missing, or the autosave is
            // at least as new as it. `>=` rather than `>` because the
            // filesystem mtime granularity can be as coarse as one second,
            // and a same-second save/autosave pair must still be recoverable.
            if original_mtime == 0 || autosave_mtime >= original_mtime {
                out.push(Recoverable {
                    path: rel.to_string_lossy().into_owned(),
                    autosave_mtime_ms: autosave_mtime,
                    original_mtime_ms: original_mtime,
                });
            }
        }
    }
    Ok(())
}

fn mtime_ms(p: &Path) -> u128 {
    fs::metadata(p)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_millis())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn atomic_write_creates_file() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("a/b/c.txt");
        atomic_write(&p, "hello").unwrap();
        assert_eq!(fs::read_to_string(&p).unwrap(), "hello");
    }

    #[test]
    fn atomic_write_replaces_content() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("x.txt");
        atomic_write(&p, "one").unwrap();
        atomic_write(&p, "two").unwrap();
        assert_eq!(fs::read_to_string(&p).unwrap(), "two");
        // No temp files left behind.
        for e in fs::read_dir(d.path()).unwrap() {
            let n = e.unwrap().file_name();
            let s = n.to_string_lossy();
            assert!(!s.contains(".tmp-"), "leftover: {s}");
        }
    }

    #[test]
    fn autosave_roundtrip() {
        let d = tempfile::tempdir().unwrap();
        let root = d.path();
        let rel = Path::new("chapters/one.tex");
        fs::create_dir_all(root.join("chapters")).unwrap();
        fs::write(root.join(rel), "original\n").unwrap();

        // Autosave is newer than the file.
        std::thread::sleep(std::time::Duration::from_millis(20));
        autosave_write(root, rel, "edited\n").unwrap();

        let list = autosave_list(root).unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].path, "chapters/one.tex");
        assert!(list[0].autosave_mtime_ms > list[0].original_mtime_ms);

        assert_eq!(autosave_read(root, rel).unwrap(), "edited\n");

        autosave_clear(root, rel).unwrap();
        assert!(autosave_list(root).unwrap().is_empty());
    }

    #[test]
    fn autosave_not_listed_when_real_file_is_newer() {
        let d = tempfile::tempdir().unwrap();
        let root = d.path();
        let rel = Path::new("main.tex");
        fs::write(root.join(rel), "old\n").unwrap();
        autosave_write(root, rel, "autosaved\n").unwrap();

        // Simulate a real save after the autosave was written.
        std::thread::sleep(std::time::Duration::from_millis(20));
        atomic_write(&root.join(rel), "real\n").unwrap();

        assert!(autosave_list(root).unwrap().is_empty());
    }

    #[test]
    fn autosave_path_rejects_escape() {
        let d = tempfile::tempdir().unwrap();
        let root = d.path();
        assert!(autosave_path(root, Path::new("../evil")).is_err());
        assert!(autosave_path(root, Path::new("/etc/passwd")).is_err());
    }
}
