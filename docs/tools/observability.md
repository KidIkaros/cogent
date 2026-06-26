# observability

Check structured logging and tracing coverage in your codebase.

## What it measures

Detects whether your code has adequate observability primitives:

- **Structured logging**: log calls with key-value pairs (e.g., `log::info!`, `winston.info`, `console.log` with objects)
- **Tracing instrumentation**: span/tracing usage (e.g., `tracing::info_span`, OpenTelemetry decorators)
- **Context propagation**: trace context propagation across async boundaries

## Why it matters

Without structured logging and tracing, production debugging is a guessing game. Observability is the difference between "something broke at 3am" and "request X failed because service Y timed out after 500ms."

## Output

```
observability score: 62%
├── structured logging: 58% (23/40 functions)
├── tracing coverage: 45% (18/40 functions)
└── context propagation: 85% (17/20 async functions)

High-priority files lacking tracing:
  src/api/handler.rs (0/5)
  src/auth/jwt.rs (0/3)
  src/db/connection.rs (0/2)
```

## Threshold

`.quality.toml`:
```toml
[observability]
min_logging = 70
min_tracing = 50
```

## Common fixes

1. **Add structured logging**:
   ```rust
   // Before
   println!("User login failed");

   // After
   log::warn!(user_id = %user.id, reason = "invalid_password", "User login failed");
   ```

2. **Instrument with tracing**:
   ```rust
   // Before
   async fn process_order(&self, order: Order) -> Result<()> { ... }

   // After
   #[tracing::instrument(skip(self))]
   async fn process_order(&self, order: Order) -> Result<()> { ... }
   ```

3. **Propagate context in async code**:
   ```rust
   // Before
   tokio::spawn(async move { heavy_work().await });

   // After
   tokio::spawn(tracing::info_span!("heavy_work").in_scope(|| {
       async { heavy_work().await }
   }));
   ```

## Framework-specific guidance

| Language | Recommended libraries |
|----------|----------------------|
| Rust | `tracing` + `tracing-subscriber` |
| Python | `structlog` + `opentelemetry` |
| JS/TS | `winston` or `pino` + `@opentelemetry/sdk-node` |
| Go | `zap` or `zerolog` + `opentelemetry-go` |
| Java | `SLF4J` + `logback` + Micrometer |

## False positives

- **CLI tools**: Single-command scripts may not need distributed tracing
- **Tests**: Test code typically doesn't require production-grade logging

Add to `.cogent-exceptions.yaml`:
```yaml
observability:
  ignore:
    - "**/tests/**"
    - "**/bin/**"
```