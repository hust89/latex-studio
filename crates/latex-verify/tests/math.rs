use latex_model::{DiagnosticKind, Severity};
use latex_verify::verify_project;
use std::path::Path;

fn write(p: &Path, s: &str) {
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(p, s).unwrap();
}

#[test]
fn project_with_unused_command_fires_diagnostic() {
    let d = tempfile::tempdir().unwrap();
    let r = d.path();

    write(
        &r.join("main.tex"),
        r#"\documentclass{article}
\newcommand{\myunusedmacro}{\alpha}
\begin{document}
Hello world.
\end{document}
"#,
    );

    let diags = verify_project(r).unwrap();
    let unused: Vec<_> = diags
        .iter()
        .filter(|d| d.kind == DiagnosticKind::UnusedCommand)
        .collect();

    assert_eq!(unused.len(), 1, "expected 1 unused command, got: {:#?}", diags);
    assert_eq!(unused[0].severity, Severity::Info);
    assert!(unused[0].message.contains("\\myunusedmacro"));
    let loc = unused[0].location.as_ref().unwrap();
    assert_eq!(loc.file, "main.tex");
    assert_eq!(loc.line, 2);
}

#[test]
fn project_with_used_command_does_not_fire() {
    let d = tempfile::tempdir().unwrap();
    let r = d.path();

    write(
        &r.join("main.tex"),
        r#"\documentclass{article}
\newcommand{\myusedmacro}{\alpha}
\begin{document}
In math mode: $\myusedmacro$.
\end{document}
"#,
    );

    let diags = verify_project(r).unwrap();
    let unused: Vec<_> = diags
        .iter()
        .filter(|d| d.kind == DiagnosticKind::UnusedCommand)
        .collect();

    assert!(unused.is_empty(), "expected 0 unused commands, got: {:#?}", unused);
}

#[test]
fn project_with_declare_math_operator_unused_and_used() {
    let d = tempfile::tempdir().unwrap();
    let r = d.path();

    write(
        &r.join("main.tex"),
        r#"\documentclass{article}
\DeclareMathOperator{\Tr}{Tr}
\DeclareMathOperator*{\argmax}{arg\,max}
\begin{document}
The trace is $\Tr(A)$.
\end{document}
"#,
    );

    let diags = verify_project(r).unwrap();
    let unused: Vec<_> = diags
        .iter()
        .filter(|d| d.kind == DiagnosticKind::UnusedCommand)
        .collect();

    assert_eq!(unused.len(), 1, "expected 1 unused operator: {:#?}", unused);
    assert!(unused[0].message.contains("\\argmax"));
    assert_eq!(unused[0].location.as_ref().unwrap().line, 3);
}

#[test]
fn multifile_project_macro_used_across_files() {
    let d = tempfile::tempdir().unwrap();
    let r = d.path();

    write(
        &r.join("main.tex"),
        r#"\documentclass{article}
\input{macros}
\begin{document}
\input{chapter}
\end{document}
"#,
    );

    write(
        &r.join("macros.tex"),
        r#"\newcommand{\activemacro}{\mathbb{R}}
\newcommand{\deadmacro}{\mathbb{C}}
"#,
    );

    write(
        &r.join("chapter.tex"),
        r#"\section{Chapter}
Space is $\activemacro^n$.
"#,
    );

    let diags = verify_project(r).unwrap();
    let unused: Vec<_> = diags
        .iter()
        .filter(|d| d.kind == DiagnosticKind::UnusedCommand)
        .collect();

    assert_eq!(unused.len(), 1, "expected only deadmacro to be reported: {:#?}", unused);
    assert!(unused[0].message.contains("\\deadmacro"));
    let loc = unused[0].location.as_ref().unwrap();
    assert_eq!(loc.file, "macros.tex");
    assert_eq!(loc.line, 2);
}
