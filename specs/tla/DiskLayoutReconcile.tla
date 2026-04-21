------------------------------- MODULE DiskLayoutReconcile -------------------------------
\* Per-resource state machine for the controller-orchestrated DiskLayout resource.
\*
\* Models the reconciliation loop between the controller (which owns
\* `DiskLayoutRow.generation`) and the node-agent (which owns
\* `DiskLayoutStatus.observed_generation` + `phase`). The central safety
\* claim is that no resource ever transitions to `Applied` unless the
\* node-agent's authoritative classifier has just declared the proposed
\* layout safe against live `lsblk` state.
\*
\* This spec deliberately abstracts over
\*   * the actual Nix evaluator and disko invocation (collapsed into a
\*     boolean `Safe[r]`),
\*   * network failures (the reconciler simply retries every tick),
\*   * controller HA / replication (covered by ControllerReplication.tla).
\*
\* See `crates/controller/src/disk_reconciler.rs` for the production loop
\* and `crates/kcore-disko-types/src/lib.rs` for the classifier.

EXTENDS Naturals, TLC

CONSTANTS
  Resources,            \* finite set of DiskLayout names
  MaxGen                \* upper bound on the controller-side `generation`

ASSUME /\ Resources # {}
       /\ MaxGen \in Nat
       /\ MaxGen >= 1

Phases == {"Pending", "Applied", "Refused", "Failed"}

VARIABLES
  Generation,    \* [r |-> Nat]      controller-owned desired generation
  Observed,      \* [r |-> Nat]      node-observed generation
  Phase,         \* [r |-> Phases]   last phase reported by node-agent
  Safe           \* [r |-> BOOLEAN]  classifier verdict for current generation

Vars == <<Generation, Observed, Phase, Safe>>

\* ----- Initial state -----------------------------------------------------------
Init ==
  /\ Generation = [r \in Resources |-> 1]
  /\ Observed   = [r \in Resources |-> 0]
  /\ Phase      = [r \in Resources |-> "Pending"]
  /\ Safe       \in [Resources -> BOOLEAN]

\* ----- Operator actions --------------------------------------------------------

\* Operator submits a *new* layout body for `r`. We bump Generation,
\* reset Observed to "behind" (so the reconciler must pick it up) and
\* clear the phase to Pending. The classifier verdict for the *new*
\* body is freshly chosen (the old one is stale).
SubmitNewLayout(r) ==
  /\ Generation[r] < MaxGen
  /\ Generation' = [Generation EXCEPT ![r] = @ + 1]
  /\ Observed'   = [Observed   EXCEPT ![r] = 0]
  /\ Phase'      = [Phase      EXCEPT ![r] = "Pending"]
  /\ \E b \in BOOLEAN : Safe' = [Safe EXCEPT ![r] = b]

\* Operator drains affected VMs / clears blockers, flipping the
\* node-side classifier verdict from dangerous to safe. This is the
\* only transition that may turn Safe[r] from FALSE to TRUE without
\* a Generation bump.
ClearBlockers(r) ==
  /\ ~Safe[r]
  /\ Safe'       = [Safe EXCEPT ![r] = TRUE]
  /\ UNCHANGED <<Generation, Observed, Phase>>

\* ----- Reconciler tick (controller-driven) -------------------------------------

\* The reconciler picks a layout that is behind, calls the node-agent,
\* and writes back observed_generation + phase based on the verdict.
\*
\* Three terminal-for-this-generation outcomes are modeled:
\*   * Safe -> Applied
\*   * !Safe -> Refused (operator must clear blockers and the next
\*                       reconcile will retry the same generation)
\*   * Safe -> Failed   (disko ran but nixos-rebuild bombed; modeled
\*                       as a non-deterministic infrastructure failure)
ReconcileApplied(r) ==
  /\ Observed[r] < Generation[r]
  /\ Safe[r]
  /\ Phase'    = [Phase    EXCEPT ![r] = "Applied"]
  /\ Observed' = [Observed EXCEPT ![r] = Generation[r]]
  /\ UNCHANGED <<Generation, Safe>>

ReconcileRefused(r) ==
  /\ Observed[r] < Generation[r]
  /\ ~Safe[r]
  /\ Phase' = [Phase EXCEPT ![r] = "Refused"]
  \* Observed stays behind: the same generation will be retried.
  /\ UNCHANGED <<Generation, Observed, Safe>>

ReconcileFailed(r) ==
  /\ Observed[r] < Generation[r]
  /\ Safe[r]
  /\ Phase'    = [Phase    EXCEPT ![r] = "Failed"]
  /\ Observed' = [Observed EXCEPT ![r] = Generation[r]]
  /\ UNCHANGED <<Generation, Safe>>

Noop == UNCHANGED Vars

Next ==
  \/ \E r \in Resources :
       \/ SubmitNewLayout(r)
       \/ ClearBlockers(r)
       \/ ReconcileApplied(r)
       \/ ReconcileRefused(r)
       \/ ReconcileFailed(r)
  \/ Noop

FairnessApplied == \A r1 \in Resources : WF_Vars(ReconcileApplied(r1))
FairnessRefused == \A r2 \in Resources : WF_Vars(ReconcileRefused(r2))

Spec ==
  Init /\ [][Next]_Vars /\ FairnessApplied /\ FairnessRefused

\* ----- Type & state-space bound ----------------------------------------------

TypeOK ==
  /\ Generation \in [Resources -> 1..MaxGen]
  /\ Observed   \in [Resources -> 0..MaxGen]
  /\ Phase      \in [Resources -> Phases]
  /\ Safe       \in [Resources -> BOOLEAN]

StateConstraint ==
  \A r \in Resources :
    /\ Generation[r] <= MaxGen
    /\ Observed[r] <= Generation[r]

\* ----- Safety invariants ------------------------------------------------------

\* The headline guarantee. Any time we are at phase Applied for `r`,
\* the most recent reconcile saw Safe[r] = TRUE. Equivalently:
\* "no Applied without classifier approval".
Safety_AppliedImpliesSafe ==
  \A r \in Resources :
    Phase[r] = "Applied" => Safe[r]

\* The reconciler can only catch up; it never moves Observed past the
\* controller's desired generation.
Safety_ObservedNeverAheadOfGeneration ==
  \A r \in Resources : Observed[r] <= Generation[r]

\* When Phase records a node-acknowledged outcome (Applied or Failed),
\* Observed must equal Generation. Refused / Pending may legitimately
\* leave Observed behind so the reconciler retries.
Safety_TerminalPhasesObservedCurrentGen ==
  \A r \in Resources :
    Phase[r] \in {"Applied", "Failed"} => Observed[r] = Generation[r]

\* ----- Liveness --------------------------------------------------------------

\* If a layout is safe and the operator stops submitting new
\* generations, the reconciler eventually reaches Applied.
Liveness_SafeEventuallyApplied ==
  \A r \in Resources :
    [](Safe[r] /\ Observed[r] < Generation[r] => <>(Phase[r] = "Applied"))

==============================================================================================
