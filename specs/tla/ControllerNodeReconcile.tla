------------------------------- MODULE ControllerNodeReconcile -------------------------------
EXTENDS Naturals, Sequences, TLC

CONSTANTS Controllers, NodeId

ASSUME Controllers /= {}

VARIABLES up, order, active, heartbeatCount

Init ==
  /\ up \in [Controllers -> BOOLEAN]
  /\ order \in Seq(Controllers)
  /\ Len(order) > 0
  /\ active = Head(order)
  /\ heartbeatCount \in [Controllers -> Nat]

IsReachable(c) == up[c] = TRUE

FirstReachable(seq) ==
  CHOOSE c \in SeqToSet(seq) : IsReachable(c)

NeedFailover ==
  ~IsReachable(active) /\ (\E c \in SeqToSet(order) : IsReachable(c))

Failover ==
  /\ NeedFailover
  /\ active' = FirstReachable(order)
  /\ UNCHANGED <<up, order, heartbeatCount>>

Heartbeat ==
  /\ IsReachable(active)
  /\ heartbeatCount' = [heartbeatCount EXCEPT ![active] = @ + 1]
  /\ UNCHANGED <<up, order, active>>

ToggleReachability ==
  /\ \E c \in Controllers :
      up' = [up EXCEPT ![c] = ~@]
  /\ UNCHANGED <<order, active, heartbeatCount>>

Noop ==
  UNCHANGED <<up, order, active, heartbeatCount>>

Next ==
  Failover \/ Heartbeat \/ ToggleReachability \/ Noop

Spec == Init /\ [][Next]_<<up, order, active, heartbeatCount>>

Safety_ActiveInControllers == active \in Controllers

Safety_HeartbeatMonotonic ==
  \A c \in Controllers : heartbeatCount[c] \in Nat

\* If there exists at least one reachable controller forever, failover/heartbeat
\* should keep active controller reachable infinitely often in bounded traces.
Liveness_IfAnyReachableThenEventuallyReachableActive ==
  (\A i \in Nat : \E c \in Controllers : up[c]) ~>
  (\A n \in Nat : <>IsReachable(active))

==============================================================================================
