use std::os::raw::c_char;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::{Path, PathBuf};

use crate::{cstring_or_error, json_error, read_cstr};

#[no_mangle]
pub extern "C" fn latex_compile(
    engine_name: *const c_char,
    entry: *const c_char,
    project_root: *const c_char,
    out_parent: *const c_char,
) -> *mut c_char {
    let payload = catch_unwind(AssertUnwindSafe(|| {
        let (Some(engine_s), Some(entry), Some(project_root), Some(out_parent)) = (
            read_cstr(engine_name),
            read_cstr(entry),
            read_cstr(project_root),
            read_cstr(out_parent),
        ) else {
            return json_error("null pointer argument");
        };

        if engine_s.is_empty() || entry.is_empty()
            || project_root.is_empty() || out_parent.is_empty()
        {
            return json_error("empty string argument");
        }

        let entry_path = Path::new(&entry);
        if !entry_path.is_file() {
            return json_error(&format!("entry is not a file: {entry}"));
        }
        let root_path = Path::new(&project_root);
        if !root_path.is_dir() {
            return json_error(&format!("project_root is not a directory: {project_root}"));
        }
        let (entry_c, root_c) = match (entry_path.canonicalize(), root_path.canonicalize()) {
            (Ok(a), Ok(b)) => (a, b),
            (Err(e), _) | (_, Err(e)) => return json_error(&format!("canonicalize: {e}")),
        };
        if !entry_c.starts_with(&root_c) {
            return json_error("entry is not inside project_root");
        }
        let out_path = PathBuf::from(&out_parent);
        if let Err(e) = std::fs::create_dir_all(&out_path) {
            return json_error(&format!("cannot create out_parent: {e}"));
        }

        let rt = match tokio::runtime::Runtime::new() {
            Ok(rt) => rt,
            Err(e) => return json_error(&format!("runtime: {e}")),
        };

        rt.block_on(async {
            let builder = latex_build::Builder;
            let req = latex_build::BuildRequest {
                engine_id: engine_s,
                entry: entry_c,
                project_root: root_c,
                out_parent: out_path,
            };
            match builder.build(&req).await {
                Ok(r) => serde_json::to_string(&r)
                    .unwrap_or_else(|e| json_error(&format!("serialize: {e}"))),
                Err(e) => json_error(&format!("{e:#}")),
            }
        })
    }))
    .unwrap_or_else(|_| json_error("panic in latex_compile"));

    cstring_or_error(payload)
}
