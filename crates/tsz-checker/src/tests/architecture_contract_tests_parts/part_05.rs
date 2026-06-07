/// Compiler-managed-name checks are semantic predicates and should have a
/// domain query boundary instead of being allowlisted as direct solver access.
#[test]
fn test_compiler_managed_name_predicate_has_domain_boundary() {
    let common_source = fs::read_to_string("src/query_boundaries/common.rs")
        .expect("failed to read query_boundaries/common.rs");
    let predicates_source = fs::read_to_string("src/query_boundaries/type_predicates.rs")
        .expect("failed to read query_boundaries/type_predicates.rs");
    let import_guard_source =
        fs::read_to_string("src/tests/architecture_contract_tests_parts/part_01.rs")
            .expect("failed to read architecture_contract_tests_parts/part_01.rs");

    assert!(
        !common_source.contains("fn is_compiler_managed_type("),
        "is_compiler_managed_type belongs in query_boundaries::type_predicates, not common.rs"
    );
    assert!(
        predicates_source.contains("fn is_compiler_managed_type("),
        "query_boundaries::type_predicates must own is_compiler_managed_type"
    );
    assert!(
        !import_guard_source.contains("\"is_compiler_managed_type\""),
        "is_compiler_managed_type must not be a direct solver import allowlist entry"
    );

    fn walk_rs(dir: &Path, files: &mut Vec<std::path::PathBuf>) {
        let Ok(entries) = fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk_rs(&path, files);
            } else if path.extension().is_some_and(|ext| ext == "rs") {
                files.push(path);
            }
        }
    }

    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files = Vec::new();
    walk_rs(&src, &mut files);
    let mut violations = Vec::new();
    for file in files {
        let rel = file.strip_prefix(&src).unwrap_or(&file);
        if rel.starts_with("query_boundaries") || rel.starts_with("tests") {
            continue;
        }
        let source = fs::read_to_string(&file).expect("failed to read checker source");
        if source.contains("tsz_solver::is_compiler_managed_type")
            || source.contains("use tsz_solver::is_compiler_managed_type")
        {
            violations.push(rel.display().to_string());
        }
    }
    assert!(
        violations.is_empty(),
        "checker code must call query_boundaries::type_predicates::is_compiler_managed_type; violations: {violations:?}"
    );
}
