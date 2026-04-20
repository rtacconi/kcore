use crate::client::controller_proto::ApplyAction;

/// Render a reconcile summary for `kctl` commands that perform a server-side
/// upsert. `kind_and_name` should be a short human label such as `"VM 'web'"`
/// or `"network 'default' on node 'nodeA'"`.
///
/// Examples:
/// - `created VM 'web' (id abc)`
/// - `updated VM 'web' (fields: cpu, memory_bytes)`
/// - `unchanged VM 'web'`
pub fn render_apply_summary(action: i32, changed_fields: &[String], kind_and_name: &str) -> String {
    match ApplyAction::try_from(action).unwrap_or(ApplyAction::Unspecified) {
        ApplyAction::Created => format!("created {kind_and_name}"),
        ApplyAction::Updated => {
            if changed_fields.is_empty() {
                format!("updated {kind_and_name}")
            } else {
                format!(
                    "updated {kind_and_name} (fields: {})",
                    changed_fields.join(", ")
                )
            }
        }
        ApplyAction::Unchanged => format!("unchanged {kind_and_name}"),
        // Don't lie when the server omitted (or regressed) the action: print
        // a neutral summary instead of pretending the resource was created.
        ApplyAction::Unspecified => format!("applied {kind_and_name}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_created() {
        assert_eq!(
            render_apply_summary(ApplyAction::Created as i32, &[], "VM 'web'"),
            "created VM 'web'"
        );
    }

    #[test]
    fn renders_updated_with_fields() {
        let fields = vec!["cpu".to_string(), "memory_bytes".to_string()];
        assert_eq!(
            render_apply_summary(ApplyAction::Updated as i32, &fields, "VM 'web'"),
            "updated VM 'web' (fields: cpu, memory_bytes)"
        );
    }

    #[test]
    fn renders_updated_without_fields() {
        assert_eq!(
            render_apply_summary(ApplyAction::Updated as i32, &[], "VM 'web'"),
            "updated VM 'web'"
        );
    }

    #[test]
    fn renders_unchanged() {
        assert_eq!(
            render_apply_summary(ApplyAction::Unchanged as i32, &[], "VM 'web'"),
            "unchanged VM 'web'"
        );
    }

    #[test]
    fn unspecified_falls_back_to_neutral_applied() {
        assert_eq!(
            render_apply_summary(ApplyAction::Unspecified as i32, &[], "VM 'web'"),
            "applied VM 'web'"
        );
    }
}

/// Property-based tests (Phase 2) — `render_apply_summary`.
#[cfg(test)]
mod proptests {
    use super::{render_apply_summary, ApplyAction};
    use proptest::prelude::*;

    proptest! {
        #![proptest_config(ProptestConfig {
            cases: 2_000,
            .. ProptestConfig::default()
        })]

        /// Output never panics and always **contains** the
        /// `kind_and_name` substring (so the operator can grep the
        /// log for the resource).
        #[test]
        fn always_contains_kind_and_name(
            action in any::<i32>(),
            fields in proptest::collection::vec("[a-z_]{1,10}", 0..5),
            kind_and_name in "[a-zA-Z][a-zA-Z 0-9'-]{0,32}",
        ) {
            let out = render_apply_summary(action, &fields, &kind_and_name);
            prop_assert!(out.contains(&kind_and_name), "{out:?} missing {kind_and_name:?}");
        }

        /// Output is always one of the four documented action prefixes.
        #[test]
        fn output_starts_with_known_verb(
            action in any::<i32>(),
            fields in proptest::collection::vec("[a-z_]{1,10}", 0..3),
        ) {
            let out = render_apply_summary(action, &fields, "X");
            let known = out.starts_with("created ")
                || out.starts_with("updated ")
                || out.starts_with("unchanged ")
                || out.starts_with("applied ");
            prop_assert!(known, "unknown verb in {out:?}");
        }

        /// `Updated` action with fields always includes the
        /// comma-joined field list.
        #[test]
        fn updated_includes_fields(
            fields in proptest::collection::vec("[a-z_]{1,10}", 1..5),
        ) {
            let out = render_apply_summary(ApplyAction::Updated as i32, &fields, "X");
            for f in &fields {
                prop_assert!(out.contains(f.as_str()), "{out:?} missing field {f:?}");
            }
        }
    }
}
