use latex_build::{BuildRequest, Builder};
use latex_model::DiagnosticKind;
use std::sync::{Mutex, OnceLock};

fn engine_lock() -> &'static Mutex<()> {
    static L: OnceLock<Mutex<()>> = OnceLock::new();
    L.get_or_init(|| Mutex::new(()))
}

fn tectonic_available() -> bool {
    std::process::Command::new("tectonic")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn require_tectonic() -> bool {
    if tectonic_available() { return true; }
    if std::env::var("LATEX_REQUIRE_TECTONIC").is_ok() {
        panic!("LATEX_REQUIRE_TECTONIC is set but `tectonic` is not on PATH");
    }
    eprintln!("SKIPPED (tectonic not on PATH; set LATEX_REQUIRE_TECTONIC=1 to fail)");
    false
}

async fn compile_source(src: &str) -> (tempfile::TempDir, latex_model::BuildResult) {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("main.tex"), src).unwrap();
    let out = dir.path().join(".latex-studio-build");
    std::fs::create_dir_all(&out).unwrap();
    let req = BuildRequest {
        engine_id: "tectonic".into(),
        entry: dir.path().join("main.tex"),
        project_root: dir.path().to_path_buf(),
        out_parent: out,
    };
    let t0 = std::time::Instant::now();
    eprintln!("  -> invoking tectonic (offline) ...");
    let r = Builder.build(&req).await.unwrap();
    eprintln!("  -> finished in {:?} (success={})", t0.elapsed(), r.success);
    (dir, r)
}

#[test]
fn compiles_minimal_document() {
    if !require_tectonic() { return; }
    let _g = engine_lock().lock().unwrap();
    std::env::set_var("LATEX_TECTONIC_NETWORK", "offline");
    let rt = tokio::runtime::Runtime::new().unwrap();
    let (_dir, r) = rt.block_on(compile_source(
        "\\documentclass{article}\\begin{document}Hello.\\end{document}"
    ));
    assert!(r.success, "build failed: {:?}", r.diagnostics);
    let pdf = r.pdf_path.expect("pdf");
    assert!(std::path::Path::new(&pdf).is_file());
    assert!(pdf.contains(&r.build_id), "pdf {pdf} not inside build {}", r.build_id);
}

#[test]
fn reports_undefined_control_sequence() {
    if !require_tectonic() { return; }
    let _g = engine_lock().lock().unwrap();
    std::env::set_var("LATEX_TECTONIC_NETWORK", "offline");
    let rt = tokio::runtime::Runtime::new().unwrap();
    let (_dir, r) = rt.block_on(compile_source(
        "\\documentclass{article}\\begin{document}\\notacommand\\end{document}"
    ));
    assert!(!r.success);
    assert!(
        r.diagnostics.iter().any(|d| d.kind == DiagnosticKind::UndefinedCommand
            || matches!(d.severity, latex_model::Severity::Error)),
        "expected an error diagnostic, got {:?}", r.diagnostics
    );
}

#[test]
fn reports_unclosed_environment_or_similar() {
    if !require_tectonic() { return; }
    let _g = engine_lock().lock().unwrap();
    std::env::set_var("LATEX_TECTONIC_NETWORK", "offline");
    let rt = tokio::runtime::Runtime::new().unwrap();
    let (_dir, r) = rt.block_on(compile_source(
        "\\documentclass{article}\\begin{document}\\begin{itemize}\\item x\\end{document}"
    ));
    assert!(!r.success);
    assert!(!r.diagnostics.is_empty(),
        "expected diagnostics for unclosed environment");
}

#[test]
fn failed_build_does_not_return_stale_pdf() {
    if !require_tectonic() { return; }
    let _g = engine_lock().lock().unwrap();
    std::env::set_var("LATEX_TECTONIC_NETWORK", "offline");
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("main.tex"),
        "\\documentclass{article}\\begin{document}ok\\end{document}").unwrap();
    let out = dir.path().join(".latex-studio-build");
    std::fs::create_dir_all(&out).unwrap();
    let rt = tokio::runtime::Runtime::new().unwrap();

    let req = BuildRequest {
        engine_id: "tectonic".into(),
        entry: dir.path().join("main.tex"),
        project_root: dir.path().to_path_buf(),
        out_parent: out.clone(),
    };
    let ok = rt.block_on(Builder.build(&req)).unwrap();
    assert!(ok.success);
    let first_pdf = ok.pdf_path.clone().unwrap();

    std::fs::write(dir.path().join("main.tex"),
        "\\documentclass{article}\\begin{document}\\badcmd\\end{document}").unwrap();
    let bad = rt.block_on(Builder.build(&req)).unwrap();
    assert!(!bad.success);
    assert_ne!(bad.pdf_path.as_deref(), Some(first_pdf.as_str()));
    assert!(bad.pdf_path.is_none(), "failed build returned pdf: {:?}", bad.pdf_path);
}

#[test]
fn unknown_engine_is_rejected() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("main.tex"),
        "\\documentclass{article}\\begin{document}x\\end{document}").unwrap();
    let out = dir.path().join("out");
    std::fs::create_dir_all(&out).unwrap();
    let req = BuildRequest {
        engine_id: "not-an-engine".into(),
        entry: dir.path().join("main.tex"),
        project_root: dir.path().to_path_buf(),
        out_parent: out,
    };
    let rt = tokio::runtime::Runtime::new().unwrap();
    let err = rt.block_on(Builder.build(&req)).unwrap_err();
    let msg = format!("{err:#}");
    assert!(msg.contains("unknown engine"), "{msg}");
}
