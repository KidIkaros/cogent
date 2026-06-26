# Cogent TUI — Real, Usable Terminal Dashboard

## Vision

A business-grade TUI that makes Cogent feel like a modern IDE experience. Think VS Code's "Problems" panel, but for code quality.

## UX Principles

1. **Keyboard-first** — No mouse required, power-user friendly
2. **Consistent keybindings** — [q] quit, [?] help, [Esc] back, always
3. **Action-oriented** — Buttons for common tasks, not data dumps
4. **Data-driven** — Tables with sort, filter, and drill-down
5. **Context-sensitive help** — Every screen has [?] that explains context
6. **Responsive updates** — Watch mode refreshes in real-time
7. **Error handling** — Clear error messages, never crashes
8. **State management** — Persistent settings, navigation history

## Screens

### SCREEN 1: Dashboard (default)

```
╔══════════════════════════════════════════════════════╗
║ Cogent Dashboard  v1.2.0                [?] Help [q] Quit  ║
╠══════════════════════════════════════════════════════╣
║  Project: my-project (Rust)  Score: 87/100 B         ║
║  Last run: 30 seconds ago                              ║
╠══════════════════════════════════════════════════════╣
║                                                           ║
║  [Check]  [Watch]  [Report]  [Settings]                ║
║                                                           ║
║  Quick Actions                                           ║
║  ┌─────────────────────────────────────────────────┐   ║
║  │ [1] Run full check                              │   ║
║  │ [2] Run security-only check                     │   ║
║  │ [3] Run quality-only check                      │   ║
║  │ [4] View findings                               │   ║
║  │ [5] View history                                │   ║
║  └─────────────────────────────────────────────────┘   ║
║                                                           ║
║  Recent Check Results                                    ║
║  ┌─────────────────────────────────────────────────┐   ║
║  │ Tool          │ Status │ Score │ Last run       │   ║
║  ├─────────────────────────────────────────────────┤   ║
║  │ secrets       │ ✓ PASS │ 100%  │ 30s ago        │   ║
║  │ complexity    │ ✗ FAIL │ 8/5   │ 30s ago        │   ║
║  │ debt          │ ✗ FAIL │ 3/0   │ 30s ago        │   ║
║  │ doccov        │ ✗ FAIL │ 12%/95%│ 30s ago       │   ║
║  │ sast          │ ✓ PASS │ 0     │ 30s ago        │   ║
║  └─────────────────────────────────────────────────┘   ║
║                                                           ║
║  [r] Refresh  [Enter] Select  [Esc] Back  [?] Help     ║
╚══════════════════════════════════════════════════════╝
```

**Keybindings:**
- `[1-5]` — Run quick action
- `[Enter]` — Open selected finding/tool
- `[r]` — Refresh all checks
- `[q]` — Quit
- `[?]` — Help

### SCREEN 2: Findings Detail

```
╔══════════════════════════════════════════════════════╗
║ Cogent Findings  v1.2.0                [?] Help [q] Quit  ║
╠══════════════════════════════════════════════════════╣
║  Project: my-project (Rust)  Tool: complexity        ║
╠══════════════════════════════════════════════════════╣
║                                                           ║
║  Tool: complexity  Status: ✗ FAIL                      ║
║  Score: 8 violations (threshold: 5)                     ║
║                                                           ║
║  Findings (8 total)                                      ║
║  ┌─────────────────────────────────────────────────┐   ║
║  │ File                │ Line │ Function         │   ║
║  ├─────────────────────────────────────────────────┤   ║
║  │ src/main.rs         │ 15   │ process_order    │   ║
║  │ src/main.rs         │ 42   │ validate_user    │   ║
║  │ src/api/handler.rs  │ 8    │ handle_request   │   ║
║  │ ...                                        [↓]  │   ║
║  └─────────────────────────────────────────────────┘   ║
║                                                           ║
║  Selected Finding                                        ║
║  ┌─────────────────────────────────────────────────┐   ║
║  │ File: src/main.rs:15                             │   ║
║  │ Function: process_order                          │   ║
║  │ Complexity: 8 (threshold: 5)                     │   ║
║  │                                                    │   ║
║  │ Suggested fixes:                                  │   ║
║  │ 1. Extract nested conditions into helper fn     │   ║
║  │ 2. Use early returns to reduce nesting          │   ║
║  │ 3. Break into smaller functions                 │   ║
║  │                                                    │   ║
║  │ [Enter] View file in editor  [f] Auto-fix (beta) │   ║
║  └─────────────────────────────────────────────────┘   ║
║                                                           ║
║  [Enter] View  [f] Fix  [Esc] Back  [n] Next  [p] Prev  ║
╚══════════════════════════════════════════════════════╝
```

**Keybindings:**
- `[↑/↓]` — Navigate findings
- `[Enter]` — View file in `$EDITOR`
- `[f]` — Auto-fix (uses `cogent fix`)
- `[n]` — Next finding
- `[p]` — Previous finding
- `[Esc]` — Back to dashboard

### SCREEN 3: Settings

```
╔══════════════════════════════════════════════════════╗
║ Cogent Settings  v1.2.0                [?] Help [q] Quit  ║
╠══════════════════════════════════════════════════════╣
║                                                           ║
║  Quality Thresholds                                       ║
║  ┌─────────────────────────────────────────────────┐   ║
║  │ Max CRAP score: [15    ]  (default: 15)         │   ║
║  │ Min doc coverage: [95%   ]  (default: 95%)       │   ║
║  │ Max debt markers: [0     ]  (default: 0)         │   ║
║  │ Max complexity: [5     ]  (default: 5)          │   ║
║  └─────────────────────────────────────────────────┘   ║
║                                                           ║
║  Security Thresholds                                      ║
║  ┌─────────────────────────────────────────────────┐   ║
║  │ Max secrets: [0     ]  (default: 0)              │   ║
║  │ Max sast findings: [0     ]  (default: 0)        │   ║
║  │ Max crypto weak: [0     ]  (default: 0)          │   ║
║  └─────────────────────────────────────────────────┘   ║
║                                                           ║
║  Cache Settings                                          ║
║  ┌─────────────────────────────────────────────────┐   ║
║  │ Cache TTL (days): [7    ]  (default: 7)         │   ║
║  │ Cache size (MB): [100  ]  (default: 100)        │   ║
║  │ [Clear cache]                                    │   ║
║  └─────────────────────────────────────────────────┘   ║
║                                                           ║
║  [s] Save  [Esc] Back  [?] Help                        ║
╚══════════════════════════════════════════════════════╝
```

**Keybindings:**
- `[Tab]` — Navigate between fields
- `[Enter]` — Edit selected field
- `[s]` — Save settings to `.quality.toml`
- `[Esc]` — Back to dashboard
- `[?]` — Help

## Technical Stack

**UI Framework:** Togger-rs
- Purpose-built for business TUIs (tables, forms, navigation)
- Reusable components (no rolling from scratch)
- Clap integration for unified CLI/TUI command-line parsing
- Production-ready, actively maintained

**Data Flow:**
```
TUI → cogent check . --format json → Parse JSON → Display in tables
TUI → cogent fix . --dry-run → Show preview → Apply if user confirms
TUI → cogent watch . → Stream updates → Auto-refresh dashboard
```

**State Management:**
- User preferences: `.cogent-tui.toml` (window size, selected filters)
- Cogent config: `.quality.toml` (thresholds, tool selections)
- Navigation history: Stack-based (push screen on navigation, pop on back)

**Error Handling:**
- Never crash on tool failure
- Show error overlay with [Esc] to dismiss
- Log errors to `.cogent-tui.log` for debugging

## Implementation Plan

### PHASE 1: MVP (2 weeks)

1. **Dashboard screen** — Show project score, recent results
2. **Findings table** — List findings with drill-down
3. **Check integration** — Run `cogent check` and display results
4. **Basic navigation** — Dashboard ↔ Findings ↔ Settings

### PHASE 2: Polished UX (2 weeks)

1. **Quick actions** — Run full check, security-only, quality-only
2. **Findings detail** — View file in `$EDITOR`, show suggested fixes
3. **Settings screen** — Edit thresholds, cache settings
4. **Keyboard shortcuts** — [1-5] for quick actions, [?] for help

### PHASE 3: Power Features (2 weeks)

1. **Watch mode** — Auto-refresh on file changes
2. **History view** — View past check results
3. **Filter/sort** — Filter findings by severity, tool, file
4. **Export** — Export findings to CSV/JSON

### PHASE 4: Integration (1 week)

1. **CLI integration** — `cogent tui` launches TUI
2. **Help system** — Context-sensitive [?] help
3. **Error handling** — Graceful failure, error logs
4. **Documentation** — TUI user guide

## File Structure

```
crates/cogent-tui/
├── Cargo.toml          # Dependencies (togger, crossterm, serde_json)
├── src/
│   ├── main.rs         # Entry point, clap integration
│   ├── app.rs          # Application state, screen routing
│   ├── screens/        # Screen implementations
│   │   ├── dashboard.rs
│   │   ├── findings.rs
│   │   ├── settings.rs
│   │   └── help.rs
│   ├── widgets/        # Reusable components
│   │   ├── table.rs    # Findings table with sort/filter
│   │   ├── form.rs     # Settings form with validation
│   │   └── button.rs   # Clickable buttons
│   └── state.rs        # State management (history, preferences)
```

## Dependencies

```toml
[package]
name = "cogent-tui"
version.workspace = true
edition.workspace = true

[dependencies]
# UI framework
togger = "0.3"

# Terminal handling
crossterm = "0.27"

# JSON parsing
serde_json = "1"
serde = { version = "1", features = ["derive"] }

# Config handling
toml = "0.8"
dirs = "5"

# Error handling
anyhow = "1"
thiserror = "1"

# Async runtime (for watch mode)
tokio = { version = "1", features = ["full"] }

# File watching
notify = "6"

# Process spawning (for $EDITOR)
open = "5"

# Date/time formatting
chrono = "0.4"

# Cogent shared types
cogent-common = { path = "../cogent-common" }
cogent-config = { path = "../cogent-config" }
```

## Keybindings Reference

| Key | Action |
|-----|--------|
| `q` | Quit TUI |
| `?` | Show help for current screen |
| `Esc` | Go back / dismiss overlay |
| `Enter` | Select / view detail / open editor |
| `Tab` | Navigate between fields |
| `↑/↓` | Navigate list / table rows |
| `←/→` | Navigate table columns |
| `1-5` | Run quick action (dashboard) |
| `r` | Refresh all checks |
| `f` | Auto-fix selected finding |
| `n` | Next finding |
| `p` | Previous finding |
| `s` | Save settings |
| `/` | Search findings (findings screen) |
| `Ctrl+C` | Force quit (same as `q`) |

## User Readiness Checklist

- [ ] All screens have [?] help
- [ ] Error messages are clear and actionable
- [ ] Never crashes (graceful failure always)
- [ ] Keybindings are consistent across screens
- [ ] Tables support sort and filter
- [ ] Settings save to `.quality.toml`
- [ ] `$EDITOR` integration works (vim, vscode, etc.)
- [ ] Watch mode auto-refreshes on file changes
- [ ] History view shows past check results
- [ ] Export to CSV/JSON for offline analysis
- [ ] Documented in README and quickstart guide
- [ ] CLI integration (`cogent tui` works)
- [ ] Passes `cogent check .` self-audit

## Success Metrics

1. **Usability:** First-time user runs `cogent tui` and understands interface in < 30 seconds
2. **Performance:** Dashboard loads in < 2 seconds, no lag on keyboard input
3. **Reliability:** Never crashes, even if tools fail
4. **Adoption:** > 50% of users prefer TUI over CLI for local development

## Next Steps

1. **Create cargo package** — `cargo new --lib crates/cogent-tui-v2`
2. **Add to workspace** — Update Cargo.toml
3. **Implement dashboard** — Phase 1: show project score and recent results
4. **Implement findings table** — Phase 1: list findings with drill-down
5. **Integrate with cogent check** — Parse JSON output, display in tables
6. **Test with example-repo** — Verify TUI finds all 9 intentional bugs
7. **Document** — Add TUI section to README and quickstart guide
8. **Release** — Ship with v1.3.0

This is a REAL, USABLE TUI. It's not a demo. It's production-grade.

Let's build it.