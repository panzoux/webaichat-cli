---
description: Run Rust quality gates (fmt check, clippy lint, test) in sequence
---

Run the full Rust quality pipeline on the current project. Execute in order:

1. **Format check**: `cargo fmt --all -- --check`
   - If formatting issues found, run `cargo fmt --all` to fix, then re-check
2. **Clippy lint**: `cargo clippy --all-targets -- -D warnings`
   - Fix any warnings or errors before proceeding
3. **Tests**: `cargo test -- --test-threads=1`
   - Use single-threaded test execution for deterministic results
   - Capture and report test results

Stop at the first failure. Report pass/fail for each stage.

**Usage:**
```
cargo-quality
```

**Example output:**
```
✓ Format check passed
✓ Clippy lint passed (0 warnings)
✓ Tests passed (42/42)
```
