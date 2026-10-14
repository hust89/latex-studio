use std::os::raw::c_char;
use std::panic::{catch_unwind, AssertUnwindSafe};

use crate::{cstring_or_error, json_error};

/// List available engines:
/// `{"engines":[{"id":"tectonic","display_name":"Tectonic","available":true,"notes":null}, ...],
///   "default":"tectonic"}`
#[no_mangle]
pub extern "C" fn latex_engines_list() -> *mut c_char {
    let payload = catch_unwind(AssertUnwindSafe(|| {
        let engines = latex_build::list_engines();
        serde_json::json!({
            "engines": engines,
            "default": latex_build::default_engine_id(),
        })
        .to_string()
    }))
    .unwrap_or_else(|_| json_error("panic in latex_engines_list"));
    cstring_or_error(payload)
}
