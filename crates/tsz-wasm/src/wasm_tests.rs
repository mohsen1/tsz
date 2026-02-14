use tsz_solver::TypeInterner;

#[test]
fn test_type_interner_basic() {
    use tsz_solver::TypeId;

    // Test the underlying TypeInterner directly (works on all targets)
    let interner = TypeInterner::new();

    // Should start empty (no user-defined types, only intrinsics)
    assert!(interner.is_empty());
    let initial_count = interner.len();
    assert_eq!(
        initial_count,
        TypeId::FIRST_USER as usize,
        "TypeInterner should have intrinsics"
    );

    // Intern a string
    let atom1 = interner.intern_string("hello");
    let atom2 = interner.intern_string("hello");
    assert_eq!(atom1, atom2); // Deduplication

    // Resolve the string
    let resolved = interner.resolve_atom(atom1);
    assert_eq!(resolved, "hello");

    // Intern a literal type - this should make it non-empty
    let _str_type = interner.literal_string("test");
    assert!(!interner.is_empty());
    assert!(interner.len() > initial_count);
}

#[test]
fn test_parallel_parsing() {
    // Test the parallel parsing directly (works on all targets)
    let files = vec![
        ("a.ts".to_string(), "let x = 1;".to_string()),
        ("b.ts".to_string(), "let y = 2;".to_string()),
    ];

    let results = tsz::parallel::parse_files_parallel(files);
    assert_eq!(results.len(), 2);
}

#[test]
fn test_parallel_compile_and_check() {
    // Test the full pipeline directly (works on all targets)
    let files = vec![
        (
            "a.ts".to_string(),
            "function add(x: number, y: number): number { return x + y; }".to_string(),
        ),
        (
            "b.ts".to_string(),
            "function mul(x: number, y: number): number { return x * y; }".to_string(),
        ),
    ];

    let program = tsz::parallel::compile_files(files);
    assert_eq!(program.files.len(), 2);

    let (_result, stats) = tsz::parallel::check_functions_with_stats(&program);
    assert_eq!(stats.file_count, 2);
    assert!(stats.function_count >= 2);
}
