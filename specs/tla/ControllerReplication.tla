------------------------------- MODULE ControllerReplication -------------------------------
EXTENDS FiniteSets, Integers, TLC

CONSTANTS
  Controllers,
  Events,
  Resources,
  ResourceOf,
  RankOf,
  TerminalOf,
  AutoTerminalStates,
  NoEvent

ASSUME /\ Controllers # {}
       /\ Events # {}
       /\ Resources # {}
       /\ ResourceOf \in [Events -> Resources]
       /\ RankOf \in [Events -> Int]
       /\ TerminalOf \in [Events -> AutoTerminalStates]
       /\ NoEvent \notin Events

VARIABLES Outbox, Delivered, Applied, Head, ReceivedOps, Frontier, Conflicts

Vars == <<Outbox, Delivered, Applied, Head, ReceivedOps, Frontier, Conflicts>>

Init ==
  /\ Outbox = [c \in Controllers |-> {}]
  /\ Delivered = [c \in Controllers |-> {}]
  /\ Applied = [c \in Controllers |-> {}]
  /\ Head = [c \in Controllers |-> [r \in Resources |-> NoEvent]]
  /\ ReceivedOps = [c \in Controllers |-> {}]
  /\ Frontier = [c \in Controllers |-> 0]
  /\ Conflicts = {}

Emit ==
  /\ \E c \in Controllers, e \in Events :
      /\ e \notin Outbox[c]
      /\ Outbox' = [Outbox EXCEPT ![c] = @ \cup {e}]
  /\ UNCHANGED <<Delivered, Applied, Head, ReceivedOps, Frontier, Conflicts>>

Deliver ==
  /\ \E src \in Controllers, dst \in Controllers, e \in Events :
      /\ src /= dst
      /\ e \in Outbox[src]
      /\ e \notin Delivered[dst]
      /\ Delivered' = [Delivered EXCEPT ![dst] = @ \cup {e}]
  /\ UNCHANGED <<Outbox, Applied, Head, ReceivedOps, Frontier, Conflicts>>

AntiEntropy ==
  /\ \E src \in Controllers, dst \in Controllers :
      /\ src /= dst
      /\ Delivered' = [Delivered EXCEPT ![dst] = @ \cup Outbox[src]]
  /\ UNCHANGED <<Outbox, Applied, Head, ReceivedOps, Frontier, Conflicts>>

ApplyEvent ==
  /\ \E c \in Controllers, e \in Delivered[c] \ Applied[c] :
      LET r == ResourceOf[e]
          incumbent == Head[c][r]
          winner ==
            IF incumbent = NoEvent \/ RankOf[e] > RankOf[incumbent]
              THEN e
              ELSE incumbent
          loser ==
            IF winner = e THEN incumbent ELSE e
          conflictRec ==
            [controller |-> c,
             resource |-> r,
             winner |-> winner,
             loser |-> loser,
             status |-> IF loser = NoEvent THEN "none" ELSE TerminalOf[loser]]
      IN
      /\ Applied' = [Applied EXCEPT ![c] = @ \cup {e}]
      /\ ReceivedOps' = [ReceivedOps EXCEPT ![c] = @ \cup {e}]
      /\ Head' = [Head EXCEPT ![c][r] = winner]
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
       /\ WF_Vars(Deliver)
       /\ WF_Vars(AntiEntropy)
       /\ WF_Vars(ApplyEvent)

TypeOK ==
  /\ Outbox \in [Controllers -> SUBSET Events]
  /\ Delivered \in [Controllers -> SUBSET Events]
  /\ Applied \in [Controllers -> SUBSET Events]
  /\ Head \in [Controllers -> [Resources -> (Events \cup {NoEvent})]]
  /\ ReceivedOps \in [Controllers -> SUBSET Events]
  /\ Frontier \in [Controllers -> Nat]
  /\ \A c \in Controllers : Frontier[c] = Cardinality(Applied[c])

NoDoubleApply ==
  \A c \in Controllers : Applied[c] = ReceivedOps[c]

DeterministicWinner ==
  \A c \in Controllers, r \in Resources :
    LET contenders == {e \in Applied[c] : ResourceOf[e] = r}
    IN
      /\ contenders = {} => Head[c][r] = NoEvent
      /\ contenders # {} =>
          /\ Head[c][r] \in contenders
          /\ \A e \in contenders : RankOf[e] <= RankOf[Head[c][r]]

NoManualRequired ==
  \A rec \in Conflicts : rec.status = "none" \/ rec.status \in AutoTerminalStates

Converged ==
  \A a \in Controllers, b \in Controllers : Applied[a] = Applied[b]

Liveness_EventualConvergence ==
  <>Converged

===========================================================================================
