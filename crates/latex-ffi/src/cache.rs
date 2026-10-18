//! Process-wide cache directory hint.
//!
//! Flutter calls `latex_set_cache_dir` once at startup with a writable
//! path. On Android this is the app's private files directory. On Linux
//! the app does not call it and the engine uses the platform default
//! (~/.cache/Tectonic).

use std::os::raw::c_char;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::PathBuf;

use crate::{cstring_or_error, json_error, read_cstr};

#[no_mangle]
pub extern "C" fn latex_set_cache_dir(path: *const c_char) -> *mut c_char {
    let payload = catch_unwind(AssertUnwindSafe(|| {
        let Some(s) = read_cstr(path) else {
            return json_error("null pointer argument");
        };
        if s.is_empty() {
            return json_error("empty string argument");
        }
        let dir = PathBuf::from(&s);
        if let Err(e) = std::fs::create_dir_all(&dir) {
            return json_error(&format!("create_dir_all {}: {e}", dir.display()));
        }
        let config = dir.join("config");
        if let Err(e) = std::fs::create_dir_all(&config) {
            return json_error(&format!("create_dir_all {}: {e}", config.display()));
        }
        std::env::set_var("LATEX_STUDIO_CACHE_DIR", &s);
        std::env::set_var("LATEX_STUDIO_CONFIG_DIR", &config);
        std::env::set_var("TECTONIC_CACHE_DIR", &s);
        std::env::set_var("TECTONIC_CONFIG_DIR", &config);
        r#"{"ok":true}"#.to_string()
    }))
    .unwrap_or_else(|_| json_error("panic in latex_set_cache_dir"));
    cstring_or_error(payload)
}
