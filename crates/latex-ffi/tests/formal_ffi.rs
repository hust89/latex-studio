//! Tests for latex_formal_check and latex_formal_available.

use std::ffi::{CStr, CString};
use std::os::raw::c_char;

use latex_ffi::{latex_formal_available, latex_formal_check, latex_free_string};

unsafe fn take(p: *mut c_char) -> String {
    assert!(!p.is_null());
    let s = CStr::from_ptr(p).to_string_lossy().into_owned();
    latex_free_string(p);
    s
}

fn c(s: &str) -> CString {
    CString::new(s).unwrap()
}

fn parse(s: &str) -> serde_json::Value {
    serde_json::from_str(s).unwrap_or_else(|e| panic!("invalid json: {e}\nraw: {s}"))
}

#[test]
fn null_args_errored() {
    let s = unsafe {
        take(latex_formal_check(
            std::ptr::null(),
            std::ptr::null(),
            std::ptr::null(),
        ))
    };
    assert!(s.contains("null pointer"), "{s}");
}

#[test]
fn empty_statement_errored() {
    let p = c("");
    let st = c("");
    let d = c("");
    let s = unsafe { take(latex_formal_check(p.as_ptr(), st.as_ptr(), d.as_ptr())) };
    assert!(s.contains("empty statement"), "{s}");
}

#[test]
fn available_reports_a_bool() {
    let s = unsafe { take(latex_formal_available()) };
    let v = parse(&s);
    assert_eq!(v["system"], "lean4");
    assert!(v["available"].is_boolean(), "{s}");
}

#[test]
fn check_returns_one_of_the_three_states() {
    // Whatever Lean's availability, the result must be one of the three
    // documented states, and only "verified" counts as a positive.
    let p = c("");
    let st = c("theorem t : 1 + 1 = 2 := rfl");
    let d = c("");
    let s = unsafe { take(latex_formal_check(p.as_ptr(), st.as_ptr(), d.as_ptr())) };
    let v = parse(&s);
    let status = v["status"].as_str().expect("status field");
    assert!(
        matches!(status, "verified" | "not_verified" | "unavailable"),
        "unexpected status: {status}\n{s}"
    );
    assert_eq!(v["system"], "lean4");
}

#[test]
fn false_statement_is_not_verified_when_lean_is_present() {
    // If Lean is not on PATH, the result must be Unavailable, not Verified
    // or NotVerified. If Lean is present, a false statement must not
    // come back as Verified.
    let avail = parse(&unsafe { take(latex_formal_available()) });
    let lean_present = avail["available"] == true;

    let p = c("");
    let st = c("theorem t : 1 + 1 = 3 := rfl");
    let d = c("");
    let s = unsafe { take(latex_formal_check(p.as_ptr(), st.as_ptr(), d.as_ptr())) };
    let v = parse(&s);
    let status = v["status"].as_str().unwrap();
    if lean_present {
        assert_eq!(status, "not_verified", "{s}");
        let diags = v["diagnostics"].as_str().unwrap_or("");
        assert!(!diags.is_empty(), "expected diagnostics: {s}");
    } else {
        assert_eq!(status, "unavailable", "{s}");
    }
}

#[test]
fn missing_project_dir_is_unavailable_not_verified() {
    let p = c("");
    let st = c("theorem t : 1 = 1 := rfl");
    let d = c("/no/such/directory/xyzzy");
    let s = unsafe { take(latex_formal_check(p.as_ptr(), st.as_ptr(), d.as_ptr())) };
    let v = parse(&s);
    assert_eq!(v["status"], "unavailable", "{s}");
}
