#[test]
fn test_bind_result_estimated_size_grows_with_content() {
    let small = parse_and_bind_single("s.ts".to_string(), "const x = 1;".to_string());
    let small_size = small.estimated_size_bytes();

    let large_source = (0..50)
        .map(|i| format!("export function fn{i}(a: number, b: string): boolean {{ return true; }}"))
        .collect::<Vec<_>>()
        .join("\n");
    let large = parse_and_bind_single("l.ts".to_string(), large_source);
    let large_size = large.estimated_size_bytes();

    assert!(
        large_size > small_size,
        "larger file ({large_size} bytes) should have bigger estimate than small file ({small_size} bytes)"
    );
}

#[test]
fn test_bind_result_estimated_size_accounts_for_flow_nodes() {
    // Code with control flow creates flow nodes
    let source = r#"
        function f(x: number) {
            if (x > 0) {
                return x;
            } else if (x < 0) {
                return -x;
            } else {
                return 0;
            }
        }
    "#;
    let result = parse_and_bind_single("flow.ts".to_string(), source.to_string());
    let size = result.estimated_size_bytes();

    // Simple file without control flow
    let simple = parse_and_bind_single("simple.ts".to_string(), "const x = 1;".to_string());
    let simple_size = simple.estimated_size_bytes();

    assert!(
        size > simple_size,
        "file with control flow ({size} bytes) should be larger than simple file ({simple_size} bytes)"
    );
}

#[test]
fn test_pre_merge_bind_total_bytes_equals_sum_of_individual() {
    let files = vec![
        ("a.ts".to_string(), "export const a = 1;".to_string()),
        (
            "b.ts".to_string(),
            "export function foo(x: number): string { return String(x); }".to_string(),
        ),
        (
            "c.ts".to_string(),
            "export interface Bar { name: string; value: number; }".to_string(),
        ),
    ];

    // Compute individual sizes before merge
    let bind_results = parse_and_bind_parallel(files);
    let individual_sum: usize = bind_results.iter().map(|r| r.estimated_size_bytes()).sum();

    let program = merge_bind_results(bind_results);
    let stats = program.residency_stats();

    assert_eq!(
        stats.pre_merge_bind_total_bytes, individual_sum,
        "pre_merge_bind_total_bytes ({}) should equal sum of individual BindResult sizes ({})",
        stats.pre_merge_bind_total_bytes, individual_sum
    );
    assert!(
        stats.pre_merge_bind_total_bytes > 0,
        "total should be nonzero for 3 files"
    );
}

#[test]
fn test_pre_merge_bind_total_bytes_scales_with_file_count() {
    let one_file = vec![("a.ts".to_string(), "export const a = 1;".to_string())];
    let three_files = vec![
        ("a.ts".to_string(), "export const a = 1;".to_string()),
        ("b.ts".to_string(), "export const b = 2;".to_string()),
        ("c.ts".to_string(), "export const c = 3;".to_string()),
    ];

    let prog1 = merge_bind_results(parse_and_bind_parallel(one_file));
    let prog3 = merge_bind_results(parse_and_bind_parallel(three_files));

    assert!(
        prog3.pre_merge_bind_total_bytes > prog1.pre_merge_bind_total_bytes,
        "3-file merge ({} bytes) should have larger pre-merge total than 1-file ({} bytes)",
        prog3.pre_merge_bind_total_bytes,
        prog1.pre_merge_bind_total_bytes
    );
}

#[test]
fn test_bound_file_estimated_size_bytes_nonzero() {
    let files = vec![
        ("a.ts".to_string(), "export const a = 1;".to_string()),
        (
            "b.ts".to_string(),
            "export function b(x: number) { if (x > 0) return x; return -x; }".to_string(),
        ),
    ];

    let bind_results = parse_and_bind_parallel(files);
    let program = merge_bind_results(bind_results);

    for file in &program.files {
        let size = file.estimated_size_bytes();
        assert!(
            size > 0,
            "BoundFile '{}' should have nonzero estimated size",
            file.file_name,
        );
    }
}

#[test]
fn test_bound_file_estimated_size_scales_with_complexity() {
    let files = vec![
        ("small.ts".to_string(), "const x = 1;".to_string()),
        (
            "large.ts".to_string(),
            r#"
            export function f(x: number, y: string) {
                if (x > 0) { return y.toUpperCase(); }
                else if (x < 0) { return y.toLowerCase(); }
                switch (y) {
                    case "a": return "A";
                    case "b": return "B";
                    default: return y;
                }
            }
            export const arr = [1, 2, 3];
            export interface Foo { bar: string; baz: number; }
            "#
            .to_string(),
        ),
    ];

    let bind_results = parse_and_bind_parallel(files);
    let program = merge_bind_results(bind_results);

    let small = program
        .files
        .iter()
        .find(|f| f.file_name.contains("small"))
        .unwrap();
    let large = program
        .files
        .iter()
        .find(|f| f.file_name.contains("large"))
        .unwrap();

    assert!(
        large.estimated_size_bytes() > small.estimated_size_bytes(),
        "larger file ({} bytes) should have bigger estimate than small file ({} bytes)",
        large.estimated_size_bytes(),
        small.estimated_size_bytes(),
    );
}

#[test]
fn test_residency_stats_total_bound_file_bytes_nonzero() {
    let files = vec![
        ("a.ts".to_string(), "export const a = 1;".to_string()),
        ("b.ts".to_string(), "export const b = 2;".to_string()),
    ];

    let bind_results = parse_and_bind_parallel(files);
    let program = merge_bind_results(bind_results);
    let stats = program.residency_stats();

    assert!(
        stats.total_bound_file_bytes > 0,
        "total_bound_file_bytes should be nonzero for a program with files"
    );

    // The total should equal the sum of individual file estimates
    let manual_sum: usize = program.files.iter().map(|f| f.estimated_size_bytes()).sum();
    assert_eq!(
        stats.total_bound_file_bytes, manual_sum,
        "total_bound_file_bytes should equal sum of per-file estimates"
    );
}

#[test]
fn test_residency_stats_unique_arena_estimated_bytes_nonzero() {
    let files = vec![
        ("a.ts".to_string(), "export const a = 1;".to_string()),
        (
            "b.ts".to_string(),
            "export function b(x: number): string { return String(x); }".to_string(),
        ),
    ];

    let bind_results = parse_and_bind_parallel(files);
    let program = merge_bind_results(bind_results);
    let stats = program.residency_stats();

    assert!(
        stats.unique_arena_estimated_bytes > 0,
        "unique_arena_estimated_bytes should be nonzero for a program with files"
    );

    // Arena size should be larger than the struct overhead alone
    assert!(
        stats.unique_arena_estimated_bytes > std::mem::size_of::<tsz_parser::parser::NodeArena>(),
        "arena estimate should exceed bare struct size when files have been parsed"
    );
}

// =============================================================================
// Skeleton extraction determinism and content validation
// =============================================================================

#[test]
fn skeleton_extraction_captures_exported_symbols() {
    let files = vec![(
        "module.ts".to_string(),
        r#"
export const API_KEY = "secret";
export function greet(name: string): string { return name; }
export class UserService {
    getUser() { return null; }
}
export enum Status { Active, Inactive }
"#
        .to_string(),
    )];

    let results = parse_and_bind_parallel(files);
    assert_eq!(results.len(), 1);

    let skeleton = extract_skeleton(&results[0]);
    assert_eq!(skeleton.file_name, "module.ts");

    // Should capture all exported symbols
    let names: Vec<&str> = skeleton.symbols.iter().map(|s| s.name.as_str()).collect();
    assert!(names.contains(&"API_KEY"), "missing API_KEY in {names:?}");
    assert!(names.contains(&"greet"), "missing greet in {names:?}");
    assert!(
        names.contains(&"UserService"),
        "missing UserService in {names:?}"
    );
    assert!(names.contains(&"Status"), "missing Status in {names:?}");
}

#[test]
fn skeleton_extraction_is_deterministic() {
    let source = r#"
export const a = 1;
export function b() {}
export class C {}
"#;

    let files = vec![("det.ts".to_string(), source.to_string())];
    let results = parse_and_bind_parallel(files);

    let skel1 = extract_skeleton(&results[0]);
    let skel2 = extract_skeleton(&results[0]);

    // Same input must produce identical skeletons
    assert_eq!(skel1.symbols.len(), skel2.symbols.len());
    assert_eq!(skel1.file_name, skel2.file_name);
    assert_eq!(
        skel1.fingerprint, skel2.fingerprint,
        "fingerprint must be deterministic"
    );
}

