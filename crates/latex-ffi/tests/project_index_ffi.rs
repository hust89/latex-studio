//! Exercises `latex_project_index` over the FFI.

use std::ffi::{CStr, CString};
use std::os::raw::c_char;

use latex_ffi::{latex_free_string, latex_project_index};

unsafe fn take_and_free(p: *mut c_char) -> String {
    assert!(!p.is_null());
    let s = CStr::from_ptr(p).to_string_lossy().into_owned();
    latex_free_string(p);
    s
}

fn write(p: &std::path::Path, s: &str) {
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(p, s).unwrap();
}

fn fixture() -> tempfile::TempDir {
    let d = tempfile::tempdir().unwrap();
    let r = d.path();
    write(&r.join("main.tex"), "\\documentclass{book}\n\\input{ch1}\n");
    write(&r.join("ch1.tex"), "\\chapter{One}\n");
    write(&r.join("refs.bib"), "@article{k, title={T}}\n");
    std::fs::create_dir_all(r.join("figures")).unwrap();
    std::fs::write(r.join("figures/plot.pdf"), b"%PDF").unwrap();
    d
}

#[test]
fn null_pointer_returns_error() {
    let s = unsafe { take_and_free(latex_project_index(std::ptr::null())) };
    assert!(s.contains("null pointer"), "{s}");
}

#[test]
fn missing_directory_returns_error() {
    let c = CString::new("/nonexistent/path/xyzzy").unwrap();
    let s = unsafe { take_and_free(latex_project_index(c.as_ptr())) };
    assert!(s.contains("error"), "{s}");
}

#[test]
fn returns_files_and_entry() {
    let dir = fixture();
    let c = CString::new(dir.path().to_str().unwrap()).unwrap();
    let s = unsafe { take_and_free(latex_project_index(c.as_ptr())) };
    let v: serde_json::Value = serde_json::from_str(&s).expect("valid json");

    let files = v["files"].as_array().expect("files array");
    let paths: Vec<&str> = files
        .iter()
        .map(|f| f["path"].as_str().unwrap())
        .collect();
    assert!(paths.contains(&"main.tex"), "{paths:?}");
    assert!(paths.contains(&"ch1.tex"), "{paths:?}");
    assert!(paths.contains(&"refs.bib"), "{paths:?}");
    assert!(paths.contains(&"figures/plot.pdf"), "{paths:?}");

    let by_path: std::collections::HashMap<&str, &str> = files
        .iter()
        .map(|f| (f["path"].as_str().unwrap(), f["kind"].as_str().unwrap()))
        .collect();
    assert_eq!(by_path["main.tex"], "Source");
    assert_eq!(by_path["refs.bib"], "Bibliography");
    assert_eq!(by_path["figures/plot.pdf"], "Asset");

    assert_eq!(v["entry"].as_str(), Some("main.tex"));
}

#[test]
fn entry_is_null_when_no_documentclass() {
    let d = tempfile::tempdir().unwrap();
    write(&d.path().join("a.tex"), "no class here\n");
    let c = CString::new(d.path().to_str().unwrap()).unwrap();
    let s = unsafe { take_and_free(latex_project_index(c.as_ptr())) };
    let v: serde_json::Value = serde_json::from_str(&s).unwrap();
    assert!(v["entry"].is_null(), "{s}");
}
