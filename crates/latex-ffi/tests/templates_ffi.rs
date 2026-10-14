use std::ffi::{CStr, CString};
use std::os::raw::c_char;

use latex_ffi::{latex_free_string, latex_template_create, latex_templates_list};

unsafe fn take(p: *mut c_char) -> String {
    assert!(!p.is_null());
    let s = CStr::from_ptr(p).to_string_lossy().into_owned();
    latex_free_string(p);
    s
}

fn c(s: &str) -> CString {
    CString::new(s).unwrap()
}

#[test]
fn list_returns_expected_names() {
    let s = unsafe { take(latex_templates_list()) };
    let v: serde_json::Value = serde_json::from_str(&s).unwrap();
    let names: Vec<&str> = v["templates"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap())
        .collect();
    for expected in ["article", "book", "beamer", "letter", "homework"] {
        assert!(names.contains(&expected), "missing {expected}: {names:?}");
    }
}

#[test]
fn create_book_writes_files() {
    let d = tempfile::tempdir().unwrap();
    let target = d.path().join("proj");
    let name_c = c("book");
    let target_c = c(target.to_str().unwrap());
    let s = unsafe { take(latex_template_create(name_c.as_ptr(), target_c.as_ptr())) };
    assert!(s.contains("\"ok\":true"), "{s}");
    assert!(target.join("main.tex").is_file());
    assert!(target.join("chapters/one.tex").is_file());
    assert!(target.join("chapters/two.tex").is_file());
}

#[test]
fn unknown_template_errors() {
    let d = tempfile::tempdir().unwrap();
    let name_c = c("nope");
    let target_c = c(d.path().join("x").to_str().unwrap());
    let s = unsafe { take(latex_template_create(name_c.as_ptr(), target_c.as_ptr())) };
    assert!(s.contains("unknown template"), "{s}");
}

#[test]
fn refuses_non_empty_target() {
    let d = tempfile::tempdir().unwrap();
    let target = d.path().join("has-stuff");
    std::fs::create_dir_all(&target).unwrap();
    std::fs::write(target.join("x"), "y").unwrap();
    let name_c = c("article");
    let target_c = c(target.to_str().unwrap());
    let s = unsafe { take(latex_template_create(name_c.as_ptr(), target_c.as_ptr())) };
    assert!(s.contains("not empty"), "{s}");
}

#[test]
fn null_args_errored() {
    let s = unsafe { take(latex_template_create(std::ptr::null(), std::ptr::null())) };
    assert!(s.contains("null pointer"), "{s}");
}
