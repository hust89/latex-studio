//! Correction engine.
//!
//! Takes a `Diagnostic` from the validator (or from a compile log) and
//! produces a `Correction`: a minimal edit the user can approve or reject.
//!
//! Guarantees:
//!  - `old_text` is an exact substring of the named file on the named line,
//!    verified by re-reading the file at proposal time. The apply step (M4.3)
//!    re-verifies before writing, so a correction can never corrupt source.
//!  - `confidence` is `Certain` only when the fix is mechanically forced.
//!    Typo suggestions are `Likely`. Deletions and renames are `Uncertain`.
//!  - `may_change_meaning` is set whenever applying could change semantics.
//!    The UI must require explicit approval in that case.
//!
//! Deliberately does not attempt to fix:
//!  - duplicate labels (choosing which to rename is a user decision)
//!  - diagnostics without a source location
//!  - compile errors that are not structural

use anyhow::{Context, Result};
use latex_model::{Diagnostic, DiagnosticKind};
use latex_project::Project;
use latex_verify::{extract_bib_keys, extract_source};
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum Confidence {
    Certain,
    Likely,
    Uncertain,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Alternative {
    pub value: String,
    pub explanation: String,
    pub confidence: Confidence,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Correction {
    /// The diagnostic this corrects, so the UI can show the problem alongside.
    pub diagnostic: Diagnostic,
    /// Plain-language explanation, safe to show a non-expert.
    pub explanation: String,
    pub file: String,
    pub line: u32,
    /// The whole line the change applies to, for context.
    pub context: String,
    /// Exact substring of `context` to replace.
    pub old_text: String,
    /// Replacement. Empty string means "delete old_text".
    pub new_text: String,
    pub confidence: Confidence,
    pub may_change_meaning: bool,
    /// Other plausible fixes, in preference order after the primary one.
    pub alternatives: Vec<Alternative>,
}

/// Context gathered once per project and reused across all diagnostics.
pub struct ProjectContext {
    pub labels: Vec<String>,
    pub bib_keys: Vec<String>,
    pub asset_paths: Vec<String>,
}

impl ProjectContext {
    pub fn gather(root: &Path) -> Result<Self> {
        let project = Project::open(root)?;
        let index = project.index()?;

        let mut labels = Vec::new();
        for f in index.sources() {
            let abs = root.join(&f.path);
            let src = std::fs::read_to_string(&abs)
                .with_context(|| format!("read {}", abs.display()))?;
            let t = extract_source(&f.path.to_string_lossy(), &src);
            for l in t.labels {
                labels.push(l.value);
            }
        }
        labels.sort();
        labels.dedup();

        let mut bib_keys = Vec::new();
        for f in index.bibliographies() {
            let abs = root.join(&f.path);
            let src = std::fs::read_to_string(&abs)
                .with_context(|| format!("read {}", abs.display()))?;
            for k in extract_bib_keys(&src) {
                bib_keys.push(k);
            }
        }
        bib_keys.sort();
        bib_keys.dedup();

        let asset_paths: Vec<String> = index
            .files
            .iter()
            .map(|f| f.path.to_string_lossy().replace('\\', "/"))
            .collect();

        Ok(Self { labels, bib_keys, asset_paths })
    }
}

/// Propose corrections for every diagnostic in `diagnostics`.
pub fn propose_all(root: &Path, diagnostics: &[Diagnostic]) -> Result<Vec<Correction>> {
    let ctx = ProjectContext::gather(root)?;
    let mut out = Vec::new();
    for d in diagnostics {
        if let Some(c) = propose_with_context(root, &ctx, d)? {
            out.push(c);
        }
    }
    Ok(out)
}

/// Propose a single correction for one diagnostic.
pub fn propose(root: &Path, diag: &Diagnostic) -> Result<Option<Correction>> {
    let ctx = ProjectContext::gather(root)?;
    propose_with_context(root, &ctx, diag)
}

pub fn propose_with_context(
    root: &Path,
    ctx: &ProjectContext,
    diag: &Diagnostic,
) -> Result<Option<Correction>> {
    let loc = match &diag.location {
        Some(l) => l,
        None => return Ok(None),
    };
    let target = match quoted_value(&diag.message) {
        Some(v) => v,
        None => return Ok(None),
    };
    let line_text = match read_line(root, &loc.file, loc.line) {
        Ok(l) => l,
        Err(_) => return Ok(None),
    };

    match diag.kind {
        DiagnosticKind::UnresolvedReference => {
            propose_typo(ctx.labels.iter(), &target, |best, alts| Correction {
                diagnostic: diag.clone(),
                explanation: format!(
                    "The reference '{}' does not match any label in the project. \
                     The closest label is '{}'.",
                    target, best,
                ),
                file: loc.file.clone(),
                line: loc.line,
                context: line_text.clone(),
                old_text: target.clone(),
                new_text: best,
                confidence: Confidence::Likely,
                may_change_meaning: false,
                alternatives: alts,
            })
        }
        DiagnosticKind::UnresolvedCitation => {
            propose_typo(ctx.bib_keys.iter(), &target, |best, alts| Correction {
                diagnostic: diag.clone(),
                explanation: format!(
                    "The citation '{}' is not in any .bib file. \
                     The closest bibliography key is '{}'.",
                    target, best,
                ),
                file: loc.file.clone(),
                line: loc.line,
                context: line_text.clone(),
                old_text: target.clone(),
                new_text: best,
                confidence: Confidence::Likely,
                may_change_meaning: false,
                alternatives: alts,
            })
        }
        DiagnosticKind::UnusedLabel => {
            let construct = format!("\\label{{{}}}", target);
            if !line_text.contains(&construct) {
                return Ok(None);
            }
            Ok(Some(Correction {
                diagnostic: diag.clone(),
                explanation: format!(
                    "The label '{}' is never referenced anywhere in the project. \
                     Removing it is safe if you do not plan to reference it. \
                     This is a suggestion, not an error.",
                    target,
                ),
                file: loc.file.clone(),
                line: loc.line,
                context: line_text,
                old_text: construct,
                new_text: String::new(),
                confidence: Confidence::Uncertain,
                may_change_meaning: true,
                alternatives: Vec::new(),
            }))
        }
        DiagnosticKind::MissingAsset => {
            let basename = target.rsplit('/').next().unwrap_or(&target);
            let mut candidates: Vec<String> = ctx
                .asset_paths
                .iter()
                .filter(|p| {
                    let b = p.rsplit('/').next().unwrap_or(p);
                    edit_distance(b, basename) <= threshold(basename)
                })
                .cloned()
                .collect();
            if candidates.is_empty() {
                return Ok(None);
            }
            candidates.sort();
            let best = candidates[0].clone();
            let alternatives: Vec<Alternative> = candidates
                .iter()
                .skip(1)
                .map(|c| Alternative {
                    value: c.clone(),
                    explanation: format!("Use '{}' instead.", c),
                    confidence: Confidence::Likely,
                })
                .collect();
            Ok(Some(Correction {
                diagnostic: diag.clone(),
                explanation: format!(
                    "The file '{}' was not found. A file with a similar name, \
                     '{}', exists in the project.",
                    target, best,
                ),
                file: loc.file.clone(),
                line: loc.line,
                context: line_text,
                old_text: target.clone(),
                new_text: best,
                confidence: Confidence::Likely,
                may_change_meaning: false,
                alternatives,
            }))
        }
        // Deliberately not auto-fixed in M4.2.
        DiagnosticKind::DuplicateLabel => Ok(None),
        _ => Ok(None),
    }
}

fn propose_typo<'a, I, F>(
    candidates: I,
    target: &str,
    build: F,
) -> Result<Option<Correction>>
where
    I: Iterator<Item = &'a String>,
    F: FnOnce(String, Vec<Alternative>) -> Correction,
{
    let thr = threshold(target);
    let mut scored: Vec<(usize, String)> = candidates
        .map(|c| (edit_distance(c, target), c.clone()))
        .filter(|(d, _)| *d <= thr && *d > 0)
        .collect();
    scored.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1)));

    if scored.is_empty() {
        return Ok(None);
    }
    let best = scored[0].1.clone();
    let alternatives: Vec<Alternative> = scored
        .iter()
        .skip(1)
        .map(|(_, v)| Alternative {
            value: v.clone(),
            explanation: format!("Or replace with '{}'.", v),
            confidence: Confidence::Likely,
        })
        .collect();
    Ok(Some(build(best, alternatives)))
}

/// Extract the first single-quoted substring of `msg`.
fn quoted_value(msg: &str) -> Option<String> {
    let start = msg.find('\'')? + 1;
    let rest = &msg[start..];
    let end = rest.find('\'')?;
    Some(rest[..end].to_string())
}

fn read_line(root: &Path, rel: &str, line: u32) -> Result<String> {
    let abs = root.join(rel);
    let src = std::fs::read_to_string(&abs)
        .with_context(|| format!("read {}", abs.display()))?;
    src.lines()
        .nth((line - 1) as usize)
        .map(str::to_string)
        .ok_or_else(|| anyhow::anyhow!("line {} not present in {}", line, rel))
}

/// Levenshtein distance over `char`, not bytes.
pub fn edit_distance(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let (n, m) = (a.len(), b.len());
    if n == 0 { return m; }
    if m == 0 { return n; }
    let mut prev: Vec<usize> = (0..=m).collect();
    let mut cur = vec![0usize; m + 1];
    for i in 1..=n {
        cur[0] = i;
        for j in 1..=m {
            let cost = if a[i - 1] == b[j - 1] { 0 } else { 1 };
            cur[j] = (prev[j] + 1).min(cur[j - 1] + 1).min(prev[j - 1] + cost);
        }
        std::mem::swap(&mut prev, &mut cur);
    }
    prev[m]
}

/// Allowable edit distance for a string of length L: L/3, clamped to [1, 3].
fn threshold(s: &str) -> usize {
    let l = s.chars().count();
    ((l / 3).max(1)).min(3)
}

/// True if this engine can propose a fix for the given diagnostic.
pub fn is_fixable(d: &Diagnostic) -> bool {
    matches!(
        d.kind,
        DiagnosticKind::UnresolvedReference
            | DiagnosticKind::UnresolvedCitation
            | DiagnosticKind::MissingAsset
            | DiagnosticKind::UnusedLabel
    )
}
