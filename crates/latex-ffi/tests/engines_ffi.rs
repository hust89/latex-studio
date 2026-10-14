use std::ffi::{CStr, CString};
use std::os::raw::c_char;

use latex_ffi::{latex_compile, latex_engines_list, latex_free_string};

unsafe fn take(p: *mut c_char) -> String {
    assert!(!p.is_null());
    let s = CStr::from_ptr(p).to_string_lossy().into_owned();
    latex_free_string(p);
    s
}

fn c(s: &str) -> CString { CString::new(s).unwrap() }

#[test]
fn list_contains_all_engines() {
    let s = unsafe { take(latex_engines_list()) };
    let v: serde_json::Value = serde_json::from_str(&s).unwrap();
    let ids: Vec<&str> = v["engines"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["id"].as_str().unwrap())
        .collect();
    for expected in ["tectonic", "pdflatex", "xelatex", "lualatex"] {
        assert!(ids.contains(&expected), "missing {expected}: {ids:?}");
    }
    assert_eq!(v["default"].as_str(), Some("tectonic"));
}

#[test]
fn unknown_engine_errors() {
    let d = tempfile::tempdir().unwrap();
    std::fs::write(d.path().join("main.tex"),
        "\\documentclass{article}\\begin{document}x\\end{document}").unwrap();
    let e = c("not-an-engine");
    let ent = c(d.path().join("main.tex").to_str().unwrap());
    let root = c(d.path().to_str().unwrap());
    let out = c(d.path().join("out").to_str().unwrap());
    let s = unsafe { take(latex_compile(e.as_ptr(), ent.as_ptr(), root.as_ptr(), out.as_ptr())) };
    assert!(s.contains("unknown engine"), "{s}");
}

#[test]
fn tectonic_compiles_via_explicit_engine_id() {
    let ok = std::process::Command::new("tectonic")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);
    if !ok {
        eprintln!("SKIPPED (tectonic not on PATH)");
        return;
    }
    std::env::set_var("LATEX_TECTONIC_NETWORK", "offline");

    let d = tempfile::tempdir().unwrap();
    std::fs::write(d.path().join("main.tex"),
        "\\documentclass{article}\\begin{document}ok\\end{document}").unwrap();
    let e = c("tectonic");
    let ent = c(d.path().join("main.tex").to_str().unwrap());
    let root = c(d.path().to_str().unwrap());
    let out = c(d.path().join("out").to_str().unwrap());
    let s = unsafe { take(latex_compile(e.as_ptr(), ent.as_ptr(), root.as_ptr(), out.as_ptr())) };
    let v: serde_json::Value = serde_json::from_str(&s).unwrap();
    assert_eq!(v["success"], true, "{s}");
}
