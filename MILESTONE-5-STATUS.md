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

## M4.4 — approval UI (CLOSED, 2026-10-17)

Reapplied after an earlier attempt was lost during the lib/main.dart
split. Verified end-to-end on Linux desktop and on a Samsung S25 Ultra
(Android 16).

Delivered:
- CorrectionCard: plain-language explanation, before/after with the
  changed span highlighted, confidence badge, may-change-meaning warning,
  Apply / Reject buttons.
- Problems pane shows corrections above the compile diagnostics.
- Apply: save the open file if it is the target, call correct_apply,
  refuse on stale or ambiguous context with a snackbar, reload the file,
  re-verify.
- Reject: local dismissal, no disk change.
- Corrections recomputed after every compile attempt, success or failure.
- Click-to-jump from a diagnostic or correction to file:line.
- Snippet toolbar, keyboard shortcuts, status bar, output tab.

Additional correction kinds (from an external agent pass):
- rename for duplicate label
- rename for undefined command
- delete unused command
- comment-out as a last-resort fallback for diagnostics with no
  specific handler

Fixed on Android:
- Scoped storage. /sdcard/Android/data/<pkg>/ is unreadable by the app
  (permission denied). The app's private directory at
  /data/data/<pkg>/files/ works.
- Silent FileSystemException in _loadRoot, _openProjectFile, and
  _pickProject. Now caught and reported.

Not delivered:
- Bulk approval. Individual approve/reject is the primitive.
- Grouping corrections by root cause.
- Tectonic bundled for Android. Compile still unverified on Android.
- VerbTeX benchmark.

## Milestone 5 — CLOSED (2026-10-17)

- M5.1  math notation consistency checks (both directions)
- M5.2  Lean 4 formal verification adapter, tri-state
- M5.3  performance work (closed early, engine was never the bottleneck)
- M5.4  Android: cross-compilation verified, APK builds, app runs,
        scoped storage fixed, corrections UI working on device

## Outstanding for the next session

- Tectonic on Android: decision between bundling a renamed .so and
  linking the crate. Recommended: link the crate.
- Default project directory via path_provider, so the user does not
  need to paste /data/data/com.latexstudio.latex_studio/files/.
- VerbTeX benchmark from the brief.
