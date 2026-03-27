------------------------------- MODULE ControllerReplication -------------------------------
EXTENDS FiniteSets, TLC

CONSTANTS Controllers, Events

ASSUME Controllers = {"c1", "c2"}

VARIABLES seen, linkUp

Init ==
  /\ seen \in [Controllers -> SUBSET Events]
  /\ seen = [c \in Controllers |-> {}]
  /\ linkUp \in [Controllers \X Controllers -> BOOLEAN]

Produce ==
  /\ \E c \in Controllers, e \in Events :
      /\ e \notin seen[c]
      /\ seen' = [seen EXCEPT ![c] = @ \cup {e}]
  /\ UNCHANGED linkUp

Deliver ==
  /\ \E src \in Controllers, dst \in Controllers :
      /\ src /= dst
      /\ linkUp[<<src, dst>>]
      /\ seen' = [seen EXCEPT ![dst] = @ \cup seen[src]]
  /\ UNCHANGED linkUp

AntiEntropy ==
  /\ \E a \in Controllers, b \in Controllers :
      /\ a /= b
      /\ linkUp[<<a, b>>] /\ linkUp[<<b, a>>]
      /\ seen' = [seen EXCEPT
                    ![a] = @ \cup seen[b],
                    ![b] = @ \cup seen[a]]
  /\ UNCHANGED linkUp

ToggleLink ==
  /\ \E a \in Controllers, b \in Controllers :
      /\ a /= b
      /\ linkUp' = [linkUp EXCEPT ![<<a, b>>] = ~@]
  /\ UNCHANGED seen

Noop ==
  UNCHANGED <<seen, linkUp>>

Next ==
  Produce \/ Deliver \/ AntiEntropy \/ ToggleLink \/ Noop

Spec == Init /\ [][Next]_<<seen, linkUp>>

Safety_Subset ==
  \A c \in Controllers : seen[c] \subseteq Events

Converged ==
  \A a \in Controllers, b \in Controllers : seen[a] = seen[b]

\* Under fair anti-entropy and eventual connectivity, replicas converge.
Liveness_EventualConvergence ==
  <>Converged

===========================================================================================
