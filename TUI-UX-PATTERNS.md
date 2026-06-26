# TUI UX Patterns from Existing Tools

This document analyzes UX patterns from two production TUIs: lazygit (Go, gocui) and tig (C, ncurses).

---

## lazygit — Modern TUI Excellence

### Key Lessons

#### 1. Context-Driven Architecture

lazygit uses a "context" system where every UI element is a context:

```go
type ContextKind int
const (
    SIDE_CONTEXT           // Files, branches, commits (left panel)
    MAIN_CONTEXT           // Main view (right panel)
    PERSISTENT_POPUP       // Commit message, menus (can return to)
    TEMPORARY_POPUP        // Generic prompts (can't return)
    EXTRAS_CONTEXT         // Command log (bottom)
    GLOBAL_CONTEXT         // Global keybindings
    DISPLAY_CONTEXT        // Views only, no keybindings
)
```

**What this means for Cogent:**
- Each screen (dashboard, findings, settings) is a context
- Findings table is a MAIN_CONTEXT
- Help overlay is a TEMPORARY_POPUP
- Settings form is a PERSISTENT_POPUP

#### 2. Keybinding System

lazygit has a sophisticated keybinding system:

- Multi-key bindings: `[c, c]` for "copy commit"
- Alternative bindings: `q` or `Esc` to quit
- Context-specific bindings: Different keys per screen
- Customizable via YAML/JSON config

**Keybinding patterns:**
- `q` — Quit
- `Esc` — Go back / dismiss popup
- `Enter` — Select / view detail
- `Tab` — Navigate between contexts
- `?` — Show keybindings for current context
- `/` — Search
- `[1-9]` — Quick actions

**Cogent keybindings:**
- Match lazygit: `[q]` quit, `[Esc]` back, `[?]` help, `[Enter]` select
- Add Cogent-specific: `[1-5]` quick actions, `[r]` refresh, `[f]` fix

#### 3. State Management

lazygit uses a layered state:

```
AppState
├── UIState (current context, focused view, window mode)
├── GitState (current branch, staging status, selected commit)
├── ControllerState (each screen has its own state)
└── CacheState (git output cache)
```

**Cogent state:**
```
TuiState
├── NavigationState (screen stack, current screen)
├── ProjectState (path, ecosystem, .quality.toml)
├── CheckState (findings, scores, last run time)
├── SettingsState (thresholds, cache settings)
└── PreferenceState (window size, selected filters, history)
```

#### 4. Screen Navigation

lazygit uses a stack-based navigation:

```
Dashboard → [Enter] → Findings → [Enter] → Finding Detail
                                  ← [Esc] ←
                  ← [Esc] ←
```

Persistent popups (commit message) can be returned to:
```
Dashboard → [c] → Commit Message → [?] → Keybindings
                 ← [Esc] ←          ← [Esc] ←
```

**Cogent navigation:**
```
Dashboard → [Enter] → Findings → [Enter] → Finding Detail
                                  ← [Esc] ←
                  ← [Esc] ←

Dashboard → [s] → Settings → [s] → Save
                  ← [Esc] ←
```

#### 5. Event Loop

lazygit uses gocui's event loop:

```go
func (gui *Gui) Run() error {
    g, err := gocui.NewGui(gocui.OutputTrue, false)
    if err != nil {
        return err
    }
    defer g.Close()

    // Set up keybindings
    for context, bindings := range keybindings {
        for key, action := range bindings {
            g.SetKeybinding(context, key, gocui.ModNone, action)
        }
    }

    // Main event loop
    return g.MainLoop()
}
```

**Cogent event loop (Togger-rs):**
```rust
fn main() -> Result<()> {
    let mut app = App::new();
    let mut terminal = ratatui::init();
    let events = Events::new();

    loop {
        // Draw current screen
        terminal.draw(|f| app.draw(f))?;

        // Handle events
        match events.next()? {
            Event::Input(key) => app.handle_input(key)?,
            Event::Tick => app.tick()?,
        }

        if app.should_quit {
            break;
        }
    }

    ratatui::restore();
    Ok(())
}
```

---

## tig — Robust Keybinding System

### Key Lessons

#### 1. Keymap System

tig uses a keymap per screen:

```c
struct keymap {
    const char *name;
    bool hidden;
    size_t size;
    struct keybinding *data;
};

static struct keymap keymaps[] = {
    { "generic" },      // Global keybindings
    { "search" },       // Search mode
    { "main" },         // Main view
    { "diff" },         // Diff view
    { "log" },          // Log view
    // ...
};
```

**Keybinding lookup:**
1. Check current screen's keymap
2. Fall back to generic keymap
3. Return request or REQ_UNKNOWN

**Cogent keymaps:**
```rust
enum Screen {
    Dashboard,
    Findings,
    Settings,
    Help,
}

fn get_keybinding(screen: Screen, key: Key) -> Option<Action> {
    match screen {
        Screen::Dashboard => dashboard_keymap.get(&key),
        Screen::Findings => findings_keymap.get(&key),
        Screen::Settings => settings_keymap.get(&key),
        Screen::Help => help_keymap.get(&key),
    }
}
```

#### 2. Key Matching

tig handles case-insensitive Ctrl keys:

```c
bool keybinding_matches(...) {
    for each key in binding {
        if (key1.modifiers.control && key2.modifiers.control) {
            // Case-insensitive for Ctrl
            if (toupper(key1.data) != toupper(key2.data))
                return false;
        } else {
            // Exact match for non-Ctrl
            if (memcmp(key1, key2, sizeof(key)))
                return false;
        }
    }
    return true;
}
```

**Cogent key matching:**
```rust
fn key_matches(binding: &Keybinding, input: &Key) -> bool {
    match (binding.modifiers, input.modifiers) {
        (Modifiers::CONTROL, Modifiers::CONTROL) => {
            // Case-insensitive for Ctrl
            binding.key.to_ascii_uppercase() == input.key.to_ascii_uppercase()
        }
        _ => binding == input,
    }
}
```

#### 3. Request System

tig uses a request enum for all actions:

```c
enum request {
    REQ_NONE,            // No action (disable binding)
    REQ_UNKNOWN,         // Key not bound
    REQ_QUIT,            // Quit
    REQ_VIEW_MAIN,       // Switch to main view
    REQ_VIEW_DIFF,       // Switch to diff view
    REQ_SCROLL_LINE_DOWN,// Scroll down
    REQ_ENTER,           // Enter current item
    // ...
};
```

**Cogent actions:**
```rust
enum Action {
    None,
    Unknown,
    Quit,
    Back,
    Refresh,
    ViewFindings,
    ViewFindingDetail,
    ViewSettings,
    SaveSettings,
    RunFullCheck,
    RunSecurityOnly,
    RunQualityOnly,
    EditInEditor,
    AutoFix,
    ShowHelp,
    Search,
}
```

---

## Summary of UX Patterns

### Screen Layout

```
┌─────────────────────────────────────────────────────┐
│ Header: Title + Version + Status + Help (q ?)       │
├─────────────────────────────────────────────────────┤
│                                                     │
│ Main Content (tables, forms, lists)                 │
│                                                     │
├─────────────────────────────────────────────────────┤
│ Footer: Keybindings for current context             │
└─────────────────────────────────────────────────────┘
```

### Keybinding Consistency

| Key | Action | Context |
|-----|--------|---------|
| `q` | Quit | All |
| `?` | Show help | All |
| `Esc` | Go back / dismiss popup | All |
| `Enter` | Select / view detail | All |
| `Tab` | Navigate between fields | Forms only |
| `↑/↓` | Navigate rows | Tables/lists only |
| `←/→` | Navigate columns | Tables only |
| `[1-9]` | Quick actions | Dashboard only |
| `r` | Refresh | Dashboard only |
| `f` | Auto-fix | Findings detail only |
| `/` | Search | Findings only |
| `s` | Save | Settings only |

### State Management

```rust
struct AppState {
    // Navigation
    screen_stack: Vec<Screen>,
    current_screen: Screen,

    // Project
    project_path: PathBuf,
    ecosystem: Ecosystem,
    config: QualityConfig,

    // Check results
    findings: Vec<Finding>,
    scores: HashMap<String, Score>,
    last_run: SystemTime,

    // Settings
    settings: SettingsState,
    pending_changes: bool,

    // Preferences (persistent)
    window_size: (u16, u16),
    selected_filters: Filters,
    history: VecDeque<HistoryEntry>,

    // Exit flag
    should_quit: bool,
}
```

### Error Handling

lazygit: Never crashes, shows error overlay
tig: Returns error codes, shows status messages

**Cogent error handling:**
```rust
fn handle_error(err: &dyn std::error::Error) {
    // Show error overlay
    show_overlay(format!("Error: {}", err));

    // Log to file for debugging
    log::error!("{:?}", err);
}
```

---

## Implementation Checklist

- [ ] Define `Screen` enum (Dashboard, Findings, Settings, Help)
- [ ] Define `Action` enum (Quit, Back, Refresh, etc.)
- [ ] Define keymap per screen (HashMap<Key, Action>)
- [ ] Implement `AppState` struct with all state
- [ ] Implement event loop (draw → handle input → update)
- [ ] Implement navigation stack (push screen on nav, pop on back)
- [ ] Implement context-sensitive help ([?] shows current screen's keybindings)
- [ ] Implement persistent settings (save to `.cogent-tui.toml`)
- [ ] Implement error overlay (never crashes)
- [ ] Test keybinding consistency (q, ?, Esc, Enter work everywhere)

---

## Next Steps

1. Read lazygit's `pkg/gui/controllers/*.go` for action patterns
2. Read lazygit's `pkg/gui/types/context.go` for context interface
3. Study tig's `src/keys.c` for keybinding system
4. Implement Cogent TUI using Togger-rs framework
5. Test with example-repo (verify all 9 bugs found)

---

References:
- lazygit: https://github.com/jesseduffield/lazygit
- tig: https://github.com/jonas/tig
- Togger-rs: https://github.com/togger-rs/togger