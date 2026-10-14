//! Build orchestration. Selects an engine by id and runs it.

use anyhow::{bail, Result};
use latex_engine_system::SystemEngine;
use latex_engine_tectonic::TectonicEngine;
use latex_model::{BuildResult, Engine, EngineInfo, EngineRequest};
use std::path::PathBuf;

pub struct BuildRequest {
    pub engine_id: String,
    pub entry: PathBuf,
    pub project_root: PathBuf,
    pub out_parent: PathBuf,
}

pub fn all_engines() -> Vec<Box<dyn Engine>> {
    vec![
        Box::new(TectonicEngine::default()),
        Box::new(SystemEngine::pdflatex()),
        Box::new(SystemEngine::xelatex()),
        Box::new(SystemEngine::lualatex()),
    ]
}

pub fn find_engine(id: &str) -> Option<Box<dyn Engine>> {
    all_engines().into_iter().find(|e| e.id() == id)
}

pub fn list_engines() -> Vec<EngineInfo> {
    all_engines()
        .into_iter()
        .map(|e| EngineInfo {
            id: e.id().to_string(),
            display_name: e.display_name().to_string(),
            available: e.available(),
            notes: e.notes(),
        })
        .collect()
}

pub fn default_engine_id() -> &'static str {
    "tectonic"
}

pub struct Builder;

impl Builder {
    pub async fn build(&self, req: &BuildRequest) -> Result<BuildResult> {
        let engine = find_engine(&req.engine_id)
            .ok_or_else(|| anyhow::anyhow!("unknown engine: {}", req.engine_id))?;
        if !engine.available() {
            let note = engine
                .notes()
                .unwrap_or_else(|| "engine not available".into());
            bail!("{} is not available: {}", engine.display_name(), note);
        }
        engine
            .compile(EngineRequest {
                entry: req.entry.clone(),
                project_root: req.project_root.clone(),
                out_parent: req.out_parent.clone(),
            })
            .await
    }
}
