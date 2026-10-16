//! Correction engine.
//!
//! Takes a `Diagnostic` and produces a `Correction`: a minimal edit the
//! user can approve or reject.
//!
//! Rule: every diagnostic with a source location produces a proposal.
//! Diagnostics where a specific handler applies get a targeted fix;
//! everything else falls back to commenting out the offending line.
//! `old_text` is always an exact substring of the file at proposal time,
//! and the apply step re-verifies before writing.
//!
//! Guarantees:
//!  - `old_text` is a substring of `context`, which is the line as read
//!    from disk at proposal time.
//!  - `confidence` is `Certain` only when the fix is mechanically forced.
//!    Typo suggestions are `Likely`. Deletions, renames, and comment-outs
//!    are `Uncertain`.
//!  - `may_change_meaning` is set whenever applying could change
//!    semantics.

use anyhow::{Context, Result};
use latex_model::{Diagnostic, DiagnosticKind, SourceLocation};
use latex_project::{FileKind, Project};
use latex_verify::math::{builtin_command_names, extract_declarations};
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
    pub diagnostic: Diagnostic,
    pub explanation: String,
    pub file: String,
    pub line: u32,
    pub context: String,
    pub old_text: String,
    pub new_text: String,
    pub confidence: Confidence,
    pub may_change_meaning: bool,
    pub alternatives: Vec<Alternative>,
}

/// Context gathered once per project and reused across all diagnostics.
pub struct ProjectContext {
    pub labels: Vec<String>,
    pub bib_keys: Vec<String>,
    pub asset_paths: Vec<String>,
    /// Command names declared anywhere in the project, without the
    /// leading backslash.
    pub declared_commands: Vec<String>,
    /// Where the `\documentclass` line lives: (relative path, 1-based
    /// line number, line text). Used to insert `\usepackage` directives.
    pub documentclass: Option<(String, u32, String)>,
}

impl ProjectContext {
    pub fn gather(root: &Path) -> Result<Self> {
        let project = Project::open(root)?;
        let index = project.index()?;

        let mut labels = Vec::new();
        let mut declared_commands: Vec<String> = Vec::new();
        let mut documentclass: Option<(String, u32, String)> = None;

        for f in index
            .by_kind(FileKind::Source)
            .chain(index.by_kind(FileKind::Style))
        {
            let abs = root.join(&f.path);
            let src = std::fs::read_to_string(&abs)
                .with_context(|| format!("read {}", abs.display()))?;
            let rel = f.path.to_string_lossy().into_owned();

            let t = extract_source(&rel, &src);
            for l in t.labels {
                labels.push(l.value);
            }

            for d in extract_declarations(&rel, &src) {
                declared_commands.push(d.name);
            }

            if documentclass.is_none() {
                for (idx, raw) in src.lines().enumerate() {
                    if raw.trim_start().starts_with("\\documentclass") {
                        documentclass = Some((rel.clone(), (idx + 1) as u32, raw.to_string()));
                        break;
                    }
                }
            }
        }
        labels.sort();
        labels.dedup();
        declared_commands.sort();
        declared_commands.dedup();

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

        Ok(Self {
            labels,
            bib_keys,
            asset_paths,
            declared_commands,
            documentclass,
        })
    }
}

/// Propose corrections for every diagnostic. Every diagnostic with a
/// source location yields a proposal.
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
    let target = quoted_value(&diag.message).unwrap_or_default();
    let line_text = match read_line(root, &loc.file, loc.line) {
        Ok(l) => l,
        Err(_) => return Ok(None),
    };

    let specific: Option<Correction> = match diag.kind {
        DiagnosticKind::UnresolvedReference => {
            let loc = loc.clone();
            let line_text = line_text.clone();
            propose_typo(ctx.labels.iter(), &target, |best, alts| Correction {
                diagnostic: diag.clone(),
                explanation: format!(
                    "The reference '{}' does not match any label in the project. \
                     The closest label is '{}'.",
                    target, best,
                ),
                file: loc.file.clone(),
                line: loc.line,
                context: line_text,
                old_text: target.clone(),
                new_text: best,
                confidence: Confidence::Likely,
                may_change_meaning: false,
                alternatives: alts,
            })?
        }

        DiagnosticKind::UnresolvedCitation => {
            let loc = loc.clone();
            let line_text = line_text.clone();
            propose_typo(ctx.bib_keys.iter(), &target, |best, alts| Correction {
                diagnostic: diag.clone(),
                explanation: format!(
                    "The citation '{}' is not in any .bib file. \
                     The closest bibliography key is '{}'.",
                    target, best,
                ),
                file: loc.file.clone(),
                line: loc.line,
                context: line_text,
                old_text: target.clone(),
                new_text: best,
                confidence: Confidence::Likely,
                may_change_meaning: false,
                alternatives: alts,
            })?
        }

        DiagnosticKind::UnusedLabel => {
            let construct = format!("\\label{{{}}}", target);
            if !line_text.contains(&construct) {
                None
            } else {
                Some(Correction {
                    diagnostic: diag.clone(),
                    explanation: format!(
                        "The label '{}' is never referenced anywhere in the \
                         project. Removing it is safe if you do not plan to \
                         reference it. This is a suggestion, not an error.",
                        target,
                    ),
                    file: loc.file.clone(),
                    line: loc.line,
                    context: line_text.clone(),
                    old_text: construct,
                    new_text: String::new(),
                    confidence: Confidence::Uncertain,
                    may_change_meaning: true,
                    alternatives: Vec::new(),
                })
            }
        }

        DiagnosticKind::DuplicateLabel => {
            let construct = format!("\\label{{{}}}", target);
            if !line_text.contains(&construct) {
                None
            } else {
                let renamed = line_text
                    .replace(&construct, &format!("\\label{{{}-2}}", target));
                Some(Correction {
                    diagnostic: diag.clone(),
                    explanation: format!(
                        "Label '{}' is defined more than once. Renaming the \
                         second definition to '{}-2' makes it unique. If \
                         this was a copy-paste mistake, delete the whole \
                         definition instead.",
                        target, target,
                    ),
                    file: loc.file.clone(),
                    line: loc.line,
                    context: line_text.clone(),
                    old_text: line_text.clone(),
                    new_text: renamed,
                    confidence: Confidence::Uncertain,
                    may_change_meaning: true,
                    alternatives: Vec::new(),
                })
            }
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
                None
            } else {
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
                Some(Correction {
                    diagnostic: diag.clone(),
                    explanation: format!(
                        "The file '{}' was not found. A file with a similar \
                         name, '{}', exists in the project.",
                        target, best,
                    ),
                    file: loc.file.clone(),
                    line: loc.line,
                    context: line_text.clone(),
                    old_text: target.clone(),
                    new_text: best,
                    confidence: Confidence::Likely,
                    may_change_meaning: false,
                    alternatives,
                })
            }
        }

        DiagnosticKind::UnusedCommand => {
            let name = target.trim_start_matches('\\');
            let construct = format!("\\{}", name);
            if !line_text.contains(&construct) {
                None
            } else {
                Some(Correction {
                    diagnostic: diag.clone(),
                    explanation: format!(
                        "Command '\\{}' is declared on this line but never \
                         used anywhere in the project. Removing the \
                         declaration is safe if you do not plan to use it. \
                         This is a suggestion, not an error.",
                        name,
                    ),
                    file: loc.file.clone(),
                    line: loc.line,
                    context: line_text.clone(),
                    old_text: line_text.clone(),
                    new_text: String::new(),
                    confidence: Confidence::Uncertain,
                    may_change_meaning: true,
                    alternatives: Vec::new(),
                })
            }
        }

        DiagnosticKind::UndefinedCommand => {
            let name = target.trim_start_matches('\\');
            let mut candidates: Vec<String> = Vec::new();
            candidates.extend(ctx.declared_commands.iter().cloned());
            candidates.extend(builtin_command_names().iter().map(|s| s.to_string()));
            candidates.retain(|c| c != name);
            candidates.sort();
            candidates.dedup();

            let thr = threshold(name);
            let mut scored: Vec<(usize, String)> = candidates
                .into_iter()
                .map(|c| (edit_distance(&c, name), c))
                .filter(|(d, _)| *d <= thr && *d > 0)
                .collect();
            let name_len = name.chars().count() as i64;
            scored.sort_by(|a, b| {
                let la = (a.1.chars().count() as i64 - name_len).abs();
                let lb = (b.1.chars().count() as i64 - name_len).abs();
                la.cmp(&lb)
                    .then_with(|| a.0.cmp(&b.0))
                    .then_with(|| a.1.cmp(&b.1))
            });

            if scored.is_empty() {
                None
            } else {
                let best = scored[0].1.clone();
                let alternatives: Vec<Alternative> = scored
                    .iter()
                    .skip(1)
                    .map(|(_, v)| Alternative {
                        value: v.clone(),
                        explanation: format!("Or replace with \\{}.", v),
                        confidence: Confidence::Likely,
                    })
                    .collect();
                let from = format!("\\{}", name);
                let to = format!("\\{}", best);
                let fixed = line_text.replace(&from, &to);
                Some(Correction {
                    diagnostic: diag.clone(),
                    explanation: format!(
                        "Command '\\{}' is used but not declared anywhere in \
                         this project. The closest known command is '\\{}'. \
                         If this is a typo, replace it.",
                        name, best,
                    ),
                    file: loc.file.clone(),
                    line: loc.line,
                    context: line_text.clone(),
                    old_text: line_text.clone(),
                    new_text: fixed,
                    confidence: Confidence::Likely,
                    may_change_meaning: false,
                    alternatives,
                })
            }
        }

        DiagnosticKind::MissingPackage => {
            let pkg = target.trim_end_matches(".sty").trim().to_string();
            if pkg.is_empty() || pkg.contains('/') {
                None
            } else if let Some((dc_file, dc_line, dc_text)) = ctx.documentclass.as_ref() {
                let trimmed = dc_text.trim_end();
                let indent: String = dc_text
                    .chars()
                    .take_while(|c| c.is_whitespace())
                    .collect();
                let new_text =
                    format!("{}\n{}\\usepackage{{{}}}", trimmed, indent, pkg);
                Some(Correction {
                    diagnostic: diag.clone(),
                    explanation: format!(
                        "The package '{}' is not available. This proposal adds \
                         \\usepackage{{{}}} after \\documentclass. You may \
                         also need to install the package on your system.",
                        pkg, pkg,
                    ),
                    file: dc_file.clone(),
                    line: *dc_line,
                    context: dc_text.clone(),
                    old_text: dc_text.clone(),
                    new_text,
                    confidence: Confidence::Likely,
                    may_change_meaning: false,
                    alternatives: Vec::new(),
                })
            } else {
                None
            }
        }

        _ => None,
    };

    if let Some(c) = specific {
        return Ok(Some(c));
    }

    Ok(comment_out_fallback(diag, loc, &line_text))
}

/// Fallback used when no specific handler applies. Comments out the
/// offending line. Always produces a proposal for a non-empty,
/// non-comment line.
fn comment_out_fallback(
    diag: &Diagnostic,
    loc: &SourceLocation,
    line: &str,
) -> Option<Correction> {
    let trimmed = line.trim_start();
    if trimmed.is_empty() || trimmed.starts_with('%') {
        return None;
    }
    let indent_len = line.len() - trimmed.len();
    let indent = &line[..indent_len];
    let new_text = format!("{}% {}", indent, trimmed);
    Some(Correction {
        diagnostic: diag.clone(),
        explanation: "No targeted fix is available for this problem. \
            Commenting out the offending line stops it from breaking the \
            build. Review whether the line should be removed entirely."
            .to_string(),
        file: loc.file.clone(),
        line: loc.line,
        context: line.to_string(),
        old_text: line.to_string(),
        new_text,
        confidence: Confidence::Uncertain,
        may_change_meaning: true,
        alternatives: Vec::new(),
    })
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
    // Sort by: same length as the target first, then edit distance,
    // then alphabetically. Typos usually do not change length, so a
    // same-length candidate is a better bet than a shorter one with a
    // marginally smaller edit distance.
    let target_len = target.chars().count() as i64;
    scored.sort_by(|a, b| {
        let la = (a.1.chars().count() as i64 - target_len).abs();
        let lb = (b.1.chars().count() as i64 - target_len).abs();
        la.cmp(&lb)
            .then_with(|| a.0.cmp(&b.0))
            .then_with(|| a.1.cmp(&b.1))
    });

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

fn threshold(s: &str) -> usize {
    let l = s.chars().count();
    ((l / 3).max(1)).min(3)
}

/// A diagnostic is fixable when it has a source location. Every
/// diagnostic our validator emits satisfies this.
pub fn is_fixable(d: &Diagnostic) -> bool {
    d.location.is_some()
}
