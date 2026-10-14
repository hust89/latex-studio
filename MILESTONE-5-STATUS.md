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

## M5.2 — Lean 4 formal verification adapter (CLOSED, 2026-10-14)

Delivered:
- latex-formal crate with FormalSystem trait and LeanEngine.
- FormalResult: Verified | NotVerified | Unavailable. Never conflated.
- FFI: latex_formal_check(preamble, statement, project_dir),
  latex_formal_available().
- Lean 4 writes diagnostics to stdout; both streams are captured.
- 6 crate tests, 6 FFI tests. Lean is present on this machine, so the
  positive and negative paths ran rather than skipped.

Not delivered:
- No UI. The Flutter shell does not yet offer a Check-with-Lean action,
  and does not display the tri-state. Deferred.

## M5.3 — performance work (not started)
## M5.4 — cross-platform hardening (not started)

## M5.4 (part 1) — Android cross-compilation (VERIFIED, 2026-10-14)

Verified:
- cargo ndk builds liblatex_ffi.so for all four Android ABIs.
- 17 FFI exports present in each .so.
- No C-library build surprises.
- Environment: NDK 25.2.9519653, cargo-ndk 4.1.2, rustup Android
  targets for all four ABIs already installed.

Not yet done:
- The .so has never been loaded by an Android process. No APK built.
- Tectonic has not been bundled for Android. Decision pending.

## M5.3 — performance work (not started)
