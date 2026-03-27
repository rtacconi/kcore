------------------------------- MODULE CrossDcReplication -------------------------------
EXTENDS FiniteSets, TLC

CONSTANTS Controllers, Events, DcOf

ASSUME Controllers = {"c1", "c2", "c3"}

VARIABLES seen, linkUp

Init ==
  /\ seen = [c \in Controllers |-> {}]
  /\ linkUp \in [Controllers \X Controllers -> BOOLEAN]

Produce ==
  /\ \E c \in Controllers, e \in Events :
      /\ e \notin seen[c]
      /\ seen' = [seen EXCEPT ![c] = @ \cup {e}]
  /\ UNCHANGED linkUp

IntraDcSync ==
  /\ \E a \in Controllers, b \in Controllers :
      /\ a /= b
      /\ DcOf[a] = DcOf[b]
      /\ linkUp[<<a, b>>]
      /\ seen' = [seen EXCEPT ![b] = @ \cup seen[a]]
  /\ UNCHANGED linkUp

CrossDcSync ==
  /\ \E a \in Controllers, b \in Controllers :
      /\ DcOf[a] /= DcOf[b]
      /\ linkUp[<<a, b>>]
      /\ seen' = [seen EXCEPT ![b] = @ \cup seen[a]]
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
  Produce \/ IntraDcSync \/ CrossDcSync \/ AntiEntropy \/ ToggleLink \/ Noop

Spec == Init /\ [][Next]_<<seen, linkUp>>

Safety_Subset ==
  \A c \in Controllers : seen[c] \subseteq Events

AllConverged ==
  \A a \in Controllers, b \in Controllers : seen[a] = seen[b]

Liveness_CrossDcConverges ==
  <>AllConverged

==========================================================================================
