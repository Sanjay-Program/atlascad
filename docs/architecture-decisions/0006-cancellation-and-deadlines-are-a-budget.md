# ADR-0006: Cancellation and deadlines are a `Budget` the caller passes in, not a trait change

- **Status:** Accepted
- **Date:** 2026-10-22
- **Deciders:** engineering (this cycle)
- **Evidence:** `crates/cad-kernel-truck/src/lib.rs` (`Budget`, `CancellationToken`, `Deadline`,
  `extrude_with_budget`, `boolean_with_budget`, `tessellate_with_budget`, `volume_with_budget`,
  `bounding_box_with_budget`), tests `an_expired_deadline_returns_timeout_and_leaves_the_kernel_usable`,
  `every_budgeted_entry_point_enforces_the_deadline`, `a_cancelled_token_stops_the_operation_and_can_be_reset`,
  `cancelling_a_running_operation_is_observed_and_the_instance_survives`,
  `the_trait_path_never_reports_cancellation`,
  `engineering/quality/tests.json#t-cad-kernel-truck`

## Context

The contract's `KernelError` has carried `Cancelled` and `Timeout` since it was written, but nothing
ever set them: a long boolean in the adapter could not be interrupted, and a UI had no way to
express "stop" or "you have ten seconds". The brief requires long-running operations to support
cancellation, deadlines, structured errors and resource limits.

The obvious move is to add `cancel: &CancellationToken` and `deadline: Instant` to the `Kernel`
trait's methods. That would be a breaking change to the trait every future adapter has to implement,
and it would push an execution-policy concern into the vocabulary every caller must learn.

## Options considered

| Option | Assessment |
|---|---|
| Add cancellation and deadline parameters to the `Kernel` trait | Breaks the trait, and puts a policy argument on every call forever. The trait would no longer describe *what* a kernel can do, but *how* it is being driven. |
| Give the adapter a global cancellation flag | Hidden state: two concurrent callers would share one flag, so one caller's cancel would abort another's work. |
| **Keep the trait plain; add `_with_budget` twins on the adapter** | Chosen. The trait stays the smallest thing two kernels can both implement; the policy lives in the adapter, where the work actually happens. |
| Threads with `JoinHandle::cancel` | Requires the operation to be interruptible, which third-party kernel code is not. It would give the illusion of preemption. |

## Decision

The trait methods keep their plain signatures and behave exactly as before. The adapter gains a
parallel set of entry points that take a `Budget`:

```rust
pub struct Budget {
    cancel: CancellationToken,
    deadline: Deadline,
}
```

* `Deadline` is either unbounded or an absolute `Instant`. It is checked **before** the work, so an
  already-expired deadline returns `Timeout` without touching the kernel at all.
* `CancellationToken` is a shared flag with `cancel()` and `reset()`. Clones observe the same flag,
  so a caller can hold one and the adapter another.
* The budget is checked before the work, after the work, and at internal checkpoints — per shell in
  the boolean path, per 4096 vertices in the weld loop, per face in the tessellation.

`KernelError::Cancelled` and `KernelError::Timeout` are therefore reachable, and a timed-out or
cancelled operation leaves the kernel instance usable: the error is fail-closed and the next call on
the same instance succeeds. This is asserted directly, because a kernel that cannot be reused after
an interruption is not much use to an interactive application.

## Consequences

* **Good:** the `Kernel` trait is unchanged, so a second kernel adapter does not have to implement
  an execution policy. The adapter's plain methods and its budgeted methods coexist, and the plain
  ones are a thin wrapper over the budgeted ones with an unbounded budget.
* **Good:** the same `Budget` type is what the agent runtime hands to an agent, so cancellation
  reaches the kernel through the agent without the agent reimplementing any of it. The geometry
  agent's cancellation path is therefore covered by the adapter's tests rather than by a parallel set.
* **Bad:** cancellation is **cooperative, not preemption**. A single pathological boolean inside
  truck's own code cannot be interrupted until it returns. This is stated in the module
  documentation and in the manifest's known-limitations field rather than left for a caller to
  discover.
* **Bad:** the shared token means a caller must `reset()` it before the next call. The kernel
  adapter documents this rule, and the agent documents the same rule for the token it shares with
  its host.

## Alternatives kept open

A trait-level `Kernel::with_budget` default method would remove the `_with_budget` naming
duplication, at the cost of putting a policy type into the contract's vocabulary. If a second
adapter ever needs the same semantics, the duplication should be folded into the trait then — as an
additive default method, not a breaking change.
