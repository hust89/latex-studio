//! Validation and correction over the FFI.
//!
//! Three functions:
//!   - `latex_verify_project`   — run the validator, return diagnostics
//!   - `latex_correct_propose`  — propose corrections for those diagnostics
//!   - `latex_correct_apply`    — apply one proposal, with safety checks
//!
//! `apply` is the only function that writes to source files. Its contract:
//!
//!   1. Re-read the target file from disk at apply time.
//!   2. Require that `old_text` occurs exactly once in the whole file.
//!   3. Replace that single occurrence with `new_text`.
//!   4. Write atomically (temp + fsync + rename).
//!
//! If any step fails, nothing is written. A stale proposal — one whose
//! context no longer matches the file — is refused, not applied with
//! best-effort matching.

use std::os::raw::c_char;
use std::panic::{catch_unwind, AssertUnwindSafe};

use crate::{cstring_or_error, json_error, read_cstr};

/// Run the validator on a project. Returns
/// `{"diagnostics":[<Diagnostic>, ...]}`.
#[no_mangle]
pub extern "C" fn latex_verify_project(project_root: *const c_char) -> *mut c_char {
    let payload = catch_unwind(AssertUnwindSafe(|| {
        let Some(root) = read_cstr(project_root) else {
            return json_error("null pointer argument");
        };
        if root.is_empty() {
            return json_error("empty string argument");
        }
        match latex_verify::verify_project(std::path::Path::new(&root)) {
            Ok(diags) => serde_json::json!({ "diagnostics": diags }).to_string(),
            Err(e) => json_error(&format!("{e:#}")),
        }
    }))
    .unwrap_or_else(|_| json_error("panic in latex_verify_project"));
    cstring_or_error(payload)
}

/// Propose corrections for a set of diagnostics. `diagnostics_json` is the
/// JSON array from `latex_verify_project`'s `diagnostics` field, or `[]`.
/// Returns `{"corrections":[<Correction>, ...]}`.
#[no_mangle]
pub extern "C" fn latex_correct_propose(
    project_root: *const c_char,
    diagnostics_json: *const c_char,
) -> *mut c_char {
    let payload = catch_unwind(AssertUnwindSafe(|| {
        let (Some(root), Some(diags_s)) =
            (read_cstr(project_root), read_cstr(diagnostics_json))
        else {
            return json_error("null pointer argument");
        };
        if root.is_empty() {
            return json_error("empty string argument");
        }
        let diagnostics: Vec<latex_model::Diagnostic> =
            match serde_json::from_str(&diags_s) {
                Ok(v) => v,
                Err(e) => return json_error(&format!("invalid diagnostics json: {e}")),
            };
        match latex_correct::propose_all(std::path::Path::new(&root), &diagnostics) {
            Ok(corrs) => serde_json::json!({ "corrections": corrs }).to_string(),
            Err(e) => json_error(&format!("{e:#}")),
        }
    }))
    .unwrap_or_else(|_| json_error("panic in latex_correct_propose"));
    cstring_or_error(payload)
}

/// Apply a single correction. `correction_json` is one element of the
/// `corrections` array from `latex_correct_propose`.
///
/// Returns on success:
///   `{"ok":true,"file":"...","line":N,"bytes_changed":M}`
///
/// Returns on refusal:
///   `{"error":"...","reason":"stale"|"ambiguous"|"missing"|"io"|"bad_input"}`
#[no_mangle]
pub extern "C" fn latex_correct_apply(
    project_root: *const c_char,
    correction_json: *const c_char,
) -> *mut c_char {
    let payload = catch_unwind(AssertUnwindSafe(|| {
        let (Some(root), Some(corr_s)) =
            (read_cstr(project_root), read_cstr(correction_json))
        else {
            return json_error("null pointer argument");
        };
        if root.is_empty() || corr_s.is_empty() {
            return json_error("empty string argument");
        }

        let corr: latex_correct::Correction = match serde_json::from_str(&corr_s) {
            Ok(c) => c,
            Err(e) => {
                return refusal("bad_input", &format!("invalid correction json: {e}"));
            }
        };

        apply_one(std::path::Path::new(&root), &corr)
    }))
    .unwrap_or_else(|_| json_error("panic in latex_correct_apply"));
    cstring_or_error(payload)
}

fn apply_one(root: &std::path::Path, corr: &latex_correct::Correction) -> String {
    let abs = root.join(&corr.file);
    if !abs.is_file() {
        return refusal("bad_input", &format!("not a file: {}", abs.display()));
    }
    let content = match std::fs::read_to_string(&abs) {
        Ok(c) => c,
        Err(e) => return refusal("io", &format!("read {}: {e}", abs.display())),
    };

    let occurrences = count_occurrences(&content, &corr.old_text);
    match occurrences {
        0 => {
            return refusal(
                "missing",
                &format!(
                    "the text to replace is no longer present in {}; \
                     the file has changed since the proposal was made",
                    corr.file
                ),
            );
        }
        1 => {}
        n => {
            return refusal(
                "ambiguous",
                &format!(
                    "the text to replace occurs {n} times in {}; \
                     refusing to guess which one",
                    corr.file
                ),
            );
        }
    }

    let updated = content.replacen(&corr.old_text, &corr.new_text, 1);
    let bytes_before = content.len();
    let bytes_after = updated.len();

    if let Err(e) = latex_project::atomic_write(&abs, &updated) {
        return refusal("io", &format!("write {}: {e:#}", abs.display()));
    }

    serde_json::json!({
        "ok": true,
        "file": corr.file,
        "line": corr.line,
        "bytes_changed": bytes_after as i64 - bytes_before as i64,
    })
    .to_string()
}

fn refusal(reason: &str, msg: &str) -> String {
    serde_json::json!({
        "error": msg,
        "reason": reason,
    })
    .to_string()
}

/// Count non-overlapping occurrences of `needle` in `haystack`.
/// An empty needle counts as 0: there is nothing to replace.
fn count_occurrences(haystack: &str, needle: &str) -> usize {
    if needle.is_empty() {
        return 0;
    }
    haystack.matches(needle).count()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_occurrences() {
        assert_eq!(count_occurrences("abc abc abc", "abc"), 3);
        assert_eq!(count_occurrences("abc", "xyz"), 0);
        assert_eq!(count_occurrences("", "x"), 0);
        assert_eq!(count_occurrences("abc", ""), 0);
    }

    #[test]
    fn counts_overlapping_as_non_overlapping() {
        assert_eq!(count_occurrences("aaa", "aa"), 1);
    }
}
