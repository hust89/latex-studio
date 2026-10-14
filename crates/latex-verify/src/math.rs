//! Mathematical and notation consistency checks.
//!
//! Two directions of notation consistency, plus supporting extraction:
//!
//!   1. Declared but never used (`DiagnosticKind::UnusedCommand`, Info).
//!   2. Used but never declared (`DiagnosticKind::UndefinedCommand`, Warning).
//!
//! Why Warning, not Error, for the second: we cannot know what an arbitrary
//! `\usepackage` provides. A command name that is neither declared in the
//! project nor on the curated allowlist of builtins might still be provided
//! by a loaded package. Firing at Warning asks the user to check; firing at
//! Error would be a false alarm on any project that uses packages heavily.
//!
//! Not a TeX parser. Line-by-line scanning, comment stripping, and a small
//! state machine for verbatim environments. Multi-line command arguments are
//! not handled; the TeX convention of one directive per line is assumed.
//!
//! We do not claim proof verification. These are deterministic, conservative
//! checks.

use anyhow::{Context, Result};
use latex_model::{Diagnostic, DiagnosticKind, Severity, SourceLocation};
use latex_project::{FileIndex, FileKind};
use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::OnceLock;

use crate::strip_comment;

// ---------- data types -------------------------------------------------------

/// A mathematical symbol or command declaration found in LaTeX source.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SymbolDeclaration {
    pub name: String,
    pub file: String,
    pub line: u32,
    pub is_math_operator: bool,
    pub raw_decl: String,
}

/// A recorded usage of a symbol in LaTeX source.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SymbolUsage {
    pub name: String,
    pub file: String,
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

// ---------- builtin command allowlist ---------------------------------------
//
// Commands provided by LaTeX itself, the standard classes, amsmath, amssymb,
// graphicx, hyperref, siunitx, booktabs, and a handful of other widely-used
// packages. A use of one of these never triggers UndefinedCommand.
//
// The list is curated, not exhaustive. False positives from a package we do
// not know about are possible and are why the diagnostic is a Warning.

const BUILTIN_COMMANDS: &[&str] = &[
    // Document structure
    "documentclass", "usepackage", "RequirePackage", "begin", "end",
    "input", "include", "subfile", "import", "subimport", "includegraphics",

    // Sectioning
    "part", "chapter", "section", "subsection", "subsubsection",
    "paragraph", "subparagraph", "appendix", "frontmatter", "mainmatter",
    "backmatter",

    // References and links
    "label", "ref", "eqref", "pageref", "autoref", "cref", "Cref", "vref",
    "Vref", "nameref", "href", "url", "hyperref", "hypertarget", "hyperlink",

    // Bibliography
    "bibliography", "bibliographystyle", "addbibresource",
    "printbibliography", "nocite", "cite", "citep", "citet", "citealp",
    "citealt", "citeauthor", "citeyear", "autocite", "parencite", "textcite",
    "footcite", "smartcite", "supercite",

    // Text formatting
    "textbf", "textit", "texttt", "textrm", "textsf", "textsc", "textsl",
    "textup", "emph", "underline", "text", "mbox", "hbox", "fbox", "framebox",
    "parbox", "raisebox", "makebox",

    // Sizes
    "tiny", "scriptsize", "footnotesize", "small", "normalsize",
    "large", "Large", "LARGE", "huge", "Huge",

    // Font families and shapes
    "rmfamily", "sffamily", "ttfamily", "bfseries", "mdseries", "upshape",
    "itshape", "slshape", "scshape", "rm", "sf", "tt", "bf", "it", "sl",
    "sc", "em",

    // Spacing
    "hspace", "vspace", "hfill", "vfill", "quad", "qquad", "enspace",
    "thinspace", "negthinspace", "smallskip", "medskip", "bigskip",
    "noindent", "indent", "par",

    // Breaks and pages
    "newline", "linebreak", "nolinebreak", "newpage", "pagebreak",
    "nopagebreak", "clearpage", "cleardoublepage", "line",

    // Lists, tables, floats
    "item", "itemize", "enumerate", "description", "tabular", "tabularx",
    "table", "figure", "caption", "centering", "hline", "toprule", "midrule",
    "bottomrule", "cmidrule",

    // Greek lowercase
    "alpha", "beta", "gamma", "delta", "epsilon", "varepsilon", "zeta",
    "eta", "theta", "vartheta", "iota", "kappa", "lambda", "mu", "nu", "xi",
    "omicron", "pi", "varpi", "rho", "varrho", "sigma", "varsigma", "tau",
    "upsilon", "phi", "varphi", "chi", "psi", "omega",

    // Greek uppercase
    "Gamma", "Delta", "Theta", "Lambda", "Xi", "Pi", "Sigma", "Upsilon",
    "Phi", "Psi", "Omega",

    // Named math operators
    "sin", "cos", "tan", "cot", "sec", "csc", "arcsin", "arccos", "arctan",
    "sinh", "cosh", "tanh", "coth", "exp", "log", "ln", "lg", "lim",
    "liminf", "limsup", "sup", "inf", "max", "min", "arg", "deg", "det",
    "dim", "hom", "ker", "Pr", "gcd",

    // Math constructs
    "frac", "dfrac", "tfrac", "cfrac", "binom", "dbinom", "tbinom",
    "sqrt", "overline", "underline", "hat", "bar", "vec", "dot", "ddot",
    "tilde", "widetilde", "widehat", "overbrace", "underbrace", "stackrel",
    "overset", "underset", "mathop", "mathbin", "mathrel", "mathord",
    "mathpunct", "mathopen", "mathclose",

    // Large operators
    "sum", "prod", "coprod", "int", "iint", "iiint", "oint", "bigcup",
    "bigcap", "bigoplus", "bigotimes", "bigwedge", "bigvee", "bigsqcup",
    "bigodot",

    // Binary operators
    "times", "div", "cdot", "pm", "mp", "ast", "star", "circ", "bullet",
    "cap", "cup", "sqcap", "sqcup", "vee", "wedge", "setminus", "oplus",
    "otimes", "odot", "oslash",

    // Relations
    "le", "leq", "ge", "geq", "ne", "neq", "approx", "sim", "simeq",
    "cong", "equiv", "propto", "ll", "gg", "lesssim", "gtrsim",

    // Set relations
    "in", "notin", "ni", "subset", "supset", "subseteq", "supseteq",
    "sqsubset", "sqsupset", "sqsubseteq", "sqsupseteq", "emptyset",
    "varnothing", "forall", "exists", "neg", "lnot", "land", "lor",

    // Arrows
    "to", "rightarrow", "leftarrow", "Rightarrow", "Leftarrow",
    "Leftrightarrow", "leftrightarrow", "uparrow", "downarrow", "mapsto",
    "longrightarrow", "longleftarrow", "Longrightarrow", "Longleftarrow",
    "Longleftrightarrow",

    // Symbols
    "infty", "partial", "nabla", "ell", "hbar", "imath", "jmath", "Re",
    "Im", "wp", "aleph", "beth", "gimel", "daleth",

    // Delimiters
    "left", "right", "langle", "rangle", "lfloor", "rfloor", "lceil",
    "rceil", "lvert", "rvert", "lVert", "rVert", "vert", "Vert",
    "lbrace", "rbrace",

    // Math fonts
    "mathbb", "mathrm", "mathbf", "mathcal", "mathit", "mathsf", "mathtt",
    "mathfrak", "mathnormal", "boldsymbol", "bm", "operatorname",

    // Math misc
    "ensuremath", "displaystyle", "textstyle", "scriptstyle",
    "scriptscriptstyle", "limits", "nolimits", "nonumber", "notag", "tag",

    // amsmath environments and helpers
    "align", "aligned", "alignat", "alignedat", "gather", "gathered",
    "split", "multline", "cases", "dcases", "matrix", "pmatrix", "bmatrix",
    "Bmatrix", "vmatrix", "Vmatrix", "subarray", "smallmatrix", "intertext",
    "shortintertext", "numberwithin",

    // siunitx
    "SI", "si", "num", "qty", "unit", "complexnum", "complexqty", "numlist",
    "numrange", "SIlist", "SIrange", "ang", "degree", "percent", "ohm",
    "siemens", "meter", "metre", "second", "kilogram", "gram", "kelvin",
    "mol", "candela", "ampere", "hertz", "newton", "pascal", "joule",
    "watt", "coulomb", "volt", "farad", "weber", "tesla", "henry", "lumen",
    "lux", "becquerel", "gray", "sievert", "katal",

    // Macro definitions
    "newcommand", "renewcommand", "providecommand", "DeclareRobustCommand",
    "newenvironment", "renewenvironment", "def", "let", "newtheorem",
    "theoremstyle", "newcounter", "setcounter", "addtocounter", "value",
    "arabic", "roman", "Roman", "alph", "Alph", "fnsymbol",

    // Miscellaneous text
    "today", "LaTeX", "TeX", "copyright", "textbackslash", "textbar",
    "textless", "textgreater", "textasciitilde", "textasciicircum",
    "textendash", "textemdash", "textquotedblleft", "textquotedblright",
    "ldots", "dots", "cdots", "vdots", "ddots", "dotsc", "dotsb",

    // Common environment names used with \begin{...} as a control sequence arg
    "document",
];

fn builtin_set() -> &'static HashSet<&'static str> {
    static SET: OnceLock<HashSet<&'static str>> = OnceLock::new();
    SET.get_or_init(|| BUILTIN_COMMANDS.iter().copied().collect())
}

// ---------- declaration extraction -------------------------------------------

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
                if after_cmd < bytes.len() && bytes[after_cmd].is_ascii_alphabetic() {
                    continue;
                }

                let mut j = after_cmd;
                if j < bytes.len() && bytes[j] == b'*' {
                    j += 1;
                }
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

/// Find all uses of `symbol` in `src`, ignoring the definition itself
/// if `decl` names this file and line.
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
        let decl_range = if is_decl_line {
            decl.and_then(|d| {
                line.find(&d.raw_decl)
                    .map(|start| (start, start + d.raw_decl.len()))
            })
        } else {
            None
        };

        let mut i = 0usize;
        while i < bytes.len() {
            if bytes[i] != b'\\' {
                i += 1;
                continue;
            }
            let mut backslashes = 0usize;
            let mut b_idx = i;
            while b_idx > 0 && bytes[b_idx - 1] == b'\\' {
                backslashes += 1;
                b_idx -= 1;
            }
            if backslashes % 2 == 1 {
                i += 1;
                continue;
            }
            let after = i + 1 + sym_bytes.len();
            if after > bytes.len() {
                i += 1;
                continue;
            }
            if &bytes[i + 1..after] == sym_bytes {
                if after < bytes.len() && bytes[after].is_ascii_alphabetic() {
                    i += 1;
                    continue;
                }
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

// ---------- used-command extraction ------------------------------------------

/// Extract every distinct command name used in `src`, in the order of first
/// appearance. Skips comments, verbatim environments, and `\verb` bodies.
///
/// A "command name" is a backslash followed by one or more ASCII letters.
/// Control symbols (`\%`, `\\`, `\,`) are ignored.
pub fn extract_used_commands(rel_file: &str, src: &str) -> Vec<SymbolUsage> {
    let mut out = Vec::new();
    let mut in_verbatim = false;

    for (idx, raw_line) in src.lines().enumerate() {
        let line_no = (idx + 1) as u32;
        let trimmed = raw_line.trim_start();

        if !in_verbatim
            && (trimmed.starts_with("\\begin{verbatim}")
                || trimmed.starts_with("\\begin{verbatim*}")
                || trimmed.starts_with("\\begin{lstlisting}")
                || trimmed.starts_with("\\begin{minted}"))
        {
            in_verbatim = true;
            continue;
        }
        if in_verbatim {
            if trimmed.starts_with("\\end{verbatim}")
                || trimmed.starts_with("\\end{verbatim*}")
                || trimmed.starts_with("\\end{lstlisting}")
                || trimmed.starts_with("\\end{minted}")
            {
                in_verbatim = false;
            }
            continue;
        }

        let line = strip_comment(raw_line);
        let bytes = line.as_bytes();
        let mut i = 0usize;

        while i < bytes.len() {
            if bytes[i] != b'\\' {
                i += 1;
                continue;
            }
            // \verb and \verb* — skip everything up to the closing delimiter.
            if line[i + 1..].starts_with("verb") {
                let after = i + 5;
                if after < bytes.len() {
                    let c = bytes[after];
                    let delim = if c == b'*' { after + 1 } else { after };
                    if delim < bytes.len() && !bytes[delim].is_ascii_alphabetic() {
                        let d = bytes[delim];
                        let mut j = delim + 1;
                        while j < bytes.len() && bytes[j] != d {
                            j += 1;
                        }
                        i = j + 1;
                        continue;
                    }
                }
            }
            let start = i + 1;
            let mut k = start;
            while k < bytes.len() && bytes[k].is_ascii_alphabetic() {
                k += 1;
            }
            if k == start {
                // Control symbol; skip both chars.
                i += 2;
                continue;
            }
            out.push(SymbolUsage {
                name: line[start..k].to_string(),
                file: rel_file.to_string(),
                line: line_no,
            });
            i = k;
        }
    }
    out
}

// ---------- analysis ---------------------------------------------------------

pub fn analyze_notation_sources(sources: &[(&str, &str)]) -> Vec<NotationReport> {
    let mut declarations = Vec::new();
    for &(rel_file, src) in sources {
        declarations.extend(extract_declarations(rel_file, src));
    }

    let mut reports = Vec::new();
    for decl in declarations {
        let mut uses = Vec::new();
        for &(rel_file, src) in sources {
            uses.extend(find_usages_in_source(
                &decl.name,
                rel_file,
                src,
                Some(&decl),
            ));
        }
        reports.push(NotationReport { declaration: decl, uses });
    }
    reports
}

// ---------- checks -----------------------------------------------------------

/// Check notation consistency across in-memory source files.
///
/// Returns:
///   - `UnusedCommand` (Info) for commands declared but never used
///   - `UndefinedCommand` (Warning) for commands used but not declared and
///     not on the builtin allowlist
pub fn check_notation_sources(sources: &[(&str, &str)]) -> Vec<Diagnostic> {
    let mut out = Vec::new();

    // --- Direction 1: declared but unused.
    let reports = analyze_notation_sources(sources);
    let mut declared_names: HashSet<String> = HashSet::new();
    let mut decl_positions: HashSet<(String, u32, String)> = HashSet::new();
    for report in &reports {
        declared_names.insert(report.declaration.name.clone());
        decl_positions.insert((
            report.declaration.file.clone(),
            report.declaration.line,
            report.declaration.name.clone(),
        ));
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

    // --- Direction 2: used but undeclared.
    let builtins = builtin_set();
    let mut first_use: HashMap<String, SymbolUsage> = HashMap::new();
    for &(rel_file, src) in sources {
        for u in extract_used_commands(rel_file, src) {
            // Declaration occurrences are not uses.
            if decl_positions.contains(&(u.file.clone(), u.line, u.name.clone())) {
                continue;
            }
            // Builtins are provided by LaTeX and its standard packages.
            if builtins.contains(u.name.as_str()) {
                continue;
            }
            // Declared somewhere in the project.
            if declared_names.contains(&u.name) {
                continue;
            }
            first_use.entry(u.name.clone()).or_insert(u);
        }
    }

    // Stable output order: by first-use file, then line, then name.
    let mut undeclared: Vec<SymbolUsage> = first_use.into_values().collect();
    undeclared.sort_by(|a, b| {
        a.file
            .cmp(&b.file)
            .then_with(|| a.line.cmp(&b.line))
            .then_with(|| a.name.cmp(&b.name))
    });
    for u in undeclared {
        out.push(Diagnostic {
            severity: Severity::Warning,
            kind: DiagnosticKind::UndefinedCommand,
            message: format!(
                "Command '\\{}' is used but not declared anywhere in this project",
                u.name
            ),
            explanation: Some(
                "It may be a typo, or it may be provided by a package loaded \
                 with \\usepackage. If it is your own macro, add \
                 \\newcommand{\\"
                    .to_string()
                    + &u.name
                    + "}{...}.",
            ),
            location: Some(SourceLocation {
                file: u.file,
                line: u.line,
                column: None,
            }),
            raw: None,
        });
    }

    out
}

// ---------- project-level entry points --------------------------------------

/// Read all source and style files from the project.
fn read_all_sources(
    root: &Path,
    index: &FileIndex,
) -> Result<Vec<(String, String)>> {
    let mut sources: Vec<(String, String)> = Vec::new();
    for f in &index.files {
        if f.kind != FileKind::Source && f.kind != FileKind::Style {
            continue;
        }
        let abs = root.join(&f.path);
        let src = std::fs::read_to_string(&abs)
            .with_context(|| format!("read {}", abs.display()))?;
        sources.push((f.path.to_string_lossy().into_owned(), src));
    }
    Ok(sources)
}

pub fn analyze_notation(root: &Path, index: &FileIndex) -> Result<Vec<NotationReport>> {
    let sources = read_all_sources(root, index)?;
    let refs: Vec<(&str, &str)> = sources
        .iter()
        .map(|(p, s)| (p.as_str(), s.as_str()))
        .collect();
    Ok(analyze_notation_sources(&refs))
}

pub fn check_notation(root: &Path, index: &FileIndex) -> Result<Vec<Diagnostic>> {
    let sources = read_all_sources(root, index)?;
    let refs: Vec<(&str, &str)> = sources
        .iter()
        .map(|(p, s)| (p.as_str(), s.as_str()))
        .collect();
    Ok(check_notation_sources(&refs))
}

// ---------- tests ------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -------- declaration extraction --------

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

    // -------- direction 1: declared but unused --------

    #[test]
    fn unused_command_fires_diagnostic() {
        let src = r#"\documentclass{article}
\newcommand{\unusedcmd}{\alpha}
\begin{document}
Hello world.
\end{document}
"#;
        let diags = check_notation_sources(&[("main.tex", src)]);
        let unused: Vec<_> = diags
            .iter()
            .filter(|d| d.kind == DiagnosticKind::UnusedCommand)
            .collect();
        assert_eq!(unused.len(), 1);
        assert_eq!(unused[0].severity, Severity::Info);
        assert_eq!(unused[0].message, "Command '\\unusedcmd' is declared but never used");
        assert_eq!(unused[0].location.as_ref().unwrap().line, 2);
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
        // \foobar is a different command from \foo; its use must not
        // count as a use of \foo. It does fire the used-undeclared check,
        // which is asserted separately.
        let src = r#"\documentclass{article}
\newcommand{\foo}{x}
\begin{document}
\foobar
\end{document}
"#;
        let diags = check_notation_sources(&[("main.tex", src)]);
        let unused: Vec<_> = diags
            .iter()
            .filter(|d| d.kind == DiagnosticKind::UnusedCommand)
            .collect();
        assert_eq!(unused.len(), 1);
        assert_eq!(unused[0].message, "Command '\\foo' is declared but never used");
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
        let unused: Vec<_> = diags
            .iter()
            .filter(|d| d.kind == DiagnosticKind::UnusedCommand)
            .collect();
        assert_eq!(unused.len(), 1);
        assert_eq!(unused[0].message, "Command '\\foo' is declared but never used");
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
        let unused: Vec<_> = diags
            .iter()
            .filter(|d| d.kind == DiagnosticKind::UnusedCommand)
            .collect();
        assert_eq!(unused.len(), 1);
        assert_eq!(unused[0].message, "Command '\\argmax' is declared but never used");
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
        let unused: Vec<_> = diags
            .iter()
            .filter(|d| d.kind == DiagnosticKind::UnusedCommand)
            .collect();
        assert!(unused.is_empty(), "both macros are used: {:#?}", unused);
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
        let unused: Vec<_> = diags
            .iter()
            .filter(|d| d.kind == DiagnosticKind::UnusedCommand)
            .collect();
        assert!(unused.is_empty(), "sharedcmd is used: {:#?}", unused);
    }

    // -------- direction 2: used but undeclared --------

    #[test]
    fn used_undeclared_command_fires_warning() {
        let src = r#"\documentclass{article}
\begin{document}
$\myvec{v}$
\end{document}
"#;
        let diags = check_notation_sources(&[("main.tex", src)]);
        let warns: Vec<_> = diags
            .iter()
            .filter(|d| d.kind == DiagnosticKind::UndefinedCommand)
            .collect();
        assert_eq!(warns.len(), 1, "{:#?}", diags);
        let w = warns[0];
        assert_eq!(w.severity, Severity::Warning);
        assert!(w.message.contains("\\myvec"), "{}", w.message);
        assert_eq!(w.location.as_ref().unwrap().line, 3);
    }

    #[test]
    fn declared_used_command_does_not_fire_warning() {
        let src = r#"\documentclass{article}
\newcommand{\myvec}[1]{\mathbf{#1}}
\begin{document}
$\myvec{v}$
\end{document}
"#;
        let diags = check_notation_sources(&[("main.tex", src)]);
        let warns: Vec<_> = diags
            .iter()
            .filter(|d| d.kind == DiagnosticKind::UndefinedCommand)
            .collect();
        assert!(warns.is_empty(), "unexpected: {:#?}", warns);
    }

    #[test]
    fn builtin_commands_do_not_fire() {
        let src = r#"\documentclass{article}
\begin{document}
$\alpha \beta \sum_{i=1}^{n} \frac{a}{b} \le \infty$
\end{document}
"#;
        let diags = check_notation_sources(&[("main.tex", src)]);
        let warns: Vec<_> = diags
            .iter()
            .filter(|d| d.kind == DiagnosticKind::UndefinedCommand)
            .collect();
        assert!(warns.is_empty(), "unexpected: {:#?}", warns);
    }

    #[test]
    fn commands_inside_verb_are_ignored() {
        let src = r#"\documentclass{article}
\begin{document}
\verb|\notacommand|
\end{document}
"#;
        let diags = check_notation_sources(&[("main.tex", src)]);
        let warns: Vec<_> = diags
            .iter()
            .filter(|d| d.kind == DiagnosticKind::UndefinedCommand)
            .collect();
        assert!(warns.is_empty(), "unexpected: {:#?}", warns);
    }

    #[test]
    fn commands_inside_verbatim_environment_are_ignored() {
        let src = r#"\documentclass{article}
\begin{document}
\begin{verbatim}
\notacommand either
\end{verbatim}
\end{document}
"#;
        let diags = check_notation_sources(&[("main.tex", src)]);
        let warns: Vec<_> = diags
            .iter()
            .filter(|d| d.kind == DiagnosticKind::UndefinedCommand)
            .collect();
        assert!(warns.is_empty(), "unexpected: {:#?}", warns);
    }

    #[test]
    fn declared_in_one_file_used_in_another_does_not_fire_warning() {
        let macros = "\\newcommand{\\myvec}[1]{\\mathbf{#1}}\n";
        let main = r#"\documentclass{article}
\begin{document}
$\myvec{v}$
\end{document}
"#;
        let diags =
            check_notation_sources(&[("macros.tex", macros), ("main.tex", main)]);
        let warns: Vec<_> = diags
            .iter()
            .filter(|d| d.kind == DiagnosticKind::UndefinedCommand)
            .collect();
        assert!(warns.is_empty(), "unexpected: {:#?}", warns);
    }

    #[test]
    fn control_symbols_are_not_commands() {
        // \%, \&, \\ and \, must not be extracted as command names.
        let src = r#"\documentclass{article}
\begin{document}
100\% \& done \\ next
$\alpha\, \beta$
\end{document}
"#;
        let diags = check_notation_sources(&[("main.tex", src)]);
        let warns: Vec<_> = diags
            .iter()
            .filter(|d| d.kind == DiagnosticKind::UndefinedCommand)
            .collect();
        assert!(warns.is_empty(), "unexpected: {:#?}", warns);
    }
}
