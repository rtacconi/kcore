use std::cmp::Ordering;

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum ValidityClass {
    Valid,
    Invalid,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum SafetyClass {
    Safe,
    Risky,
    Unsafe,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
#[allow(dead_code)]
pub enum ReconcileTerminalState {
    AutoAccepted,
    AutoRejected,
    AutoCompensated,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct ArbitrationRank<'a> {
    pub validity: ValidityClass,
    pub safety: SafetyClass,
    pub policy_priority: i32,
    pub intent_epoch: i64,
    pub logical_ts_unix_ms: i64,
    pub controller_id: &'a str,
    pub op_id: &'a str,
}

pub fn compare_rank(a: ArbitrationRank<'_>, b: ArbitrationRank<'_>) -> Ordering {
    validity_score(a.validity)
        .cmp(&validity_score(b.validity))
        .then_with(|| safety_score(a.safety).cmp(&safety_score(b.safety)))
        .then_with(|| a.policy_priority.cmp(&b.policy_priority))
        .then_with(|| a.intent_epoch.cmp(&b.intent_epoch))
        .then_with(|| a.logical_ts_unix_ms.cmp(&b.logical_ts_unix_ms))
        .then_with(|| a.controller_id.cmp(b.controller_id))
        .then_with(|| a.op_id.cmp(b.op_id))
}

pub fn loser_terminal_state(
    validity: ValidityClass,
    safety: SafetyClass,
) -> ReconcileTerminalState {
    if validity == ValidityClass::Invalid {
        ReconcileTerminalState::AutoRejected
    } else if safety == SafetyClass::Unsafe {
        ReconcileTerminalState::AutoCompensated
    } else {
        ReconcileTerminalState::AutoRejected
    }
}

pub fn parse_validity_class(value: Option<&str>) -> ValidityClass {
    match value
        .unwrap_or("valid")
        .trim()
        .to_ascii_lowercase()
        .as_str()
    {
        "invalid" => ValidityClass::Invalid,
        _ => ValidityClass::Valid,
    }
}

pub fn parse_safety_class(value: Option<&str>) -> SafetyClass {
    match value.unwrap_or("safe").trim().to_ascii_lowercase().as_str() {
        "unsafe" => SafetyClass::Unsafe,
        "risky" => SafetyClass::Risky,
        _ => SafetyClass::Safe,
    }
}

fn validity_score(value: ValidityClass) -> i32 {
    match value {
        ValidityClass::Invalid => 0,
        ValidityClass::Valid => 1,
    }
}

fn safety_score(value: SafetyClass) -> i32 {
    match value {
        SafetyClass::Unsafe => 0,
        SafetyClass::Risky => 1,
        SafetyClass::Safe => 2,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compare_rank_prefers_higher_priority_then_time() {
        let a = ArbitrationRank {
            validity: ValidityClass::Valid,
            safety: SafetyClass::Safe,
            policy_priority: 20,
            intent_epoch: 1,
            logical_ts_unix_ms: 100,
            controller_id: "ctrl-a",
            op_id: "op-1",
        };
        let b = ArbitrationRank {
            validity: ValidityClass::Valid,
            safety: SafetyClass::Safe,
            policy_priority: 10,
            intent_epoch: 9,
            logical_ts_unix_ms: 9999,
            controller_id: "ctrl-z",
            op_id: "op-9",
        };
        assert_eq!(compare_rank(a, b), Ordering::Greater);
    }

    #[test]
    fn loser_terminal_state_invalid_dominates_unsafe() {
        assert_eq!(
            loser_terminal_state(ValidityClass::Invalid, SafetyClass::Unsafe),
            ReconcileTerminalState::AutoRejected
        );
    }

    #[test]
    fn compare_rank_uses_controller_then_op_as_tiebreakers() {
        let a = ArbitrationRank {
            validity: ValidityClass::Valid,
            safety: SafetyClass::Safe,
            policy_priority: 0,
            intent_epoch: 0,
            logical_ts_unix_ms: 100,
            controller_id: "ctrl-b",
            op_id: "op-1",
        };
        let b = ArbitrationRank {
            validity: ValidityClass::Valid,
            safety: SafetyClass::Safe,
            policy_priority: 0,
            intent_epoch: 0,
            logical_ts_unix_ms: 100,
            controller_id: "ctrl-a",
            op_id: "op-999",
        };
        assert_eq!(compare_rank(a, b), Ordering::Greater);
    }
}

/// Property-based tests (Phase 2) — replication arbitration.
///
/// `compare_rank` is the single source of truth for "which controller's
/// intent wins" across the whole replication subsystem, so we want strong
/// algebraic guarantees: it must be a **total order** (reflexive,
/// antisymmetric, transitive, total). A regression here would silently
/// cause split-brain or thrashing rather than a visible crash.
#[cfg(test)]
mod proptests {
    use super::*;
    use proptest::prelude::*;

    fn arb_validity() -> impl Strategy<Value = ValidityClass> {
        prop_oneof![Just(ValidityClass::Valid), Just(ValidityClass::Invalid),]
    }

    fn arb_safety() -> impl Strategy<Value = SafetyClass> {
        prop_oneof![
            Just(SafetyClass::Safe),
            Just(SafetyClass::Risky),
            Just(SafetyClass::Unsafe),
        ]
    }

    /// Owned variant so we don't run into lifetime headaches when proptest
    /// shrinks. We materialize `&str` references at the call site.
    #[derive(Debug, Clone)]
    struct OwnedRank {
        validity: ValidityClass,
        safety: SafetyClass,
        policy_priority: i32,
        intent_epoch: i64,
        logical_ts_unix_ms: i64,
        controller_id: String,
        op_id: String,
    }

    fn arb_rank() -> impl Strategy<Value = OwnedRank> {
        (
            arb_validity(),
            arb_safety(),
            // Cap priority/epoch/ts to small ranges so proptest will
            // generate frequent collisions and exercise tiebreakers.
            -5i32..=5,
            -5i64..=5,
            -5i64..=5,
            prop::sample::select(vec!["ctrl-a", "ctrl-b", "ctrl-c"]).prop_map(String::from),
            prop::sample::select(vec!["op-1", "op-2", "op-3"]).prop_map(String::from),
        )
            .prop_map(
                |(
                    validity,
                    safety,
                    policy_priority,
                    intent_epoch,
                    logical_ts_unix_ms,
                    controller_id,
                    op_id,
                )| {
                    OwnedRank {
                        validity,
                        safety,
                        policy_priority,
                        intent_epoch,
                        logical_ts_unix_ms,
                        controller_id,
                        op_id,
                    }
                },
            )
    }

    fn rank_of<'a>(o: &'a OwnedRank) -> ArbitrationRank<'a> {
        ArbitrationRank {
            validity: o.validity,
            safety: o.safety,
            policy_priority: o.policy_priority,
            intent_epoch: o.intent_epoch,
            logical_ts_unix_ms: o.logical_ts_unix_ms,
            controller_id: &o.controller_id,
            op_id: &o.op_id,
        }
    }

    proptest! {
        #![proptest_config(ProptestConfig {
            cases: 2_000,
            .. ProptestConfig::default()
        })]

        /// Reflexivity: every rank is equal to itself.
        #[test]
        fn compare_rank_is_reflexive(o in arb_rank()) {
            let r = rank_of(&o);
            prop_assert_eq!(compare_rank(r, r), Ordering::Equal);
        }

        /// Antisymmetry / duality: `cmp(a, b) == reverse(cmp(b, a))`.
        /// A bug here would silently produce non-deterministic winners.
        #[test]
        fn compare_rank_is_antisymmetric(a in arb_rank(), b in arb_rank()) {
            let ar = rank_of(&a);
            let br = rank_of(&b);
            prop_assert_eq!(compare_rank(ar, br), compare_rank(br, ar).reverse());
        }

        /// Transitivity: `a <= b` and `b <= c` implies `a <= c`.
        /// This is the property a sort relies on; a bug here would let
        /// the arbitration order produce cycles.
        #[test]
        fn compare_rank_is_transitive(
            a in arb_rank(),
            b in arb_rank(),
            c in arb_rank(),
        ) {
            let ar = rank_of(&a);
            let br = rank_of(&b);
            let cr = rank_of(&c);
            let ab = compare_rank(ar, br);
            let bc = compare_rank(br, cr);
            if ab != Ordering::Greater && bc != Ordering::Greater {
                prop_assert!(compare_rank(ar, cr) != Ordering::Greater);
            }
            if ab != Ordering::Less && bc != Ordering::Less {
                prop_assert!(compare_rank(ar, cr) != Ordering::Less);
            }
        }

        /// Validity dominates safety which dominates policy priority and
        /// so on. A `Valid` rank with worst-of-everything else should
        /// still beat any `Invalid` rank with best-of-everything else.
        #[test]
        fn validity_strictly_dominates_other_axes(
            safety_v in arb_safety(),
            safety_i in arb_safety(),
            prio_v in any::<i32>(),
            prio_i in any::<i32>(),
            epoch_v in any::<i64>(),
            epoch_i in any::<i64>(),
            ts_v in any::<i64>(),
            ts_i in any::<i64>(),
        ) {
            let valid = OwnedRank {
                validity: ValidityClass::Valid,
                safety: safety_v,
                policy_priority: prio_v,
                intent_epoch: epoch_v,
                logical_ts_unix_ms: ts_v,
                controller_id: "z".into(),
                op_id: "z".into(),
            };
            let invalid = OwnedRank {
                validity: ValidityClass::Invalid,
                safety: safety_i,
                policy_priority: prio_i,
                intent_epoch: epoch_i,
                logical_ts_unix_ms: ts_i,
                controller_id: "a".into(),
                op_id: "a".into(),
            };
            prop_assert_eq!(
                compare_rank(rank_of(&valid), rank_of(&invalid)),
                Ordering::Greater
            );
        }

        /// `loser_terminal_state` partitions `(validity, safety)` into
        /// exactly the documented three classes. This is the regression
        /// boundary for any future policy change.
        #[test]
        fn loser_terminal_state_matches_policy_table(
            validity in arb_validity(),
            safety in arb_safety(),
        ) {
            let got = loser_terminal_state(validity, safety);
            let expected = if validity == ValidityClass::Invalid {
                ReconcileTerminalState::AutoRejected
            } else if safety == SafetyClass::Unsafe {
                ReconcileTerminalState::AutoCompensated
            } else {
                ReconcileTerminalState::AutoRejected
            };
            prop_assert_eq!(got, expected);
        }

        /// `parse_validity_class` / `parse_safety_class` never panic, are
        /// case-insensitive, and unknown inputs land in the documented
        /// default bucket.
        #[test]
        fn parse_class_helpers_default_on_unknown(s in ".{0,16}") {
            let v = parse_validity_class(Some(&s));
            let lower = s.trim().to_ascii_lowercase();
            let expected_v = if lower == "invalid" {
                ValidityClass::Invalid
            } else {
                ValidityClass::Valid
            };
            prop_assert_eq!(v, expected_v);

            let sf = parse_safety_class(Some(&s));
            let expected_sf = match lower.as_str() {
                "unsafe" => SafetyClass::Unsafe,
                "risky" => SafetyClass::Risky,
                _ => SafetyClass::Safe,
            };
            prop_assert_eq!(sf, expected_sf);
        }

        /// `None` always maps to the documented defaults (`Valid`, `Safe`).
        #[test]
        fn parse_class_helpers_none_uses_defaults(_seed in any::<u8>()) {
            prop_assert_eq!(parse_validity_class(None), ValidityClass::Valid);
            prop_assert_eq!(parse_safety_class(None), SafetyClass::Safe);
        }
    }
}
