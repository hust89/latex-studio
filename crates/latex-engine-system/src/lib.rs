//! System TeX engines: pdfLaTeX, XeLaTeX, LuaLaTeX.
//!
//! All three are driven through `latexmk`, which handles the rerun and
//! bibliography passes correctly. `latexmk` is required; a system engine
//! whose `latexmk` is missing reports as unavailable.
//!
//! Shell escape is never enabled. The engine runs with a timeout and is
//! killed on drop, matching the Tectonic adapter's behaviour.

use anyhow::{Context, Result};
use async_trait::async_trait;
use latex_model::{
    BuildPhase, BuildResult, Diagnostic, DiagnosticKind, Engine, EngineRequest, Severity,
    SourceLocation,
};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tokio::process::Command;

#[derive(Debug, Clone, Copy)]
pub enum SystemEngineKind {
    PdfLatex,
    XeLatex,
    LuaLatex,
}

impl SystemEngineKind {
    pub fn id(self) -> &'static str {
        match self {
            Self::PdfLatex => "pdflatex",
            Self::XeLatex => "xelatex",
            Self::LuaLatex => "lualatex",
        }
    }
    pub fn display_name(self) -> &'static str {
        match self {
            Self::PdfLatex => "pdfLaTeX (via latexmk)",
            Self::XeLatex => "XeLaTeX (via latexmk)",
            Self::LuaLatex => "LuaLaTeX (via latexmk)",
        }
    }
    fn latexmk_flag(self) -> &'static str {
        match self {
            Self::PdfLatex => "-pdf",
            Self::XeLatex => "-xelatex",
            Self::LuaLatex => "-lualatex",
        }
    }
    fn primary_binary(self) -> &'static str {
        match self {
            Self::PdfLatex => "pdflatex",
            Self::XeLatex => "xelatex",
            Self::LuaLatex => "lualatex",
        }
    }
}

#[derive(Debug)]
pub struct SystemEngine {
    pub kind: SystemEngineKind,
    pub latexmk: PathBuf,
    pub timeout: Duration,
}

impl SystemEngine {
    pub fn new(kind: SystemEngineKind) -> Self {
        Self {
            kind,
            latexmk: PathBuf::from("latexmk"),
            timeout: Duration::from_secs(300),
        }
    }

    pub fn pdflatex() -> Self { Self::new(SystemEngineKind::PdfLatex) }
    pub fn xelatex()  -> Self { Self::new(SystemEngineKind::XeLatex) }
    pub fn lualatex() -> Self { Self::new(SystemEngineKind::LuaLatex) }

    pub fn with_timeout(mut self, t: Duration) -> Self {
        self.timeout = t;
        self
    }
}

fn command_available(bin: &Path) -> bool {
    std::process::Command::new(bin)
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

#[async_trait]
impl Engine for SystemEngine {
    fn id(&self) -> &'static str { self.kind.id() }
    fn display_name(&self) -> &'static str { self.kind.display_name() }

    fn available(&self) -> bool {
        command_available(&self.latexmk)
            && command_available(Path::new(self.kind.primary_binary()))
    }

    fn notes(&self) -> Option<String> {
        if !command_available(&self.latexmk) {
            Some("latexmk not found on PATH".into())
        } else if !command_available(Path::new(self.kind.primary_binary())) {
            Some(format!("{} not found on PATH", self.kind.primary_binary()))
        } else {
            None
        }
    }

    async fn compile(&self, req: EngineRequest) -> Result<BuildResult> {
        let build_id = format!(
            "build_{}",
            SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos()
        );
        let out_dir = req.out_parent.join(&build_id);
        tokio::fs::create_dir_all(&out_dir).await
            .with_context(|| format!("cannot create build dir {:?}", out_dir))?;

        let entry_abs = req.entry.canonicalize()
            .with_context(|| format!("cannot resolve entry {:?}", req.entry))?;

        let start = Instant::now();

        let mut cmd = Command::new(&self.latexmk);
        cmd.current_dir(&req.project_root)
            .arg(self.kind.latexmk_flag())
            .arg("-interaction=nonstopmode")
            .arg("-halt-on-error")
            .arg(format!("-outdir={}", out_dir.display()))
            .arg(format!("-auxdir={}", out_dir.display()))
            .arg(&entry_abs)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);

        let output = match tokio::time::timeout(self.timeout, cmd.output()).await {
            Ok(Ok(o)) => o,
            Ok(Err(e)) => return Err(e).with_context(|| format!("failed to spawn {:?}", self.latexmk)),
            Err(_) => anyhow::bail!("{} timed out after {:?}", self.kind.id(), self.timeout),
        };

        let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
        let stderr = String::from_utf8_lossy(&output.stderr).into_owned();

        let log_text = read_log(&out_dir, &entry_abs).unwrap_or_default();
        let diagnostics = parse_tex_log(&log_text, &entry_abs);

        let pdf = find_pdf(&out_dir, &entry_abs);
        let success = output.status.success() && pdf.is_some();
        let phase = if success { BuildPhase::Done } else { BuildPhase::Failed };

        Ok(BuildResult {
            success,
            build_id,
            exit_code: output.status.code(),
            pdf_path: pdf.map(|p| p.to_string_lossy().into_owned()),
            diagnostics,
            stdout,
            stderr,
            duration_ms: start.elapsed().as_millis(),
            phase,
        })
    }
}

fn read_log(out_dir: &Path, entry: &Path) -> Option<String> {
    let stem = entry.file_stem()?.to_str()?;
    std::fs::read_to_string(out_dir.join(format!("{stem}.log"))).ok()
}

fn find_pdf(out_dir: &Path, entry: &Path) -> Option<PathBuf> {
    let stem = entry.file_stem()?.to_str()?;
    let p = out_dir.join(format!("{stem}.pdf"));
    p.is_file().then_some(p)
}

pub fn parse_tex_log(log: &str, entry: &Path) -> Vec<Diagnostic> {
    let lines: Vec<&str> = log.lines().collect();
    let mut diags = Vec::new();
    let mut i = 0usize;
    while i < lines.len() {
        let line = lines[i];
        if let Some(msg) = line.strip_prefix("! ") {
            let mut loc = None;
            let mut j = i + 1;
            let end_look = (i + 6).min(lines.len());
            while j < end_look {
                if let Some(rest) = lines[j].strip_prefix("l.") {
                    if let Some((n, _)) = rest.split_once(' ') {
                        if let Ok(ln) = n.parse::<u32>() {
                            loc = Some(SourceLocation {
                                file: entry.to_string_lossy().into_owned(),
                                line: ln,
                                column: None,
                            });
                        }
                    }
                    break;
                }
                j += 1;
            }
            let end = (j + 1).min(lines.len());
            diags.push(Diagnostic {
                severity: Severity::Error,
                kind: DiagnosticKind::Other,
                message: msg.trim().to_string(),
                explanation: None,
                location: loc,
                raw: Some(lines[i..end].join("\n")),
            });
            i = end;
            continue;
        }
        if let Some(rest) = line.strip_prefix("LaTeX Warning: ") {
            diags.push(Diagnostic {
                severity: Severity::Warning,
                kind: DiagnosticKind::Other,
                message: rest.trim().to_string(),
                explanation: None,
                location: None,
                raw: Some(line.to_string()),
            });
        }
        i += 1;
    }
    diags
}
