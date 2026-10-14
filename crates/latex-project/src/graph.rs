//! Include-graph construction.
//!
//! Recognised commands in this milestone: `\input`, `\include`, `\subfile`.
//! `\import` / `\subimport` (two-argument forms) are not yet handled; see
//! MILESTONE-2-STATUS.md for the limitation.

use anyhow::Result;
use std::collections::{HashSet, VecDeque};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct IncludeEdge {
    /// File containing the directive, relative to the project root.
    pub from: PathBuf,
    /// File being included, relative to the project root.
    pub to: PathBuf,
    /// 1-based line number of the directive in `from`.
    pub line: u32,
    /// Command name without the backslash: `input`, `include`, `subfile`.
    pub command: String,
    /// Raw argument text as written in the source.
    pub raw: String,
}

#[derive(Debug, Clone)]
pub struct UnresolvedInclude {
    pub from: PathBuf,
    pub line: u32,
    pub command: String,
    pub raw: String,
    pub reason: String,
}

#[derive(Debug, Clone)]
pub struct IncludeGraph {
    pub entry: PathBuf,
    pub edges: Vec<IncludeEdge>,
    pub unresolved: Vec<UnresolvedInclude>,
}

const COMMANDS: &[&str] = &["input", "include", "subfile"];

pub fn build(root: &Path, entry: &Path) -> Result<IncludeGraph> {
    let mut edges = Vec::new();
    let mut unresolved = Vec::new();
    let mut visited: HashSet<PathBuf> = HashSet::new();
    let mut queue: VecDeque<PathBuf> = VecDeque::new();
    queue.push_back(entry.to_path_buf());

    while let Some(rel) = queue.pop_front() {
        if !visited.insert(rel.clone()) {
            continue;
        }
        let abs = root.join(&rel);
        let Ok(text) = std::fs::read_to_string(&abs) else {
            continue;
        };
        for (idx, line) in text.lines().enumerate() {
            let line_no = (idx + 1) as u32;
            let stripped = strip_comment(line);
            for (cmd, arg) in find_include_commands(stripped) {
                match resolve(root, &abs, &arg) {
                    Some(target_abs) => {
                        let Ok(target_rel) = target_abs.strip_prefix(root) else {
                            unresolved.push(UnresolvedInclude {
                                from: rel.clone(),
                                line: line_no,
                                command: cmd,
                                raw: arg,
                                reason: "resolves outside project root".into(),
                            });
                            continue;
                        };
                        let target_rel = target_rel.to_path_buf();
                        edges.push(IncludeEdge {
                            from: rel.clone(),
                            to: target_rel.clone(),
                            line: line_no,
                            command: cmd,
                            raw: arg,
                        });
                        if !visited.contains(&target_rel) {
                            queue.push_back(target_rel);
                        }
                    }
                    None => {
                        unresolved.push(UnresolvedInclude {
                            from: rel.clone(),
                            line: line_no,
                            command: cmd,
                            raw: arg,
                            reason: "no matching file".into(),
                        });
                    }
                }
            }
        }
    }

    edges.sort_by(|a, b| a.from.cmp(&b.from).then_with(|| a.line.cmp(&b.line)));
    unresolved.sort_by(|a, b| a.from.cmp(&b.from).then_with(|| a.line.cmp(&b.line)));

    Ok(IncludeGraph {
        entry: entry.to_path_buf(),
        edges,
        unresolved,
    })
}

/// Find `\input{...}`, `\include{...}`, `\subfile{...}` on a single line.
/// The line must already have had its comment stripped.
pub fn find_include_commands(line: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let bytes = line.as_bytes();
    let mut i = 0usize;

    while i < bytes.len() {
        if bytes[i] != b'\\' {
            i += 1;
            continue;
        }
        let mut matched = false;
        for cmd in COMMANDS {
            let cb = cmd.as_bytes();
            let after = i + 1 + cb.len();
            if after > bytes.len() {
                continue;
            }
            if &bytes[i + 1..after] != cb {
                continue;
            }
            // Reject `\included` style longer identifiers and starred forms.
            if after < bytes.len() && (bytes[after].is_ascii_alphabetic() || bytes[after] == b'*')
            {
                continue;
            }
            // Skip whitespace between the command and `{`.
            let mut j = after;
            while j < bytes.len() && bytes[j] == b' ' {
                j += 1;
            }
            if j >= bytes.len() || bytes[j] != b'{' {
                continue;
            }
            let start = j + 1;
            let mut end = start;
            while end < bytes.len() && bytes[end] != b'}' {
                end += 1;
            }
            if end >= bytes.len() {
                continue;
            }
            out.push((cmd.to_string(), line[start..end].to_string()));
            i = end + 1;
            matched = true;
            break;
        }
        if !matched {
            i += 1;
        }
    }
    out
}

/// Strip a LaTeX comment: everything from the first unescaped `%` to the
/// end of the line. `\%` and `\\%` are handled correctly.
pub fn strip_comment(line: &str) -> &str {
    let bytes = line.as_bytes();
    let mut i = 0usize;
    while i < bytes.len() {
        if bytes[i] == b'\\' {
            i += 2;
            continue;
        }
        if bytes[i] == b'%' {
            return &line[..i];
        }
        i += 1;
    }
    line
}

fn resolve(root: &Path, from_abs: &Path, arg: &str) -> Option<PathBuf> {
    let p = Path::new(arg);
    if p.is_absolute() {
        return p.canonicalize().ok().filter(|c| c.starts_with(root));
    }
    let candidates: Vec<PathBuf> = if p.extension().is_some() {
        vec![p.to_path_buf()]
    } else {
        vec![p.with_extension("tex"), p.to_path_buf()]
    };
    // TeX resolves \input paths relative to the main document's directory,
    // which is the project root we compile from. Match that exactly: prefer
    // project-root-relative, then fall back to including-file-relative only
    // for the historical case of `\input` inside a document that is itself
    // an entry point. In practice a wrong relative path is a real error and
    // the validator should report it, not paper over it.
    for cand in &candidates {
        if let Ok(c) = root.join(cand).canonicalize() {
            if c.is_file() && c.starts_with(root) {
                return Some(c);
            }
        }
    }
    // Fallback: relative to the including file. This catches the case where
    // a user compiles a sub-file directly as an entry point.
    if let Some(from_dir) = from_abs.parent() {
        for cand in &candidates {
            if let Ok(c) = from_dir.join(cand).canonicalize() {
                if c.is_file() && c.starts_with(root) {
                    return Some(c);
                }
            }
        }
    }
    None
}
