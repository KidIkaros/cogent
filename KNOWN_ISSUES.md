# Known Issues

This document tracks known issues and intentional design decisions in the Cogent codebase.

## Slow Integration Tests (mutation-test)

`crates/mutation-test/tests/integration.rs` contains two tests marked `#[ignore]` that spawn a full `cargo test` on a workspace copy in `/tmp`. They are **not** run by `scripts/test.sh` by default to avoid blocking CI on slow hardware.

To run them explicitly:

```bash
cargo test -p mutation-test -- --ignored
```

These tests verify that `mutate --max-mutants 0` correctly confirms baseline tests pass. They take 2–10 minutes depending on machine speed.

---

*No other known issues. All compiler warnings are clean on both release and test profiles. The `test_python_docstring` integer overflow in `ast-parse-ts` has been fixed (`line.saturating_sub(1)`).*
