# Milestone 5 — Status

## M5.1 — Mathematical consistency checks (IN PROGRESS)

Delivered:
- `math.rs` module in `latex-verify`.
- Notation consistency check:
  - Extracts macro and operator declarations via `\newcommand`, `\DeclareMathOperator` (including star variants, `\renewcommand`, `\providecommand`).
  - Scans project sources for symbol invocations (excluding declarations, respecting word boundaries, handling escaping, and ignoring comments).
  - Reports `DiagnosticKind::UnusedCommand` (`Severity::Info`) when a declared symbol is never used anywhere in the project.
  - Detailed symbol usage tracking via `NotationReport`.
- Integrated notation consistency checking into `latex_verify::verify` and `verify_project`.
- Added `DiagnosticKind::UnusedCommand` to `latex-model`.
- 8 unit tests in `crates/latex-verify/src/math.rs` covering syntax variations, positive unused triggers, negative used cases, word boundary handling, commented usage ignoring, and star forms.
- 4 integration tests in `crates/latex-verify/tests/math.rs` covering project-level positive and negative cases, operator checks, and multi-file projects.

Deliberately not delivered in this step:
- Proof verification: We explicitly do not claim arbitrary mathematical proof verification.
- Remaining M5.1 checks: Equation and theorem reference prefix vocabulary extensions, dimensional consistency for units (`siunitx`, literal units), and specific contradiction rules (next steps in M5.1).
- M5.2 (formal verification adapter), M5.3 (benchmarks and performance), M5.4 (cross-platform hardening).
