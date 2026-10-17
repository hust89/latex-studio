//! Tectonic engine adapter, in-process.
//!
//! Uses `tectonic::driver::ProcessingSessionBuilder` directly. No subprocess.
//! Same code path on Linux, macOS, Windows, Android.

use anyhow::{Context, Result};
use async_trait::async_trait;
use latex_model::{
    BuildPhase, BuildResult, Diagnostic, DiagnosticKind, Engine, EngineRequest, Severity,
    SourceLocation,
};
use std::path::{Path, PathBuf};
use std::time::{Instant, SystemTime, UNIX_EPOCH};
use tectonic::config::PersistentConfig;
use tectonic::driver::ProcessingSessionBuilder;
use tectonic::errors::Error;
use tectonic::status::{MessageKind, StatusBackend};

#[derive(Debug, Clone)]
pub struct TectonicEngine {
    pub cache_dir: Option<PathBuf>,
    pub config_dir: Option<PathBuf>,
}

impl Default for TectonicEngine {
    fn default() -> Self {
        Self {
            cache_dir: std::env::var_os("LATEX_STUDIO_CACHE_DIR").map(PathBuf::from),
            config_dir: std::env::var_os("LATEX_STUDIO_CONFIG_DIR").map(PathBuf::from),
        }
    }
}

impl TectonicEngine {
    pub fn new() -> Self { Self::default() }
    pub fn with_cache_dir(mut self, dir: impl Into<PathBuf>) -> Self {
        self.cache_dir = Some(dir.into()); self
    }
    pub fn with_config_dir(mut self, dir: impl Into<PathBuf>) -> Self {
        self.config_dir = Some(dir.into()); self
    }
}

#[derive(Default)]
struct Collector {
    diagnostics: Vec<Diagnostic>,
    phase: Option<BuildPhase>,
    raw_output: String,
}

impl Collector {
    fn new() -> Self { Self::default() }
}

impl StatusBackend for Collector {
    fn report(&mut self, kind: MessageKind, args: std::fmt::Arguments<'_>, _err: Option<&Error>) {
        let text = format!("{}", args).trim().to_string();
        if text.is_empty() { return; }

        if text.starts_with("Running TeX") {
            self.phase = Some(BuildPhase::Compiling);
        } else if text.starts_with("Running BibTeX") || text.starts_with("Running Biber") {
            self.phase = Some(BuildPhase::Bibliography);
        } else if text.starts_with("Rerunning TeX") {
            self.phase = Some(BuildPhase::Rerun);
        } else if text.starts_with("Running xdvipdfmx") || text.starts_with("Running dvipdfmx") {
            self.phase = Some(BuildPhase::Rendering);
        }

        match kind {
            MessageKind::Error => self.diagnostics.push(Diagnostic {
                severity: Severity::Error,
                kind: classify(&text),
                message: text.clone(),
                explanation: None,
                location: parse_location(&text),
                raw: None,
            }),
            MessageKind::Warning => self.diagnostics.push(Diagnostic {
                severity: Severity::Warning,
                kind: classify_warning(&text),
                message: text.clone(),
                explanation: None,
                location: parse_location(&text),
                raw: None,
            }),
            _ => {}
        }

        self.raw_output.push_str(&text);
        self.raw_output.push('\n');
    }

    fn dump_error_logs(&mut self, output: &[u8]) {
        if let Ok(s) = std::str::from_utf8(output) {
            self.raw_output.push_str(s);
            if !s.ends_with('\n') { self.raw_output.push('\n'); }
        }
    }
}

#[async_trait]
impl Engine for TectonicEngine {
    fn id(&self) -> &'static str { "tectonic" }
    fn display_name(&self) -> &'static str { "Tectonic" }
    fn available(&self) -> bool { true }
    fn notes(&self) -> Option<String> {
        if self.cache_dir.is_none() {
            Some("first run downloads the TeX bundle (~300 MB)".into())
        } else { None }
    }

    async fn compile(&self, req: EngineRequest) -> Result<BuildResult> {
        let entry = req.entry.clone();
        let project_root = req.project_root.clone();
        let out_parent = req.out_parent.clone();
        let cache_dir = self.cache_dir.clone();
        let config_dir = self.config_dir.clone();

        let joined = tokio::task::spawn_blocking(move || {
            compile_blocking(&entry, &project_root, &out_parent, cache_dir, config_dir)
        })
        .await
        .context("tectonic task panicked")?;

        joined
    }
}

fn compile_blocking(
    entry: &Path,
    project_root: &Path,
    out_parent: &Path,
    cache_dir: Option<PathBuf>,
    config_dir: Option<PathBuf>,
) -> Result<BuildResult> {
    let build_id = format!(
        "build_{}",
        SystemTime::now().duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos()).unwrap_or(0)
    );
    let out_dir = out_parent.join(&build_id);
    std::fs::create_dir_all(&out_dir)
        .with_context(|| format!("create {}", out_dir.display()))?;

    let entry_abs = entry.canonicalize()
        .with_context(|| format!("canonicalize {}", entry.display()))?;

    let start = Instant::now();
    let mut status = Collector::new();

    if let Some(dir) = config_dir.as_deref() {
        std::env::set_var("TECTONIC_CONFIG_DIR", dir);
    }
    if let Some(dir) = cache_dir.as_deref() {
        std::env::set_var("TECTONIC_CACHE_DIR", dir);
    }

    let config = PersistentConfig::open(false)
        .context("open tectonic config")?;
    let bundle = config.default_bundle(false)
        .context("load tectonic bundle")?;
    let format_cache = config.format_cache_path()
        .context("format cache path")?;

    let primary_name = entry_abs.file_name()
        .and_then(|s| s.to_str()).unwrap_or("main.tex").to_string();

    let mut builder = ProcessingSessionBuilder::default();
    builder
        .bundle(bundle)
        .primary_input_path(&entry_abs)
        .tex_input_name(&primary_name)
        .format_name("latex")
        .format_cache_path(&format_cache)
        .output_dir(&out_dir)
        .keep_intermediates(true)
        .filesystem_root(project_root);

    let mut session = builder.create(&mut status)
        .context("create tectonic session")?;

    let run_result = session.run(&mut status);
    let success = run_result.is_ok();

    let stem = entry_abs.file_stem()
        .and_then(|s| s.to_str()).unwrap_or("main");
    let candidate = out_dir.join(format!("{stem}.pdf"));
    let pdf = candidate.is_file().then_some(candidate);

    let phase = if success && pdf.is_some() {
        BuildPhase::Done
    } else {
        status.phase.unwrap_or(if success { BuildPhase::Done } else { BuildPhase::Failed })
    };

    Ok(BuildResult {
        success: success && pdf.is_some(),
        build_id,
        exit_code: if success { Some(0) } else { Some(1) },
        pdf_path: pdf.map(|p| p.to_string_lossy().into_owned()),
        diagnostics: status.diagnostics,
        stdout: status.raw_output,
        stderr: String::new(),
        duration_ms: start.elapsed().as_millis(),
        phase,
    })
}

fn classify(text: &str) -> DiagnosticKind {
    let m = text.to_ascii_lowercase();
    if m.contains("undefined control sequence") { DiagnosticKind::UndefinedCommand }
    else if m.contains("file") && m.contains("not found") { DiagnosticKind::MissingAsset }
    else if m.contains("package") && m.contains("not found") { DiagnosticKind::MissingPackage }
    else if m.contains("environment") && m.contains("undefined") { DiagnosticKind::UnclosedEnvironment }
    else if m.contains("citation") && m.contains("undefined") { DiagnosticKind::UnresolvedCitation }
    else if m.contains("reference") && m.contains("undefined") { DiagnosticKind::UnresolvedReference }
    else if m.contains("emergency stop") || m.contains("fatal error") { DiagnosticKind::CompilationFailure }
    else { DiagnosticKind::Other }
}

fn classify_warning(text: &str) -> DiagnosticKind {
    let m = text.to_ascii_lowercase();
    if m.contains("citation") && m.contains("undefined") { DiagnosticKind::UnresolvedCitation }
    else if m.contains("reference") && m.contains("undefined") { DiagnosticKind::UnresolvedReference }
    else { DiagnosticKind::Other }
}

fn parse_location(text: &str) -> Option<SourceLocation> {
    let mut parts = text.splitn(4, ':');
    let file = parts.next()?.trim();
    if file.contains(' ') || file.is_empty() { return None; }
    let line: u32 = parts.next()?.trim().parse().ok()?;
    let third = parts.next()?.trim();
    let rest = parts.next();
    match (third.parse::<u32>(), rest) {
        (Ok(col), _) => Some(SourceLocation {
            file: file.to_string(), line, column: Some(col),
        }),
        (Err(_), Some(_)) => Some(SourceLocation {
            file: file.to_string(), line, column: None,
        }),
        _ => None,
    }
}
