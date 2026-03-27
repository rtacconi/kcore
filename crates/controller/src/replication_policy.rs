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
    match value.unwrap_or("valid").trim().to_ascii_lowercase().as_str() {
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
