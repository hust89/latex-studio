//! Project index, atomic save, autosave, and templates.

use std::os::raw::c_char;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::PathBuf;

use crate::{cstring_or_error, json_error, path_from_args, read_cstr};

// ---------- index -----------------------------------------------------------

#[derive(serde::Serialize)]
struct ProjectIndexJson {
    files: Vec<FileJson>,
    entry: Option<String>,
}

#[derive(serde::Serialize)]
struct FileJson {
    path: String,
    kind: &'static str,
}

#[no_mangle]
pub extern "C" fn latex_project_index(project_root: *const c_char) -> *mut c_char {
    let payload = catch_unwind(AssertUnwindSafe(|| {
        let Some(root) = read_cstr(project_root) else {
            return json_error("null pointer argument");
        };
        if root.is_empty() {
            return json_error("empty string argument");
        }

        let project = match latex_project::Project::open(&root) {
            Ok(p) => p,
            Err(e) => return json_error(&format!("{e:#}")),
        };
        let index = match project.index() {
            Ok(i) => i,
            Err(e) => return json_error(&format!("{e:#}")),
        };

        let files: Vec<FileJson> = index
            .files
            .iter()
            .map(|f| FileJson {
                path: f.path.to_string_lossy().into_owned(),
                kind: kind_name(f.kind),
            })
            .collect();

        let entry = project
            .entry(&index)
            .map(|p| p.to_string_lossy().into_owned());

        let out = ProjectIndexJson { files, entry };
        serde_json::to_string(&out)
            .unwrap_or_else(|e| json_error(&format!("serialize: {e}")))
    }))
    .unwrap_or_else(|_| json_error("panic in latex_project_index"));

    cstring_or_error(payload)
}

fn kind_name(k: latex_project::FileKind) -> &'static str {
    use latex_project::FileKind::*;
    match k {
        Source => "Source",
        Bibliography => "Bibliography",
        Style => "Style",
        Asset => "Asset",
        Other => "Other",
    }
}

// ---------- atomic save ----------------------------------------------------

#[no_mangle]
pub extern "C" fn latex_save_atomic(
    project_root: *const c_char,
    rel_path: *const c_char,
    content: *const c_char,
) -> *mut c_char {
    let payload = catch_unwind(AssertUnwindSafe(|| {
        let (Some(root_s), Some(rel_s), Some(body)) = (
            read_cstr(project_root),
            read_cstr(rel_path),
            read_cstr(content),
        ) else {
            return json_error("null pointer argument");
        };
        let (root, rel) = match path_from_args(&root_s, &rel_s) {
            Ok(v) => v,
            Err(e) => return json_error(&e),
        };

        let target = root.join(&rel);
        if let Err(e) = latex_project::atomic_write(&target, &body) {
            return json_error(&format!("{e:#}"));
        }
        let _ = latex_project::autosave_clear(&root, &rel);
        r#"{"ok":true}"#.to_string()
    }))
    .unwrap_or_else(|_| json_error("panic in latex_save_atomic"));
    cstring_or_error(payload)
}

// ---------- autosave -------------------------------------------------------

#[no_mangle]
pub extern "C" fn latex_autosave_write(
    project_root: *const c_char,
    rel_path: *const c_char,
    content: *const c_char,
) -> *mut c_char {
    let payload = catch_unwind(AssertUnwindSafe(|| {
        let (Some(root_s), Some(rel_s), Some(body)) = (
            read_cstr(project_root),
            read_cstr(rel_path),
            read_cstr(content),
        ) else {
            return json_error("null pointer argument");
        };
        let (root, rel) = match path_from_args(&root_s, &rel_s) {
            Ok(v) => v,
            Err(e) => return json_error(&e),
        };
        match latex_project::autosave_write(&root, &rel, &body) {
            Ok(()) => r#"{"ok":true}"#.to_string(),
            Err(e) => json_error(&format!("{e:#}")),
        }
    }))
    .unwrap_or_else(|_| json_error("panic in latex_autosave_write"));
    cstring_or_error(payload)
}

#[no_mangle]
pub extern "C" fn latex_autosave_list(project_root: *const c_char) -> *mut c_char {
    let payload = catch_unwind(AssertUnwindSafe(|| {
        let Some(root) = read_cstr(project_root) else {
            return json_error("null pointer argument");
        };
        if root.is_empty() {
            return json_error("empty string argument");
        }
        let root_path = PathBuf::from(&root);
        if !root_path.is_dir() {
            return json_error(&format!("not a directory: {root}"));
        }
        match latex_project::autosave_list(&root_path) {
            Ok(list) => serde_json::json!({ "recoverable": list }).to_string(),
            Err(e) => json_error(&format!("{e:#}")),
        }
    }))
    .unwrap_or_else(|_| json_error("panic in latex_autosave_list"));
    cstring_or_error(payload)
}

#[no_mangle]
pub extern "C" fn latex_autosave_read(
    project_root: *const c_char,
    rel_path: *const c_char,
) -> *mut c_char {
    let payload = catch_unwind(AssertUnwindSafe(|| {
        let (Some(root_s), Some(rel_s)) =
            (read_cstr(project_root), read_cstr(rel_path))
        else {
            return json_error("null pointer argument");
        };
        let (root, rel) = match path_from_args(&root_s, &rel_s) {
            Ok(v) => v,
            Err(e) => return json_error(&e),
        };
        match latex_project::autosave_read(&root, &rel) {
            Ok(content) => serde_json::json!({ "content": content }).to_string(),
            Err(e) => json_error(&format!("{e:#}")),
        }
    }))
    .unwrap_or_else(|_| json_error("panic in latex_autosave_read"));
    cstring_or_error(payload)
}

#[no_mangle]
pub extern "C" fn latex_autosave_clear(
    project_root: *const c_char,
    rel_path: *const c_char,
) -> *mut c_char {
    let payload = catch_unwind(AssertUnwindSafe(|| {
        let (Some(root_s), Some(rel_s)) =
            (read_cstr(project_root), read_cstr(rel_path))
        else {
            return json_error("null pointer argument");
        };
        let (root, rel) = match path_from_args(&root_s, &rel_s) {
            Ok(v) => v,
            Err(e) => return json_error(&e),
        };
        match latex_project::autosave_clear(&root, &rel) {
            Ok(()) => r#"{"ok":true}"#.to_string(),
            Err(e) => json_error(&format!("{e:#}")),
        }
    }))
    .unwrap_or_else(|_| json_error("panic in latex_autosave_clear"));
    cstring_or_error(payload)
}

// ---------- templates ------------------------------------------------------

#[no_mangle]
pub extern "C" fn latex_templates_list() -> *mut c_char {
    let payload = catch_unwind(AssertUnwindSafe(|| {
        let items: Vec<serde_json::Value> = latex_project::TEMPLATES
            .iter()
            .map(|t| {
                serde_json::json!({
                    "name": t.name,
                    "description": t.description,
                })
            })
            .collect();
        serde_json::json!({ "templates": items }).to_string()
    }))
    .unwrap_or_else(|_| json_error("panic in latex_templates_list"));
    cstring_or_error(payload)
}

#[no_mangle]
pub extern "C" fn latex_template_create(
    name: *const c_char,
    target_dir: *const c_char,
) -> *mut c_char {
    let payload = catch_unwind(AssertUnwindSafe(|| {
        let (Some(name_s), Some(target_s)) = (read_cstr(name), read_cstr(target_dir)) else {
            return json_error("null pointer argument");
        };
        if name_s.is_empty() || target_s.is_empty() {
            return json_error("empty string argument");
        }

        let template = match latex_project::find_template(&name_s) {
            Some(t) => t,
            None => return json_error(&format!("unknown template: {name_s}")),
        };
        let target = PathBuf::from(&target_s);
        match latex_project::create_template(template, &target) {
            Ok(()) => serde_json::json!({
                "ok": true,
                "path": target.to_string_lossy(),
            })
            .to_string(),
            Err(e) => json_error(&format!("{e:#}")),
        }
    }))
    .unwrap_or_else(|_| json_error("panic in latex_template_create"));
    cstring_or_error(payload)
}
