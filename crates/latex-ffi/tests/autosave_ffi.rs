//! Exercises save_atomic and the autosave roundtrip over the FFI.

use std::ffi::{CStr, CString};
use std::os::raw::c_char;
use std::path::Path;

use latex_ffi::{
    latex_autosave_clear, latex_autosave_list, latex_autosave_read, latex_autosave_write,
    latex_free_string, latex_save_atomic,
};

unsafe fn take_and_free(p: *mut c_char) -> String {
    assert!(!p.is_null());
    let s = CStr::from_ptr(p).to_string_lossy().into_owned();
    latex_free_string(p);
    s
}

fn c(s: &str) -> CString {
    CString::new(s).unwrap()
}

fn write(p: &Path, s: &str) {
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(p, s).unwrap();
}

#[test]
fn save_atomic_writes_and_clears_autosave() {
    let d = tempfile::tempdir().unwrap();
    let root = d.path();
    write(&root.join("main.tex"), "original\n");

    let root_c = c(root.to_str().unwrap());
    let rel_c = c("main.tex");

    // Autosave first.
    let content_c = c("edited\n");
    let s = unsafe {
        take_and_free(latex_autosave_write(
            root_c.as_ptr(),
            rel_c.as_ptr(),
            content_c.as_ptr(),
        ))
    };
    assert!(s.contains("\"ok\":true"), "{s}");

    // It should now appear in the list.
    let s = unsafe { take_and_free(latex_autosave_list(root_c.as_ptr())) };
    let v: serde_json::Value = serde_json::from_str(&s).unwrap();
    assert_eq!(v["recoverable"].as_array().unwrap().len(), 1);

    // Now do the real save. It should clear the autosave.
    let s = unsafe {
        take_and_free(latex_save_atomic(
            root_c.as_ptr(),
            rel_c.as_ptr(),
            content_c.as_ptr(),
        ))
    };
    assert!(s.contains("\"ok\":true"), "{s}");

    let s = unsafe { take_and_free(latex_autosave_list(root_c.as_ptr())) };
    let v: serde_json::Value = serde_json::from_str(&s).unwrap();
    assert!(v["recoverable"].as_array().unwrap().is_empty(), "{s}");

    assert_eq!(std::fs::read_to_string(root.join("main.tex")).unwrap(), "edited\n");
}

#[test]
fn autosave_read_returns_content() {
    let d = tempfile::tempdir().unwrap();
    let root = d.path();
    write(&root.join("ch.tex"), "orig\n");
    std::thread::sleep(std::time::Duration::from_millis(15));

    let root_c = c(root.to_str().unwrap());
    let rel_c = c("ch.tex");
    let content_c = c("autosaved\n");
    unsafe {
        take_and_free(latex_autosave_write(
            root_c.as_ptr(),
            rel_c.as_ptr(),
            content_c.as_ptr(),
        ))
    };

    let s = unsafe { take_and_free(latex_autosave_read(root_c.as_ptr(), rel_c.as_ptr())) };
    let v: serde_json::Value = serde_json::from_str(&s).unwrap();
    assert_eq!(v["content"], "autosaved\n");
}

#[test]
fn autosave_clear_removes() {
    let d = tempfile::tempdir().unwrap();
    let root = d.path();
    write(&root.join("x.tex"), "a\n");
    std::thread::sleep(std::time::Duration::from_millis(15));

    let root_c = c(root.to_str().unwrap());
    let rel_c = c("x.tex");
    let content_c = c("b\n");
    unsafe {
        take_and_free(latex_autosave_write(
            root_c.as_ptr(),
            rel_c.as_ptr(),
            content_c.as_ptr(),
        ))
    };

    unsafe { take_and_free(latex_autosave_clear(root_c.as_ptr(), rel_c.as_ptr())) };

    let s = unsafe { take_and_free(latex_autosave_list(root_c.as_ptr())) };
    let v: serde_json::Value = serde_json::from_str(&s).unwrap();
    assert!(v["recoverable"].as_array().unwrap().is_empty());
}

#[test]
fn rejects_absolute_relative_path() {
    let d = tempfile::tempdir().unwrap();
    let root_c = c(d.path().to_str().unwrap());
    let rel_c = c("/etc/passwd");
    let content_c = c("x");
    let s = unsafe {
        take_and_free(latex_save_atomic(
            root_c.as_ptr(),
            rel_c.as_ptr(),
            content_c.as_ptr(),
        ))
    };
    assert!(s.contains("\"error\""), "{s}");
    assert!(s.contains("must be relative"), "{s}");
}

#[test]
fn rejects_null_content() {
    let d = tempfile::tempdir().unwrap();
    let root_c = c(d.path().to_str().unwrap());
    let rel_c = c("main.tex");
    let s = unsafe {
        take_and_free(latex_save_atomic(
            root_c.as_ptr(),
            rel_c.as_ptr(),
            std::ptr::null(),
        ))
    };
    assert!(s.contains("null pointer"), "{s}");
}
