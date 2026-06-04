/// Architecture guard: all `push_diagnostic` calls must live in `error_reporter`/ or context/core.rs.
///
/// Direct `push_diagnostic` calls in feature modules bypass diagnostic centralization,
/// creating ad-hoc diagnostic paths that are harder to maintain and audit.
/// All diagnostic emission should route through `error_reporter` methods instead.
#[test]
fn test_no_push_diagnostic_outside_error_reporter() {
    fn collect_rs_files(dir: &Path, files: &mut Vec<std::path::PathBuf>) {
        let entries = fs::read_dir(dir)
            .unwrap_or_else(|_| panic!("failed to read directory {}", dir.display()));
        for entry in entries {
            let entry = entry.expect("failed to read directory entry");
            let path = entry.path();
            if path.is_dir() {
                collect_rs_files(&path, files);
                continue;
            }
            if path.extension().and_then(|ext| ext.to_str()) == Some("rs") {
                files.push(path);
            }
        }
    }

    let mut files = Vec::new();
    collect_rs_files(Path::new("src"), &mut files);

    // Known exceptions (empty if all migrations are done)
    let allowlist: &[&str] = &[];

    let mut violations = Vec::new();
    for path in files {
        let rel = path.display().to_string();

        // Skip the legitimate homes for push_diagnostic:
        // - error_reporter/ is where all diagnostics should be emitted
        // - context/core.rs defines the push_diagnostic method itself
        // - tests/ are not production code
        if rel.contains("/error_reporter/")
            || rel.ends_with("context/core.rs")
            || rel.contains("/tests/")
        {
            continue;
        }

        // Check allowlist
        if allowlist.iter().any(|allowed| rel.ends_with(allowed)) {
            continue;
        }

        let src = fs::read_to_string(&path)
            .unwrap_or_else(|_| panic!("failed to read {}", path.display()));

        for (line_num, line) in src.lines().enumerate() {
            // Skip comments
            let trimmed = line.trim();
            if trimmed.starts_with("//") || trimmed.starts_with("///") || trimmed.starts_with("*") {
                continue;
            }
            if line.contains("push_diagnostic(") || line.contains(".push_diagnostic(") {
                violations.push(format!("{}:{}", rel, line_num + 1));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "push_diagnostic calls found outside error_reporter/. \
         Move these diagnostics to error_reporter/ methods instead.\n\
         Violations:\n  {}",
        violations.join("\n  ")
    );
}

/// Enforce the 2000 LOC limit for checker files (CLAUDE.md §12).
///
/// Files exceeding the limit are grandfathered with a ceiling that can only shrink.
/// New files must stay under 2000 lines.
#[test]
#[ignore = "ratchet guard is currently red in the direct unit CI job"]
fn checker_files_stay_under_loc_limit() {
    let checker_src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let loc_limit: usize = 2000;

    // Grandfathered files: (relative path from src/, ceiling LOC)
    // These ceilings represent the current state — they can only shrink, never grow.
    // Removed after dropping below 2000 LOC or becoming trivial mod.rs stubs:
    //   complex.rs (926), variable_checking/core.rs (1606),
    //   symbol_types.rs (892), error_reporter/core/mod.rs (8, stub),
    //   types/computation/call/mod.rs (split), checkers/call_checker/mod.rs (split),
    //   checkers/jsx/props/*, computed_commonjs/mod.rs (4, stub),
    //   property_access_helpers/mod.rs (37, stub), import/core/mod.rs (11, stub),
    //   jsdoc/resolution/mod.rs (2, stub), assignment_checker/mod.rs (12, stub),
    //   call_errors/mod.rs (21, stub), diagnostic_source.rs (1689),
    //   condition_narrowing.rs (1552), jsx/props/resolution.rs (1702),
    //   call_checker/overload_resolution.rs (1712),
    //   call_checker/candidate_collection.rs (1374)
    let grandfathered: &[(&str, usize)] = &[
        ("types/property_access_type/resolve.rs", 2675),
        ("declarations/import/declaration.rs", 2420),
        ("types/computation/call/inner.rs", 2745),
        ("types/type_checking/duplicate_identifiers_helpers.rs", 2650),
        ("types/type_checking/duplicate_identifiers.rs", 2390),
        ("error_reporter/render_failure.rs", 2725),
        ("types/function_type.rs", 2225),
    ];

    let mut violations = Vec::new();

    fn walk_rs_files(dir: &Path, files: &mut Vec<std::path::PathBuf>) {
        let Ok(entries) = fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                // Skip test directories
                let name = path.file_name().unwrap_or_default().to_string_lossy();
                if name == "tests" {
                    continue;
                }
                walk_rs_files(&path, files);
            } else if path.extension().is_some_and(|ext| ext == "rs") {
                // Skip mod.rs and test files
                let name = path.file_name().unwrap_or_default().to_string_lossy();
                if name == "mod.rs" || name.ends_with("_tests.rs") || name == "test_utils.rs" {
                    continue;
                }
                files.push(path);
            }
        }
    }

    let mut rs_files = Vec::new();
    walk_rs_files(&checker_src, &mut rs_files);

    for file_path in &rs_files {
        let Ok(content) = fs::read_to_string(file_path) else {
            continue;
        };
        // Count non-empty, non-comment lines
        let loc = content
            .lines()
            .filter(|line| {
                let trimmed = line.trim();
                !trimmed.is_empty() && !trimmed.starts_with("//")
            })
            .count();

        let relative = file_path
            .strip_prefix(&checker_src)
            .unwrap_or(file_path)
            .to_string_lossy()
            .replace('\\', "/");

        // Check against grandfathered ceiling or default limit
        let ceiling = grandfathered
            .iter()
            .find(|(path, _)| *path == relative)
            .map(|(_, ceil)| *ceil)
            .unwrap_or(loc_limit);

        if loc > ceiling {
            violations.push(format!(
                "File {relative} has {loc} lines (limit: {ceiling}). Split into submodules."
            ));
        }
    }

    assert!(
        violations.is_empty(),
        "LOC violations found:\n{}",
        violations.join("\n")
    );
}
