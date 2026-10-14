
## M4.3 — FFI for validate/propose/apply (CLOSED, 2026-10-14)

Delivered:
- latex_verify_project(root) -> {diagnostics:[...]}
- latex_correct_propose(root, diagnostics_json) -> {corrections:[...]}
- latex_correct_apply(root, correction_json) -> {ok} | {error, reason}
- Apply re-reads the file; old_text must occur exactly once; refusal
  reasons are missing, ambiguous, io, bad_input.
- Submodules: compile, engines, project, publish, verify.
- 9 FFI tests.

## M4.4 — approval UI (not started)
