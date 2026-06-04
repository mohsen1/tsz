/// Regression: when the same namespace member is declared in two sibling lib
/// files (the parent namespace already merges across files), the global merge
/// must collapse the nested declarations into one symbol carrying both
/// declarations. Without this, property lookup on the namespace-scoped type
/// reports a different shape depending on which lib's declaration won the
/// initial allocation race — mirroring the real-world regression where
/// `Intl.ResolvedDateTimeFormatOptions` was split between
/// `lib.es5.d.ts` (carrying `calendar`, `numberingSystem`, ...) and
/// `lib.es2021.intl.d.ts` (carrying `dateStyle`, `formatMatcher`, ...) and
/// only one half survived in the merged shape.
#[test]
fn lib_merge_collapses_same_named_nested_interfaces_across_lib_files() {
    fn assert_merged(namespace: &str, type_name: &str, members_a: &str, members_b: &str) {
        let src_a =
            format!("declare namespace {namespace} {{ interface {type_name} {{ {members_a} }} }}");
        let src_b =
            format!("declare namespace {namespace} {{ interface {type_name} {{ {members_b} }} }}");

        let lib_files = vec![
            std::sync::Arc::new(crate::lib_loader::LibFile::from_source(
                "lib.a.d.ts".to_string(),
                src_a,
            )),
            std::sync::Arc::new(crate::lib_loader::LibFile::from_source(
                "lib.b.d.ts".to_string(),
                src_b,
            )),
        ];

        let program = merge_bind_results(parse_and_bind_parallel_with_libs(
            vec![("main.ts".to_string(), String::new())],
            &lib_files,
        ));

        let ns_id = program
            .globals
            .get(namespace)
            .unwrap_or_else(|| panic!("namespace {namespace} should be a global lib symbol"));
        let ns_sym = program
            .symbols
            .get(ns_id)
            .unwrap_or_else(|| panic!("namespace {namespace} symbol must exist"));

        let exports = ns_sym
            .exports
            .as_ref()
            .unwrap_or_else(|| panic!("namespace {namespace} must have exports"));
        let type_id = exports
            .get(type_name)
            .unwrap_or_else(|| panic!("{namespace}.{type_name} must be exported"));

        // The merged interface symbol must carry distinct declarations from
        // BOTH lib files. `symbol.declarations` deduplicates by raw NodeIndex
        // which can coincide across arenas, so verify via the (symbol, decl)
        // → arenas map: two declarations from sibling lib files must show up
        // as either two NodeIndex entries or one NodeIndex entry with two
        // arenas.
        let type_sym = program
            .symbols
            .get(type_id)
            .unwrap_or_else(|| panic!("{namespace}.{type_name} symbol must exist"));
        let total_arena_decls: usize = type_sym
            .declarations
            .iter()
            .map(|&decl_idx| {
                program
                    .declaration_arenas
                    .get(&(type_id, decl_idx))
                    .map_or(0, |v| v.len())
            })
            .sum();
        assert_eq!(
            total_arena_decls,
            2,
            "{namespace}.{type_name} should hold both lib declarations across arenas, \
             got declarations={:?} and arena counts={:?}",
            type_sym.declarations,
            type_sym
                .declarations
                .iter()
                .map(|&d| program
                    .declaration_arenas
                    .get(&(type_id, d))
                    .map_or(0, |v| v.len()))
                .collect::<Vec<_>>(),
        );
    }

    // Original repro: `Intl.ResolvedDateTimeFormatOptions` split across two libs.
    assert_merged(
        "Intl",
        "ResolvedDateTimeFormatOptions",
        "calendar: string; numberingSystem: string;",
        "dateStyle?: string; formatMatcher?: string;",
    );

    // Vary the namespace and type names to prove the rule isn't keyed on
    // any particular spelling.
    assert_merged(
        "MyOwnNS",
        "Config",
        "host: string; port: number;",
        "useTls: boolean; retries: number;",
    );

    // Renamed sibling namespace under a different alias.
    assert_merged(
        "Reflect2",
        "Capability",
        "read: boolean;",
        "write: boolean;",
    );
}

/// Negative case for the nested-merge rule: two `Foo` interfaces nested
/// inside *different* namespaces must remain distinct, even after the merge.
/// Without keying on the parent's global id, both `Foo`s would collapse and
/// `NsA.Foo` would gain members from `NsB.Foo`.
#[test]
fn lib_merge_does_not_collapse_nested_interfaces_under_different_namespaces() {
    let lib_files = vec![
        std::sync::Arc::new(crate::lib_loader::LibFile::from_source(
            "lib.a.d.ts".to_string(),
            "declare namespace NsA { interface Foo { onlyA: string; } }".to_string(),
        )),
        std::sync::Arc::new(crate::lib_loader::LibFile::from_source(
            "lib.b.d.ts".to_string(),
            "declare namespace NsB { interface Foo { onlyB: number; } }".to_string(),
        )),
    ];

    let program = merge_bind_results(parse_and_bind_parallel_with_libs(
        vec![("main.ts".to_string(), String::new())],
        &lib_files,
    ));

    let lookup = |ns: &str, type_name: &str| -> SymbolId {
        let ns_id = program.globals.get(ns).expect("namespace");
        let ns_sym = program.symbols.get(ns_id).expect("namespace symbol");
        ns_sym
            .exports
            .as_ref()
            .expect("exports")
            .get(type_name)
            .expect("type export")
    };

    let a_id = lookup("NsA", "Foo");
    let b_id = lookup("NsB", "Foo");
    assert_ne!(
        a_id, b_id,
        "NsA.Foo and NsB.Foo must remain distinct symbols; \
         the nested-merge key (global parent id, name) must scope them by parent",
    );

    let a_sym = program.symbols.get(a_id).expect("NsA.Foo");
    let b_sym = program.symbols.get(b_id).expect("NsB.Foo");
    let a_decl_count: usize = a_sym
        .declarations
        .iter()
        .map(|&d| {
            program
                .declaration_arenas
                .get(&(a_id, d))
                .map_or(0, |v| v.len())
        })
        .sum();
    let b_decl_count: usize = b_sym
        .declarations
        .iter()
        .map(|&d| {
            program
                .declaration_arenas
                .get(&(b_id, d))
                .map_or(0, |v| v.len())
        })
        .sum();
    assert_eq!(a_decl_count, 1, "NsA.Foo should hold one declaration");
    assert_eq!(b_decl_count, 1, "NsB.Foo should hold one declaration");
}

#[test]
fn test_skeleton_index_estimated_size_bytes_is_nonzero() {
    let files = vec![
        ("a.ts".to_string(), "export const a = 1;".to_string()),
        ("b.ts".to_string(), "export const b = 2;".to_string()),
        (
            "c.ts".to_string(),
            "export * from './a'; export { b } from './b';".to_string(),
        ),
    ];

    let bind_results = parse_and_bind_parallel(files);
    let program = merge_bind_results(bind_results);

    let stats = program.residency_stats();
    assert!(stats.has_skeleton_index);
    assert!(
        stats.skeleton_estimated_size_bytes > 0,
        "skeleton index should report nonzero estimated size, got 0"
    );
    // The estimate should at least cover the base struct size
    assert!(
        stats.skeleton_estimated_size_bytes >= std::mem::size_of::<SkeletonIndex>(),
        "skeleton size estimate ({}) should be >= struct size ({})",
        stats.skeleton_estimated_size_bytes,
        std::mem::size_of::<SkeletonIndex>()
    );
}

#[test]
fn test_skeleton_index_estimated_size_grows_with_content() {
    // Small project
    let small_files = vec![("a.ts".to_string(), "export const a = 1;".to_string())];
    let small_results = parse_and_bind_parallel(small_files);
    let small_program = merge_bind_results(small_results);
    let small_size = small_program
        .skeleton_index
        .as_ref()
        .unwrap()
        .estimated_size_bytes();

    // Larger project with more symbols and cross-file relationships
    let large_files = vec![
        (
            "a.ts".to_string(),
            "export const a1 = 1; export const a2 = 2; export const a3 = 3;".to_string(),
        ),
        (
            "b.ts".to_string(),
            "export const b1 = 1; export const b2 = 2; export const b3 = 3;".to_string(),
        ),
        (
            "c.ts".to_string(),
            "export * from './a'; export * from './b';".to_string(),
        ),
        (
            "d.ts".to_string(),
            "export { a1, a2 } from './a'; export { b1 } from './b';".to_string(),
        ),
    ];
    let large_results = parse_and_bind_parallel(large_files);
    let large_program = merge_bind_results(large_results);
    let large_size = large_program
        .skeleton_index
        .as_ref()
        .unwrap()
        .estimated_size_bytes();

    assert!(
        large_size > small_size,
        "larger project skeleton ({large_size} bytes) should be bigger than small ({small_size} bytes)"
    );
}

#[test]
fn test_bind_result_estimated_size_bytes_is_nonzero() {
    let result = parse_and_bind_single("a.ts".to_string(), "export const a = 1;".to_string());
    let size = result.estimated_size_bytes();
    assert!(
        size > 0,
        "estimated_size_bytes should be nonzero for any bind result"
    );
    // Must be at least the struct size itself
    assert!(
        size >= std::mem::size_of::<BindResult>(),
        "estimated size ({}) should be >= struct size ({})",
        size,
        std::mem::size_of::<BindResult>()
    );
}

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
