//! Mathematical and notation consistency checks.
//!
//! Checks that go beyond structural validation:
//!   - Notation consistency: symbols declared with `\newcommand` or
//!     `\DeclareMathOperator` that are never used in the project.
//!
//! Note on philosophy: we do not claim proof verification. We provide
//! deterministic, conservative checks for notation and consistency.

use anyhow::{Context, Result};
use latex_model::{Diagnostic, DiagnosticKind, Severity, SourceLocation};
use latex_project::FileIndex;
use std::path::Path;

use crate::strip_comment;

/// A mathematical symbol or command declaration found in LaTeX source.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SymbolDeclaration {
    /// Command name without leading backslash (e.g. "foo", "Tr").
    pub name: String,
    /// Relative path of the file containing the declaration.
    pub file: String,
    /// 1-based line number.
    pub line: u32,
    /// True if declared with `\DeclareMathOperator`.
    pub is_math_operator: bool,
    /// Exact declaration prefix on the line (e.g. `\newcommand{\foo}`).
    pub raw_decl: String,
}

/// A recorded usage of a symbol in LaTeX source.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SymbolUsage {
    /// Command name without leading backslash.
    pub name: String,
    /// Relative path of the file where the symbol was used.
    pub file: String,
    /// 1-based line number.
    pub line: u32,
}

/// Notation analysis report for a single declared symbol.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotationReport {
    pub declaration: SymbolDeclaration,
    pub uses: Vec<SymbolUsage>,
}

const DECL_COMMANDS: &[(&str, bool)] = &[
    ("newcommand", false),
    ("renewcommand", false),
    ("providecommand", false),
    ("DeclareMathOperator", true),
];

/// Extract all macro and operator declarations from a single file's source.
pub fn extract_declarations(rel_file: &str, src: &str) -> Vec<SymbolDeclaration> {
    let mut out = Vec::new();
    for (idx, raw_line) in src.lines().enumerate() {
        let line_no = (idx + 1) as u32;
        let line = strip_comment(raw_line);
        let bytes = line.as_bytes();
        let mut i = 0usize;

        while i < bytes.len() {
            if bytes[i] != b'\\' {
                i += 1;
                continue;
            }
            let mut matched = false;
            for &(cmd, is_math_op) in DECL_COMMANDS {
                let cb = cmd.as_bytes();
                let after_cmd = i + 1 + cb.len();
                if after_cmd > bytes.len() {
                    continue;
                }
                if &bytes[i + 1..after_cmd] != cb {
                    continue;
                }
                // Word boundary check: command must not be a prefix of a longer identifier,
                // except that an optional '*' may follow immediately.
                if after_cmd < bytes.len()
                    && bytes[after_cmd].is_ascii_alphabetic()
                {
                    continue;
                }

                let mut j = after_cmd;
                // Optional star `*`.
                if j < bytes.len() && bytes[j] == b'*' {
                    j += 1;
                }
                // Skip whitespace between command and argument.
                while j < bytes.len() && (bytes[j] == b' ' || bytes[j] == b'\t') {
                    j += 1;
                }

                if j >= bytes.len() {
                    continue;
                }

                let (sym_name, end_pos) = if bytes[j] == b'{' {
                    let mut k = j + 1;
                    while k < bytes.len() && (bytes[k] == b' ' || bytes[k] == b'\t') {
                        k += 1;
                    }
                    if k < bytes.len() && bytes[k] == b'\\' {
                        k += 1;
                    }
                    let sym_start = k;
                    while k < bytes.len() && bytes[k].is_ascii_alphabetic() {
                        k += 1;
                    }
                    let sym_end = k;
                    while k < bytes.len() && (bytes[k] == b' ' || bytes[k] == b'\t') {
                        k += 1;
                    }
                    if k < bytes.len() && bytes[k] == b'}' && sym_end > sym_start {
                        (line[sym_start..sym_end].to_string(), k + 1)
                    } else {
                        (String::new(), j)
                    }
                } else if bytes[j] == b'\\' {
                    let sym_start = j + 1;
                    let mut k = sym_start;
                    while k < bytes.len() && bytes[k].is_ascii_alphabetic() {
                        k += 1;
                    }
                    if k > sym_start {
                        (line[sym_start..k].to_string(), k)
                    } else {
                        (String::new(), j)
                    }
                } else {
                    (String::new(), j)
                };

                if !sym_name.is_empty() {
                    let raw_decl = line[i..end_pos].to_string();
                    out.push(SymbolDeclaration {
                        name: sym_name,
                        file: rel_file.to_string(),
                        line: line_no,
                        is_math_operator: is_math_op,
                        raw_decl,
                    });
                    i = end_pos;
                    matched = true;
                    break;
                }
            }
            if !matched {
                i += 1;
            }
        }
    }
    out
}

/// Find all usages of `symbol` in `src`.
///
/// If `decl` is provided and points to this file, occurrences within its
/// `raw_decl` span on that line are ignored (they are the definition, not a use).
pub fn find_usages_in_source(
    symbol: &str,
    rel_file: &str,
    src: &str,
    decl: Option<&SymbolDeclaration>,
) -> Vec<SymbolUsage> {
    let mut out = Vec::new();
    let sym_bytes = symbol.as_bytes();

    for (idx, raw_line) in src.lines().enumerate() {
        let line_no = (idx + 1) as u32;
        let line = strip_comment(raw_line);
        let bytes = line.as_bytes();

        let is_decl_line = match decl {
            Some(d) => d.file == rel_file && d.line == line_no,
            None => false,
        };

        // Determine the byte range of raw_decl on this line, if applicable.
        let decl_range = if is_decl_line {
            decl.and_then(|d| line.find(&d.raw_decl).map(|start| (start, start + d.raw_decl.len())))
        } else {
            None
        };

        let mut i = 0usize;
        while i < bytes.len() {
            if bytes[i] != b'\\' {
                i += 1;
                continue;
            }

            // Check how many backslashes precede bytes[i]. An odd count means
            // bytes[i] is an escaped backslash (e.g. `\\`).
            let mut backslash_count = 0usize;
            let mut b_idx = i;
            while b_idx > 0 && bytes[b_idx - 1] == b'\\' {
                backslash_count += 1;
                b_idx -= 1;
            }
            if backslash_count % 2 == 1 {
                i += 1;
                continue;
            }

            let after = i + 1 + sym_bytes.len();
            if after > bytes.len() {
                i += 1;
                continue;
            }

            if &bytes[i + 1..after] == sym_bytes {
                // Word boundary check: next char must not be ASCII alphabetic.
                if after < bytes.len() && bytes[after].is_ascii_alphabetic() {
                    i += 1;
                    continue;
                }

                // If this occurrence is inside the declaration itself, skip it.
                let in_decl = match decl_range {
                    Some((start, end)) => i >= start && i < end,
                    None => false,
                };

                if !in_decl {
                    out.push(SymbolUsage {
                        name: symbol.to_string(),
                        file: rel_file.to_string(),
                        line: line_no,
                    });
                }
                i = after;
            } else {
                i += 1;
            }
        }
    }
    out
}

/// Analyze notation usage across all provided source files.
pub fn analyze_notation_sources(sources: &[(&str, &str)]) -> Vec<NotationReport> {
    let mut declarations = Vec::new();
    for &(rel_file, src) in sources {
        declarations.extend(extract_declarations(rel_file, src));
    }

    let mut reports = Vec::new();
    for decl in declarations {
        let mut uses = Vec::new();
        for &(rel_file, src) in sources {
            uses.extend(find_usages_in_source(&decl.name, rel_file, src, Some(&decl)));
        }
        reports.push(NotationReport {
            declaration: decl,
            uses,
        });
    }
    reports
}

/// Check notation consistency across in-memory source files.
///
/// Returns diagnostics for symbols declared but never used.
pub fn check_notation_sources(sources: &[(&str, &str)]) -> Vec<Diagnostic> {
    let reports = analyze_notation_sources(sources);
    let mut out = Vec::new();

    for report in reports {
        if report.uses.is_empty() {
            let decl = &report.declaration;
            let cmd_type = if decl.is_math_operator {
                "\\DeclareMathOperator"
            } else {
                "\\newcommand"
            };
            out.push(Diagnostic {
                severity: Severity::Info,
                kind: DiagnosticKind::UnusedCommand,
                message: format!("Command '\\{}' is declared but never used", decl.name),
                explanation: Some(format!(
                    "Declared with {} at {}:{}",
                    cmd_type, decl.file, decl.line
                )),
                location: Some(SourceLocation {
                    file: decl.file.clone(),
                    line: decl.line,
                    column: None,
                }),
                raw: None,
            });
        }
    }

    out
}

/// Analyze notation usage across a project on disk.
pub fn analyze_notation(root: &Path, index: &FileIndex) -> Result<Vec<NotationReport>> {
    let mut sources = Vec::new();
    for f in index.sources() {
        let abs = root.join(&f.path);
        let src = std::fs::read_to_string(&abs)
            .with_context(|| format!("read {}", abs.display()))?;
        sources.push((f.path.to_string_lossy().into_owned(), src));
    }
    let refs: Vec<(&str, &str)> = sources
        .iter()
        .map(|(path, src)| (path.as_str(), src.as_str()))
        .collect();
    Ok(analyze_notation_sources(&refs))
}

/// Check notation consistency across a project on disk.
pub fn check_notation(root: &Path, index: &FileIndex) -> Result<Vec<Diagnostic>> {
    let mut sources = Vec::new();
    for f in index.sources() {
        let abs = root.join(&f.path);
        let src = std::fs::read_to_string(&abs)
            .with_context(|| format!("read {}", abs.display()))?;
        sources.push((f.path.to_string_lossy().into_owned(), src));
    }
    let refs: Vec<(&str, &str)> = sources
        .iter()
        .map(|(path, src)| (path.as_str(), src.as_str()))
        .collect();
    Ok(check_notation_sources(&refs))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_declarations_various_syntaxes() {
        let src = r#"
\newcommand{\R}{\mathbb{R}}
\newcommand*{\C}{\mathbb{C}}
\newcommand\myvec[1]{\mathbf{#1}}
\DeclareMathOperator{\Tr}{Tr}
\DeclareMathOperator*{\argmax}{arg\,max}
\renewcommand{\oldcmd}{new}
\providecommand{\maybe}{perhaps}
% \newcommand{\commentout}{x}
"#;
        let decls = extract_declarations("macros.tex", src);
        let names: Vec<&str> = decls.iter().map(|d| d.name.as_str()).collect();
        assert_eq!(
            names,
            vec!["R", "C", "myvec", "Tr", "argmax", "oldcmd", "maybe"]
        );
        assert!(decls[3].is_math_operator);
        assert!(decls[4].is_math_operator);
        assert!(!decls[0].is_math_operator);
    }

    #[test]
    fn unused_command_fires_diagnostic() {
        let src = r#"\documentclass{article}
\newcommand{\unusedcmd}{\alpha}
\begin{document}
Hello world.
\end{document}
"#;
        let diags = check_notation_sources(&[("main.tex", src)]);
        assert_eq!(diags.len(), 1);
        let d = &diags[0];
        assert_eq!(d.kind, DiagnosticKind::UnusedCommand);
        assert_eq!(d.severity, Severity::Info);
        assert_eq!(d.message, "Command '\\unusedcmd' is declared but never used");
        assert_eq!(d.location.as_ref().unwrap().line, 2);
    }

    #[test]
    fn used_command_does_not_fire() {
        let src = r#"\documentclass{article}
\newcommand{\usedcmd}{\alpha}
\begin{document}
In equation $\usedcmd + 1$.
\end{document}
"#;
        let diags = check_notation_sources(&[("main.tex", src)]);
        assert!(diags.is_empty(), "expected no diagnostics, got: {:#?}", diags);
    }

    #[test]
    fn prefix_name_does_not_mask_unused() {
        let src = r#"\documentclass{article}
\newcommand{\foo}{x}
\begin{document}
\foobar
\end{document}
"#;
        let diags = check_notation_sources(&[("main.tex", src)]);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].message, "Command '\\foo' is declared but never used");
    }

    #[test]
    fn commented_use_does_not_mask_unused() {
        let src = r#"\documentclass{article}
\newcommand{\foo}{x}
\begin{document}
% \foo was here
\end{document}
"#;
        let diags = check_notation_sources(&[("main.tex", src)]);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].message, "Command '\\foo' is declared but never used");
    }

    #[test]
    fn declared_math_operator_used_and_unused() {
        let src = r#"\documentclass{article}
\DeclareMathOperator{\Tr}{Tr}
\DeclareMathOperator*{\argmax}{arg\,max}
\begin{document}
$\Tr(A)$
\end{document}
"#;
        let diags = check_notation_sources(&[("main.tex", src)]);
        assert_eq!(diags.len(), 1);
        assert_eq!(
            diags[0].message,
            "Command '\\argmax' is declared but never used"
        );
    }

    #[test]
    fn macro_used_inside_another_macro() {
        let src = r#"\documentclass{article}
\newcommand{\inner}{\mathbb{R}}
\newcommand{\outer}{\inner^n}
\begin{document}
$\outer$
\end{document}
"#;
        let diags = check_notation_sources(&[("main.tex", src)]);
        assert!(diags.is_empty(), "both macros should be considered used: {:#?}", diags);
    }

    #[test]
    fn multi_file_usage_resolves() {
        let macros = r#"\newcommand{\sharedcmd}{\beta}"#;
        let chapter = r#"\section{Chapter}
See $\sharedcmd$."#;
        let diags = check_notation_sources(&[
            ("macros.tex", macros),
            ("chapter.tex", chapter),
        ]);
        assert!(diags.is_empty(), "sharedcmd used in chapter should not be flagged: {:#?}", diags);
    }
}
