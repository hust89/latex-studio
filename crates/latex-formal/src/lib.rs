//! Formal verification integration.
//!
//! One system is supported in this milestone: Lean 4, invoked as a
//! subprocess on a temporary `.lean` file.
//!
//! The result is a strict tri-state:
//!
//!   - `Verified`      — Lean ran and exited zero.
//!   - `NotVerified`   — Lean ran, exited non-zero, and reported a problem.
//!   - `Unavailable`   — Lean was not on PATH, timed out, or could not be
//!                       launched. Never reported as a verification.
//!
//! No other code in the project may treat `Unavailable` as success. The FFI
//! and the UI both propagate this distinction unchanged.
//!
//! Note on streams: Lean 4's CLI writes its error and warning text to
//! **stdout**, not stderr. Both are captured and combined in the
//! `diagnostics` field. The exit code is the authority on whether the
//! fragment was accepted.
//!
//! Scope limitation, stated so it cannot be mistaken: a bare `lean` on a
//! standalone file sees only Lean's core library. Mathlib is not available
//! unless the caller supplies a project directory that imports it. A
//! `Verified` result therefore means "Lean accepted this fragment against
//! what it can see", which may be less than the user expected. The crate
//! makes no claim beyond Lean's own output.
//!
//! First-run cost: the first Lean invocation on a fresh machine may build
//! its own library cache. On WSL that can take 60–90 seconds. Subsequent
//! invocations of a trivial fragment complete in well under a second.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;
use tokio::process::Command;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum FormalResult {
    /// Lean exited zero. Its stdout is preserved.
    Verified {
        system: String,
        stdout: String,
    },
    /// Lean exited non-zero. `diagnostics` contains stdout and stderr
    /// combined, which is where Lean prints its error text.
    NotVerified {
        system: String,
        diagnostics: String,
        exit_code: Option<i32>,
    },
    /// The check did not run to a verdict. Never a positive result.
    Unavailable {
        system: String,
        reason: String,
    },
}

impl FormalResult {
    pub fn is_verified(&self) -> bool {
        matches!(self, FormalResult::Verified { .. })
    }
    pub fn is_unavailable(&self) -> bool {
        matches!(self, FormalResult::Unavailable { .. })
    }
    pub fn system(&self) -> &str {
        match self {
            FormalResult::Verified { system, .. } => system,
            FormalResult::NotVerified { system, .. } => system,
            FormalResult::Unavailable { system, .. } => system,
        }
    }
}

#[derive(Debug, Clone)]
pub struct FormalRequest {
    pub preamble: String,
    pub statement: String,
    pub timeout: Duration,
    pub project_dir: Option<PathBuf>,
}

impl FormalRequest {
    pub fn new(statement: impl Into<String>) -> Self {
        Self {
            preamble: String::new(),
            statement: statement.into(),
            timeout: Duration::from_secs(120),
            project_dir: None,
        }
    }
    pub fn with_preamble(mut self, preamble: impl Into<String>) -> Self {
        self.preamble = preamble.into();
        self
    }
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }
    pub fn with_project_dir(mut self, dir: impl Into<PathBuf>) -> Self {
        self.project_dir = Some(dir.into());
        self
    }
    fn full_source(&self) -> String {
        if self.preamble.trim().is_empty() {
            self.statement.clone()
        } else {
            let mut s = self.preamble.trim_end().to_string();
            s.push('\n');
            s.push('\n');
            s.push_str(self.statement.trim_end());
            s.push('\n');
            s
        }
    }
}

#[async_trait]
pub trait FormalSystem: Send + Sync {
    fn name(&self) -> &'static str;
    fn available(&self) -> bool;
    async fn check(&self, req: FormalRequest) -> FormalResult;
}

// ---------- Lean 4 ----------------------------------------------------------

#[derive(Debug, Clone)]
pub struct LeanEngine {
    pub binary: PathBuf,
}

impl Default for LeanEngine {
    fn default() -> Self {
        Self { binary: PathBuf::from("lean") }
    }
}

impl LeanEngine {
    pub fn new(binary: impl Into<PathBuf>) -> Self {
        Self { binary: binary.into() }
    }
    pub fn available_on_path() -> bool {
        Self::default().available()
    }
}

fn binary_available(bin: &Path) -> bool {
    std::process::Command::new(bin)
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

#[async_trait]
impl FormalSystem for LeanEngine {
    fn name(&self) -> &'static str {
        "lean4"
    }

    fn available(&self) -> bool {
        binary_available(&self.binary)
    }

    async fn check(&self, req: FormalRequest) -> FormalResult {
        let source = req.full_source();

        let tmp_dir = match make_temp_dir() {
            Ok(d) => d,
            Err(e) => {
                return FormalResult::Unavailable {
                    system: self.name().into(),
                    reason: format!("could not create temp directory: {e}"),
                };
            }
        };

        let lean_file = tmp_dir.path().join("fragment.lean");
        if let Err(e) = tokio::fs::write(&lean_file, source.as_bytes()).await {
            return FormalResult::Unavailable {
                system: self.name().into(),
                reason: format!("could not write fragment: {e}"),
            };
        }

        let mut cmd = Command::new(&self.binary);
        if let Some(dir) = req.project_dir.as_deref() {
            if !dir.is_dir() {
                return FormalResult::Unavailable {
                    system: self.name().into(),
                    reason: format!("project_dir is not a directory: {}", dir.display()),
                };
            }
            cmd.current_dir(dir);
        } else {
            cmd.current_dir(tmp_dir.path());
        }
        cmd.arg(&lean_file)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);

        let output = match tokio::time::timeout(req.timeout, cmd.output()).await {
            Ok(Ok(o)) => o,
            Ok(Err(e)) => {
                return FormalResult::Unavailable {
                    system: self.name().into(),
                    reason: format!("failed to spawn lean: {e}"),
                };
            }
            Err(_) => {
                return FormalResult::Unavailable {
                    system: self.name().into(),
                    reason: format!("lean timed out after {:?}", req.timeout),
                };
            }
        };

        let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
        let stderr = String::from_utf8_lossy(&output.stderr).into_owned();

        if output.status.success() {
            FormalResult::Verified {
                system: self.name().into(),
                stdout,
            }
        } else {
            // Lean writes errors to stdout. Combine both streams for the
            // diagnostics field so nothing is lost regardless of version.
            let mut diag = stdout;
            if !stderr.trim().is_empty() {
                if !diag.is_empty() && !diag.ends_with('\n') {
                    diag.push('\n');
                }
                diag.push_str(&stderr);
            }
            FormalResult::NotVerified {
                system: self.name().into(),
                diagnostics: diag.trim().to_string(),
                exit_code: output.status.code(),
            }
        }
    }
}

// ---------- temp dir helper -------------------------------------------------

struct TempDir {
    path: PathBuf,
}

impl TempDir {
    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

fn make_temp_dir() -> std::io::Result<TempDir> {
    let base = std::env::temp_dir();
    let unique = format!(
        "latex-formal-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    );
    let path = base.join(unique);
    std::fs::create_dir_all(&path)?;
    Ok(TempDir { path })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lean_available() -> bool {
        LeanEngine::available_on_path()
    }

    #[tokio::test]
    async fn unavailable_when_binary_missing() {
        let engine = LeanEngine::new("/nonexistent/lean-that-does-not-exist");
        let r = engine
            .check(FormalRequest::new("theorem t : 1 = 1 := rfl"))
            .await;
        assert!(r.is_unavailable(), "{r:?}");
    }

    #[tokio::test]
    async fn project_dir_missing_is_unavailable() {
        let engine = LeanEngine::default();
        let r = engine
            .check(
                FormalRequest::new("theorem t : 1 = 1 := rfl")
                    .with_project_dir("/no/such/dir"),
            )
            .await;
        assert!(r.is_unavailable(), "{r:?}");
    }

    #[tokio::test]
    async fn verified_on_trivial_truth() {
        if !lean_available() {
            eprintln!("SKIPPED (lean not on PATH)");
            return;
        }
        let engine = LeanEngine::default();
        let r = engine
            .check(FormalRequest::new("theorem t : 1 + 1 = 2 := rfl"))
            .await;
        assert!(r.is_verified(), "{r:?}");
    }

    #[tokio::test]
    async fn not_verified_on_false_statement() {
        if !lean_available() {
            eprintln!("SKIPPED (lean not on PATH)");
            return;
        }
        let engine = LeanEngine::default();
        let r = engine
            .check(FormalRequest::new("theorem t : 1 + 1 = 3 := rfl"))
            .await;
        assert!(!r.is_verified(), "{r:?}");
        match r {
            FormalResult::NotVerified { diagnostics, exit_code, .. } => {
                assert!(!diagnostics.is_empty(), "expected diagnostic text");
                assert_ne!(exit_code, Some(0), "expected non-zero exit");
            }
            _ => panic!("expected NotVerified"),
        }
    }

    #[test]
    fn full_source_joins_preamble_and_statement() {
        let req = FormalRequest::new("theorem t : 1 = 1 := rfl").with_preamble("import Std\n");
        let s = req.full_source();
        assert!(s.starts_with("import Std"));
        assert!(s.contains("theorem t : 1 = 1 := rfl"));
    }

    #[test]
    fn full_source_without_preamble() {
        let req = FormalRequest::new("example : 1 = 1 := rfl");
        assert_eq!(req.full_source(), "example : 1 = 1 := rfl");
    }
}
