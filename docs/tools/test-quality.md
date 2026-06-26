# test-quality

Detect non-deterministic test patterns that cause flaky test suites.

## What it measures

Scans your test files for common sources of flakiness:

- **Time dependencies**: `Date.now()`, `SystemTime::now()`, time-based assertions
- **Random number usage**: `Math.random()`, `rand::thread_rng()` without seeding
- **Order dependencies**: Tests that rely on execution order or shared mutable state
- **External I/O**: Tests that read/write to files without isolation
- **Network calls**: Tests that make real HTTP requests or hit databases

## Why it matters

Flaky tests erode trust in CI. Developers start ignoring red builds, and real failures slip through. A test suite with >5% flaky rate is effectively broken.

## Output

```
test-quality score: 71%
├── time-dependent: 3 found
├── unseeded-random: 2 found
├── order-dependent: 0 found
├── external-io: 5 found
└── network-calls: 1 found

Flaky tests:
  tests/auth_spec.js:42 — uses Date.now() without mocking
  tests/order_flow.rs:128 — uses rand::thread_rng() without seed
  tests/integration/api_test.py:89 — writes to /tmp/orders.db without cleanup
```

## Threshold

`.quality.toml`:
```toml
[test-quality]
max_flaky_score = 10  # % of tests flagged as potentially flaky
```

## Common fixes

1. **Mock time in tests**:
   ```javascript
   // Before
   expect(Date.now()).toBeGreaterThan(timestamp);

   // After
   jest.useFakeTimers().setSystemTime(1700000000000);
   expect(Date.now()).toBe(1700000000000);
   ```

2. **Seed random numbers**:
   ```rust
   // Before
   let random = rand::thread_rng().gen_range(0..100);

   // After
   let mut rng = StdRng::seed_from_u64(42); // deterministic seed
   let random = rng.gen_range(0..100);
   ```

3. **Isolate file I/O**:
   ```python
   # Before
   with open('/tmp/data.json', 'w') as f:
       json.dump(data, f)

   # After
   with tempfile.NamedTemporaryFile(mode='w', delete=True) as f:
       json.dump(data, f)
       # Test using f.name, auto-deleted on exit
   ```

4. **Mock network calls**:
   ```python
   # Before
   response = requests.get('https://api.example.com/status')

   # After
   with patch('requests.get') as mock_get:
       mock_get.return_value.status_code = 200
       response = requests.get('https://api.example.com/status')
   ```

5. **Don't share state between tests**:
   ```rust
   // Before
   static mut COUNTER: u32 = 0;

   // After
   // Each test gets its own Counter instance
   #[test]
   fn test_counter() {
       let mut counter = Counter::new();
       // ...
   }
   ```

## Framework-specific guidance

| Language | Recommended libraries |
|----------|----------------------|
| Rust | `mockall` + `tempfile` |
| Python | `pytest-mock` + `unittest.mock` |
| JS/TS | Jest/Vitest fakes + `nock` |
| Go | `testify/mock` + `httptest` |

## False positives

- **Fuzz tests**: Intentionally use randomness and may not need determinism
- **Load tests**: May intentionally use real I/O and random input

Add to `.cogent-exceptions.yaml`:
```yaml
test-quality:
  ignore:
    - "tests/fuzz/**"
    - "tests/load/**"
```