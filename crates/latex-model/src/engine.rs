//! Engine abstraction.
//!
//! Every LaTeX engine — Tectonic, pdfLaTeX, XeLaTeX, LuaLaTeX — implements
//! `Engine`. The rest of the system never depends on a concrete engine.

use crate::BuildResult;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct EngineRequest {
    pub entry: PathBuf,
    pub project_root: PathBuf,
    pub out_parent: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EngineInfo {
    pub id: String,
    pub display_name: String,
    pub available: bool,
    pub notes: Option<String>,
}

#[async_trait]
pub trait Engine: Send + Sync {
    fn id(&self) -> &'static str;
    fn display_name(&self) -> &'static str;
    fn available(&self) -> bool;
    fn notes(&self) -> Option<String> {
        None
    }
    async fn compile(&self, req: EngineRequest) -> anyhow::Result<BuildResult>;
}
