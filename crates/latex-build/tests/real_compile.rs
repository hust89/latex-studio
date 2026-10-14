use latex_build::{BuildRequest, Builder};
use latex_engine_tectonic::TectonicEngine;
use latex_model::DiagnosticKind;

fn tectonic_available() -> bool {
    std::process::Command::new("tectonic")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Returns false if the test should be skipped. When LATEX_REQUIRE_TECTONIC is
/// set, a missing engine is a hard failure rather than a silent skip.
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
        entry: dir.path().join("main.tex"),
        project_root: dir.path().to_path_buf(),
        out_parent: out,
    };
    let r = Builder::new(TectonicEngine::default()).build(&req).await.unwrap();
    (dir, r)
}

#[test]
fn compiles_minimal_document() {
    if !require_tectonic() { return; }
    let rt = tokio::runtime::Runtime::new().unwrap();
    let (_dir, r) = rt.block_on(compile_source(
        "\\documentclass{article}\\begin{document}Hello.\\end{document}"
    ));
    assert!(r.success, "build failed: {:?}", r.diagnostics);
    let pdf = r.pdf_path.expect("pdf");
    assert!(std::path::Path::new(&pdf).is_file());
    // PDF must be inside this build's unique subdir.
    assert!(pdf.contains(&r.build_id), "pdf {pdf} not inside build {}", r.build_id);
}

#[test]
fn reports_undefined_control_sequence() {
    if !require_tectonic() { return; }
    let rt = tokio::runtime::Runtime::new().unwrap();
    let (_dir, r) = rt.block_on(compile_source(
        "\\documentclass{article}\\begin{document}\\notacommand\\end{document}"
    ));
    assert!(!r.success);
    assert!(
        r.diagnostics.iter().any(|d| d.kind == DiagnosticKind::UndefinedCommand),
        "expected UndefinedCommand, got {:?}", r.diagnostics
    );
}

#[test]
fn reports_unclosed_environment_or_similar() {
    if !require_tectonic() { return; }
    let rt = tokio::runtime::Runtime::new().unwrap();
    let (_dir, r) = rt.block_on(compile_source(
        "\\documentclass{article}\\begin{document}\\begin{itemize}\\item x\\end{document}"
    ));
    assert!(!r.success);
    assert!(!r.diagnostics.is_empty(), "expected diagnostics for unclosed environment");
}

#[test]
fn failed_build_does_not_return_stale_pdf() {
    if !require_tectonic() { return; }
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("main.tex"),
        "\\documentclass{article}\\begin{document}ok\\end{document}").unwrap();
    let out = dir.path().join(".latex-studio-build");
    std::fs::create_dir_all(&out).unwrap();
    let rt = tokio::runtime::Runtime::new().unwrap();

    // First build succeeds.
    let req = BuildRequest {
        entry: dir.path().join("main.tex"),
        project_root: dir.path().to_path_buf(),
        out_parent: out.clone(),
    };
    let ok = rt.block_on(Builder::new(TectonicEngine::default()).build(&req)).unwrap();
    assert!(ok.success);
    let first_pdf = ok.pdf_path.clone().unwrap();

    // Break the source, rebuild.
    std::fs::write(dir.path().join("main.tex"),
        "\\documentclass{article}\\begin{document}\\badcmd\\end{document}").unwrap();
    let bad = rt.block_on(Builder::new(TectonicEngine::default()).build(&req)).unwrap();
    assert!(!bad.success);
    // Must not return the first build's PDF.
    assert_ne!(bad.pdf_path.as_deref(), Some(first_pdf.as_str()));
    assert!(bad.pdf_path.is_none(), "failed build returned pdf: {:?}", bad.pdf_path);
}
