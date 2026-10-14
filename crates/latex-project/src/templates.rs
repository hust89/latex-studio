//! Built-in project templates.
//!
//! Each template is a named set of files: relative path -> content. Creating
//! a template writes each file atomically under a target directory. Refuses
//! to overwrite a non-empty directory.

use anyhow::{bail, Context, Result};
use std::path::{Component, Path, PathBuf};

use crate::save::atomic_write;

#[derive(Debug, Clone, Copy)]
pub struct Template {
    pub name: &'static str,
    pub description: &'static str,
    /// (relative path, content)
    pub files: &'static [(&'static str, &'static str)],
}

pub const TEMPLATES: &[Template] = &[
    Template {
        name: "article",
        description: "A single-file article",
        files: &[("main.tex", ARTICLE)],
    },
    Template {
        name: "book",
        description: "A multi-chapter book with two chapters",
        files: &[
            ("main.tex", BOOK_MAIN),
            ("chapters/one.tex", BOOK_CH1),
            ("chapters/two.tex", BOOK_CH2),
        ],
    },
    Template {
        name: "beamer",
        description: "A presentation",
        files: &[("main.tex", BEAMER)],
    },
    Template {
        name: "letter",
        description: "A formal letter",
        files: &[("main.tex", LETTER)],
    },
    Template {
        name: "homework",
        description: "A homework assignment",
        files: &[("main.tex", HOMEWORK)],
    },
];

pub fn find(name: &str) -> Option<&'static Template> {
    TEMPLATES.iter().find(|t| t.name == name)
}

/// Create `template` under `target`. `target` must not exist or must be empty.
pub fn create(template: &Template, target: &Path) -> Result<()> {
    if target.exists() {
        if !target.is_dir() {
            bail!("target exists and is not a directory: {}", target.display());
        }
        let mut entries = std::fs::read_dir(target)
            .with_context(|| format!("read {}", target.display()))?;
        if entries.next().is_some() {
            bail!("target directory is not empty: {}", target.display());
        }
    } else {
        std::fs::create_dir_all(target)
            .with_context(|| format!("create {}", target.display()))?;
    }

    for (rel, content) in template.files {
        let rel_path = Path::new(rel);
        // Sanity: refuse anything that is not a plain relative path.
        for c in rel_path.components() {
            match c {
                Component::Normal(_) => {}
                _ => bail!("template file path is not plain: {rel}"),
            }
        }
        let abs: PathBuf = target.join(rel_path);
        atomic_write(&abs, content)
            .with_context(|| format!("write {}", abs.display()))?;
    }
    Ok(())
}

const ARTICLE: &str = r#"\documentclass{article}
\usepackage{amsmath,amssymb,amsthm}
\usepackage{graphicx}
\usepackage{hyperref}

\title{Title}
\author{Author}
\date{\today}

\begin{document}
\maketitle

\section{Introduction}
Write here.

\end{document}
"#;

const BOOK_MAIN: &str = r#"\documentclass{book}
\usepackage{amsmath,amssymb,amsthm}
\usepackage{graphicx}
\usepackage{hyperref}

\title{Title}
\author{Author}
\date{\today}

\begin{document}
\frontmatter
\maketitle
\tableofcontents

\mainmatter
\include{chapters/one}
\include{chapters/two}

\end{document}
"#;

const BOOK_CH1: &str = r#"\chapter{First chapter}

Write here.
"#;

const BOOK_CH2: &str = r#"\chapter{Second chapter}

Write here.
"#;

const BEAMER: &str = r#"\documentclass{beamer}
\usetheme{Madrid}

\title{Title}
\author{Author}
\date{\today}

\begin{document}

\frame{\titlepage}

\begin{frame}{First slide}
Write here.
\end{frame}

\end{document}
"#;

const LETTER: &str = r#"\documentclass{letter}
\usepackage{hyperref}

\signature{Your Name}
\address{Your Address \\ Your City}

\begin{document}

\begin{letter}{Recipient \\ Their Address}
\opening{Dear Recipient,}

Write here.

\closing{Sincerely,}

\end{letter}

\end{document}
"#;

const HOMEWORK: &str = r#"\documentclass{article}
\usepackage{amsmath,amssymb,amsthm}

\newtheorem{problem}{Problem}

\title{Homework}
\author{Your Name}
\date{\today}

\begin{document}
\maketitle

\begin{problem}
State the problem here.
\end{problem}

\textbf{Solution.} Write here.

\end{document}
"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_template_is_listed_and_findable() {
        for t in TEMPLATES {
            assert!(find(t.name).is_some(), "{} not findable", t.name);
            assert!(!t.files.is_empty(), "{} has no files", t.name);
        }
    }

    #[test]
    fn create_writes_files() {
        let d = tempfile::tempdir().unwrap();
        let target = d.path().join("newproject");
        create(find("book").unwrap(), &target).unwrap();

        assert!(target.join("main.tex").is_file());
        assert!(target.join("chapters/one.tex").is_file());
        assert!(target.join("chapters/two.tex").is_file());
        let s = std::fs::read_to_string(target.join("main.tex")).unwrap();
        assert!(s.contains("\\documentclass{book}"));
    }

    #[test]
    fn refuses_non_empty_target() {
        let d = tempfile::tempdir().unwrap();
        let target = d.path().join("existing");
        std::fs::create_dir_all(&target).unwrap();
        std::fs::write(target.join("keep.txt"), "x").unwrap();
        let err = create(find("article").unwrap(), &target).unwrap_err();
        assert!(format!("{err:#}").contains("not empty"));
    }

    #[test]
    fn allows_empty_target() {
        let d = tempfile::tempdir().unwrap();
        let target = d.path().join("empty");
        std::fs::create_dir_all(&target).unwrap();
        create(find("article").unwrap(), &target).unwrap();
        assert!(target.join("main.tex").is_file());
    }
}
