# Engineering AI

**As of:** 2026-10-31 · **Status:** no AI feature is implemented.

There is no language model, no local inference runtime, no cloud AI provider, no API key handling and
no natural-language-to-CAD path in this workspace. The agent crates are a **permissioned tool
runtime**, not an AI: `agent.cad.geometry-ops` is a deterministic agent whose operations are declared
in code.

Nothing in this repository may be described as AI-assisted engineering until the pipeline below
exists and is tested end to end.

## What exists that a future AI layer can use

* A typed operation surface: the studio HTTP API and the `atlas` CLI, both with validation and
  structured errors.
* A permission-aware agent contract and runtime (`cad-agent-api`, `cad-agent-runtime`) with grants,
  budgets and schema checks.
* Stable entity identities and a revision-pinned artifact model, so an operation can name exactly
  what it changed and what became stale.
* The `.zcad` undo/redo history, which gives an operation a reversal path inside the document.

## Required execution pipeline (not implemented)

```text
user instruction
  → intent parsing
  → typed operation plan (no free-form code)
  → permission check
  → engineering validation against the real document
  → preview / diff
  → explicit approval
  → execution through the same API a user uses
  → verification (rebuild + measurement)
  → audit trail entry
```

Every step is inspectable and reversible. A plan is data, not an executable script.

## Hard rules

* AI must never invent a simulation result, a material property, a drawing dimension or a
  manufacturing capability.
* AI must not bypass solver, kernel or document validation, and must not send machine code to a
  physical machine.
* AI must not modify a protected document without the same permission and approval a human needs.
* Untrusted scripts are never executed automatically.
* AI must not access documents outside the session's authority.
* No claim of engineering compliance, certification or physical safety may be generated.
* Every generated dimension or annotation must reference actual model geometry and be validated
  before acceptance.

## Cost and infrastructure position

Local computation is the default: geometry, rebuilds, drafting and local previews run in-process with
no cloud dependency. If a metered model is ever added, it must have per-session budgets, cancellation,
timeouts, usage metering and a visible cost estimate, consistent with `cad-entitlements` and the
under-US$100/month early-stage target for a defined low-usage workload. That target has not been
measured in this workspace because no metered service is integrated.

## Next step

Do not start with natural language. Start with a **typed operation plan** and a preview/apply
endpoint for parameter and sketch edits, then add an intent parser in front of it. That ordering keeps
the safety properties testable and means the AI layer cannot do anything the normal API cannot.
