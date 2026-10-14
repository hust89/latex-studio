use anyhow::Result;
use latex_engine_tectonic::TectonicEngine;
use latex_model::BuildResult;
use std::path::PathBuf;

pub struct BuildRequest {
    pub entry: PathBuf,
    pub project_root: PathBuf,
    pub out_parent: PathBuf,
}

pub struct Builder { pub engine: TectonicEngine }

impl Builder {
    pub fn new(engine: TectonicEngine) -> Self { Self { engine } }

    pub async fn build(&self, req: &BuildRequest) -> Result<BuildResult> {
        self.engine
            .compile(&req.entry, &req.project_root, &req.out_parent)
            .await
    }
}
