------------------------------- MODULE ControllerReplication -------------------------------
EXTENDS FiniteSets, Integers, TLC

CONSTANTS
  Controllers,
  Events,
  Resources,
  NoEvent

ASSUME /\ Controllers # {}
       /\ Events # {}
       /\ Events \subseteq {"e1", "e2", "e3", "e4", "e5", "e6"}
       /\ Resources # {}
       /\ Resources \subseteq {"vm/v1", "network/n1", "security-group/web"}
       /\ NoEvent \notin Events

AutoTerminalStates == {"auto_rejected", "auto_compensated", "auto_accepted"}

ResourceOf(e) ==
  CASE e \in {"e1", "e2"} -> "vm/v1"
    [] e \in {"e3", "e4"} -> "network/n1"
    [] e \in {"e5", "e6"} -> "security-group/web"
    [] OTHER -> "vm/v1"

RankOf(e) ==
  CASE e = "e1" -> 1
    [] e = "e2" -> 5
    [] e = "e3" -> 2
    [] e = "e4" -> 4
    [] e = "e5" -> 3
    [] e = "e6" -> 6
    [] OTHER -> 0

TerminalOf(e) ==
  CASE e = "e3" -> "auto_compensated"
    [] e \in {"e2", "e6"} -> "auto_accepted"
    [] OTHER -> "auto_rejected"

VARIABLES Outbox, Delivered, Applied, ResourceHead, ReceivedOps, Frontier, Conflicts

Vars == <<Outbox, Delivered, Applied, ResourceHead, ReceivedOps, Frontier, Conflicts>>

Init ==
  /\ Outbox = [c \in Controllers |-> {}]
  /\ Delivered = [c \in Controllers |-> {}]
  /\ Applied = [c \in Controllers |-> {}]
  /\ ResourceHead = [c \in Controllers |-> [r \in Resources |-> NoEvent]]
  /\ ReceivedOps = [c \in Controllers |-> {}]
  /\ Frontier = [c \in Controllers |-> 0]
  /\ Conflicts = {}

Emit ==
  /\ \E c \in Controllers, e \in Events :
      /\ e \notin Outbox[c]
      /\ Outbox' = [Outbox EXCEPT ![c] = @ \cup {e}]
  /\ UNCHANGED <<Delivered, Applied, ResourceHead, ReceivedOps, Frontier, Conflicts>>

Deliver ==
  /\ \E src \in Controllers, dst \in Controllers, e \in Events :
      /\ src /= dst
      /\ e \in Outbox[src]
      /\ e \notin Delivered[dst]
      /\ Delivered' = [Delivered EXCEPT ![dst] = @ \cup {e}]
  /\ UNCHANGED <<Outbox, Applied, ResourceHead, ReceivedOps, Frontier, Conflicts>>

AntiEntropy ==
  /\ \E src \in Controllers, dst \in Controllers :
      /\ src /= dst
      /\ Delivered' = [Delivered EXCEPT ![dst] = @ \cup Outbox[src]]
  /\ UNCHANGED <<Outbox, Applied, ResourceHead, ReceivedOps, Frontier, Conflicts>>

ApplyEvent ==
  /\ \E c \in Controllers :
      \E e \in (Delivered[c] \ Applied[c]) :
      LET r == ResourceOf(e)
          incumbent == ResourceHead[c][r]
          winner ==
            IF incumbent = NoEvent \/ RankOf(e) > RankOf(incumbent)
              THEN e
              ELSE incumbent
          loser ==
            IF winner = e THEN incumbent ELSE e
          conflictRec ==
            [controller |-> c,
             resource |-> r,
             winner |-> winner,
             loser |-> loser,
             status |-> IF loser = NoEvent THEN "none" ELSE TerminalOf(loser)]
      IN
      /\ Applied' = [Applied EXCEPT ![c] = @ \cup {e}]
      /\ ReceivedOps' = [ReceivedOps EXCEPT ![c] = @ \cup {e}]
      /\ ResourceHead' = [ResourceHead EXCEPT ![c][r] = winner]
      /\ Frontier' = [Frontier EXCEPT ![c] = Cardinality(Applied[c] \cup {e})]
      /\ Conflicts' =
          IF incumbent = NoEvent \/ incumbent = e
            THEN Conflicts
            ELSE Conflicts \cup {conflictRec}
  /\ UNCHANGED <<Outbox, Delivered>>

Noop ==
  UNCHANGED Vars

Next ==
  Emit \/ Deliver \/ AntiEntropy \/ ApplyEvent \/ Noop

Spec ==
  Init /\ [][Next]_Vars
       /\ WF_Vars(Emit)
       /\ WF_Vars(Deliver)
       /\ WF_Vars(AntiEntropy)
       /\ WF_Vars(ApplyEvent)

TypeOK ==
  /\ Outbox \in [Controllers -> SUBSET Events]
  /\ Delivered \in [Controllers -> SUBSET Events]
  /\ Applied \in [Controllers -> SUBSET Events]
  /\ ResourceHead \in [Controllers -> [Resources -> (Events \cup {NoEvent})]]
  /\ ReceivedOps \in [Controllers -> SUBSET Events]
  /\ Frontier \in [Controllers -> Nat]
  /\ \A c \in Controllers : Frontier[c] = Cardinality(Applied[c])

NoDoubleApply ==
  \A c \in Controllers : Applied[c] = ReceivedOps[c]

DeterministicWinner ==
  \A c \in Controllers, r \in Resources :
    LET contenders == {e \in Applied[c] : ResourceOf(e) = r}
    IN
      /\ contenders = {} => ResourceHead[c][r] = NoEvent
      /\ contenders # {} =>
          /\ ResourceHead[c][r] \in contenders
          /\ \A e \in contenders : RankOf(e) <= RankOf(ResourceHead[c][r])

NoManualRequired ==
  \A rec \in Conflicts : rec.status = "none" \/ rec.status \in AutoTerminalStates

CompensatedConflictsHaveLoser ==
  \A rec \in Conflicts :
    rec.status = "auto_compensated" => rec.loser # NoEvent

Converged ==
  \A a \in Controllers, b \in Controllers : Applied[a] = Applied[b]

AnyApplied ==
  Cardinality(UNION {Applied[c] : c \in Controllers}) > 0

HeadsAgreeWhenConverged ==
  Converged =>
    \A a \in Controllers, b \in Controllers, r \in Resources :
      ResourceHead[a][r] = ResourceHead[b][r]

Liveness_EventualConvergenceAfterWork ==
  <> (AnyApplied /\ Converged)

===========================================================================================
