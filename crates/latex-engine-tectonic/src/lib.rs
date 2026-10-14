use async_trait::async_trait;
use anyhow::{Context, Result};
use latex_model::{BuildResult, Diagnostic, DiagnosticKind, Severity, SourceLocation, Engine, EngineRequest};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tokio::process::Command;

#[derive(Debug)]
pub struct TectonicEngine {
    pub binary: PathBuf,
    pub timeout: Duration,
}

impl Default for TectonicEngine {
    fn default() -> Self {
        Self { binary: PathBuf::from("tectonic"), timeout: Duration::from_secs(180) }
    }
}

impl TectonicEngine {
    pub fn new(binary: impl Into<PathBuf>) -> Self {
        Self { binary: binary.into(), ..Default::default() }
    }
    pub fn with_timeout(mut self, t: Duration) -> Self { self.timeout = t; self }

    /// Compile `entry`. All artefacts are written under
    /// `<out_parent>/<build_id>/`, so a PDF returned here cannot be a leftover
    /// from a previous run.
    ///
    /// `project_root` becomes the process working directory, so relative
    /// `\input`, `\includegraphics`, and `.bib` paths resolve the way the user
    /// expects.
    async fn compile_inner(
        &self,
        entry: &Path,
        project_root: &Path,
        out_parent: &Path,
    ) -> Result<BuildResult> {
        let build_id = format!(
            "build_{}",
            SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos()
        );
        let out_dir = out_parent.join(&build_id);
        tokio::fs::create_dir_all(&out_dir).await
            .with_context(|| format!("cannot create build dir {:?}", out_dir))?;

        // Entry path is passed absolute so it is unambiguous regardless of cwd.
        let entry_abs = entry.canonicalize()
            .with_context(|| format!("cannot resolve entry file {:?}", entry))?;

        let start = Instant::now();
        let fut = Command::new(&self.binary)
            .current_dir(project_root)
            .arg("--outdir").arg(&out_dir)
            .arg("--keep-logs")
            .arg(&entry_abs)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output();

        let output = match tokio::time::timeout(self.timeout, fut).await {
            Ok(Ok(o))  => o,
            Ok(Err(e)) => return Err(e).with_context(|| format!("failed to spawn {:?}", self.binary)),
            Err(_)     => anyhow::bail!("tectonic timed out after {:?}", self.timeout),
        };

        let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
        let stderr = String::from_utf8_lossy(&output.stderr).into_owned();

        // Prefer the .log inside this build's out_dir; concatenate modern
        // stderr rendering from Tectonic as a secondary source.
        let log_text = read_adjacent_log(&out_dir, &entry_abs).unwrap_or_default();
        let mut diagnostics = parse_tex_log(&log_text, &entry_abs);
        diagnostics.extend(parse_tectonic_stderr(&stderr, &entry_abs));

        let pdf = find_pdf(&out_dir, &entry_abs);
        let success = output.status.success() && pdf.is_some();

        Ok(BuildResult {
            success,
            build_id,
            exit_code: output.status.code(),
            pdf_path: pdf.map(|p| p.to_string_lossy().into_owned()),
            diagnostics,
            stdout,
            stderr,
            duration_ms: start.elapsed().as_millis(),
        })
    }
}

fn read_adjacent_log(out_dir: &Path, entry: &Path) -> Option<String> {
    let stem = entry.file_stem()?.to_str()?;
    std::fs::read_to_string(out_dir.join(format!("{stem}.log"))).ok()
}

fn find_pdf(out_dir: &Path, entry: &Path) -> Option<PathBuf> {
    let stem = entry.file_stem()?.to_str()?;
    let p = out_dir.join(format!("{stem}.pdf"));
    p.is_file().then_some(p)
}

/// Parse the traditional TeX log format (pdfTeX, XeTeX, LuaTeX, Tectonic's
/// `.log` output). Extracts `! message` errors with a `l.NN` continuation and
/// `LaTeX Warning:` lines.
pub fn parse_tex_log(log: &str, entry: &Path) -> Vec<Diagnostic> {
    let lines: Vec<&str> = log.lines().collect();
    let mut diags = Vec::new();
    let mut i = 0usize;

    while i < lines.len() {
        let line = lines[i];

        if let Some(msg) = line.strip_prefix("! ") {
            let mut loc = None;
            let mut j = i + 1;
            let lookahead = (i + 6).min(lines.len());
            while j < lookahead {
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
                kind: classify(msg),
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
                kind: classify_warning(rest),
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

/// Parse Tectonic's modern stderr rendering:
///   error: main.tex:12: Undefined control sequence \foo
///   error: main.tex:3:7: expected ...
pub fn parse_tectonic_stderr(stderr: &str, entry: &Path) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    for line in stderr.lines() {
        let Some(rest) = line.strip_prefix("error: ") else { continue };
        // Look for "<file>:<line>:" possibly followed by "<col>:".
        let mut parts = rest.splitn(3, ':');
        let (Some(file_part), Some(line_part)) = (parts.next(), parts.next()) else { continue };
        let Ok(line_n) = line_part.trim().parse::<u32>() else { continue };
        let remainder = parts.next().unwrap_or("").trim();

        // If the next token is a bare integer, it's a column.
        let (col, message) = match remainder.split_once(':') {
            Some((c, m)) if c.trim().parse::<u32>().is_ok() =>
                (c.trim().parse::<u32>().ok(), m.trim()),
            _ => (None, remainder),
        };

        // Only attribute to entry file if the path matches its basename;
        // included files will have their own basename in the message.
        let file = if file_part.ends_with(
            entry.file_name().and_then(|s| s.to_str()).unwrap_or("")
        ) {
            entry.to_string_lossy().into_owned()
        } else {
            file_part.to_string()
        };

        out.push(Diagnostic {
            severity: Severity::Error,
            kind: classify(message),
            message: message.to_string(),
            explanation: None,
            location: Some(SourceLocation { file, line: line_n, column: col }),
            raw: Some(line.to_string()),
        });
    }
    out
}

fn classify(msg: &str) -> DiagnosticKind {
    let m = msg.to_ascii_lowercase();
    if m.contains("undefined control sequence")               { DiagnosticKind::UndefinedCommand }
    else if m.contains("file") && m.contains("not found")     { DiagnosticKind::MissingAsset }
    else if m.contains("package") && m.contains("not found")  { DiagnosticKind::MissingPackage }
    else if m.contains("environment") && m.contains("undefined") { DiagnosticKind::UnclosedEnvironment }
    else if m.contains("citation") && m.contains("undefined") { DiagnosticKind::UnresolvedCitation }
    else if m.contains("reference") && m.contains("undefined"){ DiagnosticKind::UnresolvedReference }
    else if m.contains("emergency stop") || m.contains("fatal error") { DiagnosticKind::CompilationFailure }
    else                                                       { DiagnosticKind::Other }
}

fn classify_warning(msg: &str) -> DiagnosticKind {
    let m = msg.to_ascii_lowercase();
    if m.contains("citation") && m.contains("undefined")  { DiagnosticKind::UnresolvedCitation }
    else if m.contains("reference") && m.contains("undefined") { DiagnosticKind::UnresolvedReference }
    else                                                   { DiagnosticKind::Other }
}


#[async_trait]
impl Engine for TectonicEngine {
    fn id(&self) -> &'static str { "tectonic" }
    fn display_name(&self) -> &'static str { "Tectonic" }

    fn available(&self) -> bool {
        std::process::Command::new(&self.binary)
            .arg("--version")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }

    fn notes(&self) -> Option<String> {
        Some("single-binary, offline after first run".into())
    }

    async fn compile(&self, req: EngineRequest) -> anyhow::Result<BuildResult> {
        self.compile_inner(&req.entry, &req.project_root, &req.out_parent).await
    }
}
