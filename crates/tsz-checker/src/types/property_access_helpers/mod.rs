//! Helper methods for property access type resolution.
//!
//! Contains expando function/property detection, union/type-parameter property
//! checks, strict bind/call/apply method synthesis, and import.meta CJS checks.
//!
//! Extracted from `property_access_type/mod.rs` to keep module size manageable.

mod access_semantics;
mod expando;
mod iterator_methods;

#[cfg(test)]
mod tests {
    use crate::context::CheckerOptions;

    fn get_diagnostics(source: &str) -> Vec<(u32, String)> {
        crate::test_utils::check_with_options(
            source,
            CheckerOptions {
                no_property_access_from_index_signature: true,
                ..CheckerOptions::default()
            },
        )
        .into_iter()
        .map(|d| (d.code, d.message_text))
        .collect()
    }

    #[test]
    fn explicit_property_in_intersection_suppresses_ts4111() {
        let diagnostics = get_diagnostics(
            r#"
type Bag = { foo: string } & { [k: string]: string };
declare const bag: Bag;
bag.foo;
"#,
        );

        let ts4111 = diagnostics
            .iter()
            .filter(|(code, _)| *code == 4111)
            .collect::<Vec<_>>();
        assert!(
            ts4111.is_empty(),
            "Explicit properties in intersections should not be treated as pure index-signature access. Got: {diagnostics:?}"
        );
    }
}
