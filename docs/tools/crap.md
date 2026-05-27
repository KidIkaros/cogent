# CRAP (Change Risk Anti-Pattern)

## What it measures

CRAP scores measure how risky a function is to change. It combines two signals:

1. **Cyclomatic complexity** — how many paths through the function
2. **Test coverage** — how many of those paths are exercised by tests

A high-CRAP function is complex *and* untested — the most dangerous kind of code to modify.

## Formula

```
CRAP = complexity² × (1 − coverage/100)³ + complexity
```

Coverage has an outsized impact because of the cubic term. A function with complexity 10 and 100% coverage has CRAP = 10. The same function with 0% coverage has CRAP ≈ 1010.

## Threshold meaning

| Ecosystem | `max_avg` | Meaning |
|-----------|-----------|---------|
| Rust | 15.0 | Strict; every complex function must be tested |
| Go | 20.0 | Moderate; allows some untested helpers |
| Python / JS | 20.0 | Moderate; dynamic languages tend to have higher variance |
| Unknown | 30.0 | Lenient fallback |

The threshold applies to the **average CRAP score across all functions**, not per-function.

## Example output (text)

```
  ✗ crap  1.2s  Average CRAP 24.5 exceeds threshold 15.0 (3 offenders)
    src/engine.rs:42  calculate_score  CRAP 45.2  complexity 8  coverage 12%
    src/engine.rs:89  normalize        CRAP 38.1  complexity 6  coverage 0%
    src/parser.rs:12  parse_token      CRAP 31.4  complexity 7  coverage 5%
```

## How to fix

### 1. Add tests (fastest impact)

Because coverage is cubed in the formula, adding tests drives CRAP down faster than refactoring:

```rust
// Before: calculate_score has 12% coverage → CRAP 45.2
// After:  add tests for edge cases → coverage 90% → CRAP 9.8
```

Target the top offenders first. A function with complexity 8 needs only ~50% coverage to drop below CRAP 15.

### 2. Refactor complex functions

If a function is genuinely complex (complexity > 10), split it:

```rust
// Before: one 40-line function with 4 nested conditionals
fn process_order(order: &Order) -> Result<Invoice, Error> {
    if order.items.is_empty() { ... }
    if order.customer.vip { ... }
    for item in &order.items { ... }
    if order.shipping.express { ... }
    Ok(invoice)
}

// After: delegating helpers, each testable in isolation
fn process_order(order: &Order) -> Result<Invoice, Error> {
    validate_order(order)?;
    let pricing = compute_pricing(order)?;
    let shipping = compute_shipping(order, &pricing)?;
    Ok(Invoice::new(pricing, shipping))
}
```

### 3. Delete dead complex code

If a complex function is unused (check `cogent deadcode`), remove it. Zero complexity = zero CRAP.

## Common pitfalls

- **Focusing on per-function CRAP instead of average.** One high-CRAP function is fine if the rest of the codebase is well-tested.
- **Refactoring without adding tests first.** You risk introducing bugs in already-fragile code.
- **Ignoring coverage accuracy.** Ensure your lcov file includes all source files; missing files artificially inflate coverage.

## Related

- `cogent explain complexity` — understand the complexity component
- `cogent explain doccov` — doc comments make APIs testable
- `cogent explain mutate` — verify your tests actually catch bugs
