//! C ABI for the LaTeX Studio core.
//!
//! Each public function returns a heap-allocated, NUL-terminated JSON string
//! — either a result object on success or `{"error":"..."}` on failure. The
//! caller frees with `latex_free_string`.
//!
//! Only `#[no_mangle] pub extern "C"` items are part of the ABI. The
//! re-exports below let the crate's own tests import them by Rust path.

use std::ffi::{CStr, CString};
use std::os::raw::c_char;
use std::path::PathBuf;

mod compile;
mod engines;
mod project;
mod publish;

pub use compile::latex_compile;
pub use engines::latex_engines_list;
pub use project::{
    latex_autosave_clear, latex_autosave_list, latex_autosave_read, latex_autosave_write,
    latex_project_index, latex_save_atomic, latex_template_create, latex_templates_list,
};
pub use publish::latex_publish_archive;

// ---------- helpers shared across submodules --------------------------------

pub(crate) fn json_error(msg: &str) -> String {
    serde_json::json!({ "error": msg }).to_string()
}

pub(crate) fn cstring_or_error(json: String) -> *mut c_char {
    let c = CString::new(json)
        .unwrap_or_else(|_| CString::new("{\"error\":\"nul byte in result\"}").unwrap());
    c.into_raw()
}

/// Read a C string pointer as an owned Rust `String`.
///
/// Returns `None` if the pointer is null. This is the **only** place in the
/// crate that dereferences a raw pointer from the ABI. Submodules work with
/// owned `String`s and contain no `unsafe` blocks.
pub(crate) fn read_cstr(p: *const c_char) -> Option<String> {
    if p.is_null() {
        return None;
    }
    // SAFETY: `p` is non-null (checked above) and is a valid NUL-terminated
    // string per the ABI contract documented at the top of this file.
    Some(unsafe { CStr::from_ptr(p) }.to_string_lossy().into_owned())
}

/// Validate a project-root / relative-path pair.
///
/// Both arguments are non-empty Rust strings. `rel_path` must be relative.
/// `project_root` must exist and be a directory.
pub(crate) fn path_from_args(
    project_root: &str,
    rel_path: &str,
) -> Result<(PathBuf, PathBuf), String> {
    if project_root.is_empty() || rel_path.is_empty() {
        return Err("empty string argument".into());
    }
    let root_path = PathBuf::from(project_root);
    if !root_path.is_dir() {
        return Err(format!("project_root is not a directory: {project_root}"));
    }
    let rel_path = PathBuf::from(rel_path);
    if rel_path.is_absolute() {
        return Err("rel_path must be relative".into());
    }
    Ok((root_path, rel_path))
}

#[no_mangle]
pub extern "C" fn latex_free_string(p: *mut c_char) {
    if p.is_null() {
        return;
    }
    // SAFETY: p was produced by CString::into_raw in one of the submodules.
    unsafe { drop(CString::from_raw(p)); }
}
