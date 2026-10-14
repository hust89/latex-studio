//! Tests for latex_verify_project, latex_correct_propose, latex_correct_apply.

use std::ffi::{CStr, CString};
use std::os::raw::c_char;

use latex_ffi::{
    latex_correct_apply, latex_correct_propose, latex_free_string, latex_verify_project,
};

unsafe fn take(p: *mut c_char) -> String {
    assert!(!p.is_null());
    let s = CStr::from_ptr(p).to_string_lossy().into_owned();
    latex_free_string(p);
    s
}

fn c(s: &str) -> CString {
    CString::new(s).unwrap()
}

fn write(p: &std::path::Path, s: &str) {
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(p, s).unwrap();
}

/// Fixture:
///   - `fig:experiment` is defined and referenced (via the correct ref).
///   - `\ref{fig:experiement}` is a typo of it.
///   - `sec:orphan` is defined and never referenced.
fn fixture() -> tempfile::TempDir {
    let d = tempfile::tempdir().unwrap();
    write(
        &d.path().join("main.tex"),
        "\\documentclass{article}\n\
         \\begin{document}\n\
         \\label{fig:experiment}\n\
         See \\ref{fig:experiment} and \\ref{fig:experiement}.\n\
         \\label{sec:orphan}\n\
         \\end{document}\n",
    );
    d
}

#[test]
fn verify_null_errors() {
    let s = unsafe { take(latex_verify_project(std::ptr::null())) };
    assert!(s.contains("null pointer"), "{s}");
}

#[test]
fn verify_reports_the_two_problems() {
    let d = fixture();
    let root = c(d.path().to_str().unwrap());
    let s = unsafe { take(latex_verify_project(root.as_ptr())) };
    let v: serde_json::Value = serde_json::from_str(&s).unwrap();
    let diags = v["diagnostics"].as_array().unwrap();
    let kinds: Vec<&str> = diags
        .iter()
        .map(|d| d["kind"].as_str().unwrap())
        .collect();
    assert!(kinds.contains(&"UnresolvedReference"), "{kinds:?}");
    assert!(kinds.contains(&"UnusedLabel"), "{kinds:?}");
}

#[test]
fn propose_null_errors() {
    let s = unsafe { take(latex_correct_propose(std::ptr::null(), std::ptr::null())) };
    assert!(s.contains("null pointer"), "{s}");
}

#[test]
fn propose_from_diagnostics_json() {
    let d = fixture();
    let root = c(d.path().to_str().unwrap());

    let verify_json = unsafe { take(latex_verify_project(root.as_ptr())) };
    let v: serde_json::Value = serde_json::from_str(&verify_json).unwrap();
    let diags_json = v["diagnostics"].to_string();

    let diags_c = c(&diags_json);
    let propose_json = unsafe { take(latex_correct_propose(root.as_ptr(), diags_c.as_ptr())) };
    let p: serde_json::Value = serde_json::from_str(&propose_json).unwrap();
    let corrections = p["corrections"].as_array().unwrap();

    assert_eq!(corrections.len(), 2, "{corrections:#?}");

    let typo = corrections
        .iter()
        .find(|c| c["old_text"] == "fig:experiement")
        .expect("typo proposal");
    assert_eq!(typo["new_text"], "fig:experiment");

    let delete = corrections
        .iter()
        .find(|c| c["old_text"] == "\\label{sec:orphan}")
        .expect("delete proposal");
    assert_eq!(delete["new_text"], "");
}

#[test]
fn apply_null_errors() {
    let s = unsafe { take(latex_correct_apply(std::ptr::null(), std::ptr::null())) };
    assert!(s.contains("null pointer"), "{s}");
}

#[test]
fn apply_writes_when_context_matches() {
    let d = fixture();
    let root = c(d.path().to_str().unwrap());

    let corr = serde_json::json!({
        "diagnostic": {
            "severity": "Error",
            "kind": "UnresolvedReference",
            "message": "Reference 'fig:experiement' has no matching \\label",
            "explanation": null,
            "location": { "file": "main.tex", "line": 4, "column": null },
            "raw": null
        },
        "explanation": "typo",
        "file": "main.tex",
        "line": 4,
        "context": "See \\ref{fig:experiment} and \\ref{fig:experiement}.",
        "old_text": "fig:experiement",
        "new_text": "fig:experiment",
        "confidence": "Likely",
        "may_change_meaning": false,
        "alternatives": []
    })
    .to_string();
    let corr_c = c(&corr);

    let s = unsafe { take(latex_correct_apply(root.as_ptr(), corr_c.as_ptr())) };
    let v: serde_json::Value = serde_json::from_str(&s).unwrap();
    assert_eq!(v["ok"], true, "{s}");

    let after = std::fs::read_to_string(d.path().join("main.tex")).unwrap();
    // No more typo, and both refs now point at the correct label.
    assert!(!after.contains("fig:experiement"), "file: {after}");
    assert_eq!(after.matches("\\ref{fig:experiment}").count(), 2, "file: {after}");
}

#[test]
fn apply_refuses_when_old_text_missing() {
    let d = fixture();
    let root = c(d.path().to_str().unwrap());

    let corr = serde_json::json!({
        "diagnostic": {
            "severity": "Error",
            "kind": "UnresolvedReference",
            "message": "Reference 'never_there' has no matching \\label",
            "explanation": null,
            "location": { "file": "main.tex", "line": 4, "column": null },
            "raw": null
        },
        "explanation": "x",
        "file": "main.tex",
        "line": 4,
        "context": "See \\ref{never_there}.",
        "old_text": "never_there",
        "new_text": "fig:experiment",
        "confidence": "Likely",
        "may_change_meaning": false,
        "alternatives": []
    })
    .to_string();
    let corr_c = c(&corr);

    let s = unsafe { take(latex_correct_apply(root.as_ptr(), corr_c.as_ptr())) };
    let v: serde_json::Value = serde_json::from_str(&s).unwrap();
    assert_eq!(v["reason"], "missing", "{s}");

    let after = std::fs::read_to_string(d.path().join("main.tex")).unwrap();
    assert!(after.contains("fig:experiement"), "file: {after}");
}

#[test]
fn apply_refuses_when_old_text_ambiguous() {
    let d = tempfile::tempdir().unwrap();
    write(
        &d.path().join("main.tex"),
        "\\documentclass{article}\n\
         \\begin{document}\n\
         \\ref{fig:x} and \\ref{fig:x}.\n\
         \\end{document}\n",
    );
    let root = c(d.path().to_str().unwrap());

    let corr = serde_json::json!({
        "diagnostic": {
            "severity": "Error",
            "kind": "UnresolvedReference",
            "message": "Reference 'fig:x' has no matching \\label",
            "explanation": null,
            "location": { "file": "main.tex", "line": 3, "column": null },
            "raw": null
        },
        "explanation": "x",
        "file": "main.tex",
        "line": 3,
        "context": "\\ref{fig:x} and \\ref{fig:x}.",
        "old_text": "fig:x",
        "new_text": "fig:y",
        "confidence": "Likely",
        "may_change_meaning": false,
        "alternatives": []
    })
    .to_string();
    let corr_c = c(&corr);

    let s = unsafe { take(latex_correct_apply(root.as_ptr(), corr_c.as_ptr())) };
    let v: serde_json::Value = serde_json::from_str(&s).unwrap();
    assert_eq!(v["reason"], "ambiguous", "{s}");

    let after = std::fs::read_to_string(d.path().join("main.tex")).unwrap();
    assert_eq!(after.matches("fig:x").count(), 2);
}

#[test]
fn apply_rejects_bad_json() {
    let d = fixture();
    let root = c(d.path().to_str().unwrap());
    let bad = c("not json at all");

    let s = unsafe { take(latex_correct_apply(root.as_ptr(), bad.as_ptr())) };
    let v: serde_json::Value = serde_json::from_str(&s).unwrap();
    assert_eq!(v["reason"], "bad_input", "{s}");
}
