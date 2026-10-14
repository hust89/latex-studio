//! Formal verification over the FFI.
//!
//! One function: `latex_formal_check(preamble, statement, project_dir)`.
//!
//! Returns a JSON object with a `status` discriminator:
//!
//!   {"status":"verified",     "system":"lean4", "stdout":"..."}
//!   {"status":"not_verified", "system":"lean4", "diagnostics":"...", "exit_code":1}
//!   {"status":"unavailable",  "system":"lean4", "reason":"..."}
//!
//! The three states are distinct and the caller MUST NOT treat `unavailable`
//! as a positive result. This is the whole point of the function.

use std::os::raw::c_char;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::PathBuf;
use std::time::Duration;

use latex_formal::{FormalRequest, FormalSystem, LeanEngine};

use crate::{cstring_or_error, json_error, read_cstr};

/// Run a Lean 4 check on a fragment.
///
/// `preamble` may be empty. `statement` must be a self-contained Lean
/// declaration. `project_dir` may be empty; when non-empty it must be a
/// directory containing a Lake project, so that Mathlib imports resolve.
#[no_mangle]
pub extern "C" fn latex_formal_check(
    preamble: *const c_char,
    statement: *const c_char,
    project_dir: *const c_char,
) -> *mut c_char {
    let payload = catch_unwind(AssertUnwindSafe(|| {
        let (Some(preamble), Some(statement), Some(project_dir)) = (
            read_cstr(preamble),
            read_cstr(statement),
            read_cstr(project_dir),
        ) else {
            return json_error("null pointer argument");
        };
        if statement.trim().is_empty() {
            return json_error("empty statement");
        }

        let rt = match tokio::runtime::Runtime::new() {
            Ok(rt) => rt,
            Err(e) => return json_error(&format!("runtime: {e}")),
        };

        let engine = LeanEngine::default();
        let mut req = FormalRequest::new(statement).with_preamble(preamble);
        req.timeout = Duration::from_secs(60);
        if !project_dir.trim().is_empty() {
            req = req.with_project_dir(PathBuf::from(project_dir));
        }

        let result = rt.block_on(engine.check(req));
        serde_json::to_string(&result)
            .unwrap_or_else(|e| json_error(&format!("serialize: {e}")))
    }))
    .unwrap_or_else(|_| json_error("panic in latex_formal_check"));

    cstring_or_error(payload)
}

/// Report whether a formal system is available on this machine.
/// Returns `{"available":true|false,"system":"lean4"}`.
#[no_mangle]
pub extern "C" fn latex_formal_available() -> *mut c_char {
    let payload = catch_unwind(AssertUnwindSafe(|| {
        let engine = LeanEngine::default();
        serde_json::json!({
            "system": "lean4",
            "available": engine.available(),
        })
        .to_string()
    }))
    .unwrap_or_else(|_| json_error("panic in latex_formal_available"));
    cstring_or_error(payload)
}
