use latex_correct::{edit_distance, propose, propose_all, Confidence};
use latex_model::{Diagnostic, DiagnosticKind, Severity, SourceLocation};
use latex_verify::verify_project;
use std::path::Path;

fn write(p: &Path, s: &str) {
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(p, s).unwrap();
}

#[test]
fn edit_distance_basics() {
    assert_eq!(edit_distance("abc", "abc"), 0);
    assert_eq!(edit_distance("abc", "abd"), 1);
    assert_eq!(edit_distance("abc", "abcd"), 1);
    assert_eq!(edit_distance("", "abc"), 3);
}

#[test]
fn proposes_reference_typo_fix() {
    let d = tempfile::tempdir().unwrap();
    let r = d.path();
    write(
        &r.join("main.tex"),
        "\\documentclass{article}\n\\begin{document}\n\\label{fig:experiment}\nSee \\ref{fig:experiment} and \\ref{fig:experiement}.\n\\label{sec:orphan}\n\\end{document}\n",
    );
    let diags = verify_project(r).unwrap();
    let diag = diags
        .iter()
        .find(|d| d.kind == DiagnosticKind::UnresolvedReference)
        .expect("expected unresolved reference");
    let c = propose(r, diag).unwrap().expect("proposal");
    assert_eq!(c.old_text, "fig:experiement");
    assert_eq!(c.new_text, "fig:experiment");
    assert_eq!(c.confidence, Confidence::Likely);
    assert!(!c.may_change_meaning);
}

#[test]
fn proposes_citation_typo_fix() {
    let d = tempfile::tempdir().unwrap();
    let r = d.path();
    write(
        &r.join("main.tex"),
        "\\documentclass{article}\n\\begin{document}\nSee \\cite{knuth85}.\n\\end{document}\n",
    );
    write(&r.join("refs.bib"), "@article{knuth84, title={}}\n");
    let diags = verify_project(r).unwrap();
    let diag = diags
        .iter()
        .find(|d| d.kind == DiagnosticKind::UnresolvedCitation)
        .expect("expected unresolved citation");
    let c = propose(r, diag).unwrap().expect("proposal");
    assert_eq!(c.old_text, "knuth85");
    assert_eq!(c.new_text, "knuth84");
    assert_eq!(c.confidence, Confidence::Likely);
}

#[test]
fn comment_out_fallback_when_nothing_is_close() {
    let d = tempfile::tempdir().unwrap();
    let r = d.path();
    write(
        &r.join("main.tex"),
        "\\documentclass{article}\n\\begin{document}\n\\label{fig:experiment}\nSee \\ref{totally_unrelated_zzz}.\n\\end{document}\n",
    );
    let diags = verify_project(r).unwrap();
    let diag = diags
        .iter()
        .find(|d| d.kind == DiagnosticKind::UnresolvedReference)
        .expect("expected unresolved reference");
    let c = propose(r, diag)
        .unwrap()
        .expect("fallback should produce a proposal");
    assert!(c.new_text.starts_with("% "), "{}", c.new_text);
    assert_eq!(c.confidence, Confidence::Uncertain);
    assert!(c.may_change_meaning);
    assert_eq!(c.line, 4);
}

#[test]
fn proposes_deletion_of_unused_label() {
    let d = tempfile::tempdir().unwrap();
    let r = d.path();
    write(
        &r.join("main.tex"),
        "\\documentclass{article}\n\\begin{document}\n\\label{sec:orphan}\nHello.\n\\end{document}\n",
    );
    let diags = verify_project(r).unwrap();
    let diag = diags
        .iter()
        .find(|d| d.kind == DiagnosticKind::UnusedLabel)
        .expect("expected unused label");
    let c = propose(r, diag).unwrap().expect("proposal");
    assert_eq!(c.old_text, "\\label{sec:orphan}");
    assert_eq!(c.new_text, "");
    assert_eq!(c.confidence, Confidence::Uncertain);
    assert!(c.may_change_meaning);
}

#[test]
fn proposes_rename_for_duplicate_label() {
    let d = tempfile::tempdir().unwrap();
    let r = d.path();
    write(
        &r.join("main.tex"),
        "\\documentclass{article}\n\\begin{document}\n\\label{sec:intro}\nSee \\ref{sec:intro}.\n\\end{document}\n",
    );
    write(
        &r.join("chapter.tex"),
        "\\section{Intro}\n\\label{sec:intro}\n",
    );
    let diags = verify_project(r).unwrap();
    let diag = diags
        .iter()
        .find(|d| d.kind == DiagnosticKind::DuplicateLabel)
        .expect("expected duplicate label");
    let c = propose(r, diag).unwrap().expect("proposal");
    assert!(
        c.new_text.contains("sec:intro-2"),
        "expected rename, got: {}",
        c.new_text
    );
    assert_eq!(c.confidence, Confidence::Uncertain);
    assert!(c.may_change_meaning);
}

#[test]
fn proposes_deletion_of_unused_command() {
    let d = tempfile::tempdir().unwrap();
    let r = d.path();
    write(
        &r.join("main.tex"),
        "\\documentclass{article}\n\\newcommand{\\unusedcmd}{x}\n\\begin{document}\nHello.\n\\end{document}\n",
    );
    let diags = verify_project(r).unwrap();
    let diag = diags
        .iter()
        .find(|d| d.kind == DiagnosticKind::UnusedCommand)
        .expect("expected unused command");
    let c = propose(r, diag).unwrap().expect("proposal");
    assert_eq!(c.new_text, "");
    assert_eq!(c.confidence, Confidence::Uncertain);
    assert_eq!(c.line, 2);
}

#[test]
fn proposes_rename_for_undefined_command() {
    let d = tempfile::tempdir().unwrap();
    let r = d.path();
    write(
        &r.join("main.tex"),
        "\\documentclass{article}\n\\begin{document}\n$\\betta$\n\\end{document}\n",
    );
    let diags = verify_project(r).unwrap();
    let diag = diags
        .iter()
        .find(|d| d.kind == DiagnosticKind::UndefinedCommand)
        .expect("expected undefined command");
    let c = propose(r, diag).unwrap().expect("proposal");
    assert!(c.new_text.contains("\\beta"), "{}", c.new_text);
    assert!(!c.new_text.contains("\\betta"), "{}", c.new_text);
    assert_eq!(c.confidence, Confidence::Likely);
}

#[test]
fn comment_out_fallback_for_diagnostic_with_no_specific_handler() {
    // A Syntax diagnostic has no specific handler. It must still produce
    // a proposal (comment out the offending line).
    let d = tempfile::tempdir().unwrap();
    let r = d.path();
    write(
        &r.join("main.tex"),
        "\\documentclass{article}\n\\begin{document}\nthis line is broken\n\\end{document}\n",
    );
    let diag = Diagnostic {
        severity: Severity::Error,
        kind: DiagnosticKind::Syntax,
        message: "unexpected token".into(),
        explanation: None,
        location: Some(SourceLocation {
            file: "main.tex".into(),
            line: 3,
            column: None,
        }),
        raw: None,
    };
    let c = propose(r, &diag).unwrap().expect("fallback");
    assert!(c.new_text.starts_with("% "), "{}", c.new_text);
    assert_eq!(c.line, 3);
}

#[test]
fn no_proposal_without_location() {
    let diag = Diagnostic {
        severity: Severity::Error,
        kind: DiagnosticKind::UnresolvedReference,
        message: "Reference 'x' has no matching \\label".into(),
        explanation: None,
        location: None,
        raw: None,
    };
    let d = tempfile::tempdir().unwrap();
    assert!(propose(d.path(), &diag).unwrap().is_none());
}

#[test]
fn every_corpus_diagnostic_gets_a_proposal() {
    let mut p = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.pop();
    p.pop();
    p.push("corpus/broken-references");
    assert!(p.is_dir(), "missing corpus: {}", p.display());

    let diags = verify_project(&p).unwrap();
    let with_location: Vec<_> = diags.iter().filter(|d| d.location.is_some()).collect();
    assert!(!with_location.is_empty(), "no diagnostics with a location to test");

    let proposals = propose_all(&p, &diags).unwrap();
    assert_eq!(
        proposals.len(),
        with_location.len(),
        "expected one proposal per located diagnostic. \
         Located: {}, Proposed: {}, diags: {:#?}",
        with_location.len(),
        proposals.len(),
        diags
    );

    for c in &proposals {
        assert!(!c.context.is_empty(), "context should not be empty: {:#?}", c);
        assert!(!c.explanation.is_empty(), "explanation should not be empty: {:#?}", c);
    }
}

#[test]
fn every_diagnostic_in_a_clean_project_is_empty() {
    // Sanity check that the invariant "every located diagnostic has a
    // proposal" is not vacuously true because the validator is silent.
    let d = tempfile::tempdir().unwrap();
    let r = d.path();
    write(
        &r.join("main.tex"),
        "\\documentclass{article}\n\\begin{document}\nGood text.\n\\end{document}\n",
    );
    let diags = verify_project(r).unwrap();
    assert!(diags.is_empty(), "clean project had diagnostics: {:#?}", diags);
}
