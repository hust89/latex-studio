use std::ffi::{CStr, CString};
use std::os::raw::c_char;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::{Path, PathBuf};

// ---------- helpers ---------------------------------------------------------

fn json_error(msg: &str) -> String {
    serde_json::json!({ "error": msg }).to_string()
}

fn cstring_or_error(json: String) -> *mut c_char {
    let c = CString::new(json)
        .unwrap_or_else(|_| CString::new("{\"error\":\"nul byte in result\"}").unwrap());
    c.into_raw()
}

fn path_from_args(
    project_root: *const c_char,
    rel_path: *const c_char,
) -> Result<(PathBuf, PathBuf), String> {
    if project_root.is_null() || rel_path.is_null() {
        return Err("null pointer argument".into());
    }
    let root = unsafe { CStr::from_ptr(project_root) }
        .to_string_lossy()
        .into_owned();
    let rel = unsafe { CStr::from_ptr(rel_path) }
        .to_string_lossy()
        .into_owned();
    if root.is_empty() || rel.is_empty() {
        return Err("empty string argument".into());
    }
    let root_path = PathBuf::from(&root);
    if !root_path.is_dir() {
        return Err(format!("project_root is not a directory: {root}"));
    }
    let rel_path = PathBuf::from(&rel);
    if rel_path.is_absolute() {
        return Err("rel_path must be relative".into());
    }
    Ok((root_path, rel_path))
}

// ---------- compile ---------------------------------------------------------

#[no_mangle]
pub extern "C" fn latex_compile(
    engine_name: *const c_char,
    entry: *const c_char,
    project_root: *const c_char,
    out_parent: *const c_char,
) -> *mut c_char {
    let payload = catch_unwind(AssertUnwindSafe(|| {
        if engine_name.is_null() || entry.is_null()
            || project_root.is_null() || out_parent.is_null()
        {
            return json_error("null pointer argument");
        }
        let engine_s = unsafe { CStr::from_ptr(engine_name) }
            .to_string_lossy()
            .into_owned();
        let entry = unsafe { CStr::from_ptr(entry) }.to_string_lossy().into_owned();
        let project_root = unsafe { CStr::from_ptr(project_root) }
            .to_string_lossy()
            .into_owned();
        let out_parent = unsafe { CStr::from_ptr(out_parent) }
            .to_string_lossy()
            .into_owned();

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

#[no_mangle]
pub extern "C" fn latex_free_string(p: *mut c_char) {
    if p.is_null() { return; }
    unsafe { drop(CString::from_raw(p)); }
}

// ---------- project model ---------------------------------------------------

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
        if project_root.is_null() {
            return json_error("null pointer argument");
        }
        let root = unsafe { CStr::from_ptr(project_root) }
            .to_string_lossy()
            .into_owned();
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

// ---------- atomic save and autosave ---------------------------------------

#[no_mangle]
pub extern "C" fn latex_save_atomic(
    project_root: *const c_char,
    rel_path: *const c_char,
    content: *const c_char,
) -> *mut c_char {
    let payload = catch_unwind(AssertUnwindSafe(|| {
        if content.is_null() {
            return json_error("null pointer argument");
        }
        let (root, rel) = match path_from_args(project_root, rel_path) {
            Ok(v) => v,
            Err(e) => return json_error(&e),
        };
        let body = unsafe { CStr::from_ptr(content) }
            .to_string_lossy()
            .into_owned();

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

#[no_mangle]
pub extern "C" fn latex_autosave_write(
    project_root: *const c_char,
    rel_path: *const c_char,
    content: *const c_char,
) -> *mut c_char {
    let payload = catch_unwind(AssertUnwindSafe(|| {
        if content.is_null() {
            return json_error("null pointer argument");
        }
        let (root, rel) = match path_from_args(project_root, rel_path) {
            Ok(v) => v,
            Err(e) => return json_error(&e),
        };
        let body = unsafe { CStr::from_ptr(content) }
            .to_string_lossy()
            .into_owned();
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
        if project_root.is_null() {
            return json_error("null pointer argument");
        }
        let root = unsafe { CStr::from_ptr(project_root) }
            .to_string_lossy()
            .into_owned();
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
        let (root, rel) = match path_from_args(project_root, rel_path) {
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
        let (root, rel) = match path_from_args(project_root, rel_path) {
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
        if name.is_null() || target_dir.is_null() {
            return json_error("null pointer argument");
        }
        let name_s = unsafe { CStr::from_ptr(name) }
            .to_string_lossy()
            .into_owned();
        let target_s = unsafe { CStr::from_ptr(target_dir) }
            .to_string_lossy()
            .into_owned();
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
