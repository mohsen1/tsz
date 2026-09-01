use std::{fs, path::PathBuf};

#[test]
fn polymorphic_this_receiver_uses_relation_outcome_boundary() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("src/assignability/polymorphic_this_diagnostics.rs");
    let source = fs::read_to_string(&path)
        .unwrap_or_else(|err| panic!("failed to read {}: {err}", path.display()));
    let branch = source
        .split("let receiver_type = self.get_type_of_node(access.expression);")
        .nth(1)
        .expect("missing polymorphic this receiver relation branch")
        .split("let default_atom = self.ctx.types.intern_string(\"default\");")
        .next()
        .expect("missing end of polymorphic this receiver relation branch");
    let compact: String = branch.chars().filter(|c| !c.is_whitespace()).collect();

    assert!(
        compact.contains(
            "self.polymorphic_this_receiver_relation_outcome(receiver_type,target).related"
        ),
        "receiver relation should use the polymorphic-this receiver request"
    );
    assert!(
        compact.contains("self.polymorphic_this_receiver_relation_outcome(member,target).related"),
        "intersection member relation should use the polymorphic-this receiver request"
    );
    assert!(
        !compact.contains("assign_relation_outcome(receiver_type,target)")
            && !compact.contains("assign_relation_outcome(member,target)"),
        "polymorphic this receiver branch should not use generic assignment relation outcomes"
    );
    assert!(
        !branch.contains("diagnostic_relation_boolean_guard"),
        "polymorphic this receiver branch should not use the legacy boolean guard"
    );
}
