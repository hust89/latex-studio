//! Run the validator against the checked-in corpus.
//! Fails if the corpus does not produce the diagnostics we expect.

use latex_model::{DiagnosticKind, Severity};
use latex_verify::verify_project;
use std::path::PathBuf;

fn corpus_dir(name: &str) -> PathBuf {
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.pop(); // crates/
    p.pop(); // workspace root
    p.join("corpus").join(name)
}

#[test]
fn broken_references_corpus_reports_expected() {
    let dir = corpus_dir("broken-references");
    assert!(dir.is_dir(), "missing corpus: {}", dir.display());
    let diags = verify_project(&dir).unwrap();

    let errors: Vec<_> = diags.iter().filter(|d| d.severity == Severity::Error).collect();

    assert!(
        errors.iter().any(|d| d.kind == DiagnosticKind::UnresolvedReference),
        "expected an unresolved reference: {:#?}", errors,
    );
    assert!(
        errors.iter().any(|d| d.kind == DiagnosticKind::UnresolvedCitation),
        "expected an unresolved citation: {:#?}", errors,
    );
    assert!(
        errors.iter().any(|d| d.kind == DiagnosticKind::MissingAsset),
        "expected a missing asset: {:#?}", errors,
    );
    assert!(
        errors.iter().any(|d| d.kind == DiagnosticKind::DuplicateLabel),
        "expected a duplicate label: {:#?}", errors,
    );
}
