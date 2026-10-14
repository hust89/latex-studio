//! Exercises the exported C ABI directly from Rust.

use std::ffi::{CStr, CString};
use std::os::raw::c_char;

use latex_ffi::{latex_compile, latex_free_string};

fn c(s: &str) -> CString { CString::new(s).unwrap() }

unsafe fn take_and_free(p: *mut c_char) -> String {
    assert!(!p.is_null(), "latex_compile returned NULL");
    let s = CStr::from_ptr(p).to_string_lossy().into_owned();
    latex_free_string(p);
    s
}

fn json_error_of(s: &str) -> Option<String> {
    let v: serde_json::Value = serde_json::from_str(s).ok()?;
    v.get("error").and_then(|e| e.as_str()).map(|s| s.to_string())
}

fn tectonic_available() -> bool {
    std::process::Command::new("tectonic")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn require_tectonic() -> bool {
    if tectonic_available() { return true; }
    if std::env::var("LATEX_REQUIRE_TECTONIC").is_ok() {
        panic!("LATEX_REQUIRE_TECTONIC set but `tectonic` not on PATH");
    }
    eprintln!("SKIPPED (tectonic not on PATH)");
    false
}

#[test]
fn null_pointers_return_error_not_crash() {
    let s = unsafe {
        take_and_free(latex_compile(
            std::ptr::null(),
            std::ptr::null(),
            std::ptr::null(),
            std::ptr::null(),
        ))
    };
    assert_eq!(json_error_of(&s).as_deref(), Some("null pointer argument"));
}

#[test]
fn empty_strings_return_error() {
    let empty = c("");
    let (e, r, o, g) = (empty.as_ptr(), empty.as_ptr(), empty.as_ptr(), empty.as_ptr());
    let s = unsafe { take_and_free(latex_compile(g, e, r, o)) };
    assert_eq!(json_error_of(&s).as_deref(), Some("empty string argument"));
}

#[test]
fn missing_entry_file_reports_error() {
    let dir = tempfile::tempdir().unwrap();
    let engine = c("tectonic");
    let e = c(dir.path().join("nope.tex").to_str().unwrap());
    let r = c(dir.path().to_str().unwrap());
    let o = c(dir.path().join("out").to_str().unwrap());
    let s = unsafe { take_and_free(latex_compile(engine.as_ptr(), e.as_ptr(), r.as_ptr(), o.as_ptr())) };
    assert!(
        json_error_of(&s).unwrap_or_default().contains("entry is not a file"),
        "unexpected: {s}"
    );
}

#[test]
fn entry_outside_project_root_rejected() {
    let a = tempfile::tempdir().unwrap();
    let b = tempfile::tempdir().unwrap();
    std::fs::write(a.path().join("main.tex"), "\\documentclass{article}").unwrap();
    let engine = c("tectonic");
    let e = c(a.path().join("main.tex").to_str().unwrap());
    let r = c(b.path().to_str().unwrap());
    let o = c(b.path().join("out").to_str().unwrap());
    let s = unsafe { take_and_free(latex_compile(engine.as_ptr(), e.as_ptr(), r.as_ptr(), o.as_ptr())) };
    assert!(
        json_error_of(&s).unwrap_or_default().contains("entry is not inside"),
        "unexpected: {s}"
    );
}

#[test]
fn real_compile_returns_valid_build_result_json() {
    if !require_tectonic() { return; }
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("main.tex"),
        "\\documentclass{article}\\begin{document}ok\\end{document}",
    ).unwrap();
    let engine = c("tectonic");
    let e = c(dir.path().join("main.tex").to_str().unwrap());
    let r = c(dir.path().to_str().unwrap());
    let o = c(dir.path().join("out").to_str().unwrap());

    std::env::set_var("LATEX_TECTONIC_NETWORK", "offline");

    let s = unsafe { take_and_free(latex_compile(engine.as_ptr(), e.as_ptr(), r.as_ptr(), o.as_ptr())) };
    let v: serde_json::Value = serde_json::from_str(&s)
        .unwrap_or_else(|e| panic!("invalid JSON from FFI: {e}\nraw: {s}"));

    assert_eq!(v["success"], serde_json::Value::Bool(true), "not success: {s}");
    let pdf = v["pdf_path"].as_str().expect("pdf_path");
    assert!(std::path::Path::new(pdf).is_file());
    assert!(v["build_id"].as_str().unwrap().starts_with("build_"));
    assert!(v["diagnostics"].is_array());
}
