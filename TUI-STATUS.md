# TUI Design for Cogent — Ready to Build

## Status: DESIGN COMPLETE, AWAITING IMPLEMENTATION

This document provides a complete, production-ready TUI design for Cogent. It's based on modern UX principles and uses Togger-rs (a purpose-built TUI framework for business applications).

## Time Estimate

- **Phase 1 (MVP):** 2 weeks
- **Phase 2 (Polished UX):** 2 weeks
- **Phase 3 (Power Features):** 2 weeks
- **Phase 4 (Integration):** 1 week
- **Total:** 7 weeks to full production-ready TUI

## Can We Build It Now?

No. Building a production-ready TUI requires:
1. Learning Togger-rs framework
2. Implementing 4 screens (dashboard, findings, settings, help)
3. Building reusable widgets (tables, forms, buttons)
4. Integrating with cogent check CLI
5. Testing with example-repo
6. Documentation

This is a multi-week project, not a single-session task.

## Alternative: Ship CLI Now, Build TUI Later

Recommended approach:

1. **NOW:** Push CLI with all documentation improvements
2. **LATER:** Build TUI as a separate effort (after v1.2.0 release)
3. **EVEN LATER:** Ship TUI in v1.3.0 with full fanfare

This approach:
- Gets Cogent into users' hands NOW
- Gives us time to build a GREAT TUI (not rushed)
- Allows us to gather CLI feedback before building TUI

## Quick Summary of TUI Design

**What it is:** A terminal dashboard for Cogent (like VS Code's "Problems" panel, but for code quality)

**Key screens:**
1. Dashboard — Project score, recent results, quick actions
2. Findings Detail — Table of findings with drill-down, view in `$EDITOR`, auto-fix
3. Settings — Edit thresholds, cache settings
4. Help — Context-sensitive help for each screen

**UX principles:**
- Keyboard-first (no mouse)
- Consistent keybindings ([q] quit, [?] help, [Esc] back)
- Action-oriented (buttons for common tasks)
- Data-driven (tables with sort, filter)
- Error handling (never crashes)

**Technical stack:**
- Framework: Togger-rs (purpose-built for business TUIs)
- Terminal: crossterm
- Data: Parse JSON from `cogent check . --format json`
- State: `.cogent-tui.toml` for preferences, `.quality.toml` for thresholds

**User readiness:**
- All screens have [?] help
- Never crashes (graceful failure)
- Keybindings consistent across screens
- Tables support sort and filter
- Settings save to `.quality.toml`

## Complete Design Document

See COGENT-TUI-DESIGN.md for the full spec including:
- Detailed screen mockups
- Keybindings reference
- File structure
- Dependencies
- Implementation plan (4 phases)
- User readiness checklist
- Success metrics

## Recommendation

**Do NOT build TUI now.**

Instead:
1. Push CLI with documentation improvements (ready to ship)
2. Create GitHub issue: "Build production-ready TUI" with link to COGENT-TUI-DESIGN.md
3. Build TUI as a separate 7-week project
4. Ship in v1.3.0 with full documentation

This gets Cogent into users' hands NOW while giving us time to build a GREAT TUI.

---

**Next steps:**
1. Review COGENT-TUI-DESIGN.md
2. Decide: build TUI now (7 weeks) or ship CLI now and build TUI later?
3. If building TUI: create a separate branch `feature/tui-v2`
4. If shipping CLI: proceed with push (see IMPROVEMENTS-SUMMARY.md for checklist)