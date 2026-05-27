# Cyclomatic Complexity

## What it measures

Cyclomatic complexity counts the number of independent paths through a function. It is computed as:

```
complexity = 1 + number of decision points
```

Decision points include:
- `if` / `else if` / `else`
- `match` arms
- `for` / `while` loops
- `&&` / `||` in conditions (each counts)
- `?` / `try` early returns (sometimes counted, depending on tool)

Higher complexity means:
- Harder to understand
- Harder to test exhaustively
- Higher probability of bugs

## Threshold meaning

| Metric | Value | Meaning |
|--------|-------|---------|
| `min_complexity` (single-tool default) | 5 | Report every function ≥ 5 |
| `min_complexity` (single-tool strict) | 10 | Report every function ≥ 10 |
| `max_violations` (check mode) | 0 | Zero functions may exceed complexity 10 |

The `cogent check` default is strict: **zero functions may have complexity ≥ 10**.

## Example output (text)

```
  ✗ complexity  1.1s  3 functions exceed complexity 10 (threshold: 0)
    src/engine.rs:42   calculate_score    complexity 14
    src/parser.rs:12   parse_expression   complexity 12
    src/render.rs:89   draw_frame         complexity 11
```

## How to fix

### 1. Extract nested conditionals into named helpers

```rust
// Before: complexity 14
fn calculate_score(board: &Board, player: Player) -> i32 {
    let mut score = 0;
    for row in &board.rows {
        for cell in row {
            if cell.owner == player {
                if cell.bonus {
                    if cell.streak >= 3 {
                        score += cell.value * 3;
                    } else {
                        score += cell.value * 2;
                    }
                } else {
                    score += cell.value;
                }
            } else if cell.owner.is_neutral() {
                if cell.adjacent_to(player) {
                    score += 1;
                }
            }
        }
    }
    score
}

// After: main function complexity 5, helpers testable in isolation
fn calculate_score(board: &Board, player: Player) -> i32 {
    board.rows.iter().flat_map(|r| r.iter())
        .filter(|c| c.owner == player)
        .map(|c| score_cell(c))
        .sum()
    + board.neutral_cells_adjacent_to(player).count() as i32
}

fn score_cell(cell: &Cell) -> i32 {
    match (cell.bonus, cell.streak >= 3) {
        (true, true) => cell.value * 3,
        (true, false) => cell.value * 2,
        _ => cell.value,
    }
}
```

### 2. Replace deep match/if chains with lookup tables

```rust
// Before: complexity 12, hard to extend
fn status_message(code: u16) -> &'static str {
    if code == 200 { "OK" }
    else if code == 201 { "Created" }
    else if code == 204 { "No Content" }
    else if code == 400 { "Bad Request" }
    else if code == 401 { "Unauthorized" }
    else if code == 403 { "Forbidden" }
    else if code == 404 { "Not Found" }
    else if code == 500 { "Internal Server Error" }
    else { "Unknown" }
}

// After: complexity 2, data-driven
const MESSAGES: &[(u16, &str)] = &[
    (200, "OK"),
    (201, "Created"),
    (204, "No Content"),
    (400, "Bad Request"),
    (401, "Unauthorized"),
    (403, "Forbidden"),
    (404, "Not Found"),
    (500, "Internal Server Error"),
];

fn status_message(code: u16) -> &'static str {
    MESSAGES.iter().find(|(c, _)| *c == code)
        .map(|(_, msg)| *msg)
        .unwrap_or("Unknown")
}
```

### 3. Use early returns to reduce nesting

```rust
// Before: deep nesting, complexity 11
fn process(data: Option<Data>) -> Result<Output, Error> {
    if let Some(d) = data {
        if d.valid() {
            if d.has_permission() {
                Ok(transform(d))
            } else {
                Err(Error::Permission)
            }
        } else {
            Err(Error::Invalid)
        }
    } else {
        Err(Error::Missing)
    }
}

// After: flat guard clauses, complexity 4
fn process(data: Option<Data>) -> Result<Output, Error> {
    let d = data.ok_or(Error::Missing)?;
    if !d.valid() { return Err(Error::Invalid); }
    if !d.has_permission() { return Err(Error::Permission); }
    Ok(transform(d))
}
```

## Common pitfalls

- **Optimizing for complexity without adding tests.** Extracting a helper without tests can introduce subtle behavior changes.
- **Counting generated code.** Macro-generated match arms can inflate complexity. Consider `#[allow(clippy::cognitive_complexity)]` for macro output modules.
- **One-size-fits-all thresholds.** A state-machine `transition` function with complexity 15 may be cleaner than 5 helper functions with shared mutable state.

## Related

- `cogent explain crap` — complexity is half of the CRAP formula
- `cogent explain dup` — complex code is often duplicated code
- `cogent explain riskmap` — complex files that change frequently are bug hotspots
