use latex_engine_tectonic::{parse_tex_log, parse_tectonic_stderr};
use latex_model::{DiagnosticKind, Severity};
use std::path::Path;

#[test]
fn parses_undefined_control_sequence_with_location() {
    let log = "This is pdfTeX...\n(./main.tex\n! Undefined control sequence.\nl.12 \\foo\n               bar\n)\n";
    let diags = parse_tex_log(log, Path::new("main.tex"));
    assert_eq!(diags.len(), 1);
    let d = &diags[0];
    assert_eq!(d.severity, Severity::Error);
    assert_eq!(d.kind, DiagnosticKind::UndefinedCommand);
    assert_eq!(d.location.as_ref().unwrap().line, 12);
}

#[test]
fn parses_latex_warning() {
    let log = "LaTeX Warning: Reference `fig:x' on page 1 undefined on input line 5.\n";
    let diags = parse_tex_log(log, Path::new("main.tex"));
    assert_eq!(diags.len(), 1);
    assert_eq!(diags[0].severity, Severity::Warning);
    assert_eq!(diags[0].kind, DiagnosticKind::UnresolvedReference);
}

#[test]
fn ignores_ordinary_log_lines() {
    let log = "This is pdfTeX, Version 3.141592653\nentering extended mode\n";
    assert!(parse_tex_log(log, Path::new("main.tex")).is_empty());
}

#[test]
fn does_not_merge_two_errors() {
    let log = "! First error.\nl.1 \\a\n! Second error.\nl.2 \\b\n";
    let diags = parse_tex_log(log, Path::new("main.tex"));
    assert_eq!(diags.len(), 2);
    assert_eq!(diags[0].location.as_ref().unwrap().line, 1);
    assert_eq!(diags[1].location.as_ref().unwrap().line, 2);
}

#[test]
fn parses_tectonic_modern_stderr_without_column() {
    let s = "error: main.tex:12: Undefined control sequence \\foo\n";
    let diags = parse_tectonic_stderr(s, Path::new("/proj/main.tex"));
    assert_eq!(diags.len(), 1);
    let d = &diags[0];
    assert_eq!(d.kind, DiagnosticKind::UndefinedCommand);
    let loc = d.location.as_ref().unwrap();
    assert_eq!(loc.line, 12);
    assert_eq!(loc.column, None);
    assert!(loc.file.ends_with("main.tex"));
}

#[test]
fn parses_tectonic_modern_stderr_with_column() {
    let s = "error: main.tex:3:7: expected '}'\n";
    let diags = parse_tectonic_stderr(s, Path::new("/proj/main.tex"));
    assert_eq!(diags.len(), 1);
    let loc = diags[0].location.as_ref().unwrap();
    assert_eq!(loc.line, 3);
    assert_eq!(loc.column, Some(7));
}

#[test]
fn tectonic_stderr_ignores_non_error_lines() {
    let s = "info: downloading bundle\nwarning: caching\n";
    assert!(parse_tectonic_stderr(s, Path::new("main.tex")).is_empty());
}
