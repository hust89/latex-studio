use std::ffi::{CStr, CString};
use std::os::raw::c_char;

use latex_ffi::{latex_free_string, latex_publish_archive};

unsafe fn take(p: *mut c_char) -> String {
    assert!(!p.is_null());
    let s = CStr::from_ptr(p).to_string_lossy().into_owned();
    latex_free_string(p);
    s
}

fn c(s: &str) -> CString { CString::new(s).unwrap() }

fn write(p: &std::path::Path, s: &str) {
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(p, s).unwrap();
}

#[test]
fn null_pointer_errored() {
    let s = unsafe { take(latex_publish_archive(std::ptr::null(), std::ptr::null(), std::ptr::null())) };
    assert!(s.contains("null pointer"), "{s}");
}

#[test]
fn missing_root_errored() {
    let r = c("/no/such/dir/xyzzy");
    let o = c("/tmp/never");
    let e = c("tectonic");
    let s = unsafe { take(latex_publish_archive(r.as_ptr(), o.as_ptr(), e.as_ptr())) };
    assert!(s.contains("not a directory"), "{s}");
}

#[test]
fn creates_archive_with_manifest() {
    let d = tempfile::tempdir().unwrap();
    write(&d.path().join("main.tex"),
        "\\documentclass{article}\\begin{document}hi\\end{document}\n");
    write(&d.path().join("refs.bib"), "@article{k}\n");
    let out = d.path().join("published.zip");

    let r = c(d.path().to_str().unwrap());
    let o = c(out.to_str().unwrap());
    let e = c("tectonic");
    let s = unsafe { take(latex_publish_archive(r.as_ptr(), o.as_ptr(), e.as_ptr())) };
    let v: serde_json::Value = serde_json::from_str(&s).unwrap();
    assert!(out.is_file(), "zip not created: {s}");
    assert!(v["file_count"].as_u64().unwrap() >= 2, "{s}");

    // Manifest readable from the zip.
    let f = std::fs::File::open(&out).unwrap();
    let mut z = zip::ZipArchive::new(f).unwrap();
    let mut m = z.by_name("MANIFEST.json").unwrap();
    let mut buf = String::new();
    std::io::Read::read_to_string(&mut m, &mut buf).unwrap();
    let mv: serde_json::Value = serde_json::from_str(&buf).unwrap();
    assert_eq!(mv["engine"], "tectonic");
}
