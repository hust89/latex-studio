use std::os::raw::c_char;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::PathBuf;

use crate::{cstring_or_error, json_error, read_cstr};

/// Create a portable source archive of `project_root` at `out_zip`.
/// Returns a PublishReport as JSON on success, `{"error":"..."}` otherwise.
#[no_mangle]
pub extern "C" fn latex_publish_archive(
    project_root: *const c_char,
    out_zip: *const c_char,
    engine_id: *const c_char,
) -> *mut c_char {
    let payload = catch_unwind(AssertUnwindSafe(|| {
        let (Some(root_s), Some(out_s), Some(engine_s)) = (
            read_cstr(project_root),
            read_cstr(out_zip),
            read_cstr(engine_id),
        ) else {
            return json_error("null pointer argument");
        };
        if root_s.is_empty() || out_s.is_empty() || engine_s.is_empty() {
            return json_error("empty string argument");
        }
        let root_path = PathBuf::from(&root_s);
        if !root_path.is_dir() {
            return json_error(&format!("project_root is not a directory: {root_s}"));
        }
        let out_path = PathBuf::from(&out_s);

        let opts = latex_publish::PublishOptions {
            project_root: root_path,
            out_zip: out_path,
            engine_id: engine_s,
        };
        match latex_publish::create_archive(&opts) {
            Ok(r) => serde_json::to_string(&r)
                .unwrap_or_else(|e| json_error(&format!("serialize: {e}"))),
            Err(e) => json_error(&format!("{e:#}")),
        }
    }))
    .unwrap_or_else(|_| json_error("panic in latex_publish_archive"));
    cstring_or_error(payload)
}
