use std::fs;
use std::path::Path;

#[test]
fn jsx_props_validation_uses_relation_outcome_boundary() {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let source_path = Path::new(manifest_dir).join("src/checkers/jsx/props/validation.rs");
    let source = fs::read_to_string(source_path).expect("read JSX props validation source");

    assert!(
        !source.contains("jsx_props_relation_outcome("),
        "JSX props validation relation probes should route through the JSX props boundary helper"
    );
    assert!(
        source.contains("jsx::props_are_assignable(")
            || source.contains("checkers::jsx::props_are_assignable("),
        "JSX props validation should use query_boundaries::checkers::jsx::props_are_assignable"
    );
    assert!(
        !source.contains("diagnostic_relation_boolean_guard("),
        "JSX props validation should not regress to raw boolean relation guards"
    );
}

#[test]
fn jsx_props_relation_outcome_uses_jsx_props_request() {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let source_path = Path::new(manifest_dir).join("src/assignability/relation_outcome_helpers.rs");
    let source = fs::read_to_string(source_path).expect("read relation outcome helpers source");

    assert!(
        source.contains("fn jsx_props_relation_outcome")
            && source.contains("RelationRequest::jsx_props(source, target)"),
        "JSX props diagnostics should have a request-shaped RelationKind::JsxProps helper"
    );
}

#[test]
fn jsx_generic_managed_attrs_uses_jsx_props_boundary() {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let source_path = Path::new(manifest_dir).join("src/checkers/jsx/props/attr_check_pipeline.rs");
    let source = fs::read_to_string(source_path).expect("read JSX attr check pipeline source");

    let function_start = source
        .find("fn emit_jsx_generic_managed_attrs_assignability")
        .expect("find generic managed attrs helper");
    let function = &source[function_start..];

    assert!(
        function.contains("jsx::props_are_assignable("),
        "generic managed attrs final assignability should use the JSX props boundary"
    );
    assert!(
        !function.contains("jsx::types_are_assignable("),
        "generic managed attrs final assignability should not use the generic JSX assignment helper"
    );
}

#[test]
fn jsx_checker_props_boolean_probes_use_domain_boundary() {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let jsx_dir = Path::new(manifest_dir).join("src/checkers/jsx");
    let mut violations = Vec::new();
    collect_jsx_props_relation_violations(&jsx_dir, &jsx_dir, &mut violations);

    assert!(
        violations.is_empty(),
        "JSX checker props boolean probes should use query_boundaries::checkers::jsx::props_are_assignable; violations: {violations:?}"
    );
}

fn collect_jsx_props_relation_violations(dir: &Path, root: &Path, violations: &mut Vec<String>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_jsx_props_relation_violations(&path, root, violations);
            continue;
        }
        if path.extension().is_none_or(|ext| ext != "rs") {
            continue;
        }
        let source = fs::read_to_string(&path).expect("read JSX checker source");
        if source.contains("jsx_props_relation_outcome(") {
            let rel = path.strip_prefix(root).unwrap_or(&path);
            violations.push(rel.display().to_string());
        }
    }
}
