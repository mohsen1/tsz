use std::fs;

fn namespace_static_side_block(source: &str) -> &str {
    let start = source
        .find("let derived_sym = self.ctx.binder.get_node_symbol(class_idx);")
        .expect("expected namespace static-side branch");
    let rest = &source[start..];
    let end = rest
        .find("self.pop_type_parameters(derived_type_param_updates);")
        .expect("expected branch end");
    &rest[..end]
}

#[test]
fn class_namespace_static_side_diagnostic_uses_relation_outcome_boundary() {
    let source = fs::read_to_string("src/classes/class_checker.rs")
        .expect("failed to read class checker source");
    let branch = namespace_static_side_block(&source);

    assert!(
        branch.contains("assign_relation_outcome(derived_ctor_type, base_ctor_type)"),
        "namespace-merged class static-side TS2417 check should route through relation outcome boundary"
    );
    assert!(
        !branch.contains("!self.is_assignable_to(derived_ctor_type, base_ctor_type)"),
        "namespace-merged class static-side TS2417 check should not use a raw boolean assignability gate"
    );
}
