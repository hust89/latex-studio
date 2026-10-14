use latex_model::{DiagnosticKind, Severity};
use latex_verify::{
    extract_bib_keys, extract_source, scan_line, strip_comment, verify_project,
};
use std::path::Path;

fn write(p: &Path, s: &str) {
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(p, s).unwrap();
}

// ---------- scanner unit tests ----------------------------------------------

#[test]
fn strip_comment_simple() {
    assert_eq!(strip_comment("a % comment"), "a ");
}

#[test]
fn strip_comment_escaped_percent() {
    assert_eq!(strip_comment("a \\% b"), "a \\% b");
}

#[test]
fn scan_simple_label() {
    let v = scan_line("\\label{fig:x}");
    assert_eq!(v, vec![("label".into(), "fig:x".into())]);
}

#[test]
fn scan_graphics_with_options() {
    let v = scan_line("\\includegraphics[width=5cm]{fig.png}");
    assert_eq!(v, vec![("includegraphics".into(), "fig.png".into())]);
}

#[test]
fn scan_cite_with_multiple_keys() {
    let v = scan_line("\\cite{a,b,c}");
    assert_eq!(v, vec![("cite".into(), "a,b,c".into())]);
}

#[test]
fn scan_rejects_longer_identifiers() {
    // `\labelled` is not `\label`.
    assert!(scan_line("\\labelled{x}").is_empty());
}

#[test]
fn extract_source_collects_everything() {
    let src = "\
\\section{Intro}\\label{sec:intro}
See \\ref{fig:x} and \\eqref{eq:y}.
\\cite{knuth84,lamport94}
\\includegraphics[width=5cm]{plot.png}
% \\ref{commented} should be ignored
";
    let t = extract_source("main.tex", src);
    assert_eq!(t.labels.len(), 1);
    assert_eq!(t.labels[0].value, "sec:intro");
    assert_eq!(t.refs.len(), 2);
    assert_eq!(t.refs[0].value, "fig:x");
    assert_eq!(t.refs[1].value, "eq:y");
    assert_eq!(t.cites.len(), 2);
    assert_eq!(t.cites[0].value, "knuth84");
    assert_eq!(t.cites[1].value, "lamport94");
    assert_eq!(t.graphics.len(), 1);
    assert_eq!(t.graphics[0].value, "plot.png");
    // Line numbers are 1-based.
    assert_eq!(t.labels[0].line, 1);
    assert_eq!(t.refs[0].line, 2);
}

#[test]
fn extract_bib_keys_simple() {
    let src = "\
@article{knuth84,
  title = {The TeXbook}
}
@book(lamport94,
  title = {LaTeX}
)
@string{foo = \"bar\"}
";
    let keys = extract_bib_keys(src);
    assert_eq!(keys, vec!["knuth84", "lamport94"]);
}

// ---------- integration: a project with intentional errors ------------------

fn broken_project() -> tempfile::TempDir {
    let d = tempfile::tempdir().unwrap();
    let r = d.path();

    // main.tex: one good label, two bad references, one bad citation,
    // one missing asset, one duplicate label (also in chapter1).
    write(
        &r.join("main.tex"),
        r#"\documentclass{article}
\begin{document}
\section{One}\label{sec:one}
See \ref{sec:two} and \ref{sec:missing}.
See \cite{knuth84} and \cite{ghost99}.
\includegraphics[width=5cm]{figures/plot.png}
\includegraphics{figures/gone.pdf}
\input{chapter}
\end{document}
"#,
    );

    // chapter.tex: defines sec:two (good), duplicates sec:one (bad),
    // defines an unused label (info).
    write(
        &r.join("chapter.tex"),
        r#"\section{Two}\label{sec:two}
\section{One again}\label{sec:one}
\subsection{Never cited}\label{sec:unused}
"#,
    );

    // refs.bib: only knuth84 exists.
    write(
        &r.join("refs.bib"),
        r#"@article{knuth84, title={The TeXbook}}
@book{lamport94, title={LaTeX}}
"#,
    );

    // The one asset that exists. Content is arbitrary; only the path matters.
    std::fs::create_dir_all(r.join("figures")).unwrap();
    std::fs::write(r.join("figures/plot.png"), b"\x89PNG").unwrap();

    d
}

#[test]
fn broken_project_reports_expected_errors() {
    let d = broken_project();
    let diags = verify_project(d.path()).unwrap();

    let errors: Vec<_> = diags
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .collect();
    let infos: Vec<_> = diags
        .iter()
        .filter(|d| d.severity == Severity::Info)
        .collect();

    // Two bad refs on main.tex lines 4 and 4 — same line but two refs.
    let unresolved_refs: Vec<_> = errors
        .iter()
        .filter(|d| d.kind == DiagnosticKind::UnresolvedReference)
        .collect();
    assert_eq!(unresolved_refs.len(), 1, "expected one unresolved ref: {:#?}", errors);
    assert_eq!(unresolved_refs[0].location.as_ref().unwrap().line, 4);

    // One bad citation on main.tex line 5.
    let unresolved_cites: Vec<_> = errors
        .iter()
        .filter(|d| d.kind == DiagnosticKind::UnresolvedCitation)
        .collect();
    assert_eq!(unresolved_cites.len(), 1);
    assert_eq!(unresolved_cites[0].location.as_ref().unwrap().line, 5);

    // One missing asset on main.tex line 7.
    let missing_assets: Vec<_> = errors
        .iter()
        .filter(|d| d.kind == DiagnosticKind::MissingAsset)
        .collect();
    assert_eq!(missing_assets.len(), 1);
    assert_eq!(missing_assets[0].location.as_ref().unwrap().line, 7);

    // One duplicate label: sec:one appears twice; second occurrence is in chapter.tex line 2.
    let dupes: Vec<_> = errors
        .iter()
        .filter(|d| d.kind == DiagnosticKind::DuplicateLabel)
        .collect();
    assert_eq!(dupes.len(), 1);
    let dl = dupes[0].location.as_ref().unwrap();
    assert_eq!(dl.file, "chapter.tex");
    assert_eq!(dl.line, 2);

    // One unused label: sec:unused.
    let unused: Vec<_> = infos
        .iter()
        .filter(|d| d.kind == DiagnosticKind::UnusedLabel)
        .collect();
    assert!(unused.iter().any(|d| d.message.contains("sec:unused")), "{:#?}", unused);
}

#[test]
fn clean_project_reports_no_errors() {
    let d = tempfile::tempdir().unwrap();
    let r = d.path();
    write(
        &r.join("main.tex"),
        r#"\documentclass{article}
\begin{document}
\section{One}\label{sec:one}
See \ref{sec:one} and \cite{knuth84}.
\includegraphics{figures/plot.pdf}
\end{document}
"#,
    );
    write(&r.join("refs.bib"), "@article{knuth84, title={}}\n");
    write(&r.join("figures/plot.pdf"), "%PDF-1.4");

    let diags = verify_project(r).unwrap();
    let errors: Vec<_> = diags
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .collect();
    assert!(errors.is_empty(), "clean project had errors: {:#?}", errors);
}

#[test]
fn graphics_without_extension_resolves_with_known_ext() {
    let d = tempfile::tempdir().unwrap();
    let r = d.path();
    write(
        &r.join("main.tex"),
        r#"\documentclass{article}
\begin{document}
\includegraphics{figures/plot}
\end{document}
"#,
    );
    write(&r.join("figures/plot.pdf"), "%PDF-1.4");
    let diags = verify_project(r).unwrap();
    let errors: Vec<_> = diags
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .collect();
    assert!(errors.is_empty(), "unexpected: {:#?}", errors);
}
