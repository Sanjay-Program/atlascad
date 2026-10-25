# User pain points and workflow opportunities

**Reviewed:** 2026-10-23. Community posts below are anecdotal reports, not representative user research. They are hypotheses to validate through interviews and workflow observation; vote counts and single posts do not establish prevalence.

| Observed report | Source | Product implication | Confidence |
|---|---|---|---|
| Users report difficulty finding commands across workbenches, remembering workflows after a break, and inconsistent defaults/tool placement in FreeCAD. Replies disagree, including users who find the interface workable after learning it. | [FreeCAD community thread, 2026-01-09](https://www.reddit.com/r/FreeCAD/comments/1q7w7il/why_is_the_ui_so_complicated/) | Test one discoverable command system, stable defaults, contextual help, and a compact beginner workflow without removing expert access. | Anecdotal; recurring themes in the thread, not a population estimate. |
| A separate discussion reports workbench-specific learning curves and inconsistent UI behavior; other commenters argue the tool is learnable and the user's own workflow matters. | [FreeCAD community discussion, 2026-03-19](https://www.reddit.com/r/FreeCAD/comments/1rxrxp3/i_actually_love_freecad_a_lot_it_just_hurts_me/) | Make commands and task panels behave consistently across capabilities; validate with novice and experienced users. | Anecdotal and mixed. |
| A Fusion user describes delays in cloud save/update behavior while working with linked parts, and says local/offline cache behavior improved their particular workflow. Other replies offer different workflows. | [Fusion community discussion, 2025-04-14](https://www.reddit.com/r/Fusion360/comments/1jz15fm/is_anyone_switching_to_offline_mode_frequently/) | Keep core edit/save/recompute local and measure latency; make any future sync state and conflicts explicit. This does not establish that cloud workflows are generally slow. | Anecdotal, one assembly workflow. |
| FreeCAD users describe failures that can be hard to diagnose, including topological naming and fillet/recompute failures; other replies attribute some failures to model strategy or differing expectations. | [FreeCAD migration discussion, 2026-09-10](https://www.reddit.com/r/FreeCAD/comments/1wco5s4/how_difficult_is_the_switch_to_freecad/) | Identify failed feature and changed dependency, preserve recoverable history, and distinguish validated diagnosis from a suggestion. | Anecdotal and contested. |

## Research hypotheses to test

1. Beginners benefit from a small set of stable first-run workflows with visible progress and a recoverable example project.
2. Users will trust a local-first product more when native files are portable, saves are durable, and no account is required for core geometry.
3. Actionable recompute errors may matter more than adding another modeling command; measure task completion and recovery time.
4. Advanced users still need searchable commands, explicit parameter control, keyboard access, and a coherent extension surface.

These are product hypotheses, not findings. No interviews or usability study have been run in this repository.
