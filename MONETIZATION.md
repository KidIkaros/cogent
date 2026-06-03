# Cogent — Monetization Strategy

> **Status:** Strategy Document v1.0
> **Context:** This strategy is grounded in the convergence of two megatrends:
> 1. The **$13–15B Application Security Testing (AST) market** growing at ~25% CAGR
> 2. The **emerging agent economy** where AI agents replace humans as the primary consumers of software

---

## 🧭 The Strategic Landscape

### Where Cogent Sits

| Dimension | Cogent's Position |
|:---|:---|
| **Problem** | Security & quality tooling is fragmented — teams run 3-5 tools with separate configs, dashboards, and pricing |
| **Solution** | One CLI, 32 commands, zero config, structured JSON/NDJSON/SARIF output |
| **Moat** | Agent-native design (MCP server, NDJSON, deterministic exit codes), offline/air-gapped, multi-language |
| **Competition** | SonarQube (~$100-175M ARR), Snyk (~$300-400M ARR), Semgrep ($30/contributor/mo), GitHub Advanced Security ($30-49/active committer/mo) |
| **Market TAM** | $13-15B AST market, growing 25%+ CAGR, enterprise deal sizes $25K-500K+ ARR |

### The Core Thesis

> **Cogent wins by being the *first* security tool built for the agent era, not by being a cheaper SonarQube.**

Traditional tools sell to humans (dashboards, seats, UIs). Cogent sells **deterministic, verifiable audit outcomes** to autonomous agents. This is a fundamentally different market with different pricing dynamics.

---

## 🏛️ The Strategy: Four Revenue Layers

The strategy uses a **layered approach** — each layer builds on the one below it, capturing value at progressively higher margins.

```
                    ▲
                   / \            HIGHER MARGIN
                  / 4 \           Layer 4: Verify Network
                 /─────\          (per-verification, outcome-based)
                /  3   \
               /────────\        
              /   2      \       Layer 3: Enterprise
             /────────────\      (annual contracts)
            /     1        \
           /────────────────\   Layer 2: Cogent Cloud
          /        0          \  (SaaS subscriptions)
         /──────────────────────\
        /           0             \  Layer 0: Open-Core CLI
       /────────────────────────────\  (free, drives adoption)
                                      LARGER AUDIENCE
```

---

## Layer 0: Open-Core CLI (Free)

**What it is:** The `cogent` CLI binary — all 32 commands, MCP server, agent integration. Free and open source under Apache-2.0 / OPL-1.1.

**Purpose:** 
- Drive massive adoption (the "bottom-up" motion)
- Build the community 
- Become the default security audit tool for AI agents
- Create a moat through ubiquity

**Production readiness:** Already at v1.1.1, passes 24/25 checks on its own codebase, CI-validated, Docker image, Homebrew formula.

**Why free:** This is the **distribution layer**. Every developer who installs Cogent, every agent that calls `cogent check . --format ndjson`, is a potential conversion to a paid layer. Open-core companies like GitLab, HashiCorp, and Elastic built billion-dollar businesses on exactly this model.

**Key metric:** Monthly active installations / agent invocations.

---

## Layer 1: Cogent Cloud (SaaS — $29-99/month)

**What it is:** A hosted dashboard that gives teams visibility across all their repositories.

**Features:**
- Cross-repo audit history & trend tracking
- Health score dashboards (A-F grades across all repos)
- SARIF aggregation (one place to see all findings)
- Team collaboration (share reports, assign fixes)
- Historical diffing — see quality improving or degrading over time
- GitHub/GitLab/Bitbucket integration (auto-import repos)
- Slack/Teams notifications on regressions

**Pricing:**
| Tier | Price | Repos | Users | Features |
|:---|:---|:---|:---|:---|
| **Starter** | $29/mo | Up to 10 | 5 | Dashboard, history, GitHub integration |
| **Team** | $99/mo | Up to 50 | 25 | All Starter + SARIF aggregation, Slack alerts, historical diff |
| **Growth** | $299/mo | Unlimited | Unlimited | All Team + API access, custom integrations, priority support |

**Why this works:**
- The CLI is free and drives adoption. Once a team has 10+ repos using it, manually checking each one is painful → they pay for the dashboard.
- Low friction — no separate login, just `cogent cloud connect` to link your CLI to the dashboard.
- Aligns with our headless philosophy: the CLI does the work, the cloud provides the *optional* human-friendly UI.

**Revenue projection:** If Cogent reaches 10,000 active CLI installs, a 3% conversion to Cloud Starter = 300 customers × $29/mo = ~$104K ARR. If 1% goes to Team = 100 × $99/mo = ~$119K ARR. **Total Cloud: ~$223K ARR at 10K installs.**

---

## Layer 2: Enterprise (Annual Contracts — $15K-50K/year)

**What it is:** The enterprise-grade version for regulated industries and large organizations.

**Features:**
- **SSO/SAML** — mandatory for enterprise procurement
- **RBAC** — role-based access control for large teams
- **Audit logging** — who ran which audit when, for compliance
- **On-premise deployment** — Docker image that runs in your VPC, no data leaves your network
- **Custom compliance packs** — PCI-DSS, SOC2, HIPAA, FedRAMP-specific rule sets
- **Custom rule engine** — write proprietary rules for internal standards
- **SLA support** — guaranteed uptime, priority ticket handling
- **Bulk licensing** — seat-agnostic, based on number of repos or org size

**Pricing:**
| Tier | Price | What You Get |
|:---|:---|:---|
| **Enterprise Starter** | $15K/yr | SSO, RBAC, audit logging, on-prem deployment, email support |
| **Enterprise Pro** | $35K/yr | All Starter + custom compliance packs, custom rules, phone support |
| **Enterprise Premier** | $50K+/yr | All Pro + dedicated TAM, custom SLA, training, co-developed rules |

**Why this works:**
- This is the standard enterprise playbook (SonarQube, Snyk, Semgrep all do this).
- The CLI is already air-gapped and offline-capable — a huge advantage for regulated enterprises that can't use cloud tools.
- The open-core model means adoption happens bottom-up (developers install the free CLI), then procurement happens top-down (CISO mandates enterprise version).

**Revenue projection:** If 0.5% of 10K installs convert to Enterprise Starter = 50 customers × $15K = $750K ARR. If 0.2% go Pro = 20 × $35K = $700K ARR. **Total Enterprise: ~$1.45M ARR at 10K installs.**

---

## Layer 3: Cogent Verify Network (🚀 The Game-Changer)

This is the most innovative layer — and the one most aligned with the future we've been discussing.

### The Problem It Solves

In the agent economy, trust is a premium:

- **Agent A** wants to buy a service from **Agent B**
- **Agent A** needs to verify: *"Is Agent B's codebase secure? Is it compliant? Is it safe to transact with?"*
- Today, there's no standard way for one agent to *prove* its code quality to another agent

### The Solution

**Cogent Verify** is a cryptographic attestation service that turns `cogent check .` into a verifiable, tamper-proof asset.

**How it works:**

```
1. Agent B runs:  cogent check . --verify
2. Cogent CLI audits the codebase (deterministic, local, offline)
3. CLI submits a hash of the results to the Cogent Verify Network
4. Network signs the attestation with a cryptographic key
5. Output: A machine-readable attestation token

Attestation {
  repo_hash: "sha256:abc...",
  score: 100,
  grade: "A",
  checks_passed: 25,
  checks_failed: 0,
  timestamp: 2026-06-01T12:00:00Z,
  signature: "0x...",
  signer: "cogent-verify/v1"
}

6. Agent A queries: "Has Agent B passed a verified audit?"
   → Cogent Verify responds with the latest attestation
```

### What Agents Can Do With This

| Use Case | How It Works | Value |
|:---|:---|:---|
| **Agent-to-agent trust** | Before transacting, Agent A checks Agent B's Cogent Verify attestation | Prevents rogue agents from spreading malicious code |
| **Supply chain security** | Verify that every dependency in your supply chain has passed a security audit | Real-time SBOM + attestation |
| **CI/CD gating** | Deploy only if the latest commit has a Cogent Verify attestation with score ≥ 90 | Deterministic deployment gate |
| **Insurance / Compliance** | Use attestations as evidence of due diligence for cyber insurance audits | Lower premiums, faster audits |
| **Smart contract auditing** | DeFi protocols require Cogent Verify attestation before allowing a contract to be listed | Trustless security for DeFi |

### Pricing Model: Outcome-Based

This is the **outcome-based pricing** model we discussed — agents pay for *verified outcomes*, not seats:

| Service | Price | What You Get |
|:---|:---|:---|
| **Verify Lite** | Free | Basic attestation (CLI-only, no network signature) |
| **Verify Standard** | $0.10 per attestation | Network-signed attestation, public verification |
| **Verify Pro** | $0.50 per attestation | All Standard + human review of critical findings, insurance-grade |
| **Verify Enterprise** | Custom | Private verification network, dedicated signers, SLA |

**Why this pricing works:**
- **No per-seat pricing** (consistent with Cogent's brand promise)
- **Pay for outcome, not effort** — you pay when an attestation is generated, not for the CI job
- **Scales with value** — a DeFi protocol verifying a smart contract before a $10M listing will happily pay $0.50 for a trusted attestation
- **Aligned incentives** — Cogent only makes money when agents actually *use* the verification service

### The Network Effect Moat

The Verify Network gets **more valuable as more people use it**:

1. More attestations → richer reputation data → more valuable queries
2. More agents verifying → more reason for other agents to get verified
3. More verified agents → higher trust in the network → higher willingness to pay

This creates a **data flywheel** that's extremely hard for competitors to replicate.

### Revenue Projection

Conservative estimates for Year 1:
- 1,000 monthly active CLI installs
- 10% use Verify Standard = 100 × ~100 attestations/mo × $0.10 = $1,000/mo
- 2% use Verify Pro = 20 × ~50 attestations/mo × $0.50 = $500/mo
- **Total: ~$18K ARR**

Year 3 (scaled):
- 100,000 monthly active CLI installs
- 5% use Verify Standard = 5,000 × ~50 attestations/mo × $0.10 = $25,000/mo
- 1% use Verify Pro = 1,000 × ~30 attestations/mo × $0.50 = $15,000/mo
- **Total: ~$480K ARR from Verify alone**

---

## 📊 Revenue Projections Summary

### Year 1 (Conservative)

| Layer | Customers | Revenue |
|:---|:---|:---|
| Layer 0: CLI | 1,000 installs | $0 (free) |
| Layer 1: Cloud | 30 teams × $29-99/mo | ~$15K ARR |
| Layer 2: Enterprise | 3 customers × $15-35K/yr | ~$65K ARR |
| Layer 3: Verify | ~120 active users | ~$18K ARR |
| **Total** | | **~$98K ARR** |

### Year 3 (Growth)

| Layer | Customers | Revenue |
|:---|:---|:---|
| Layer 0: CLI | 100,000 installs | $0 (free) |
| Layer 1: Cloud | 3,000 teams | ~$500K ARR |
| Layer 2: Enterprise | 100 customers | ~$3.5M ARR |
| Layer 3: Verify | ~6,000 active users | ~$480K ARR |
| **Total** | | **~$4.48M ARR** |

### Year 5 (Scale)

| Layer | Customers | Revenue |
|:---|:---|:---|
| Layer 0: CLI | 1M+ installs | $0 (free) |
| Layer 1: Cloud | 15,000 teams | ~$2.5M ARR |
| Layer 2: Enterprise | 500 customers | ~$17.5M ARR |
| Layer 3: Verify | ~50,000 active users | ~$4.8M ARR |
| **Total** | | **~$24.8M ARR** |

---

## ⚖️ Comparison: Cogent vs. Incumbent Pricing Models

| Vendor | Model | Price Point | Alignment with Agent Era |
|:---|:---|:---|:---|
| **SonarQube** | Per LoC | ~$150K+/yr for 1M lines | ❌ — LoC doesn't map to agent value |
| **Snyk** | Per developer | ~$50-100/dev/mo | ❌ — Agents aren't developers |
| **Semgrep** | Per contributor | $30/contributor/mo | ❌ — Same issue |
| **GitHub GHAS** | Per active committer | $30-49/committer/mo | ❌ — Same issue |
| **Cogent Verify** | Per outcome (attestation) | $0.10-0.50/verify | ✅ — Agent-native, outcome-based |

**Cogent's pricing advantage:** When the customer is an AI agent, "per developer" makes no sense. Per-verification (outcome-based) is the only model that:
- Scales naturally with agent usage
- Aligns cost with value delivered
- Doesn't require a human procurement process
- Is machine-readable and machine-payable (via crypto/streaming payments)

---

## 🛡️ Defensibility: Why This Works

| Threat | How We're Protected |
|:---|:---|
| **Someone forks the CLI** | The CLI is free — forking doesn't hurt us. The moat is the Verify Network (network effects + cryptographic trust) |
| **Competitor builds a verification network** | First-mover advantage + the CLI has 32 commands they'd need to replicate. By the time they catch up, we have the reputation ledger |
| **Agents don't care about verification** | They will when they need to prove their security to another agent. This is inevitable in the agent economy |
| **Enterprises won't pay for "just a CLI"** | That's why we have Cloud + Enterprise layers. Enterprises buy SSO, RBAC, compliance packs |
| **Pricing too low** | The core CLI is free by design. The value capture is in verification and enterprise features, which have high perceived value |

---

## 🎯 Recommended Next Steps

### Immediate (Now — 3 months)

1. **Ship Cogent Cloud (MVP)**
   - Dashboard showing audit history across repos
   - Basic team features (shared reports)
   - $29-99/mo pricing
   - Single `cogent cloud connect` command to link CLI to cloud

2. **Design the Verify Network protocol**
   - Define the attestation schema
   - Build the signing infrastructure
   - Create a public verification API
   - Ship `cogent check . --verify` flag

3. **Build the enterprise sales motion**
   - SSO/SAML integration
   - RBAC for the Cloud dashboard
   - On-prem Docker deployment
   - Create sales collateral targeting CISOs

### Medium (3 — 12 months)

4. **Launch Verify Network (public beta)**
   - Partner with 3-5 DeFi / smart contract projects as first users
   - $0.10-0.50 per verification
   - Public verification API with documentation for agent integration

5. **Enterprise compliance packs**
   - PCI-DSS, SOC2, HIPAA specific rule sets
   - Charge as add-ons ($5-10K/yr each)

6. **Agent marketplace**
   - Pre-built agent skills for major agent frameworks (LangChain, Olas, Hermes)
   - Let other developers sell their agent skills on the platform
   - Take 20% commission

### Long (12+ months)

7. **Cogent becomes the default audit layer for the agent economy**
   - Every major agent platform integrates Cogent Verify by default
   - Smart contract platforms require Cogent attestation for listing
   - Insurance companies accept Cogent attestations as audit evidence

---

## 💡 The Provocative Take

> **Cogent's CLI is not the product. The product is *trust*.**

The CLI is the distribution mechanism — it gets Cogent into every CI pipeline, every developer's machine, and every agent's toolkit. The monetization comes from being the **trust layer** that the agent economy runs on.

When an agent needs to prove "my codebase is secure" to another agent before a transaction, it calls Cogent Verify. That verification is worth $0.10-0.50 because it unblocks an agent-to-agent transaction worth potentially thousands of dollars.

**This is the outcome-based pricing model we've been discussing — applied to security auditing.**

---

## Appendix: Comparable Benchmarks

| Company | Model | Revenue | Valuation | Free Users | Conversion |
|:---|:---|:---|:---|:---|:---|
| **SonarSource** | Open-core + Enterprise | ~$100-175M ARR | $4.7B | 100K+ | ~3% |
| **Snyk** | Freemium + Enterprise | ~$300-400M ARR | $7.4B | 500K+ | ~2-4% |
| **Semgrep (r2c)** | Free Tier + Paid | ~$10-20M ARR (est.) | Unicorn | 100K+ | ~2-3% |
| **GitLab** | Open-core + SaaS + Enterprise | ~$500M+ ARR (public) | $8B+ | Millions | ~5% |
| **HashiCorp** | Open-core + Enterprise | ~$300M ARR (pre-acquisition) | Sold for $7.2B | Millions | ~2% |

**Cogent's path:** Follow the open-core playbook proven by these companies, but add the Verify Network as a unique, agent-native revenue layer that none of them have.

---

*Strategy document v1.0 — June 2026*
*Built on the thesis: "In a world of agents, trust is the premium product."*
