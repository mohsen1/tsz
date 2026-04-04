//! Helper methods for property access type resolution.
//!
//! Contains expando function/property detection, union/type-parameter property
//! checks, strict bind/call/apply method synthesis, and import.meta CJS checks.
//!
//! Extracted from `property_access_type.rs` to keep module size manageable.

mod access_semantics;
mod expando;

#[cfg(test)]
mod tests {
    use crate::context::CheckerOptions;
    use crate::query_boundaries::type_construction::TypeInterner;
    use crate::state::CheckerState;
    use tsz_binder::BinderState;
    use tsz_parser::parser::ParserState;

    fn get_diagnostics(source: &str) -> Vec<(u32, String)> {
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let mut binder = BinderState::new();
        binder.bind_source_file(parser.get_arena(), root);

        let types = TypeInterner::new();
        let mut checker = CheckerState::new(
            parser.get_arena(),
            &binder,
            &types,
            "test.ts".to_string(),
            CheckerOptions {
                no_property_access_from_index_signature: true,
                ..CheckerOptions::default()
            },
        );

        checker.check_source_file(root);

        checker
            .ctx
            .diagnostics
            .iter()
            .map(|d| (d.code, d.message_text.clone()))
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
