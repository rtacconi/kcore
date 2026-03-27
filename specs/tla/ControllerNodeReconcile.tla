------------------------------- MODULE ControllerNodeReconcile -------------------------------
EXTENDS Naturals, TLC

CONSTANTS Controllers, Priority

ASSUME /\ Controllers # {}
       /\ Priority \in [Controllers -> Nat]
       /\ \A a, b \in Controllers : (Priority[a] = Priority[b]) => (a = b)

VARIABLES Up, Active, HeartbeatCount

Vars == <<Up, Active, HeartbeatCount>>

ReachableControllers(u) == {c \in Controllers : u[c]}

BestReachable(u) ==
  CHOOSE c \in ReachableControllers(u) :
    \A d \in ReachableControllers(u) : Priority[c] <= Priority[d]

Init ==
  /\ Up = [c \in Controllers |-> TRUE]
  /\ Active = BestReachable(Up)
  /\ HeartbeatCount = [c \in Controllers |-> 0]

Failover ==
  /\ ~Up[Active]
  /\ ReachableControllers(Up) # {}
  /\ Active' = BestReachable(Up)
  /\ UNCHANGED <<Up, HeartbeatCount>>

Heartbeat ==
  /\ Up[Active]
  /\ HeartbeatCount' = [HeartbeatCount EXCEPT ![Active] = @ + 1]
  /\ UNCHANGED <<Up, Active>>

ToggleReachability ==
  /\ \E c \in Controllers :
      Up' = [Up EXCEPT ![c] = ~@]
  /\ UNCHANGED <<Active, HeartbeatCount>>

Noop ==
  UNCHANGED Vars

Next ==
  Failover \/ Heartbeat \/ ToggleReachability \/ Noop

Spec ==
  Init /\ [][Next]_Vars
       /\ WF_Vars(Failover)
       /\ WF_Vars(Heartbeat)

TypeOK ==
  /\ Up \in [Controllers -> BOOLEAN]
  /\ Active \in Controllers
  /\ HeartbeatCount \in [Controllers -> Nat]

Safety_ActiveReachableOrNoReachable ==
  Up[Active] \/ ReachableControllers(Up) = {}

Safety_HeartbeatCountersNatural ==
  \A c \in Controllers : HeartbeatCount[c] \in Nat

Liveness_IfAnyReachableThenEventuallyReachableActive ==
  []((ReachableControllers(Up) # {}) => <>Up[Active])

Liveness_HeartbeatsProgressWhenReachable ==
  []((ReachableControllers(Up) # {}) => <>(
      \E c \in Controllers : HeartbeatCount[c] > 0
  ))

==============================================================================================
