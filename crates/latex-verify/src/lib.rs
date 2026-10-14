//! Structural validator for LaTeX projects.
//!
//! Checks that go beyond what the compiler reports:
//!   - duplicate `\label`
//!   - `\ref` with no matching `\label`
//!   - `\cite` with no matching bibliography entry
//!   - `\includegraphics` pointing at a missing file
//!   - `\label` never referenced (info)
//!
//! Not a TeX parser. Per-line scanning of the constructs above, with
//! comments stripped. Multi-line command arguments are not handled; the
//! TeX convention of one directive per line is assumed. This is a
//! documented limitation, not a defect.

use anyhow::{Context, Result};
use latex_model::{Diagnostic, DiagnosticKind, Severity, SourceLocation};
use latex_project::{FileIndex, Project};
use std::collections::{HashMap, HashSet};
use std::path::Path;

#[derive(Debug, Clone)]
pub struct Located {
    pub value: String,
    pub file: String,
    pub line: u32,
}

#[derive(Debug, Default, Clone)]
pub struct Extracted {
    pub labels: Vec<Located>,
    pub refs: Vec<Located>,
    pub cites: Vec<Located>,
    pub graphics: Vec<Located>,
}

const REF_COMMANDS: &[&str] = &[
    "ref", "eqref", "pageref", "autoref", "cref", "Cref", "vref", "Vref",
];
const CITE_COMMANDS: &[&str] = &[
    "cite", "citep", "citet", "citealp", "citealt", "citeauthor", "citeyear",
    "autocite", "parencite", "textcite", "footcite", "smartcite", "supercite",
];
const GRAPHICS_COMMANDS: &[&str] = &["includegraphics"];
const ALL_COMMANDS: &[&str] = &[
    "label", "ref", "eqref", "pageref", "autoref", "cref", "Cref", "vref", "Vref",
    "cite", "citep", "citet", "citealp", "citealt", "citeauthor", "citeyear",
    "autocite", "parencite", "textcite", "footcite", "smartcite", "supercite",
    "includegraphics",
];

const IMAGE_EXTS: &[&str] = &[
    "pdf", "png", "jpg", "jpeg", "eps", "svg", "gif", "tif", "tiff", "bmp", "ps",
];

/// Strip a LaTeX comment: everything from the first unescaped `%`.
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

/// Scan one line for any of the tracked commands, returning
/// `(command_name, argument_string)` pairs.
///
/// Handles `\includegraphics[opts]{path}` by skipping an optional `[...]`
/// block. For cite commands, the caller splits the argument by comma.
pub fn scan_line(line: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let bytes = line.as_bytes();
    let mut i = 0usize;

    while i < bytes.len() {
        if bytes[i] != b'\\' {
            i += 1;
            continue;
        }
        let mut matched = false;
        for cmd in ALL_COMMANDS {
            let cb = cmd.as_bytes();
            let after = i + 1 + cb.len();
            if after > bytes.len() {
                continue;
            }
            if &bytes[i + 1..after] != cb {
                continue;
            }
            // Word boundary: reject `\labels` when looking for `\label`.
            if after < bytes.len() && bytes[after].is_ascii_alphabetic() {
                continue;
            }
            // Skip whitespace.
            let mut j = after;
            while j < bytes.len() && (bytes[j] == b' ' || bytes[j] == b'\t') {
                j += 1;
            }
            // Skip optional `[...]` (used by \includegraphics and some cite forms).
            if j < bytes.len() && bytes[j] == b'[' {
                while j < bytes.len() && bytes[j] != b']' {
                    j += 1;
                }
                if j < bytes.len() {
                    j += 1;
                }
                while j < bytes.len() && (bytes[j] == b' ' || bytes[j] == b'\t') {
                    j += 1;
                }
            }
            // Expect `{`.
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
            let value = line[start..end].trim().to_string();
            if !value.is_empty() {
                out.push((cmd.to_string(), value));
            }
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

/// Extract every tracked construct from a single source string.
pub fn extract_source(rel_file: &str, src: &str) -> Extracted {
    let mut out = Extracted::default();
    for (idx, raw_line) in src.lines().enumerate() {
        let line_no = (idx + 1) as u32;
        let line = strip_comment(raw_line);
        for (cmd, arg) in scan_line(line) {
            if cmd == "label" {
                out.labels.push(Located { value: arg, file: rel_file.into(), line: line_no });
            } else if REF_COMMANDS.contains(&cmd.as_str()) {
                out.refs.push(Located { value: arg, file: rel_file.into(), line: line_no });
            } else if CITE_COMMANDS.contains(&cmd.as_str()) {
                for key in arg.split(',').map(str::trim).filter(|s| !s.is_empty()) {
                    out.cites.push(Located {
                        value: key.to_string(),
                        file: rel_file.into(),
                        line: line_no,
                    });
                }
            } else if GRAPHICS_COMMANDS.contains(&cmd.as_str()) {
                out.graphics.push(Located { value: arg, file: rel_file.into(), line: line_no });
            }
        }
    }
    out
}

/// Extract bibliography keys from a `.bib` file. Handles `@article{key,`
/// and `@book(key,`. Skips `@string`, `@preamble`, `@comment`.
pub fn extract_bib_keys(src: &str) -> Vec<String> {
    let mut out = Vec::new();
    for line in src.lines() {
        let t = line.trim_start();
        if !t.starts_with('@') {
            continue;
        }
        let rest = &t[1..];
        let (kind, after) = match rest.find(|c: char| c == '{' || c == '(') {
            Some(i) => (&rest[..i], &rest[i + 1..]),
            None => continue,
        };
        let kind = kind.trim().to_ascii_lowercase();
        if kind == "string" || kind == "preamble" || kind == "comment" {
            continue;
        }
        let end = after
            .find(|c| c == ',' || c == '}' || c == ')')
            .unwrap_or(after.len());
        let key = after[..end].trim().trim_matches('"');
        if !key.is_empty() {
            out.push(key.to_string());
        }
    }
    out
}

/// Run all checks. `root` is the project root; `index` comes from
/// `latex_project::Project::index()`.
pub fn verify(root: &Path, index: &FileIndex, entry: Option<&Path>) -> Result<Vec<Diagnostic>> {
    // 1. Collect tokens from every .tex file.
    //    Entry-file tokens go first so that when two files define the same
    //    label, the entry file is treated as the original definition and
    //    the second file is reported as the duplicate.
    let mut tokens = Extracted::default();
    let entry_rel = entry.map(|p| p.to_string_lossy().replace('\\', "/"));
    let mut ordered: Vec<&latex_project::FileEntry> = index.sources().collect();
    if let Some(ref e) = entry_rel {
        ordered.sort_by_key(|f| {
            let p = f.path.to_string_lossy().replace('\\', "/");
            if &p == e { 0 } else { 1 }
        });
    }
    for f in ordered {
        let abs = root.join(&f.path);
        let src = std::fs::read_to_string(&abs)
            .with_context(|| format!("read {}", abs.display()))?;
        let rel = f.path.to_string_lossy().into_owned();
        let t = extract_source(&rel, &src);
        tokens.labels.extend(t.labels);
        tokens.refs.extend(t.refs);
        tokens.cites.extend(t.cites);
        tokens.graphics.extend(t.graphics);
    }

    // 2. Collect bib keys from every .bib file.
    let mut bib_keys: HashSet<String> = HashSet::new();
    for f in index.bibliographies() {
        let abs = root.join(&f.path);
        let src = std::fs::read_to_string(&abs)
            .with_context(|| format!("read {}", abs.display()))?;
        for k in extract_bib_keys(&src) {
            bib_keys.insert(k);
        }
    }

    // 3. Index the labels.
    let mut label_locs: HashMap<String, Vec<Located>> = HashMap::new();
    for l in &tokens.labels {
        label_locs.entry(l.value.clone()).or_default().push(l.clone());
    }

    // 4. All known file paths, for asset resolution.
    let known_paths: HashSet<String> = index
        .files
        .iter()
        .map(|f| f.path.to_string_lossy().replace('\\', "/"))
        .collect();

    let mut out: Vec<Diagnostic> = Vec::new();

    // 4a. Duplicate labels.
    for (name, locs) in &label_locs {
        if locs.len() < 2 {
            continue;
        }
        for extra in &locs[1..] {
            out.push(Diagnostic {
                severity: Severity::Error,
                kind: DiagnosticKind::DuplicateLabel,
                message: format!("Label '{name}' is defined more than once"),
                explanation: Some(format!(
                    "First defined at {}:{}",
                    locs[0].file, locs[0].line
                )),
                location: Some(SourceLocation {
                    file: extra.file.clone(),
                    line: extra.line,
                    column: None,
                }),
                raw: None,
            });
        }
    }

    // 4b. Undefined references.
    for r in &tokens.refs {
        if !label_locs.contains_key(&r.value) {
            out.push(Diagnostic {
                severity: Severity::Error,
                kind: DiagnosticKind::UnresolvedReference,
                message: format!("Reference '{}' has no matching \\label", r.value),
                explanation: None,
                location: Some(SourceLocation {
                    file: r.file.clone(),
                    line: r.line,
                    column: None,
                }),
                raw: None,
            });
        }
    }

    // 4c. Undefined citations.
    for c in &tokens.cites {
        if !bib_keys.contains(&c.value) {
            out.push(Diagnostic {
                severity: Severity::Error,
                kind: DiagnosticKind::UnresolvedCitation,
                message: format!("Citation '{}' has no matching entry in any .bib file", c.value),
                explanation: None,
                location: Some(SourceLocation {
                    file: c.file.clone(),
                    line: c.line,
                    column: None,
                }),
                raw: None,
            });
        }
    }

    // 4d. Missing graphics files.
    for g in &tokens.graphics {
        if !asset_resolves(&known_paths, &g.value) {
            out.push(Diagnostic {
                severity: Severity::Error,
                kind: DiagnosticKind::MissingAsset,
                message: format!("Graphics file '{}' not found in the project", g.value),
                explanation: None,
                location: Some(SourceLocation {
                    file: g.file.clone(),
                    line: g.line,
                    column: None,
                }),
                raw: None,
            });
        }
    }

    // 4e. Unused labels (stylistic).
    let used: HashSet<&str> = tokens.refs.iter().map(|r| r.value.as_str()).collect();
    for (name, locs) in &label_locs {
        if !used.contains(name.as_str()) {
            let l = &locs[0];
            out.push(Diagnostic {
                severity: Severity::Info,
                kind: DiagnosticKind::UnusedLabel,
                message: format!("Label '{name}' is never referenced"),
                explanation: None,
                location: Some(SourceLocation {
                    file: l.file.clone(),
                    line: l.line,
                    column: None,
                }),
                raw: None,
            });
        }
    }

    // Stable order: by file, then line, then severity.
    out.sort_by(|a, b| {
        let fa = a.location.as_ref().map(|l| l.file.as_str()).unwrap_or("");
        let fb = b.location.as_ref().map(|l| l.file.as_str()).unwrap_or("");
        fa.cmp(fb)
            .then_with(|| {
                let la = a.location.as_ref().map(|l| l.line).unwrap_or(0);
                let lb = b.location.as_ref().map(|l| l.line).unwrap_or(0);
                la.cmp(&lb)
            })
            .then_with(|| format!("{:?}", a.kind).cmp(&format!("{:?}", b.kind)))
    });

    Ok(out)
}

/// Convenience: open the project, index it, run all checks.
pub fn verify_project(root: &Path) -> Result<Vec<Diagnostic>> {
    let project = Project::open(root)?;
    let index = project.index()?;
    let entry = project.entry(&index);
    verify(root, &index, entry.as_deref())
}

fn asset_resolves(known: &HashSet<String>, arg: &str) -> bool {
    let normalized = arg.replace('\\', "/");
    if known.contains(&normalized) {
        return true;
    }
    // Try common image extensions for the extensionless form.
    let p = Path::new(&normalized);
    if p.extension().is_none() {
        for ext in IMAGE_EXTS {
            let with_ext = format!("{normalized}.{ext}");
            if known.contains(&with_ext) {
                return true;
            }
        }
    }
    false
}
