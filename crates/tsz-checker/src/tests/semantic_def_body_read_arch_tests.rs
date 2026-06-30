use std::fs;

#[test]
fn semantic_def_body_consumers_use_resolver_adapter() {
    for path in [
        "src/assignability/assignability_eval.rs",
        "src/state/state_checking/property/excess_property_tail.rs",
        "src/state/type_resolution/reference_type_params.rs",
    ] {
        let source = fs::read_to_string(path)
            .unwrap_or_else(|err| panic!("failed to read {path} for architecture guard: {err}"));
        assert!(
            !source.contains("definition_store.get_body"),
            "{path} should use CheckerContext::get_semantic_def_body for semantic body reads"
        );
        assert!(
            source.contains("get_semantic_def_body("),
            "{path} should route semantic DefId body reads through the resolver adapter"
        );
    }
}
