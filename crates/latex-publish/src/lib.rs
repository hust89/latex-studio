//! Publishing: portable source archive with a reproducible manifest.
//!
//! The archive contains:
//!   - every project source and asset file at its original relative path
//!   - `output.pdf` at the root, if a successful PDF exists under
//!     `.latex-studio-build/build_*/`
//!   - `MANIFEST.json` describing what is inside, with SHA-256 hashes and
//!     the engine that produced the PDF
//!
//! Build artefacts themselves (`.latex-studio-build/`) are excluded.

use anyhow::{anyhow, Context, Result};
use chrono::Utc;
use latex_project::Project;
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::fs::File;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use zip::write::SimpleFileOptions;

pub const MANIFEST_NAME: &str = "MANIFEST.json";
pub const PDF_NAME: &str = "output.pdf";

#[derive(Debug, Clone)]
pub struct PublishOptions {
    pub project_root: PathBuf,
    pub out_zip: PathBuf,
    pub engine_id: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct PublishReport {
    pub zip_path: String,
    pub file_count: usize,
    pub zip_bytes: u64,
    pub pdf_included: bool,
    pub manifest_in_zip: String,
    pub created_at: String,
    pub engine_id: String,
}

#[derive(Debug, Serialize)]
struct Manifest {
    created_at: String,
    generator: String,
    engine: String,
    entry: Option<String>,
    pdf_included: bool,
    files: Vec<ManifestFile>,
}

#[derive(Debug, Serialize)]
struct ManifestFile {
    path: String,
    bytes: u64,
    sha256: String,
}

pub fn create_archive(opts: &PublishOptions) -> Result<PublishReport> {
    let project = Project::open(&opts.project_root)?;
    let index = project.index()?;
    let entry = project.entry(&index);

    // Find the most recent successful PDF under .latex-studio-build/.
    let pdf = find_latest_pdf(&opts.project_root);

    // Open the output archive. Overwrite if it exists.
    if let Some(parent) = opts.out_zip.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("create {}", parent.display()))?;
    }
    let file = File::create(&opts.out_zip)
        .with_context(|| format!("create {}", opts.out_zip.display()))?;
    let mut zip = zip::ZipWriter::new(file);
    let opts_file = SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);

    let mut manifest_files: Vec<ManifestFile> = Vec::new();

    // Every file that latex-project knows about.
    for f in &index.files {
        let abs = opts.project_root.join(&f.path);
        let bytes = std::fs::read(&abs)
            .with_context(|| format!("read {}", abs.display()))?;
        let hash = sha256_hex(&bytes);

        let name = f.path.to_string_lossy().replace('\\', "/");
        zip.start_file(&name, opts_file)
            .with_context(|| format!("zip start {name}"))?;
        zip.write_all(&bytes)
            .with_context(|| format!("zip write {name}"))?;

        manifest_files.push(ManifestFile {
            path: name,
            bytes: bytes.len() as u64,
            sha256: hash,
        });
    }

    // Include the PDF if we found one.
    let mut pdf_included = false;
    if let Some(pdf_path) = pdf {
        let bytes = std::fs::read(&pdf_path)
            .with_context(|| format!("read pdf {}", pdf_path.display()))?;
        zip.start_file(PDF_NAME, opts_file)?;
        zip.write_all(&bytes)?;
        pdf_included = true;
    }

    // The manifest itself. Hashes cover what came out of Project::index()
    // plus the PDF, so anything can be verified after extraction.
    let manifest = Manifest {
        created_at: Utc::now().to_rfc3339(),
        generator: format!("LaTeX Studio {}", env!("CARGO_PKG_VERSION")),
        engine: opts.engine_id.clone(),
        entry: entry.as_ref().map(|p| p.to_string_lossy().into_owned()),
        pdf_included,
        files: manifest_files,
    };
    let manifest_bytes = serde_json::to_vec_pretty(&manifest)?;
    zip.start_file(MANIFEST_NAME, opts_file)?;
    zip.write_all(&manifest_bytes)?;

    zip.finish().context("finalize zip")?;

    let zip_bytes = std::fs::metadata(&opts.out_zip)?.len();

    Ok(PublishReport {
        zip_path: opts.out_zip.to_string_lossy().into_owned(),
        file_count: manifest.files.len(),
        zip_bytes,
        pdf_included,
        manifest_in_zip: MANIFEST_NAME.to_string(),
        created_at: manifest.created_at,
        engine_id: opts.engine_id.clone(),
    })
}

/// Most recent `*.pdf` under `<root>/.latex-studio-build/build_*/`, by mtime.
fn find_latest_pdf(root: &Path) -> Option<PathBuf> {
    let base = root.join(".latex-studio-build");
    let mut best: Option<(std::time::SystemTime, PathBuf)> = None;
    let entries = std::fs::read_dir(&base).ok()?;
    for e in entries.flatten() {
        let p = e.path();
        if !p.is_dir() {
            continue;
        }
        let name = p.file_name().and_then(|s| s.to_str()).unwrap_or("");
        if !name.starts_with("build_") {
            continue;
        }
        let Ok(inner) = std::fs::read_dir(&p) else { continue };
        for f in inner.flatten() {
            let fp = f.path();
            if fp.extension().and_then(|s| s.to_str()) != Some("pdf") {
                continue;
            }
            let Ok(meta) = std::fs::metadata(&fp) else { continue };
            let Ok(mtime) = meta.modified() else { continue };
            match &best {
                Some((t, _)) if *t >= mtime => {}
                _ => best = Some((mtime, fp)),
            }
        }
    }
    best.map(|(_, p)| p)
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(bytes);
    hex::encode(h.finalize())
}

/// Read the manifest back out of a published archive. Used by tests and
/// by any tool that wants to verify a published zip.
pub fn read_manifest(zip_path: &Path) -> Result<String> {
    let f = File::open(zip_path)?;
    let mut z = zip::ZipArchive::new(f)?;
    let mut entry = z
        .by_name(MANIFEST_NAME)
        .map_err(|_| anyhow!("no {MANIFEST_NAME} in {}", zip_path.display()))?;
    let mut s = String::new();
    entry.read_to_string(&mut s)?;
    Ok(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(p: &Path, s: &str) {
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, s).unwrap();
    }

    fn fixture() -> tempfile::TempDir {
        let d = tempfile::tempdir().unwrap();
        let r = d.path();
        write(&r.join("main.tex"), "\\documentclass{article}\n\\begin{document}hi\\end{document}\n");
        write(&r.join("chapters/one.tex"), "\\chapter{One}\n");
        write(&r.join("refs.bib"), "@article{k}\n");
        // A fake build artefact.
        write(&r.join(".latex-studio-build/build_1/main.pdf"), "%PDF-1.4 fake");
        write(&r.join(".latex-studio-build/build_1/main.log"), "log text");
        d
    }

    #[test]
    fn archive_contains_sources_and_pdf() {
        let d = fixture();
        let out = d.path().join("out.zip");
        let report = create_archive(&PublishOptions {
            project_root: d.path().to_path_buf(),
            out_zip: out.clone(),
            engine_id: "tectonic".into(),
        })
        .unwrap();

        assert!(report.pdf_included);
        assert!(report.file_count >= 3);
        assert!(out.is_file());

        // Read back names.
        let f = File::open(&out).unwrap();
        let mut z = zip::ZipArchive::new(f).unwrap();
        let mut names = Vec::new();
        for i in 0..z.len() {
            names.push(z.by_index(i).unwrap().name().to_string());
        }
        for expected in ["main.tex", "chapters/one.tex", "refs.bib", PDF_NAME, MANIFEST_NAME] {
            assert!(names.contains(&expected.to_string()), "missing {expected}: {names:?}");
        }
        // No build artefacts leaked.
        for n in &names {
            assert!(!n.contains(".latex-studio-build"), "leaked: {n}");
        }
    }

    #[test]
    fn manifest_is_valid_json_with_hashes() {
        let d = fixture();
        let out = d.path().join("out.zip");
        create_archive(&PublishOptions {
            project_root: d.path().to_path_buf(),
            out_zip: out.clone(),
            engine_id: "tectonic".into(),
        })
        .unwrap();

        let json = read_manifest(&out).unwrap();
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["engine"], "tectonic");
        assert_eq!(v["pdf_included"], true);
        let files = v["files"].as_array().unwrap();
        assert!(files.iter().any(|f| f["path"] == "main.tex"));
        let mf = files.iter().find(|f| f["path"] == "main.tex").unwrap();
        assert_eq!(mf["sha256"].as_str().unwrap().len(), 64);
    }

    #[test]
    fn archive_without_a_pdf_marks_pdf_not_included() {
        let d = tempfile::tempdir().unwrap();
        write(&d.path().join("main.tex"), "\\documentclass{article}\n");
        let out = d.path().join("out.zip");
        let report = create_archive(&PublishOptions {
            project_root: d.path().to_path_buf(),
            out_zip: out.clone(),
            engine_id: "tectonic".into(),
        })
        .unwrap();
        assert!(!report.pdf_included);

        let json = read_manifest(&out).unwrap();
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["pdf_included"], false);
    }

    #[test]
    fn overwrites_existing_zip() {
        let d = fixture();
        let out = d.path().join("out.zip");
        std::fs::write(&out, "not a zip").unwrap();
        create_archive(&PublishOptions {
            project_root: d.path().to_path_buf(),
            out_zip: out.clone(),
            engine_id: "tectonic".into(),
        })
        .unwrap();
        // Should be a valid zip now.
        let f = File::open(&out).unwrap();
        let z = zip::ZipArchive::new(f);
        assert!(z.is_ok(), "not a zip: {:?}", z.err());
    }
}
