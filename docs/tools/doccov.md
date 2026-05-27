# Documentation Coverage

## What it measures

Documentation coverage is the percentage of **public functions, structs, traits, enums, and modules** that have doc comments.

| Language | Doc comment style |
|----------|-------------------|
| Rust | `///` or `/** */` or `//!` |
| Go | `//` immediately before exported identifiers |
| Python | `"""` or `'''` docstrings on public functions/classes |
| JavaScript / TypeScript | `/** */` JSDoc on exported functions/classes |

Missing docs make APIs harder to discover, harder to maintain, and harder for new contributors to onboard.

## Threshold meaning

| Ecosystem | `min_pct` | Meaning |
|-----------|-----------|---------|
| Rust | 95% | Near-complete coverage; only complex internal traits may be undocumented |
| Go | 80% | Good coverage; exported packages should be fully documented |
| Python / JS | 80% / 70% | Moderate; dynamic languages often have fewer explicit public APIs |
| Unknown | 50% | Lenient fallback |

## Example output (text)

```
  ✗ doc_coverage  0.8s  42% coverage (threshold: 95%)
    src/api.rs:12   fn handle_request       missing docs
    src/api.rs:34   struct ResponseBuilder  missing docs
    src/db.rs:5     trait ConnectionPool    missing docs
```

## How to fix

### 1. Add doc comments to every public item

```rust
// Before
pub fn handle_request(req: Request) -> Response;

// After
/// Handle an incoming HTTP request and route it to the appropriate handler.
///
/// # Errors
/// Returns `400 Bad Request` if the body is malformed JSON.
/// Returns `404 Not Found` if no route matches the path.
pub fn handle_request(req: Request) -> Response;
```

### 2. Enable compiler warnings

Add to your crate root:

```rust
#![warn(missing_docs)]
```

Or for stricter enforcement:

```rust
#![deny(missing_docs)]
```

This turns missing docs into compile-time warnings or errors, preventing regressions.

### 3. Use rustdoc for tested examples

Doc comments with code blocks are tested by `cargo test`:

```rust
/// Parse a date string in RFC 3339 format.
///
/// # Examples
///
/// ```
/// use mycrate::parse_date;
/// let d = parse_date("2024-01-15T09:00:00Z").unwrap();
/// assert_eq!(d.year(), 2024);
/// ```
pub fn parse_date(s: &str) -> Result<DateTime, Error>;
```

If the example breaks, CI fails. This keeps docs and code in sync.

## Common pitfalls

- **Documenting private items.** Doc coverage only measures public APIs. Don't waste time on `pub(crate)` internals unless they are complex.
- **Writing "what" instead of "why".** Bad: `/// Sets the name.` Good: `/// Sets the display name used in the UI header.`
- **Forgetting `# Errors` and `# Panics` sections.** Callers need to know what can go wrong.

## Related

- `cogent explain crap` — well-documented functions are easier to test
- `cogent explain complexity` — complex functions need the best docs
- `cogent explain debt` — TODO markers in doc comments are still debt
