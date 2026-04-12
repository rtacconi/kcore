------------------------------- MODULE CrossDcReplication -------------------------------
EXTENDS FiniteSets, Integers, TLC

CONSTANTS
  Controllers,
  Events,
  Dcs,
  NoEvent

ASSUME /\ Controllers # {}
       /\ Events # {}
       /\ Events \subseteq {"e1", "e2", "e3", "e4", "e5", "e6"}
       /\ Dcs = {"DC1", "DC2"}
       /\ NoEvent \notin Events

DcOf(c) ==
  IF c = "c3" THEN "DC2" ELSE "DC1"

VARIABLES Outbox, Delivered, Applied, DcHead, ReceivedOps, Frontier, LinkUp

Vars == <<Outbox, Delivered, Applied, DcHead, ReceivedOps, Frontier, LinkUp>>

Init ==
  /\ Outbox = [c \in Controllers |-> {}]
  /\ Delivered = [c \in Controllers |-> {}]
  /\ Applied = [c \in Controllers |-> {}]
  /\ DcHead = [dc \in Dcs |-> NoEvent]
  /\ ReceivedOps = [c \in Controllers |-> {}]
  /\ Frontier = [c \in Controllers |-> 0]
  /\ LinkUp \in [Controllers \X Controllers -> BOOLEAN]

Emit ==
  /\ \E c \in Controllers, e \in Events :
      /\ e \notin Outbox[c]
      /\ Outbox' = [Outbox EXCEPT ![c] = @ \cup {e}]
  /\ UNCHANGED <<Delivered, Applied, DcHead, ReceivedOps, Frontier, LinkUp>>

Deliver ==
  /\ \E src \in Controllers, dst \in Controllers, e \in Events :
      /\ src /= dst
      /\ LinkUp[<<src, dst>>]
      /\ e \in Outbox[src]
      /\ e \notin Delivered[dst]
      /\ Delivered' = [Delivered EXCEPT ![dst] = @ \cup {e}]
  /\ UNCHANGED <<Outbox, Applied, DcHead, ReceivedOps, Frontier, LinkUp>>

IntraDcAntiEntropy ==
  /\ \E src \in Controllers, dst \in Controllers :
      /\ src /= dst
      /\ DcOf(src) = DcOf(dst)
      /\ LinkUp[<<src, dst>>]
      /\ Delivered' = [Delivered EXCEPT ![dst] = @ \cup Outbox[src]]
  /\ UNCHANGED <<Outbox, Applied, DcHead, ReceivedOps, Frontier, LinkUp>>

CrossDcAntiEntropy ==
  /\ \E src \in Controllers, dst \in Controllers :
      /\ src /= dst
      /\ DcOf(src) # DcOf(dst)
      /\ LinkUp[<<src, dst>>]
      /\ Delivered' = [Delivered EXCEPT ![dst] = @ \cup Outbox[src]]
  /\ UNCHANGED <<Outbox, Applied, DcHead, ReceivedOps, Frontier, LinkUp>>

ApplyEvent ==
  /\ \E c \in Controllers :
      \E e \in (Delivered[c] \ Applied[c]) :
      LET dc == DcOf(c)
      IN
      /\ Applied' = [Applied EXCEPT ![c] = @ \cup {e}]
      /\ ReceivedOps' = [ReceivedOps EXCEPT ![c] = @ \cup {e}]
      /\ Frontier' = [Frontier EXCEPT ![c] = Cardinality(Applied[c] \cup {e})]
      /\ DcHead' =
          IF DcHead[dc] = NoEvent
            THEN [DcHead EXCEPT ![dc] = e]
            ELSE DcHead
  /\ UNCHANGED <<Outbox, Delivered, LinkUp>>

ToggleLink ==
  /\ \E src \in Controllers, dst \in Controllers :
      /\ src /= dst
      /\ LinkUp' = [LinkUp EXCEPT ![<<src, dst>>] = ~@]
  /\ UNCHANGED <<Outbox, Delivered, Applied, DcHead, ReceivedOps, Frontier>>

Noop ==
  UNCHANGED Vars

Next ==
  Emit
  \/ Deliver
  \/ IntraDcAntiEntropy
  \/ CrossDcAntiEntropy
  \/ ApplyEvent
  \/ ToggleLink
  \/ Noop

Spec ==
  Init /\ [][Next]_Vars
       /\ WF_Vars(Emit)
       /\ WF_Vars(Deliver)
       /\ WF_Vars(IntraDcAntiEntropy)
       /\ WF_Vars(CrossDcAntiEntropy)
       /\ WF_Vars(ApplyEvent)

TypeOK ==
  /\ Outbox \in [Controllers -> SUBSET Events]
  /\ Delivered \in [Controllers -> SUBSET Events]
  /\ Applied \in [Controllers -> SUBSET Events]
  /\ ReceivedOps \in [Controllers -> SUBSET Events]
  /\ Frontier \in [Controllers -> Nat]
  /\ DcHead \in [Dcs -> (Events \cup {NoEvent})]
  /\ \A c \in Controllers : Frontier[c] = Cardinality(Applied[c])

NoDoubleApply ==
  \A c \in Controllers : Applied[c] = ReceivedOps[c]

IntraDcSubsetSafety ==
  \A a \in Controllers, b \in Controllers :
    DcOf(a) = DcOf(b) => Applied[a] \subseteq Events /\ Applied[b] \subseteq Events

CrossDcEventualPropagationCandidate ==
  \A c \in Controllers : Applied[c] \subseteq UNION {Outbox[d] : d \in Controllers}

Divergence ==
  \E a \in Controllers, b \in Controllers : Applied[a] # Applied[b]

AnyApplied ==
  Cardinality(UNION {Applied[c] : c \in Controllers}) > 0

Connected ==
  \A src \in Controllers, dst \in Controllers :
    src /= dst => LinkUp[<<src, dst>>]

Liveness_CrossDcConvergesAfterWork ==
  (<>[]Connected) => <> (AnyApplied /\ ~Divergence)

==========================================================================================
