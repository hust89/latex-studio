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
        "\\documentclass{article}\n\\begin{document}\n\\label{fig:experiment}\nSee \\ref{fig:experiement}.\n\\end{document}\n",
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
    assert!(c.explanation.contains("closest label"), "{}", c.explanation);
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
fn no_proposal_when_nothing_is_close() {
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
    assert!(propose(r, diag).unwrap().is_none());
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
fn no_proposal_for_duplicate_label() {
    let diag = Diagnostic {
        severity: Severity::Error,
        kind: DiagnosticKind::DuplicateLabel,
        message: "Label 'sec:intro' is defined more than once".into(),
        explanation: None,
        location: Some(SourceLocation {
            file: "main.tex".into(),
            line: 1,
            column: None,
        }),
        raw: None,
    };
    let d = tempfile::tempdir().unwrap();
    assert!(propose(d.path(), &diag).unwrap().is_none());
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
fn corpus_proposals_are_sensible() {
    let mut p = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.pop(); // crates/
    p.pop(); // workspace root
    p.push("corpus/broken-references");
    assert!(p.is_dir(), "missing corpus: {}", p.display());

    let diags = verify_project(&p).unwrap();
    let proposals = propose_all(&p, &diags).unwrap();

    let deletes: Vec<_> = proposals.iter().filter(|c| c.new_text.is_empty()).collect();
    assert!(!deletes.is_empty(), "expected at least one deletion proposal: {:#?}", proposals);
    assert!(
        deletes.iter().any(|c| c.old_text == "\\label{sec:orphan}"),
        "expected sec:orphan deletion: {:#?}", deletes,
    );
}
