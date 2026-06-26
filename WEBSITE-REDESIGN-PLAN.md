# Cogent Website Redesign Plan

## Problem Statement

The current website (https://kidikaros.github.io/cogent/) feels stale and outdated. It lacks:
- Visual impact and modern design
- Interactivity and animations
- Clear visual hierarchy
- Working demos (asciinema may fail to load)
- "Try it now" experience

---

## Design Goals

1. **Modern aesthetic** — Inspired by Vercel, Linear, Supabase
2. **Interactive** — Users can try Cogent without installing
3. **Fast** — No external dependencies (drop asciinema, use native)
4. **Clear** — Better visual hierarchy, less text
5. **Conversion-focused** — Guide users to install

---

## Visual Direction

Inspiration sites:
- https://vercel.com (dark mode, gradients, subtle animations)
- https://linear.app (clean typography, hero video, animated stats)
- https://supabase.com (grid layouts, code snippets with syntax highlighting)
- https://rust-lang.org (rusty branding, orange/black accent)

**Color palette:**
- Primary: #e4f222 (Cogent green) — keep brand color
- Dark: #0a0a0a (near-black background)
- Light: #ffffff (text)
- Accent: #7c3aed (purple) — for gradients
- Surface: #1a1a1a (card backgrounds)

**Typography:**
- Headings: Inter (modern sans-serif)
- Code: JetBrains Mono (monospace)
- Body: Inter

---

## Section-by-Section Redesign

### 1. Hero Section

**Current:**
- Static text + 2 buttons
- No animation

**New:**
```
┌─────────────────────────────────────────────────────────┐
│  [Logo] Cogent                                 [GitHub]  │
├─────────────────────────────────────────────────────────┤
│                                                         │
│  Security · Quality · Compliance                       │
│                                                         │
│  One CLI. 31 audit tools. Zero config.                │
│                                                         │
│  [Gradient bar]                                         │
│                                                         │
│  [Get started] [View on GitHub]                         │
│                                                         │
│  ┌─────────────────────────────────────────────────┐   │
│  │ $ cogent init                                   │   │
│  │ → detected: Rust                                │   │
│  │ ✓ wrote .quality.toml                           │   │
│  │                                                  │   │
│  │ $ cogent check .                                │   │
│  │ ╔════════════════════════════════════════╗       │   │
│  │ ║  COGENT CHECK  ·  PASSED ✓              ║       │   │
│  │ ╠════════════════════════════════════════╣       │   │
│  │ ║  31/31 checks passed  ·  5.1s total     ║       │   │
│  │ ║  Score: 100/100  A                       ║       │   │
│  │ ╚════════════════════════════════════════╝       │   │
│  └─────────────────────────────────────────────────┘   │
│                                                         │
│  Trusted by teams at: [logos]                          │
│                                                         │
└─────────────────────────────────────────────────────────┘
```

**Features:**
- Animated terminal (typing effect)
- Gradient accent bar
- Trust badges (logos)
- Smooth fade-in animations

---

### 2. Stats Section

**Current:**
```
31  Audit tools
5   Categories
10+ Languages
0   Runtime deps
```

**New:**
```
┌─────────────────────────────────────────────────────────┐
│                                                         │
│      31           5           10+           0          │
│  Audit Tools  Categories   Languages  Runtime Deps    │
│                                                         │
│  [Animated count-up]                                    │
│                                                         │
└─────────────────────────────────────────────────────────┘
```

**Features:**
- Animated count-up numbers
- Gradient text
- Hover effects on cards

---

### 3. Tools Section

**Current:**
- Long list of 31 tools in text
- No visual grouping

**New:**
```
┌─────────────────────────────────────────────────────────┐
│  The 31-tool engine suite                                │
├─────────────────────────────────────────────────────────┤
│                                                         │
│  [Quality 16] [Security 7] [Compliance 2]              │
│  [Supply Chain 2] [Operations 4]                       │
│                                                         │
│  ┌─────────────┬─────────────┬─────────────┬──────────┐│
│  │ crap        │ secrets     │ licenses    │ sast     ││
│  │ debt        │ taint       │ sbom        │ crypto   ││
│  │ doccov      │ errhandle   │ supply-chain│ access   ││
│  │ ...         │ ...         │ outdated    │ ...      ││
│  └─────────────┴─────────────┴─────────────┴──────────┘│
│                                                         │
│  [Hover for description]                                │
│                                                         │
└─────────────────────────────────────────────────────────┘
```

**Features:**
- Filterable by category
- Hover tooltips for each tool
- Search box
- Click to view tool details

---

### 4. Comparison Section

**Current:**
- Large table with 5 columns
- Hard to read
- Overwhelming

**New:**
```
┌─────────────────────────────────────────────────────────┐
│  Why Cogent?                                            │
├─────────────────────────────────────────────────────────┤
│                                                         │
│  ┌─────────────────────────────────────────────────┐   │
│  │ Single binary                                   │   │
│  │ No JVM, no cloud token, no per-seat pricing     │   │
│  │ Others: SonarQube (JVM+DB), Snyk (cloud)        │   │
│  └─────────────────────────────────────────────────┘   │
│                                                         │
│  ┌─────────────────────────────────────────────────┐   │
│  │ Machine-first output                            │   │
│  │ JSON · NDJSON · SARIF for CI and agents         │   │
│  │ Others: Limited output formats                  │   │
│  └─────────────────────────────────────────────────┘   │
│                                                         │
│  ┌─────────────────────────────────────────────────┐   │
│  │ Deterministic gates                              │   │
│  │ Same input, same exit code (0/1/2)              │   │
│  │ Others: Non-deterministic, flaky                 │   │
│  └─────────────────────────────────────────────────┘   │
│                                                         │
│  [View full comparison →]                              │
│                                                         │
└─────────────────────────────────────────────────────────┘
```

**Features:**
- Card-based layout (not table)
- Focus on 3 key differentiators
- Link to full comparison modal
- Clear "us vs them" contrast

---

### 5. Installation Section

**Current:**
- 4 copy-paste blocks
- No interactivity

**New:**
```
┌─────────────────────────────────────────────────────────┐
│  Install in 60 seconds                                  │
├─────────────────────────────────────────────────────────┤
│                                                         │
│  [macOS] [Linux] [Windows] [Docker] [Cargo]           │
│                                                         │
│  ┌─────────────────────────────────────────────────┐   │
│  │ $ brew tap kidikaros/cogent                       │   │
│  │ $ brew install cogent                            │   │
│  │                                                  │   │
│  │ [Copy] [Test in browser]                         │   │
│  └─────────────────────────────────────────────────┘   │
│                                                         │
│  Want to try without installing?                       │
│  [Open Web Demo →]                                     │
│                                                         │
└─────────────────────────────────────────────────────────┘
```

**Features:**
- Tabbed interface (one set of commands, switch platforms)
- "Test in browser" button (opens mini-demo)
- Link to web demo (no install needed)

---

### 6. Web Demo (NEW)

**What it is:**
- Interactive terminal in browser
- Users can run `cogent init`, `cogent check .`, `cogent explain`
- No installation needed
- Uses JavaScript to simulate CLI

**Implementation:**
- HTML/JS terminal component
- Pre-loaded with demo codebase
- Real-ish responses (not actually running Cogent)

---

## Technical Implementation

### Framework: No Framework

Use vanilla HTML/CSS/JS to keep it simple:
- No build step
- No external dependencies
- Works on GitHub Pages

### Key Technologies:

1. **CSS:**
   - CSS Grid for layout
   - CSS Variables for theming
   - CSS Animations for micro-interactions

2. **JavaScript:**
   - Vanilla JS for interactivity
   - No external libraries (except optional syntax highlighting)
   - Code splitting for web demo

3. **Icons:**
   - Inline SVG (no external font)
   - Lucide icons (modern, consistent)

### File Structure:
```
site/
  index.html        # Main landing page
  demo.html         # Web demo (interactive terminal)
  styles.css        # Global styles
  web-demo.js       # Demo terminal logic
  fonts/            # Embedded fonts (or system fonts)
  logos/            # Trust badges (SVG)
```

---

## Phase 1: Hero + Stats + Install (MVP)

**Time:** 2-3 hours

**Deliverables:**
1. Animated hero with terminal
2. Animated stats
3. Tabbed installation
4. Better color scheme

**Impact:** High (first impression)

---

## Phase 2: Tools + Comparison

**Time:** 2-3 hours

**Deliverables:**
1. Filterable tools grid
2. Card-based comparison
3. Tool detail tooltips

**Impact:** Medium (information architecture)

---

## Phase 3: Web Demo

**Time:** 3-4 hours

**Deliverables:**
1. Interactive terminal in browser
2. Pre-loaded demo project
3. Real-ish CLI responses

**Impact:** High (try before buy)

---

## Phase 4: Polish

**Time:** 1-2 hours

**Deliverables:**
1. Smooth animations
2. Mobile responsive
3. Accessibility (a11y)
4. Performance optimization

**Impact:** Low (nice to have)

---

## Next Steps

1. **Approve this plan** — Do you like the direction?
2. **Start with Phase 1** — Hero + stats + install
3. **Iterate** — Get feedback, adjust

---

**Total time:** 8-12 hours
**Priority:** Start with Phase 1 (highest impact)