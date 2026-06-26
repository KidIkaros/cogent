# debuggability

Detect patterns that make production debugging difficult: contextless unwraps, silent panics, and swallowed errors.

## What it measures

Scans for anti-patterns that hide error context:

- **Contextless unwraps**: `.unwrap()`, `.expect("")`, `unwrap_or_default()` without helpful messages
- **Silent panics**: `panic!()` with static or generic messages
- **Swallowed errors**: Errors logged but ignored, or errors returned without context
- **Generic error types**: Returning `Box<dyn Error>` without wrapping in domain-specific errors

## Why it matters

When code fails in production, you need to know:
- What operation failed?
- What were the inputs?
- Where in the logic did it fail?

Contextless panics give you none of these. Debugging becomes a game of "add print statements and redeploy."

## Output

```
debuggability score: 62%

Contextless unwraps: 8 found
  src/api/mod.rs:45 — .unwrap() on database connection
  src/auth/jwt.rs:23 — .expect("failed to parse")
  src/config.rs:12 — .unwrap_or_default() on API key

Silent panics: 2 found
  src/main.rs:67 — panic!("Critical failure")
  src/worker.rs:34 — panic!("Job failed")

Swallowed errors: 3 found
  src/handler.rs:89 — error logged but not propagated
  src/client.rs:45 — error returned without wrapping

Generic error types: 5 found
  src/service.rs:12 — returns Box<dyn Error>
```

## Threshold

`.quality.toml`:
```toml
[debuggability]
max_unwraps = 0
max_panics = 0
max_swallowed_errors = 0
```

## Common fixes

1. **Replace unwraps with helpful messages**:
   ```rust
   // Before
   let conn = pool.get().unwrap();

   // After
   let conn = pool.get().expect("Failed to get DB connection from pool");

   // Better: propagate the error
   let conn = pool.get().context("Failed to get DB connection")?;
   ```

2. **Use `anyhow::Context` or `thiserror` for error wrapping**:
   ```rust
   // Before
   fn load_config(path: &Path) -> Result<Config, Box<dyn Error>> {
       let content = fs::read_to_string(path)?;
       toml::from_str(&content)?
   }

   // After (with anyhow)
   fn load_config(path: &Path) -> Result<Config> {
       let content = fs::read_to_string(path)
           .with_context(|| format!("Failed to read config file: {}", path.display()))?;
       toml::from_str(&content)
           .with_context(|| format!("Failed to parse config file: {}", path.display()))?
   }
   ```

3. **Don't swallow errors**:
   ```rust
   // Before
   if let Err(e) = process_job(&job) {
       log::error!("Job failed: {:?}", e);
       // Error is lost!
   }

   // After
   process_job(&job).with_context(|| format!("Failed to process job ID {}", job.id))?;
   ```

4. **Avoid panics in production code**:
   ```rust
   // Before
   if user.role != "admin" {
       panic!("Unauthorized");
   }

   // After
   if user.role != "admin" {
       return Err(anyhow::anyhow!("User {} is not authorized to access admin resources", user.id));
   }
   ```

## Language-specific patterns

| Language | Anti-pattern | Fix |
|----------|--------------|-----|
| Rust | `.unwrap()` | Use `?` with `anyhow::Context` |
| Python | `assert condition` | Raise custom exception with message |
| JS/TS | `throw new Error("generic")` | Throw `new Error("Specific context: details")` |
| Go | `panic("...")` | Return error with wrapping |
| Java | `throw new RuntimeException()` | Throw domain-specific exception |

## False positives

- **CLI tools**: Unwraps at the binary entry point are acceptable (fast failure is OK)
- **Tests**: Test helpers often use `.unwrap()` intentionally

Add to `.cogent-exceptions.yaml`:
```yaml
debuggability:
  ignore:
    - "**/bin/**"
    - "**/tests/**"
```