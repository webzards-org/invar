# Invar — MVP Design Document

> A Claude Code plugin that brings a spec-driven, two-phase
> generate-and-verify loop to any FastAPI project.

## The name

Invar is a nickel–iron alloy whose defining property is that it barely moves:
heat it, load it, and its dimensions stay where they were. Its name is short
for *invariable*, and it was made for instruments — measuring rods, clock
pendulums, reference standards — where a reading is only worth anything if the
thing doing the measuring hasn't changed shape since the last reading.

That is the whole idea of this tool.

An AI coding agent is fast, capable, and under pressure — a long session, a
half-understood ticket, a test that won't pass. Everything under pressure
deforms somewhere, and what deforms first is the specification: the acceptance
criterion gets quietly relaxed until the code satisfies it. The result still
looks correct, which is worse than a visible failure.

**Invar holds the specification fixed while the code is generated against it.**
One human-readable scenario is both the input to generation and the criterion
for verification, approved by a person and then checksummed, so the party doing
the implementing cannot move the target it is being measured against. Around
that sit the things that make the fixed point trustworthy: a business context
the agent must plan within, a decision log that remembers why, and
deterministic checks that render every verdict.

The measuring rod does not stretch to fit the measurement.

*(The name also has a second, more literal fit: `BUSINESS_CONTEXT.md`'s
**Invariants** — §5 — are the rules the application must obey no matter what
any single ticket wants.)*

---

> **Revision note.** This document supersedes the previous MVP design, which
> was written when the tool was a standalone CLI named `scenera`. The CLI
> command, the config file and the state directory are now `invar`,
> `invar.yaml` and `.invar/`; the feature tag is `@invar` and the context
> marker is `<!-- invar-context: v1 -->`.
>
> **Delivery form changed.** MVP ships as a **Claude Code plugin that contains
> the CLI** (§13), not as a runtime-agnostic CLI. Hooks and an isolated
> planning context are part of the protocol, not optional improvements. The CLI
> remains the contract and holds all the logic — that is what keeps a second
> runtime, or a separate plugin per language, cheap later (§13b, §14.2) — but
> other runtimes are **not a supported target in MVP**.
>
> Six things changed in the flow: the approval gate now covers the Plan *and*
> the scenarios (§3); `.feature` files carry an approval checksum (§4, with
> canonicalization and check scope in §4d); `Status` no longer has
> `superseded` (§4); deleting a scenario is a legitimate cycle outcome declared
> in the Plan (§4c); framework and package context are new, separate,
> versioned context sources with their own sync command (§5b, §8); and agent
> hooks are specified (§13). §14 states two design constraints that shape the
> whole thing and must not be designed around later: an **external context API
> with an MCP front end** (§14.1) and **multi-language support** (§14.2).


## 1. Purpose

Close the loop between "what the developer wants the code to do" and "what the
AI-generated code actually does," by using a single, human-readable scenario as
the source of truth for both **generation** and **verification**.

Positioning: an "AI experience" layer for FastAPI — works standalone on any
FastAPI project, and works *better* when a structured `AI_CONTEXT.md` is
present (whether authored by FastKit, hand-written, or by another tool).

## 2. Terminology

| Term | Definition |
|---|---|
| **Scenario** | A single behavior specification, written in Gherkin (`Given/When/Then`), stored as a `.feature` file. The unit of work for the whole pipeline. |
| **Scenario Agent** (Phase 1) | Claude Code operating under the Phase 1 instructions, in a **planning sub-agent with its own context** (§13a). Converses to clarify intent, then drafts a Plan and the Scenario files, which are approved together at one gate. Does not write implementation code. |
| **Codegen Agent** (Phase 2) | The main session operating under the Phase 2 instructions. Takes an approved Plan + `confirmed` Scenario (+ context, if present) and produces implementation code and its matching behavioral test. Does **not** read the decision log (§7) — enforced by context separation, not instruction. |
| **Plugin** | The delivery form of the whole system: a Claude Code plugin bundling the `invar` CLI, the hook definitions (§13), the instruction files, and the planning sub-agent. What the developer installs. |
| **CLI** (`invar`) | The binary inside the plugin. Holds **all** the logic and is the only component that renders a verdict. Runnable on its own, which is why it also runs in CI where no agent exists (§13b). |
| **Instruction files** | Agent-facing protocol files written by `invar init` (`CLAUDE.md`, `AGENTS.md`, `.cursor/rules/`). They state the two phases, when to stop and ask, what may be written in each phase, and which command to call when. The plugin's hooks fire the commands; the instruction files carry the reasoning the hooks cannot. |
| **Ticket** | The unit of work entering the pipeline — a JIRA (or equivalent) issue pasted by the developer. Its id (e.g. `PROJ-412`) links the Plan, the branch, and the decision entry. |
| **Plan** | The prose specification derived from a Ticket, **written by the agent** and stored at `.invar/plans/<TICKET>-<slug>/spec.md` (structure: §4a). Carries what Gherkin cannot: ticket context, scope, open questions and answers, rejected alternatives, and any scenario the cycle intends to delete (§4c). Approved by the developer at the gate, consumed by the Codegen Agent, and **deleted within the same PR** once the cycle passes (§12). |
| **Project context** — `AI_CONTEXT.md` | Optional, machine-readable file describing **this project's own** architectural conventions (layer boundaries, naming, module import rules). Its presence activates the extended lint rule set, so its content is *binding* and must be the project's own decisions — never generic framework advice (§5a). May be FastKit's published version, hand-authored, or derived from the existing code with developer confirmation. Absence is a valid, supported state. |
| **Business context** — `BUSINESS_CONTEXT.md` | Optional, machine-readable file describing business rules the *application* must obey — distinct from `AI_CONTEXT.md`, which describes how *code* must be structured. Read by **both** agents. Two categories: **Invariants** (always true, e.g. "every tenant-scoped model must be filtered by `account_id`") and **Regional/parameterized facts** (vary by a parameter such as country or plan tier). Owned and maintained by the developer or team; `invar` never generates its content (§5). Absence — and an empty-but-marked file — are valid, supported states. |
| **Framework context** | External, read-only context for the *framework* at the exact installed version (e.g. "in this version use `@field_validator`, not `@validator`"). Answers what a model trained on mixed versions gets wrong. A separate source, never merged into `AI_CONTEXT.md` (§5b). |
| **Package context** | External, read-only context for an installed third-party package, pinned to the exact version from the lockfile. Same channel and same rules as framework context (§5b, §14.1). |
| **Context marker** | A header comment (`<!-- invar-context: v1 -->`) at the top of a context file that lets the tool distinguish a recognized context file from an unrelated document with the same filename. `invar init` writes the marker into the empty files it scaffolds. |
| **Base rule set** | Lint rules that apply to every project regardless of whether `AI_CONTEXT.md` is present — general FastAPI best practices (e.g. `response_model` present, no bare `except Exception`, no direct DB session use inside a router function). |
| **Extended rule set** | Additional lint rules derived from or declared in `AI_CONTEXT.md`. Only active when a recognized project context file is present. Two categories: (1) **intra-module** — layer-boundary rules within a single module, e.g. "no SQLAlchemy import in `router.py`"; (2) **inter-module (DDD/bounded-context)** — rules governing how modules may reference each other, e.g. "`orders` may not import `payments.repository` directly." Inter-module rules are checked against the project's module import graph, not a single file. |
| **Ubiquitous language check** | A *non-mechanical* category, explicitly excluded from lint: whether a name (`reserve_stock`, `Invoice`, `settle`) matches the business domain's actual vocabulary. Lint can flag naming *inconsistency* across modules but cannot verify domain-correctness — that remains a human/Scenario Agent review concern. |
| **Decision log** | An insert-only, human-readable record of what was built and why, stored as one file per entry under `.invar/decisions/`. Read by the Scenario Agent as *advisory* (non-binding) context. Written as the last step of a cycle, after implementation. Linear and chronological in MVP; a weighted decision graph is post-MVP (§7). |
| **Config file** | Single YAML file (`invar.yaml`) at project root controlling infrastructure hints, lint rule overrides, language selection, and pipeline behavior. |
| **Language adapter** | The three-to-four language-specific operations behind an explicit interface: dependency discovery, test discovery, test execution, lint. Everything else in `invar` is language-neutral (§14.2). |

## 3. Pipeline (two-phase loop)

Both phases run inside Claude Code, driven by the plugin (§13): its hooks fire
the deterministic commands at the right events, its instruction files state the
protocol, and its planning sub-agent isolates Phase 1. The CLI renders every
verdict. `invar` still calls no LLM of its own (§11).

```
[0] Setup (once per project)
    AI_CONTEXT.md · BUSINESS_CONTEXT.md · invar.yaml · .invar/
    The quality of everything downstream depends on this step.

    Context sources, assembled at read time by `invar context show`:
      AI_CONTEXT.md        project's own conventions   binding (lint)
      BUSINESS_CONTEXT.md  business rules              binding (gate)
      framework context    version-correct usage       advisory
      package context      version-correct usage       advisory
      .invar/decisions/  past decisions              advisory

Developer
   │  pastes ticket, creates branch
   ▼
[1] Scenario Agent — runs in the planning sub-agent, own context (§13a)
   │   `invar context sync` (conditional, §5b) then `invar context show`
   │   reads: ticket + all context sources + targeted reads of the code base
   │   (module structure and public interfaces of the layers the ticket
   │    touches — never a full repo scan)
   │
   ├── clarifying questions ──► Developer
   │   (mandatory whenever the ticket touches a Regional/parameterized fact)
   │
   ▼
   drafts Plan → .invar/plans/<TICKET>-<slug>/spec.md
   │  `invar plan validate` — mechanical, agent-callable, run in-phase
   │
   ▼
   writes Scenario .feature files as `Status: draft`
   │
   ▼
[GATE] Developer approves the Plan *and* the scenarios
   │     `invar plan approve` → `invar scenario confirm`
   │     mandatory; nothing downstream runs without it
   │     (loop back to [1] if rejected)
   ▼
[2] Codegen Agent
   │  reads: approved Plan + confirmed Scenario + AI_CONTEXT.md +
   │         BUSINESS_CONTEXT.md + framework/package context + invar.yaml
   │  does NOT read: .invar/decisions/
   │  writes: implementation code + generated pytest-bdd step definitions
   │  may NOT write: any confirmed .feature file (§4)
   │
   │  If the Scenario turns out to be impossible or self-contradictory —
   │  not merely failing — the cycle returns to [1]: `invar decision
   │  record --type failed`, then a new Plan. The Codegen Agent never
   │  repairs the specification (§4c).
   │
   ▼
[3] Verification — `invar generate`
   │  a) Approval integrity (§4d)
   │     · each .feature body against its own Confirmed-checksum — always,
   │       self-contained, works with or without the Plan
   │     · the Plan body against its Approved-checksum — only while the Plan
   │       exists. A Plan absent *with* a matching decision entry is a closed
   │       cycle, not a failure; absent *without* one is a hard error.
   │  b) Lint check (structural) — base rules always; extended rules if a
   │     recognized AI_CONTEXT.md is present
   │  c) Behavioral test run — after confirming exactly one pytest-bdd test
   │     file corresponds to this Scenario (§4b), runs pytest against
   │     `--tests-path` if given, else the *entire* suite under
   │     `pipeline.tests_dir` (§4b) — the safe default a CI/CD pipeline
   │     relies on; `--tests-path` is an explicit, opt-in narrowing for
   │     fast local iteration, never the default
   │
   ▼
   Pass ──► write decision entry (§7) ──► delete the Plan, in this same PR (§12)
   Fail ──► write decision entry (type: failed), invalidate the Plan's
            approval and return this ticket's scenarios to `draft` (§4c),
            report violations, do not silently modify code further
```

Design rule: **the developer-approval gate is mandatory in MVP** and cannot be
disabled via config. This is a deliberate safety boundary, not a configurable
preference, to prevent the Codegen Agent from acting on a misunderstood
request.

The gate is enforced **mechanically, not by instruction**: `invar generate`
refuses to run against a Scenario whose `Status` is not `confirmed` or whose
checksum is stale, and `scenario confirm` requires an approved Plan. This
matters because `invar` does not control the agent (§11) — a rule that
depended on the agent obeying a prompt would not be a boundary at all.

**The gate covers the Plan and the scenarios together.** The previous design
put it on the Plan alone, so that scenarios written from an approved Plan
inherited `confirmed` without ever being read by a human. That left a real
gap: the scenarios are the actual acceptance criteria, and a bad translation
from approved prose would be verified as correct by lint and tests. The
developer now approves the Plan (`plan approve`) and then confirms the
scenarios (`scenario confirm`) in the same sitting, on one screen. This is one
review, not two — the second command exists so that the confirmation is
recorded per file, with a checksum (§4).

Rationale for the order: a misunderstanding is still cheaper to catch in prose
than in Gherkin, so the Plan is read first. `scenario confirm` refuses without
an approved Plan for the same ticket, so the order is mechanical, not advisory.

**One review means one command, not one per file.** A Plan producing six
scenarios across three modules would otherwise mean six `scenario confirm`
invocations, which is typing, not reviewing — and typing that the developer
will soon want to script, which is how a gate quietly stops being one.
`invar scenario confirm --plan <ticket>` confirms every draft scenario the
Plan lists, printing each file first and refusing if the Plan's
`## Scenarios to write` and the files on disk disagree. That last check is what
keeps the bulk form honest: it cannot confirm a scenario the approved Plan
never announced.

## 4. Scenario document structure

Scenarios are standard Gherkin `.feature` files, stored under `specs/`, with
one required metadata block at the top of each file.

```gherkin
# specs/orders/reserve-stock.feature

@invar
Feature: Reserve stock on order placement
  Module: orders
  Status: confirmed          # draft | confirmed
  Ticket: PROJ-412
  Confirmed-by: <git-username>
  Confirmed-at: 2026-08-03
  Confirmed-checksum: sha256:1f4a…    # written by `scenario confirm`

  Scenario: Stock is reserved when an order is placed
    Given a Product with sku "ABC-123" and stock 10
    When a customer places an order for 3 units of "ABC-123"
    Then the Product stock for "ABC-123" is 7
    And an OrderCreated event is emitted

  Scenario: Order is rejected when stock is insufficient
    Given a Product with sku "ABC-123" and stock 2
    When a customer places an order for 3 units of "ABC-123"
    Then the order is rejected with reason "insufficient_stock"
    And the Product stock for "ABC-123" remains 2
```

Rules for this format:

- The `@invar` tag marks a feature file as tool-managed (distinguishes it
  from hand-written Gherkin used for other purposes in the same repo).
- `Status` is `draft` or `confirmed`. **There is no `superseded`.** The
  previous design had one, but nothing set it and nothing read it: a scenario
  whose behavior no longer applies is either updated in place or deleted
  (§4c). A third state that no command writes is only a source of confusion,
  and a dead-but-present scenario still costs CI time.
- `draft` scenarios are never sent to the Codegen Agent automatically.
- **`Confirmed-checksum` is written only by `invar scenario confirm`**, over
  the file body (everything below the metadata block). `invar generate`
  refuses to run when the checksum does not match. The Codegen Agent therefore
  cannot quietly relax an acceptance criterion it cannot satisfy — the change
  is mechanically visible, not a matter of trusting the prompt.
- **The checksum is not permanent immutability.** It binds a file to *one
  approval*. Changing a scenario is legitimate; doing it inside a cycle
  without re-approval is not. A new ticket that changes desired behavior
  updates the `.feature` file and passes a new gate, which issues a new
  checksum. Same logic as the Plan body checksum (§4a).
- Each `Scenario:` block must map to exactly one generated `pytest-bdd` test
  function — no scenario may span multiple unrelated behaviors.
- `Ticket` records which cycle last confirmed this file. The permanent record
  of *why* the behavior changed lives in the decision log (§7), not here.
- Plain Gherkin only in MVP. EARS-style acceptance criteria is a possible
  post-MVP upgrade, not required now.

## 4a. Plan document structure — `.invar/plans/<TICKET>-<slug>/spec.md`

The Plan is **written by the agent**, not by `invar`. There is no `plan new`
command: the agent creates the directory and the file from the developer's
instructions, following the convention below. `invar` validates and approves;
it does not author.

Because the agent writes it, the body is deliberately loosely specified — how
a Plan reads will vary with the agent and the ticket. What is fixed is the
**header** and the **minimum required sections**, since those are what
`invar` and the Codegen Agent depend on.

```markdown
---
Ticket: PROJ-412
Module: orders
Status: draft              # draft | approved
Approved-by:               # written by `invar plan approve`, never by the agent
Approved-at:
Approved-checksum:
---

## Ticket
<pasted ticket text, verbatim>

## Intent
What the change is meant to achieve, in the developer's terms.

## Scope
Which modules and layers are touched; what is explicitly out of scope.

## Open questions / answers
The clarifying questions asked in Phase 1 and the developer's answers.

## Rejected alternatives
Approaches considered and dropped, with the reason.

## Scenarios to write
The list of .feature files this Plan will produce.

## Scenarios to delete
Existing .feature files this cycle removes, each with its test file and a
one-line reason. Empty in most cycles. See §4c.
```

Rules for this format:

- The header mirrors the `.feature` metadata block (§4) deliberately — one
  convention, two artifacts.
- `Approved-by` / `Approved-at` / `Approved-checksum` are written **only** by
  `invar plan approve`, from git config. An agent that fills them in itself
  is out of protocol; see the honesty note below.
- **Open questions / answers** and **Rejected alternatives** are the sections
  that justify the Plan's existence. Everything else could be inferred from
  the ticket and the diff; these two cannot, and they are what a reviewer and
  a second developer actually need.
- The directory name (`<TICKET>-<slug>`) is the join key across the Plan, the
  branch, and the decision entry. `invar plan validate` fails loudly if it
  does not match the expected shape.

**`plan validate` is mechanical; `plan approve` is human.** The split matters
for where the developer's attention is spent. `validate` checks the path, the
directory-name shape, the header fields and the presence of the required
sections — the same result no matter who runs it — so the Scenario Agent runs
it itself, inside Phase 1, and fixes what it finds. `approve` is the gate and
only the developer runs it. **`approve` refuses when `validate` does not
pass**, so there is no path in which a developer approves a mechanically
broken Plan, and no cycle in which a human is asked to notice a misnamed
directory.

**Honesty note on the gate.** With approval stored in the Plan header, a
misbehaving agent could write `Status: approved` itself. `invar plan approve`
therefore also records a checksum of the Plan body at approval time; if the
body changes afterwards, the approval is stale and `scenario confirm` refuses.
This does not make forgery impossible — nothing enforced inside the repo can —
but it does make the common failure mode (agent edits the Plan after approval
and proceeds) mechanically detectable rather than invisible.

## 4b. Locating a Scenario's pytest-bdd test — `.feature` → test file

`invar generate` (§8) needs to confirm a `.feature` file is actually wired to
a pytest-bdd test before it can run the suite as verification evidence. There
is no field anywhere recording that link — the binding call inside a test file
is the only place it exists — so the convention has to cover both how that call
is written and how `invar` reads it back.

**Convention: the feature path is written relative to the project root**, not
relative to the test file (pytest-bdd's own default resolution). This keeps
discovery a plain string match — `invar` never has to emulate pytest-bdd's
path resolution — at the cost of a one-time project setup:
`bdd_features_base_dir` must be set to the project root in the pytest config,
so pytest-bdd's own runtime resolution agrees with what `invar` reads
statically.

**Both binding forms are recognized:** the module-level `scenarios(...)` call,
and the per-scenario `@scenario(...)` decorator. pytest-bdd supports both and a
project may reasonably use either — a decorator is the normal choice when one
scenario needs its own fixtures.

```python
# tests/test_reserve_stock.py — module-level form
from pytest_bdd import scenarios

scenarios("specs/orders/reserve-stock.feature")
```

```python
# tests/test_reserve_stock.py — decorator form
from pytest_bdd import scenario

@scenario("specs/orders/reserve-stock.feature", "Stock is reserved when an order is placed")
def test_stock_is_reserved():
    pass
```

```toml
# pyproject.toml
[tool.pytest.ini_options]
bdd_features_base_dir = "."
```

**Search directory:** `pipeline.tests_dir` in `invar.yaml` (§6) — the
developer sets this explicitly; `invar` does not infer it. Default, if the
key is absent, is `tests/`. `invar generate` parses every `.py` file under it,
recursively, and looks for a `scenarios(...)` call or a `@scenario(...)`
decorator whose first argument is a **string literal** matching the target
`.feature` file's path, resolved relative to the project root. The match must be
inside the call or the decorator itself, not anywhere else in the file, so an
unrelated comment or docstring mentioning the same path cannot produce a false
positive.

**A non-literal path argument is unsupported and reported, never resolved.**
`scenarios(FEATURE_DIR / "reserve-stock.feature")` or a path built from a
constant cannot be matched by a static read, and `invar` will not evaluate
Python to find out what it means — that is exactly the path-resolution
emulation this convention exists to avoid. When a candidate file contains a
binding call whose argument is not a literal, `generate` says so by name
("`tests/test_x.py` binds a feature with a non-literal path; invar cannot
match it — use a string literal") instead of reporting "no test found." A wrong
diagnosis here costs more than the restriction: the developer would go looking
for a missing test that exists.

**How test files are organized under `tests_dir` is entirely the developer's
choice** — one directory per ticket, flat, mirroring `specs/`'s module
layout, or anything else. `invar` has no opinion: the search is recursive.

**Ambiguity is a hard error, same as the Plan lookup (§4a,
`find_plan_for_scenario`):** zero matching test *files*, or more than one,
fails loudly. `invar` never guesses which test file corresponds to a
Scenario. Several `@scenario(...)` decorators in **one** file, binding the
several scenarios of one feature, are the normal decorator case and are not
ambiguity — the rule is one feature to one file, not one feature to one call.

**A missing `bdd_features_base_dir` is reported as a setup failure, not as a
missing test.** Both produce "no test found for this Scenario," and they are
different problems: one means Phase 0 was never finished, the other means the
Codegen Agent did not wire the test. `invar doctor` (§8) checks the setup
preconditions explicitly, and `generate` names which of the two it hit.

**What actually runs, once the file is found, is a separate decision from
finding it.** By default `invar generate` runs the *entire* suite under
`pipeline.tests_dir` — passing behavior for this one Scenario in isolation
doesn't prove the rest of the application still works, which is the actual
point of the behavioral test run, and it's the safe choice a CI/CD pipeline
should always get without having to opt into it. `invar generate <name>
--tests-path <path>` narrows what's *executed* to `<path>` (a file or
directory, resolved relative to the project root) — an explicit, opt-in
override for fast local iteration while a developer or agent is still
working on one Scenario, never invar's own default. `--tests-path` only
changes what pytest runs; the discovery step above always happens first and
is unaffected by it.

**`--tests-path` never closes a cycle.** The in-flight hook (§13) uses it for
fast feedback, but the `generate` that writes the decision entry runs the full
suite. Otherwise the entry — which is permanent, and which a future Scenario
Agent reads as evidence — would claim more than was actually verified.

## 4c. Changing or removing a Scenario

Desired behavior changes. A later ticket contradicts an earlier one, or a
behavior is dropped entirely. Both are ordinary, and both are **Phase 1 work**.

**The Codegen Agent never edits a confirmed `.feature` file.** If it could,
the gate would be pointless: an agent whose implementation fails would be able
to relax its own success criterion. The checksum (§4) makes the attempt
visible; this rule states the intent behind it.

**Changing behavior:** a new ticket produces a new Plan, the `.feature` file is
updated in place, and the gate issues a new checksum. The file is not
versioned or duplicated — it describes the application's *current* behavior,
which is its job as a behavioral test suite. The permanent record that the
behavior used to be different is the decision entry (§7), which is exactly the
question the log exists to answer six months later.

**Removing behavior:** the scenario is **deleted**, together with its pytest-bdd
test file. There is no tombstone state. Two constraints:

- The deletion is **declared in the Plan** (`## Scenarios to delete`, §4a) and
  therefore passes the gate and appears in the PR diff and in code review. It
  is a reviewed decision, never a side effect.
- **A `.feature` file and its test file are deleted as a pair.** By §4b each
  scenario has exactly one test file, whose `scenarios(...)` call points at the
  feature path; leaving the test behind breaks discovery for the whole suite.

**`clean-spec` still never touches `specs/` (§12).** That prohibition is
unchanged and not in tension with the above: deletion here is a declared,
reviewed act inside a cycle, not something a cleanup command may do on its own
judgement.

**An impossible Scenario returns to Phase 1.** If the Codegen Agent finds a
confirmed Scenario self-contradictory or unimplementable — as opposed to merely
failing — it stops, records a decision entry of `type: failed`, and the cycle
restarts at Phase 1. It does not propose an edit and it does not proceed.
"Scenario required X; impossible because Y" is one of the more valuable entries
the log will ever hold, and the next Scenario Agent reads it as advisory
context.

**A `failed` entry resets the cycle's approval state.** Writing `type: failed`
for a ticket — whether from `generate` or manually — does three things
atomically:

1. sets the Plan's `Status` back to `draft` and clears `Approved-by`,
   `Approved-at`, `Approved-checksum`
2. sets every `.feature` file whose `Ticket` matches that ticket back to
   `Status: draft` and clears its `Confirmed-checksum`
3. leaves the file *contents* untouched — nothing is reverted, only
   de-approved

Without this, a failed cycle leaves confirmed scenarios and an approved Plan
behind, and the next `generate` would run against an approval that the last
attempt already disproved. Phase 1 then re-plans and the gate is passed again;
a new approval issues new checksums (§4d).

Scope is the ticket, not the module: a pre-existing scenario this cycle never
touched keeps its confirmation. A scenario this cycle *updated* does return to
`draft` even though its test may still pass — that is correct, since the
behavior it now describes was never approved.

## 4d. Checksums — what is hashed, and when each check applies

Two checksums, one purpose: make it mechanically visible when an artifact
changed after it was approved (§4a, honesty note). Neither prevents forgery;
both make the ordinary failure mode detectable.

**What is hashed.** Text is canonicalized before hashing, so that the same
content always produces the same digest:

1. decode as UTF-8, strip a BOM if present
2. normalize line endings: `\r\n` and `\r` → `\n`
3. strip trailing whitespace from every line
4. strip trailing blank lines, then append exactly one `\n`
5. for a `.feature`, hash only the body — everything after the metadata block;
   for a Plan, only the body — everything after the YAML header

Steps 2 and 3 are not cosmetic. Without them a mixed Windows/Unix team, or
simply `core.autocrlf=true`, produces a different digest for identical content
and every integrity check fails for a reason the developer cannot see. For the
same reason `init` writes a `.gitattributes` entry marking `*.feature` and
`.invar/plans/**/*.md` as `text eol=lf`, so the working tree and the index
agree.

Digests are recorded as `sha256:<hex>`.

**When each check applies.**

| Check | Applies |
|---|---|
| `.feature` body vs. its own `Confirmed-checksum` | Always. Self-contained — the expected digest is in the file's own header, so it holds in CI, on a fresh clone, and after the Plan is gone. |
| Plan body vs. its `Approved-checksum` | Only while the Plan exists on disk. |
| `scenario confirm` refuses on a stale Plan | At confirm time, when the Plan is necessarily present. |

**A missing Plan is not an integrity failure by itself.** The Plan is deleted
inside the PR once its cycle passes (§12), so any later run — the CI run in
particular — finds it gone. `generate` therefore resolves absence the same way
`clean-spec` does:

- **absent, and a decision entry exists for that ticket** → the cycle is
  closed. Plan checks are skipped; `.feature` checks still run. Not an error.
- **absent, and no decision entry exists** → hard error. That combination means
  a Plan was removed without its cycle ever completing, which is exactly the
  signal `clean-spec` refuses to erase (§12).

This is what makes §10's "full loop runs in CI with no agent and no plugin" a
real test rather than a contradiction: in CI the scenarios still verify
themselves, and the Plan's absence is evidence of a closed cycle rather than a
missing file.

## 5. Business context — `BUSINESS_CONTEXT.md`

```markdown
<!-- invar-context: v1 -->
# BUSINESS_CONTEXT.md

## Invariants (always true, no exceptions)

- Every tenant-scoped model MUST be filtered by `account_id`. No scenario or
  generated query may return cross-account data under any circumstance.
  → Enforced via `fastkit_core.database.scopes.AgencyScope` where applicable.

- Any state-changing action performed by an operator role MUST trigger an
  admin notification, regardless of which module the action originates from.

## Regional / parameterized facts

| Country | Payment provider | Billing cutoff (local) | Timezone |
|---|---|---|---|
| DE | Stripe | 23:59 CET | Europe/Berlin |
| RS | NLB Pay | 23:59 CET | Europe/Belgrade |
```

Rules for this file:

- **Owned by the developer or the team. `invar` never generates its
  content.** These are facts that exist nowhere except in the team's heads;
  there is no source to derive them from, and a guessed invariant is worse
  than none. `init` scaffolds an empty file with the marker and the two
  section headings, and nothing else.
- **An empty-but-marked file behaves exactly like an absent one.** No
  mandatory clarifying questions, no enforcement obligations. `init` must not
  switch on behavior the developer never asked for.
- **Invariants** are binding on both agents at all times; the Scenario Agent
  must not draft a scenario that contradicts one, and the Codegen Agent must
  implement the stated enforcement mechanism when relevant.
- **Regional/parameterized facts** are primarily consumed by the Scenario
  Agent. Whenever a scenario touches something covered by this table
  (e.g. billing, payment), asking "which country/tenant does this apply to?"
  is a **mandatory** clarifying question, not optional.
- A domain *fact* (Germany uses Stripe) belongs here. The *technical
  convention* derived from it (all timestamps stored in UTC, converted to
  local time only at display time) belongs in `AI_CONTEXT.md` instead — this
  file states business reality, not implementation approach.

**Post-MVP distribution.** Business rules are organization-wide, not
project-wide: the same invariant holds across every service a team runs, and
copying the file between repositories is the real pain. The intended answer is
a **resolvable reference** — a git URL with a pinned ref in `invar.yaml`,
fetched and cached by `invar context sync`, with the exact commit recorded.
That gives centralization, versioning, offline work and an audit trail while
hosting nothing. The team remains the source of truth; any distribution
mechanism, including a future hosted service, only has to prove that what
arrived is what the team published, at which revision. This file is the most
sensitive artifact in the system, and a design that requires sending it to a
third party will be refused by exactly the organizations that need it most.

## 5a. Project context — `AI_CONTEXT.md`

This file holds **this project's own conventions**: coding style, layer
boundaries inside a module, naming, and the inter-module import rules that
define the project's bounded contexts.

**It must not hold generic framework best practices**, for two reasons.

First, it activates the extended lint rule set (§2, §6) — rules that **fail the
build**. Content in this file is a statement that the project has *decided*
something. Generic advice written here silently switches on rules the team
never agreed to, and the first thing the developer sees is a red CI on rules
that are not theirs.

Second, it would not help. General framework guidance is abundant in model
training data; repeating it changes no behavior while making the setup look
finished, so the content that *would* have mattered never gets written. The
rules that matter here — "no SQLAlchemy import in `router.py`", "`orders` may
not import `payments.repository`" — cannot be derived from a framework
version. They are decisions.

**How it gets populated:**

- **Empty project (no code yet):** `init` writes the marker and the section
  headings, nothing more. There is nothing to derive.
- **Existing project:** conventions may be **proposed from the existing code,
  with evidence, and confirmed by the developer** before they are written.
  "No `modules/*/router.py` imports SQLAlchemy — make this a rule?" is a
  detected convention with a citation. It becomes binding only on a yes. A
  proposal is never written to the file unconfirmed.

**Test-style conventions belong here too**, and this is the one section `init`
should prompt for rather than leave blank. The Codegen Agent writes step
definitions (§3, Phase 2), and whether they are synchronous (`TestClient`) or
asynchronous (`httpx.AsyncClient` with `pytest-asyncio` or `anyio`), which
fixtures provide the session and the client, and how the database is reset
between scenarios are all **project decisions with no default**. Both styles
are viable in FastAPI, they cannot be mixed within a suite, and an agent that
guesses wrong produces step definitions that fail for reasons unrelated to the
behavior under test — the most confusing failure in the whole loop, because the
scenario is correct and the code is correct.

Two consequences:

- In an existing project this is derivable with high confidence from the
  existing tests, and is a good candidate for the proposal-plus-confirmation
  path above.
- In an empty project there is nothing to derive, so the first cycle will
  guess. `invar doctor` (§8) therefore warns when `AI_CONTEXT.md` states no
  test-style convention and `tests_dir` contains no tests to infer one from.

Version-correct framework usage belongs in framework context (§5b), which is
advisory and version-pinned, not in this file.

## 5b. External context sources — framework and package context

Two failure modes have nothing to do with the project's own conventions:

- **Version drift.** The model writes the API of the version it saw most
  during training. Whether that matches the installed version is luck.
- **Unknown packages.** A useful package with few stars is barely present in
  training data, so it is either ignored or hallucinated.

Both are answered by context that is **external, read-only, and pinned to the
exact installed version**: the framework at its version, and each installed
package at its version.

**These are separate sources. They are never merged into `AI_CONTEXT.md`.**

- Standing differs. `AI_CONTEXT.md` is binding and drives lint; external
  context is advisory and is caught downstream by lint, tests, or the gate.
  Merged, every source inherits the same enforcement — including sources for
  which lint is the wrong judge.
- Lifecycle differs. External context changes on every dependency upgrade;
  `AI_CONTEXT.md` is the developer's file and the tool does not rewrite it.
  Merged, an upgrade cannot tell the developer's edits from the tool's.
- Provenance is lost. The developer must be able to see which rule is theirs
  and which came from a package author.

`invar context show` assembles all sources at read time, each labelled with
its origin and version. That is where they meet — not on disk.

### `invar context sync`

Fetches and caches external context for the framework and the installed
dependencies.

- **Reads the lockfile, not the manifest.** A manifest says `>=0.4`; a lockfile
  says `0.4.7`. Resolution starts from what is actually installed. With no
  lockfile, read the installed environment; never resolve a range.
- **Version resolution is a documented order, not an exact string match.**
  Requiring an exact patch match would report "no context" for almost every
  package, since a registry entry published against `0.4.0` is nearly always
  correct for `0.4.7`. The order is:
  1. exact version match → use it, marked `exact`
  2. highest published version with the same major and minor, at or below the
     installed patch → use it, marked `compatible`, carrying the version it was
     published against
  3. highest published version with the same major → use it, marked `stale`
  4. nothing with the same major → **no context**. A major-version mismatch is
     the case most likely to be actively wrong, and silence is better than
     confident error.

  Pre-1.0 packages are treated as if the minor were the major, following the
  usual convention that `0.x` minors may break: for an installed `0.4.7`,
  step 3 considers only `0.4.*`, never `0.5.*`.
- **The resolution result is always visible.** `context show` labels every
  external document with its origin, the version it was published against, the
  installed version, and which of `exact` / `compatible` / `stale` produced it.
  This is the difference between a useful fallback and the silent staleness in
  §14.1 that destroys trust: the agent and the developer can both see that the
  guidance was written for an older release.
- **Conditional.** The cache records a hash of the lockfile; if it is
  unchanged, sync does nothing. This is what makes it safe to run before every
  planning phase: it is almost always a no-op, and the developer never waits.
- **Non-blocking, in every failure mode.** No network, no entry for a package,
  service unreachable — all are ordinary states, not errors. Most packages will
  never have context. The cache is the read path; the network only refreshes
  it. A sync that can block planning is a sync that gets turned off.
- **Language-aware.** Which lockfile to read is a language adapter concern
  (§14.2): `uv.lock` / `poetry.lock` / installed distributions for Python,
  `composer.lock` for PHP, `package-lock.json` for Node.
- **Cached per installed version, not per published version.** The key is
  `(ecosystem, package, installed version)` and the entry stores which
  published version answered it and under which match class. A cache keyed by
  package name alone will serve `0.4` context for an installed `0.7`; a cache
  keyed by published version alone cannot tell a resolved `compatible` hit from
  a fresh lookup after an upgrade.

This command is also the **only** package-detection path, including during
`init` (§8). An empty project acquires dependencies minutes after setup, so
detection cannot be an event that happens once — `init` simply performs the
first sync.

The registry this command talks to is specified in §14.1.

## 6. Config file — `invar.yaml`

```yaml
# invar.yaml — single config file at project root

language: python               # python | php | node — selects the language
                               # adapter (§14.2). Absent means python.

framework: fastapi             # used to request framework context (§5b)

infrastructure:
  database: postgresql
  cache: redis
  broker: none                 # none | rabbitmq | redis-streams

pipeline:
  require_confirmation: true   # fixed true in MVP, present for forward-compat only
  test_framework: pytest-bdd
  tests_dir: tests             # where `invar generate` looks for pytest-bdd
                               # tests (§4b), and what it runs by default;
                               # developer-set, defaults to "tests" if absent

context:
  sync_on_plan: true           # run `context sync` before planning (§13)
  # business_context_ref:      # post-MVP (§5): git URL + pinned ref
  #   url: git@github.com:acme/contexts.git
  #   ref: 9f2c1ab

lint:
  enabled: true                # global on/off — on by default
  rules:
    # base rules ship enabled by default; extended rules appear here
    # automatically once a recognized AI_CONTEXT.md is detected
    no-bare-except:
      enabled: true
      severity: error
    response-model-required:
      enabled: true
      severity: warning

    # extended — intra-module (layer boundaries within one module)
    no-sqlalchemy-in-router:
      enabled: true
      severity: error

    # extended — inter-module (DDD / bounded-context boundaries)
    no-cross-module-repository-import:
      enabled: true
      severity: error
      # e.g. blocks `from modules.payments.repository import ...`
      # anywhere outside modules/payments/**
      ignore_type_checking: true   # see below — default true

exclude:
  - "specs/**/drafts/**"
```

**Import-graph rules ignore `if TYPE_CHECKING:` imports by default.** In Python
DDD this is the standard way to get a type name across a module boundary
without creating a runtime cycle:

```python
if TYPE_CHECKING:
    from modules.payments.repository import PaymentRepository
```

That import does not exist at runtime and creates no runtime coupling, so
flagging it makes the rule fire on the very pattern the architecture uses to
stay clean — the fastest route to the whole extended rule set being switched
off. The import graph a rule is checked against is therefore the *runtime*
graph.

`ignore_type_checking: false` is available per rule for teams who consider even
a type-level reference across a bounded context a violation. That is a
defensible position — it is a real dependency in the design, just not at
runtime — which is why it is a switch rather than a decision made for them.

Note on what the config holds: **switches, not rule logic.** A rule's
implementation lives in the tool; the config says whether it applies and at
what severity. Extended rules appear here *because* a recognized
`AI_CONTEXT.md` was detected — which is why the content of that file decides
which rules exist at all, and why §5a is strict about what may go in it.

Inline suppression (same convention across the whole FastKit/invar toolchain):

```python
except Exception:  # invar-lint: ignore[no-bare-except]
```

## 7. Decision log — `.invar/decisions/`

A chronological record of what was actually built and why. Read by the
**Scenario Agent** as additional, *non-binding* context. The Codegen Agent does
not read it in MVP: it receives a confirmed Scenario that is already the
distillate of everything in the log, and giving it the log as well risks it
"improving" the Scenario instead of implementing it. Because Phase 1 runs in
the planning sub-agent (§13a), the log is never handed to Phase 2 — a boundary
of default access rather than a hard one, which is adequate here only because
the log is advisory (§13a).

Purpose: capture the changes that are not critical to business logic or to the
architectural context, but that improve the quality of planning and the
agent's understanding of the application over time.

**Standing relative to the other context sources:**

| Source | Strength | Consequence of violation |
|---|---|---|
| `BUSINESS_CONTEXT.md` — Invariants | Binding | Agent must not draft a scenario that contradicts it; gate rejects |
| `AI_CONTEXT.md` | Binding | Lint fails |
| Framework / package context (§5b) | Advisory | None directly — caught by lint, tests, or the gate |
| `.invar/decisions/` | Advisory | None — it only informs questions and assumptions |

Because it is advisory, no agent may fail or block on the content of the
decision log. The Scenario Agent may depart from a past entry when the
developer asks for it; it should merely note the departure ("previously
decided X, this goes against it — intentional?"). This is also what makes the
log safe to write automatically.

### Structure

One file per entry, modelled on database migration files: separate entries are
separate files, so two branches merge without conflict. A single file with
prepended entries would conflict on every parallel write, since both sides
would insert at the same position with identical surrounding context.

```
.invar/decisions/
  2026-08-14-093012-orders-reserve-stock.md
  2026-08-14-141500-payments-refund-flow.md
```

Filename: `<ISO-date>-<HHMMSS>-<module>-<slug>.md`. The timestamp establishes
ordering; `module` is `global` when an entry is not tied to one module.

**Insert-only:** existing files are never modified or deleted programmatically.
The developer may edit them by hand; the tool neither detects nor cares.

### Entry format

```markdown
---
id: d-4f2a
date: 2026-08-14T09:30:12
ticket: PROJ-412
module: orders
scenario: reserve-stock
type: correction
---

Reservation happens on `placed`, not on `paid`. Originally generated against
`paid`; corrected because stock must be locked before payment.
```

`id` is stable and never changes — it is the reference target for the post-MVP
decision graph. `ticket` links the entry back to the Plan it came from. The
body is free text.

The decision entry is what **survives** the Plan: the Plan is deleted at the
end of the cycle (§12), the entry is permanent. Six months later, "why is
reservation on `placed`?" — or "why did this scenario change, and what did it
say before?" — is answerable from `.invar/decisions/` alone. This is why
`.feature` files do not need history of their own (§4c).

**Entry types:**

- `confirmed` — cycle completed successfully. **This is what `invar
  generate` always writes on a clean pass** — a single invocation has no
  way to know whether the result was corrected along the way versus a
  first-try clean pass, so it never distinguishes.
- `correction` — developer rejected a draft or corrected generated code.
  **Reserved for the manual `invar decision record` path** — a
  developer noting, after the fact, that they fixed something by hand
  outside the pipeline. `invar generate` never writes this type itself.
- `failed` — generation failed, or a confirmed Scenario turned out to be
  impossible (§4c). Written deliberately: a failed attempt is often more
  useful context than a successful one, and without it the agent repeats the
  same mistake months later.
- `note` — manual entry by the developer

### Who writes

Writing is the **last step of a cycle, after implementation** — the entry then
describes what actually came out, including corrections made during
generation, rather than what was intended. One entry per cycle:
`invar scenario confirm` writes nothing.

- `invar generate` calls `decision record` automatically at the end, on
  success (`confirmed`) and on failure (`failed`)
- `invar decision record` manually, when the developer changes something
  outside the pipeline, or when a cycle returns to Phase 1 (§4c)

### How it is read

At the start of Phase 1, `invar context show` loads files in reverse
filename order (newest first): first all entries for the requested module, then
`global`, up to a budget of roughly 50 entries or ~4000 tokens. Older entries
drop off silently.

Conflicts between two entries are **not detected** (§9). The newer entry wins
simply because it is read first.

### Boundary against the other context files

If the same rule shows up in the log three or more times, it does not belong
there — it belongs in `BUSINESS_CONTEXT.md` (a business fact) or
`AI_CONTEXT.md` (a code convention). MVP does not automate this check.

## 8. CLI surface (MVP)

The CLI holds all the logic. The plugin (§13) and the MCP server (§14.1) are
callers of these commands, never replacements for them. That is what keeps the
pipeline runnable in CI where no agent exists, and what makes a second runtime
— or a separate plugin per language (§14.2) — a packaging job rather than a
rewrite.

```bash
invar init                      # scaffolds specs/, .invar/, invar.yaml,
                                  # instruction files, hook config; writes
                                  # AI_CONTEXT.md and BUSINESS_CONTEXT.md as
                                  # EMPTY marked files (never their content);
                                  # writes .gitattributes entries pinning
                                  # .feature and plan files to LF (§4d) and
                                  # adds .invar/decisions|plans to the
                                  # agent ignore config (§13a); then performs
                                  # the first `context sync`
invar doctor                    # checks setup preconditions: marker files
                                  # recognized, bdd_features_base_dir set,
                                  # tests_dir exists, language adapter
                                  # available. Distinguishes "setup
                                  # incomplete" from "nothing found" (§4b)
invar context sync              # fetches/caches framework + package context
                                  # for installed versions; conditional on the
                                  # lockfile hash; non-blocking (§5b)
invar context show [--module m] # prints the assembled context — all sources,
                                  # each labelled with origin and version,
                                  # plus decision entries within budget
invar scenario validate <n>     # checks a .feature file: @invar tag,
                                  # metadata block, one behavior per Scenario
invar scenario confirm <n>      # marks Status: confirmed and writes
                                  # Confirmed-checksum; refuses without an
                                  # approved Plan for the same ticket
invar scenario confirm --plan <ticket>  # bulk form: confirms every draft
                                  # scenario listed in that Plan's
                                  # `## Scenarios to write`, in one step.
                                  # Prints each file it is about to confirm
                                  # and refuses on any mismatch between the
                                  # Plan's list and what exists on disk.
invar generate <n> [--tests-path p]  # verifies code the agent just wrote:
                                  # approval integrity + lint + behavioral
                                  # tests, then writes a decision entry.
                                  # --tests-path narrows what pytest runs
                                  # (§4b); omitted, the entire tests_dir runs.
                                  # A cycle is only closed by a full run.
invar lint check [--ci]         # verification only, independent of generation
invar decision record           # writes a decision entry; called
                                  # automatically by `generate`, available
                                  # manually for out-of-pipeline changes
invar decision log              # concatenates .invar/decisions/
                                  # newest-first (rendered view)
invar plan validate <ticket>    # mechanical: path, directory-name shape,
                                  # complete header, required sections.
                                  # Agent-callable, run inside Phase 1.
invar plan approve <ticket>     # the human gate: records Approved-by /
                                  # Approved-at + body checksum. Refuses if
                                  # `plan validate` does not pass.
invar clean-spec [ticket] [--dry-run]  # deletes plans under .invar/plans/;
                    [--force] [--merged] # never touches specs/ or
                                         # .invar/decisions/. [ticket] scopes
                                         # deletion to one Plan; --force is
                                         # required to delete anything (§12).
```

## 9. Explicitly out of scope for MVP

- Chat UI and diagram-based scenario authoring (CLI + `.feature` files only)
- `invar` calling an LLM itself — the developer's own agent is the runtime (§11)
- Automatic ticket fetching from JIRA or similar — the developer pastes the
  ticket text; no issue-tracker integration in MVP
- Post-merge cleanup automation — Plans are deleted inside the PR (§12)
- Recursive / vector-based project memory (linear, chronological decision log only)
- Weighted decision graph — a `.invar/decisions.graph.yml` holding edges and
  weights referencing entry `id`s, layered above the markdown files without
  altering them. MVP is a linear chronological log; the stable `id` is
  introduced now precisely so the graph can be added later.
- **Community-contributed use-case entries in the context registry** (§14.1) —
  only author-published context in the first phase. Community contributions
  reintroduce the ranking-and-trust problem the registry exists to solve, and
  need an explicit standing hierarchy (author = binding, community = advisory)
  plus version-range filtering before they are safe to consume.
- **Hosted business-context service** — MVP keeps `BUSINESS_CONTEXT.md` local
  and team-owned; the next step is a pinned git reference (§5)
- Non-FastAPI framework adapters — the interfaces are defined now (§14.2), the
  adapters are not written
- Profile selection (`fastkit` / `vanilla` / `custom`) — replaced by simple
  context-file presence detection
- EARS notation — Gherkin only
- Automatic conflict detection between decision log entries
- Ubiquitous-language / domain-naming correctness checks (naming *consistency*
  across modules is in scope; naming *correctness* against the business domain
  is a human review concern)
- Aggregate-root write-path enforcement — a valid DDD rule category, deferred
  due to higher false-positive risk than import-graph checks
- Guided/interactive authoring of `BUSINESS_CONTEXT.md`
- Automatic enforcement-mechanism mapping for every Invariant — MVP relies on
  the Invariant text itself pointing to the mechanism, as in §5
- Per-module/per-service test isolation — `pipeline.tests_dir` (and
  `--tests-path`) address one directory tree in one deployable. A modular
  monolith is already supported; true microservices need per-module
  configuration this MVP doesn't have.
- Sandboxed test execution — tests are behavioral and need a real database and
  cache (§6), so an isolation boundary strong enough to be a security boundary
  is not connected enough for them to pass. Reproducibility (clean environment,
  locked dependencies) is the real need and is addressed without a sandbox.

## 10. Acceptance criteria for MVP completion

- [ ] `invar init` scaffolds a working project structure, writes
      `AI_CONTEXT.md` and `BUSINESS_CONTEXT.md` as empty marked files, and
      performs an initial `context sync`
- [ ] An empty-but-marked `BUSINESS_CONTEXT.md` behaves identically to an
      absent one — no mandatory clarifying questions, no enforcement
- [ ] `invar doctor` distinguishes an incomplete Phase 0 from a genuine
      "nothing found" (e.g. missing `bdd_features_base_dir`)
- [ ] `invar context sync` is a no-op when the lockfile hash is unchanged,
      never blocks on network or missing entries, and caches per installed
      version
- [ ] `invar context show` labels every source with its origin and version,
      and never merges external context into `AI_CONTEXT.md`
- [ ] Scenario Agent produces a valid `.feature` file from a natural-language
      conversation, including at least one clarifying question when the
      request is ambiguous
- [ ] `draft` scenarios are never auto-generated into code
- [ ] The developer approves the Plan **and** confirms the scenarios before
      Phase 2; `scenario confirm` refuses without an approved Plan
- [ ] `invar plan approve` refuses when `invar plan validate` fails
- [ ] `invar plan approve` writes `Approved-by` / `Approved-at` plus a body
      checksum; `scenario confirm` refuses when the Plan body changed after
      approval
- [ ] `invar scenario confirm` writes `Confirmed-checksum`, and
      `invar generate` refuses when a confirmed `.feature` file has changed
      since confirmation
- [ ] `invar scenario confirm --plan <ticket>` confirms every scenario the
      approved Plan lists in one step, and refuses when the Plan's list and the
      files on disk disagree
- [ ] Checksums survive a mixed CRLF/LF team: the same content produces the
      same digest after canonicalization (§4d), and `init` writes the
      `.gitattributes` entries
- [ ] `invar generate` passes in CI **after** the Plan has been deleted: a
      missing Plan with a matching decision entry is a closed cycle, a missing
      Plan without one is a hard error (§4d)
- [ ] A `failed` decision entry returns the Plan to `draft` and every
      `.feature` of that ticket to `draft`, clearing both checksums without
      altering file contents (§4c)
- [ ] `Status` accepts only `draft` and `confirmed`
- [ ] Test discovery recognizes both `scenarios(...)` and `@scenario(...)`,
      and reports a non-literal path argument as such rather than as "no test
      found" (§4b)
- [ ] Import-graph rules ignore imports inside `if TYPE_CHECKING:` by default,
      with a per-rule switch to include them (§6)
- [ ] `context sync` resolves a published version for an installed version by
      the documented order, labels the result `exact` / `compatible` / `stale`,
      and returns nothing across a major-version boundary (§5b)
- [ ] `context show` displays, for every external document, the published
      version, the installed version, and the match class
- [ ] A cycle that deletes a scenario declares it in the Plan and removes the
      `.feature` file together with its pytest-bdd test file
- [ ] `clean-spec` never touches `specs/` or `.invar/decisions/`, defaults to
      `--dry-run`, and refuses to delete a Plan with no corresponding decision
      entry
- [ ] Codegen Agent produces implementation + matching `pytest-bdd` step
      definitions from a `confirmed` scenario, and never edits a confirmed
      `.feature` file
- [ ] A confirmed Scenario found to be impossible produces a `failed` decision
      entry and returns the cycle to Phase 1 — the Codegen Agent does not
      propose a specification edit
- [ ] Base lint rule set runs on a project with no `AI_CONTEXT.md` present
- [ ] Extended lint rule set activates automatically when a valid, marked
      `AI_CONTEXT.md` is present, and includes at least one working
      inter-module (DDD/bounded-context) rule checked against the project's
      module import graph
- [ ] A convention detected from existing code is proposed with evidence and
      written to `AI_CONTEXT.md` only after developer confirmation
- [ ] Generated step definitions follow the project's declared test-style
      convention (sync `TestClient` or async client, fixtures, DB reset), and
      `invar doctor` warns when no convention is declared and none can be
      inferred (§5a)
- [ ] Full loop (`invar generate`) fails loudly (non-zero exit, clear report)
      on any approval-integrity, lint, or behavioral test failure — never
      silently "fixes" and re-submits
- [ ] `invar generate` runs the **full** suite under `pipeline.tests_dir`
      when closing a cycle; `--tests-path` never produces a decision entry
      that claims more than was run
- [ ] `invar generate` writes a decision entry as its last step, including on
      failure, as a new file — never by modifying an existing entry
- [ ] Scenario Agent loads decision entries newest-first (module entries before
      `global`) and treats them as advisory
- [ ] When a valid `BUSINESS_CONTEXT.md` is present, the Scenario Agent asks a
      mandatory clarifying question whenever a scenario touches a
      Regional/parameterized fact, and the Codegen Agent implements the stated
      enforcement mechanism for at least one Invariant
- [ ] `invar init` writes instruction files (`CLAUDE.md`, `AGENTS.md`,
      `.cursor/rules/`) describing the two-phase protocol
- [ ] Installing the plugin is the only setup step: it brings the CLI, the
      hooks, the instruction files and the planning sub-agent, and the full
      loop is completable without the developer wiring anything by hand
- [ ] Phase 1 runs in the planning sub-agent, and the main session never
      receives the decision log
- [ ] Hooks run `context sync`, lint, and current-scenario tests
      non-blockingly, and never run `plan approve` or `scenario confirm` (§13)
- [ ] The full loop runs in CI with no agent and no plugin present — the
      standing proof that no logic leaked out of the CLI (§13b). This replaces
      the previous "completable from two different runtimes" criterion, which
      contradicted shipping on one runtime.

## 11. Runtime — the developer's agent, and no LLM of our own

> **`invar` ships no LLM calls. The developer's agent does the thinking; the
> CLI does the judging.**

This is unchanged from the previous design and is not a consequence of picking
one runtime. Calling a model API ourselves would make the developer pay for
tokens twice, duplicate the agent they already have open, and put a second,
weaker coding agent inside a tool whose entire value is deterministic
verification.

**What did change is the target.** The previous design tried to support every
agent runtime at once and paid for it: the protocol could only be *stated* in
instruction files and hoped for, because nothing else is portable across
runtimes. §11 then had to admit that an agent ignoring its instruction file
cannot be stopped.

MVP targets **Claude Code only**, and spends the difference on mechanism:
hooks fire the commands instead of the agent remembering to (§13), and a
planning sub-agent enforces the Phase 1 / Phase 2 context split
architecturally instead of by instruction (§13a). One runtime where the
protocol actually holds is worth more than three where it is a suggestion.

"Scenario Agent" and "Codegen Agent" are therefore **two processes with
separate contexts**, not two roles of one session.

What the plugin supplies:

1. **Deterministic commands** — `init`, `doctor`, `context sync/show`,
   `lint check`, `generate`, `plan validate/approve`, `scenario
   validate/confirm`, `decision record`, `clean-spec`
2. **Context assembly** — `invar context show` prints all context sources
   plus in-budget decision entries in an agent-consumable form
3. **Hooks** — the commands fire on the right events rather than on the agent's
   memory (§13)
4. **Planning sub-agent** — isolates Phase 1's context (§13a)
5. **Instruction files** — written by `init`, carrying what hooks cannot: when
   to stop and ask, what may be written in each phase, why the gate exists

**The cost, still stated plainly:** the agent is not controlled, only guided.
Hooks improve *when* things run and the sub-agent improves *what a phase can
see*, but neither is an enforcement layer. Every boundary that matters is
enforced by state on disk (§3, §4) — approved Plan, `Status: confirmed`,
checksums — and **no boundary may be moved out of the on-disk checks into a
hook or a sub-agent boundary.** That rule is what keeps the CI run in §10 a
real proof rather than a formality.

## 12. Plan lifecycle — `.invar/plans/`

A Plan is a **consumable instruction, not documentation**. It exists to carry
intent from ticket to code, and is deleted once the code exists.

```
specs/orders/reserve-stock.feature          # permanent — lives as long as the code
.invar/plans/PROJ-412-reserve-stock/      # consumable — deleted by clean-spec
    spec.md
```

The two are separated **by tree, not by file**, so `clean-spec` can delete a
whole directory without inspecting its contents. Scenario files are the
behavioral test suite — alongside unit and integration tests, they are the
guarantee that the application does what it should — and must never be
removable by a cleanup command. (Deleting a scenario as a declared, reviewed
part of a cycle is a different act; see §4c.)

### Why it lives in git and not in tmp

- A terminal has no session across branch switches. A developer moving between
  branches during a day would otherwise lose all record of how the work was
  reasoned about; git restores it with the branch.
- A second developer can pick up the work without regenerating the Plan.
- **Code review:** the Plan is in the PR diff, so a reviewer — and CodeRabbit —
  reads the *intent* instead of inferring it from the diff. This reduces review
  comments born of wrong assumptions, and makes a flawed specification
  reviewable in plain language, before the flaw is only visible in code.

### Deletion happens inside the PR

When `invar generate` completes a cycle successfully, the Plan is deleted as
part of the same branch. The merge therefore brings an already-clean state to
`main`.

This is deliberately **not** a post-merge GitHub Action. A bot committing to
`main` after every PR would require `contents: write`, a branch-protection
exemption, `[skip ci]` handling, and would hand every developer a foreign
commit on each `pull`. Deleting inside the PR needs none of that, and
CodeRabbit has already seen the file — it reads the diff, and the deletion is
simply the last commit on that branch.

### `clean-spec` guarantees

The command exists for Plans left behind by abandoned or failed cycles.

- Never touches `specs/` or `.invar/decisions/` — only `.invar/plans/`
- Defaults to `--dry-run`. **`--force` is always required to actually delete
  anything** — naming a `[ticket]` never bypasses it.
- An optional `[ticket]` positional argument scopes deletion to that one Plan
- `--merged` limits deletion to Plans whose ticket maps to a merged branch —
  a second, independent scope filter, composable with `[ticket]`
- **Refuses to delete a Plan with no corresponding decision entry** — a missing
  entry means the cycle never finished properly. That is a signal, not garbage.

### `--merged` branch detection

**Convention: search `git branch --merged <default-branch> --all` for a
branch name containing the ticket id as a substring.** The same approach
already used by the Plan lookup (§4a) and the pytest-bdd test lookup (§4b):
plain substring matching, nothing smarter.

- **Zero matching merged branches** means "not merged" — the Plan simply isn't
  eligible for deletion under `--merged` yet. Not an error.
- **Ambiguity is a hard error, same as §4a/§4b:** more than one matching branch
  fails loudly. `invar` never guesses.

**`<default-branch>` detection:** `git symbolic-ref refs/remotes/origin/HEAD`.
If that ref doesn't exist, the fallback is **`main`**.

## 13. The plugin — delivery form of the system

**What the developer installs is a Claude Code plugin.** It bundles four
things, and none of them is optional in MVP:

| Component | Role |
|---|---|
| `invar` CLI | All the logic. The only component that renders a verdict. |
| Hooks (§13, below) | Fire the commands at the right events |
| Planning sub-agent (§13a) | Isolates Phase 1's context |
| Instruction files | Carry the protocol's reasoning, written by `init` |

One install, no manual wiring, no relaying of command output by hand. That
friction — `plan approve`, then `scenario confirm`, then `generate`, then
pasting each result back to the agent — is what kills adoption of a tool like
this faster than any missing feature, and removing it is the main reason the
plugin exists.

Why one runtime is acceptable is in §11; why the logic nevertheless stays in
the CLI is in §13b.

### Hooks

A hook is an **event, not a judge**. It decides *when* a deterministic command
runs, never *whether* its verdict counts. Every boundary stays where §3 and §4
put it: on disk.

| Event | Command | Blocking? |
|---|---|---|
| Before planning (Phase 1) | `invar context sync` | No |
| After a code edit (Phase 2) | `invar lint check` | No |
| During Phase 2 iteration | `invar generate <n> --tests-path <current>` | No |
| End of Phase 2 | `invar generate <n>` (full suite) | Yes — fails loudly, writes the decision entry |
| — | `plan approve`, `scenario confirm` | **Never hooked** |

**Nothing in-flight blocks.** A sync with no network, a package with no context
entry, a lint failure mid-edit — all ordinary. The hook reports to the agent
and the developer and the work continues. A hook that halts an editing session
gets switched off within a week, and then none of it runs. The real gate is the
`generate` at the end, which re-runs lint anyway, so nothing is lost by letting
an in-flight warning be ignored.

**The in-flight test run is feedback; the closing run is evidence.** Hooked
iteration uses `--tests-path` for the current scenario, which is fast and
useful. The `generate` that closes the cycle runs the full suite, because it
writes a permanent decision entry, and that entry must not claim more than was
verified (§4b). Passing one scenario in isolation does not show the rest of
the application still works — which is the whole point of the behavioral test
run.

**Human decisions are never hooked.** A hook that confirms a scenario has
abolished the gate. `plan approve` and `scenario confirm` are commands the
developer runs deliberately; that deliberateness is their entire function.

**The largest gain is the return path, not the enforcement.** Today the
developer copies command output back to the agent by hand. A hook puts lint and
test results into the agent's context immediately, in the same turn — a
shorter loop and one less place for an error to get lost in transit.

### 13a. The planning sub-agent

**Phase 1 runs in its own sub-agent**, with its own context. It reads every
context source and the decision log, and returns **only the Plan and the
scenarios** to the main session. This is part of the protocol, not a
nice-to-have available on some runtimes — it is one of the two things the
single-runtime choice buys (§11).

Two reasons, in order of importance:

1. **§7's rule gets a mechanism.** The Codegen Agent must not see the decision
   log, or it starts "improving" the Scenario instead of implementing it. A
   separate context means the log is never *handed* to Phase 2, rather than
   Phase 2 being asked not to look.
2. **Context cost.** Everything Phase 1 reads would otherwise sit in the main
   session for the rest of the cycle.

**This is a boundary of default access, not a hard one — and the previous
wording overstated it.** The Phase 2 session has the repository on disk and its
own search tools; nothing stops a grep from reaching `.invar/decisions/`. Two
things keep that acceptable:

- `invar init` adds `.invar/decisions/` and `.invar/plans/` to the
  agent's ignore configuration (`.claudeignore` or the runtime's equivalent),
  so the main session's file search does not surface them. That is a real
  reduction in accidental reads, which is the actual failure mode — an agent
  grepping for a symbol and pulling in a decision entry, not an agent deciding
  to go looking.
- The log is **advisory** (§7). A leak degrades Phase 2's focus; it cannot
  break a gate or falsify a verdict, because nothing in the log is binding.

The distinction matters beyond this paragraph: §11's rule is that no boundary
that *matters* may live anywhere but on disk. This one does not qualify, which
is precisely why an ignore file is an adequate answer here and would not be for
the approval gate.

This works only if the Plan is genuinely self-sufficient (§4a). If Phase 2 has
to re-read the context sources, the isolation is nominal and the budget is
spent twice. That is a test of the Plan format, not a new design.

### 13b. The plugin is a shell; the CLI is the system

Shipping on one runtime is a choice about **who we support**. It is not a
licence to let the runtime shape the architecture. One rule enforces the
difference:

> **No logic in the plugin.** It fires CLI commands and passes output back.
> Nothing decides anything there.

Three reasons this matters even though Claude Code is the only target:

- **CI has no plugin.** The pipeline has to run in CI, where there is no agent
  at all. Behavior that lives in the plugin is behavior CI does not have — and
  the CI run is therefore the standing test that nothing leaked (§10).
- **Plugin APIs move.** Against a thin shell that is a few days of repair.
  Against a thick one it is a blockage on the whole protocol.
- **The next target is cheap only if this holds.** A second runtime, or a
  **separate plugin per language** (§14.2) — which is the likely shape once
  Laravel or Node arrives, since each ecosystem has its own agent conventions —
  is then a packaging job over the same CLI and the same on-disk formats
  (§14.3), not a second implementation.

So: one supported runtime, deliberately. Portability is retained as a property
of the CLI, not advertised as a feature, and not tested against other runtimes
in MVP.

## 14. Design constraints

These two are not features. They are constraints on how MVP is built, because
retrofitting either is expensive and both are certain.

### 14.1 External context API + MCP front end — must be designed for now

Package and framework context (§5b) has to come from somewhere. The intended
shape:

- **An external HTTP API** serving context documents keyed by
  `(ecosystem, package, version)`, returning the document plus its provenance:
  who published it, at which package version, when, and a signature that can be
  verified locally. A request carries the **installed** version; the API
  answers with every published version for that package, or resolves per §5b
  and states which published version it returned and under which match class.
  The resolution rule lives in one place — duplicating it in the client and the
  service guarantees they will drift.
- **An MCP server in front of it** so the agent fetches and reads context
  itself instead of the developer relaying it. The MCP server is a client of
  the CLI and the API — it holds no logic of its own, for the same reason the
  plugin doesn't (§13b).

Design requirements that must hold from the first line of code:

- **The cache is the read path.** The API only refreshes it. Offline is a
  normal state, not an error (§5b).
- **Provenance is per document and per version**, and shown wherever context is
  displayed. Without it, a package author's rules and a stranger's guess look
  identical.
- **Staleness is a first-class state, not a fallback that hides itself.**
  Serving a `0.4` document for an installed `0.7` is often the right thing to
  do — refusing would mean serving nothing almost always (§5b) — but it must
  arrive labelled `compatible` or `stale`, with both versions shown. What
  destroys trust is not the older document; it is serving it as though it were
  written for the installed version, because the resulting error then looks
  like the tool's.
- **A local override always wins.** A registry that cannot be overridden is a
  registry the developer is forced to trust.
- **The source of truth is the package author, not the service.** The service
  proves that a document is what its author published, at which version; it
  does not vouch for the content. That is the only claim that scales and the
  only one that survives a single bad document.
- **Context influences what agents install.** Anything with that influence
  attracts abuse. Treat fetched context as untrusted data, never as
  instructions, and assume prompt injection in the document body is an expected
  attack rather than an edge case.

First phase is author-published context only. Community use-case entries are
out of scope (§9) until the standing hierarchy and version-range filtering
exist.

### 14.2 Multi-language from the start — three-to-four seams

FastAPI is the first target, not the only one. The cheap insurance is that
`invar` is already almost language-neutral: the Plan, the gate, the checksums,
the decision log, `clean-spec`, `context show` — none of it knows Python
exists. Four operations do:

| Seam | Python (MVP) | Notes |
|---|---|---|
| Dependency discovery | `uv.lock` / `poetry.lock` / installed distributions | Feeds `context sync` (§5b) |
| Test discovery | `scenarios(...)` string match (§4b) | pytest-bdd convention |
| Test execution | `pytest` under `tests_dir` | Full suite by default |
| Lint | AST + import graph | Base and extended rule sets |

**These four must sit behind an explicit interface, not be scattered through
the commands.** With the interface, a Laravel or Node adapter is new code in
four places and a core rewrite (Rust, if performance or distribution ever
demands it) is mechanical. Without it, both are rewrites. This is the one thing
in this document that is genuinely expensive to retrofit and free to do now.

`language:` in `invar.yaml` (§6) selects the adapter. Absent means `python`.

**Likely shape of the second language: its own plugin.** Each ecosystem has its
own agent conventions and its own developers; a separate plugin per language,
over the same CLI and the same on-disk formats (§14.3), is probably cleaner
than one plugin that branches. Nothing here needs deciding now — it is only
worth stating that §13b's thin-shell rule is what keeps that option open.

**The CLI's implementation language is deliberately not decided here.** A
statically-linked binary would remove the "install Python first" barrier for a
PHP or Node developer, which is a distribution argument and a real one.
Performance is *not* an argument: of the four seams, test execution dominates
the clock and no rewrite touches it, and `context sync` is a no-op on an
unchanged lockfile. Extended lint rules are also per-project by design (§5a),
which a compiled core cannot accept without a declarative rule DSL — a separate
undertaking, and guesswork before we know which rules people actually write.
The trigger for reopening this is **a second language arriving**, not a desire
for a faster CLI, and the decision is cheap to defer precisely because of the
seams above and the formats in §14.3.

### 14.3 The real contract is the on-disk format

Every present and future surface — CLI, plugin, MCP server, registry, a
possible Rust core, other-language adapters — reads the same four artifacts:

- the `.feature` metadata block (§4)
- the Plan header and required sections (§4a)
- `.invar/decisions/*.md` entry format (§7)
- `invar.yaml` (§6)

As long as these are stable and documented, the pieces can be built in any
order and can coexist during a transition. From here on, a change to any of
them is a change to a public contract: made deliberately, with the reason
recorded, not as a side effect of an implementation edit.

New keys may be added at any time under one rule: **an absent key means the
previous behavior.** That is what makes the additions in this revision —
`language`, `context`, `Confirmed-checksum`, `## Scenarios to delete` —
non-breaking, and what keeps the next ones non-breaking too.
