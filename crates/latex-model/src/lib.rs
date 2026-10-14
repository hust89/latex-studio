use serde::{Deserialize, Serialize};

pub mod engine;
pub use engine::{Engine, EngineInfo, EngineRequest};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SourceLocation {
    pub file: String,
    pub line: u32,
    pub column: Option<u32>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum Severity { Error, Warning, Info }

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum DiagnosticKind {
    Syntax,
    UndefinedCommand,
    MissingPackage,
    UnresolvedReference,
    UnresolvedCitation,
    MissingAsset,
    Bibliography,
    CompilationFailure,
    UnclosedEnvironment,
    Other,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Diagnostic {
    pub severity: Severity,
    pub kind: DiagnosticKind,
    pub message: String,
    pub explanation: Option<String>,
    pub location: Option<SourceLocation>,
    pub raw: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuildResult {
    pub success: bool,
    pub build_id: String,
    pub exit_code: Option<i32>,
    pub pdf_path: Option<String>,
    pub diagnostics: Vec<Diagnostic>,
    pub stdout: String,
    pub stderr: String,
    pub duration_ms: u128,
}
