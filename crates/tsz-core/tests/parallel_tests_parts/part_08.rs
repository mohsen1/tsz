#[test]
fn skeleton_validate_against_merged_shorthand_ambient() {
    // Shorthand ambient modules (declare module "x"; without body)
    // should match between skeleton and legacy merge.
    let files = vec![
        (
            "shorthands.d.ts".to_string(),
            r#"
            declare module "shorthand-a";
            declare module "shorthand-b";
            "#
            .to_string(),
        ),
        (
            "more.d.ts".to_string(),
            r#"declare module "shorthand-c";"#.to_string(),
        ),
    ];
    let results = parse_and_bind_parallel(files);
    let program = merge_bind_results(results);

    let idx = program.skeleton_index.as_ref().unwrap();
    assert_eq!(
        idx.shorthand_ambient_modules, program.shorthand_ambient_modules,
        "skeleton and legacy shorthand_ambient_modules must match"
    );
    // Verify actual content
    assert!(
        program.shorthand_ambient_modules.len() >= 3,
        "should have at least 3 shorthand ambient modules, got {}",
        program.shorthand_ambient_modules.len()
    );
}

#[test]
fn skeleton_validate_against_merged_module_export_specifiers() {
    // Module export specifiers (keys of module_exports from ambient declare module blocks)
    // should match between skeleton and legacy merge (after filtering user file names).
    let files = vec![
        (
            "types.d.ts".to_string(),
            r#"
            declare module "pkg-a" {
                export function foo(): void;
            }
            declare module "pkg-b" {
                export const bar: number;
            }
            "#
            .to_string(),
        ),
        ("user.ts".to_string(), "export const x = 1;".to_string()),
    ];
    let results = parse_and_bind_parallel(files);
    let program = merge_bind_results(results);

    // The merge validation already ran. Now verify the module_export_specifiers
    // in the skeleton contain the ambient module keys.
    let idx = program.skeleton_index.as_ref().unwrap();
    let has_pkg_a = idx
        .module_export_specifiers
        .iter()
        .any(|s| s.contains("pkg-a"));
    let has_pkg_b = idx
        .module_export_specifiers
        .iter()
        .any(|s| s.contains("pkg-b"));
    assert!(
        has_pkg_a,
        "skeleton should track module_export_specifier for pkg-a"
    );
    assert!(
        has_pkg_b,
        "skeleton should track module_export_specifier for pkg-b"
    );

    // Both legacy module_exports and skeleton module_export_specifiers
    // include user file names (from the binder's own module_exports for
    // external modules). The validation filters these out when comparing
    // ambient-module topology.
    assert!(
        program.module_exports.contains_key("user.ts"),
        "legacy module_exports should include user file name"
    );
}

#[test]
fn skeleton_validate_mixed_ambient_and_user_files() {
    // A realistic mix: ambient modules, shorthand modules, user files with exports,
    // and cross-file re-exports. The debug assertion in merge_bind_results
    // validates all three skeleton sets match the legacy merge.
    let files = vec![
        (
            "globals.d.ts".to_string(),
            r#"
            declare module "my-globals" {
                export interface Config { debug: boolean; }
            }
            declare module "*.css";
            "#
            .to_string(),
        ),
        (
            "lib.ts".to_string(),
            r#"
            export function helper() { return 42; }
            export const VERSION = "1.0";
            "#
            .to_string(),
        ),
        (
            "reexporter.ts".to_string(),
            r#"export { helper } from "./lib";"#.to_string(),
        ),
    ];
    let results = parse_and_bind_parallel(files);
    let program = merge_bind_results(results);

    // If merge_bind_results didn't panic, all skeleton validations passed.
    let idx = program.skeleton_index.as_ref().unwrap();

    // Verify skeleton metadata is coherent.
    assert_eq!(idx.file_count, 3);
    assert!(
        idx.total_reexport_count >= 1,
        "should have at least one re-export edge"
    );

    // Shorthand ambient for *.css
    let (exact, patterns) = idx.build_declared_module_sets();
    assert!(
        patterns.iter().any(|p| p == "*.css"),
        "should have wildcard pattern for *.css"
    );
    assert!(
        exact.iter().any(|e| e == "my-globals"),
        "should have exact declared module 'my-globals'"
    );
}

// =============================================================================
// Skeleton Fingerprinting Tests
// =============================================================================

#[test]
fn skeleton_fingerprint_deterministic_across_rebuilds() {
    let source = "let x = 1; export function foo(): number { return 42; }";
    let files1 = vec![("a.ts".to_string(), source.to_string())];
    let files2 = vec![("a.ts".to_string(), source.to_string())];

    let results1 = parse_and_bind_parallel(files1);
    let results2 = parse_and_bind_parallel(files2);

    let skel1 = extract_skeleton(&results1[0]);
    let skel2 = extract_skeleton(&results2[0]);

    assert_eq!(
        skel1.fingerprint, skel2.fingerprint,
        "identical source should produce identical skeleton fingerprints"
    );
    assert_ne!(
        skel1.fingerprint, 0,
        "fingerprint should not be zero for non-trivial files"
    );
}

#[test]
fn skeleton_fingerprint_changes_on_symbol_add() {
    let files_v1 = vec![("a.ts".to_string(), "let x = 1;".to_string())];
    let files_v2 = vec![("a.ts".to_string(), "let x = 1; let y = 2;".to_string())];

    let results_v1 = parse_and_bind_parallel(files_v1);
    let results_v2 = parse_and_bind_parallel(files_v2);

    let skel_v1 = extract_skeleton(&results_v1[0]);
    let skel_v2 = extract_skeleton(&results_v2[0]);

    assert_ne!(
        skel_v1.fingerprint, skel_v2.fingerprint,
        "adding a symbol should change the skeleton fingerprint"
    );
}

#[test]
fn skeleton_fingerprint_stable_when_body_changes() {
    // Changing a function body should NOT change the skeleton fingerprint,
    // since the skeleton only captures top-level symbol topology.
    let files_v1 = vec![(
        "a.ts".to_string(),
        "function foo(): number { return 1; }".to_string(),
    )];
    let files_v2 = vec![(
        "a.ts".to_string(),
        "function foo(): number { return 42; }".to_string(),
    )];

    let results_v1 = parse_and_bind_parallel(files_v1);
    let results_v2 = parse_and_bind_parallel(files_v2);

    let skel_v1 = extract_skeleton(&results_v1[0]);
    let skel_v2 = extract_skeleton(&results_v2[0]);

    assert_eq!(
        skel_v1.fingerprint, skel_v2.fingerprint,
        "changing a function body should not change the skeleton fingerprint"
    );
}

#[test]
fn skeleton_fingerprint_changes_on_export_toggle() {
    // Adding `export` to a declaration changes the skeleton
    // (is_exported flag flips).
    let files_v1 = vec![("a.ts".to_string(), "let x = 1;".to_string())];
    let files_v2 = vec![("a.ts".to_string(), "export let x = 1;".to_string())];

    let results_v1 = parse_and_bind_parallel(files_v1);
    let results_v2 = parse_and_bind_parallel(files_v2);

    let skel_v1 = extract_skeleton(&results_v1[0]);
    let skel_v2 = extract_skeleton(&results_v2[0]);

    assert_ne!(
        skel_v1.fingerprint, skel_v2.fingerprint,
        "toggling export should change the skeleton fingerprint"
    );
}

#[test]
fn skeleton_fingerprint_independent_of_file_name() {
    // Script files (no import/export) with the same source under different
    // file names should produce identical fingerprints.
    // Note: external modules (with export/import) include the file name in
    // `module_export_specifiers`, so their fingerprints legitimately differ.
    let source = "let x = 1;";
    let files_a = vec![("a.ts".to_string(), source.to_string())];
    let files_b = vec![("b.ts".to_string(), source.to_string())];

    let results_a = parse_and_bind_parallel(files_a);
    let results_b = parse_and_bind_parallel(files_b);

    let skel_a = extract_skeleton(&results_a[0]);
    let skel_b = extract_skeleton(&results_b[0]);

    assert_eq!(
        skel_a.fingerprint, skel_b.fingerprint,
        "fingerprint should be independent of file name for script files"
    );
    assert_ne!(skel_a.file_name, skel_b.file_name);
}

#[test]
fn skeleton_fingerprint_changes_on_declared_module() {
    let files_v1 = vec![("a.d.ts".to_string(), "declare const x: number;".to_string())];
    let files_v2 = vec![(
        "a.d.ts".to_string(),
        r#"declare const x: number; declare module "foo" { export const y: string; }"#.to_string(),
    )];

    let results_v1 = parse_and_bind_parallel(files_v1);
    let results_v2 = parse_and_bind_parallel(files_v2);

    let skel_v1 = extract_skeleton(&results_v1[0]);
    let skel_v2 = extract_skeleton(&results_v2[0]);

    assert_ne!(
        skel_v1.fingerprint, skel_v2.fingerprint,
        "adding a declared module should change the fingerprint"
    );
}

#[test]
fn skeleton_compute_fingerprint_matches_stored() {
    // Verify that recomputing the fingerprint yields the same value
    // as the one stored at extraction time.
    let files = vec![(
        "a.ts".to_string(),
        "export interface Foo { x: number; }".to_string(),
    )];
    let results = parse_and_bind_parallel(files);
    let skel = extract_skeleton(&results[0]);

    assert_eq!(
        skel.fingerprint,
        skel.compute_fingerprint(),
        "stored fingerprint must match recomputed fingerprint"
    );
}

// =============================================================================
// SkeletonIndex aggregate fingerprint tests
// =============================================================================

