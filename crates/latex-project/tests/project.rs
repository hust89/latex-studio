use latex_project::{FileKind, Project};
use std::path::Path;

fn write(p: &Path, content: &str) {
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(p, content).unwrap();
}

/// A small multi-file book that exercises every path-resolution and
/// comment-handling case we care about.
fn fixture() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();

    write(
        &root.join("main.tex"),
        r#"\documentclass{book}
\begin{document}
\include{chapters/one}
\include{chapters/two}
% \include{chapters/deleted}    <- commented out, must not appear in graph
\bibliography{refs}
\end{document}
"#,
    );

    write(
        &root.join("chapters/one.tex"),
        r#"\chapter{One}
\input{shared/macros}
Text with a percent sign \% not a comment.
"#,
    );

    write(
        &root.join("chapters/two.tex"),
        r#"\chapter{Two}
\input{shared/macros}
"#,
    );

    write(
        &root.join("shared/macros.tex"),
        r#"\newcommand{\foo}{bar}
"#,
    );

    write(
        &root.join("refs.bib"),
        r#"@article{key, title={T}}
"#,
    );

    // Things that must be ignored.
    std::fs::create_dir_all(root.join(".git")).unwrap();
    std::fs::write(root.join(".git/HEAD"), "ref: refs/heads/main\n").unwrap();
    std::fs::create_dir_all(root.join(".latex-studio-build/build_1")).unwrap();
    std::fs::write(root.join(".latex-studio-build/build_1/main.tex"), "x").unwrap();

    dir
}

#[test]
fn index_finds_expected_files_and_ignores_the_rest() {
    let dir = fixture();
    let project = Project::open(dir.path()).unwrap();
    let index = project.index().unwrap();

    let sources: Vec<_> = index
        .sources()
        .map(|f| f.path.to_string_lossy().into_owned())
        .collect();
    assert_eq!(
        sources,
        vec![
            "chapters/one.tex",
            "chapters/two.tex",
            "main.tex",
            "shared/macros.tex",
        ]
    );

    let bibs: Vec<_> = index
        .bibliographies()
        .map(|f| f.path.to_string_lossy().into_owned())
        .collect();
    assert_eq!(bibs, vec!["refs.bib"]);

    // Nothing from .git or .latex-studio-build.
    for f in &index.files {
        let s = f.path.to_string_lossy();
        assert!(!s.contains(".git"), "leaked: {}", s);
        assert!(!s.contains(".latex-studio-build"), "leaked: {}", s);
    }
}

#[test]
fn entry_prefers_main_tex() {
    let dir = fixture();
    let project = Project::open(dir.path()).unwrap();
    let index = project.index().unwrap();
    let entry = project.entry(&index).expect("entry");
    assert_eq!(entry, Path::new("main.tex"));
}

#[test]
fn entry_returns_none_when_no_documentclass() {
    let dir = tempfile::tempdir().unwrap();
    write(&dir.path().join("a.tex"), "\\input{b}\n");
    write(&dir.path().join("b.tex"), "no class here\n");
    let project = Project::open(dir.path()).unwrap();
    let index = project.index().unwrap();
    assert!(project.entry(&index).is_none());
}

#[test]
fn include_graph_is_correct() {
    let dir = fixture();
    let project = Project::open(dir.path()).unwrap();
    let g = project.include_graph(Path::new("main.tex")).unwrap();

    let edges: Vec<_> = g
        .edges
        .iter()
        .map(|e| {
            (
                e.from.to_string_lossy().into_owned(),
                e.to.to_string_lossy().into_owned(),
                e.line,
                e.command.clone(),
            )
        })
        .collect();

    assert_eq!(
        edges,
        vec![
            ("chapters/one.tex".into(), "shared/macros.tex".into(), 2, "input".into()),
            ("chapters/two.tex".into(), "shared/macros.tex".into(), 2, "input".into()),
            ("main.tex".into(), "chapters/one.tex".into(), 3, "include".into()),
            ("main.tex".into(), "chapters/two.tex".into(), 4, "include".into()),
        ]
    );

    assert!(g.unresolved.is_empty(), "unresolved: {:?}", g.unresolved);
}

#[test]
fn commented_out_include_is_not_in_graph() {
    let dir = fixture();
    let project = Project::open(dir.path()).unwrap();
    let g = project.include_graph(Path::new("main.tex")).unwrap();
    for e in &g.edges {
        assert!(
            !e.raw.contains("deleted"),
            "commented include leaked into graph: {:?}",
            e
        );
    }
}

#[test]
fn missing_include_is_reported_as_unresolved() {
    let dir = tempfile::tempdir().unwrap();
    write(
        &dir.path().join("main.tex"),
        "\\documentclass{article}\n\\begin{document}\n\\input{chapters/nope}\n\\end{document}\n",
    );
    let project = Project::open(dir.path()).unwrap();
    let g = project.include_graph(Path::new("main.tex")).unwrap();
    assert_eq!(g.edges.len(), 0);
    assert_eq!(g.unresolved.len(), 1);
    assert_eq!(g.unresolved[0].line, 3);
    assert_eq!(g.unresolved[0].raw, "chapters/nope");
    assert_eq!(g.unresolved[0].reason, "no matching file");
}

#[test]
fn cyclic_includes_terminate() {
    let dir = tempfile::tempdir().unwrap();
    write(
        &dir.path().join("main.tex"),
        "\\documentclass{article}\n\\input{a}\n",
    );
    write(&dir.path().join("a.tex"), "\\input{b}\n");
    write(&dir.path().join("b.tex"), "\\input{a}\n");
    let project = Project::open(dir.path()).unwrap();
    let g = project.include_graph(Path::new("main.tex")).unwrap();
    // main -> a -> b -> a (cycle: a is not re-enqueued a second time).
    assert_eq!(g.edges.len(), 3);
}

#[test]
fn extension_appended_when_missing() {
    let dir = tempfile::tempdir().unwrap();
    write(
        &dir.path().join("main.tex"),
        "\\documentclass{article}\n\\input{body}\n",
    );
    write(&dir.path().join("body.tex"), "hello\n");
    let project = Project::open(dir.path()).unwrap();
    let g = project.include_graph(Path::new("main.tex")).unwrap();
    assert_eq!(g.edges.len(), 1);
    assert_eq!(g.edges[0].to, Path::new("body.tex"));
}

#[test]
fn asset_classification() {
    let dir = tempfile::tempdir().unwrap();
    write(&dir.path().join("main.tex"), "\\documentclass{article}\n");
    std::fs::create_dir_all(dir.path().join("figures")).unwrap();
    std::fs::write(dir.path().join("figures/plot.pdf"), b"%PDF-1.4").unwrap();
    std::fs::write(dir.path().join("figures/plot.png"), b"\x89PNG").unwrap();
    let project = Project::open(dir.path()).unwrap();
    let index = project.index().unwrap();
    let assets: Vec<_> = index
        .by_kind(FileKind::Asset)
        .map(|f| f.path.to_string_lossy().into_owned())
        .collect();
    assert_eq!(assets, vec!["figures/plot.pdf", "figures/plot.png"]);
}
