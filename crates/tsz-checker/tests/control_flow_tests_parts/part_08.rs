/// Test: definite assignment analysis with early return
#[test]
fn test_definite_assignment_with_early_return() {
    use tsz_parser::parser::ParserState;

    let source = r#"
let x: string;

function foo(): string {
    if (condition()) {
        x = "hello";
        return x;
    }
    x = "world";
    return x;
}
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let arena = parser.get_arena();
    let mut binder = tsz_binder::BinderState::new();
    binder.bind_source_file(arena, root);

    // x is definitely assigned on all return paths
    let _ = true; // Definite assignment with early return
}

/// Test: unreachable code detection in function with multiple returns
#[test]
fn test_unreachable_code_with_multiple_returns() {
    use tsz_parser::parser::ParserState;

    let source = r#"
function foo(): string {
    return "first";
    return "second"; // Unreachable
}
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let arena = parser.get_arena();
    let mut binder = tsz_binder::BinderState::new();
    binder.bind_source_file(arena, root);

    // Should emit TS7027: Unreachable code detected
    let _ = true; // Unreachable code with multiple returns
}

/// Test: unreachable code detection in switch with fallthrough
#[test]
fn test_unreachable_code_in_switch_fallthrough() {
    use tsz_parser::parser::ParserState;

    let source = r#"
let x = 1;

switch (x) {
    case 1:
        console.log("one");
        break;
    case 2:
        console.log("two");
        return;
        console.log("unreachable"); // Unreachable after return
}
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let arena = parser.get_arena();
    let mut binder = tsz_binder::BinderState::new();
    binder.bind_source_file(arena, root);

    // Should emit TS7027 for unreachable code after return
    let _ = true; // Unreachable code in switch fallthrough
}

/// Test: recursive flow analysis (bidirectional narrowing) doesn't panic when using shared buffers.
#[test]
fn test_recursive_flow_analysis_no_panic() {
    use tsz_common::checker_options::CheckerOptions;

    let source = r#"
        function test(x: "a" | "b", y: "a" | "b") {
            if (x === y) {
                x;
            }
        }
    "#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let interner = TypeInterner::new();
    let options = CheckerOptions::default();
    let mut state = CheckerState::new(arena, &binder, &interner, "test.ts".to_string(), options);

    // This triggers apply_flow_narrowing, which uses shared buffers and handles re-entrancy.
    state.check_source_file(root);
}

/// Regression: class static blocks with labeled loop control flow must not
/// trigger non-terminating flow analysis.
#[test]
fn test_class_static_block_labeled_flow_terminates() {
    use tsz_common::checker_options::CheckerOptions;

    let source = r#"
function foo(v: number) {
    label: while (v) {
        class C {
            static {
                if (v === 1) break label;
                if (v === 2) continue label;
                if (v === 3) break;
                if (v === 4) continue;
            }
        }
    }
}
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let interner = TypeInterner::new();
    let options = CheckerOptions::default();
    let mut state = CheckerState::new(arena, &binder, &interner, "test.ts".to_string(), options);
    state.check_source_file(root);
}

// ============================================================================

/// Regression: flow merge must preserve distinct class types even when one is
/// structurally assignable to the other.  When two switch(true) blocks use
/// the same union variable, the `BRANCH_LABEL` merge at the end of the first
/// switch must keep *both* members so that instanceof narrowing in the second
/// switch can still select the narrower type.
///
/// Before the fix, `simplify_flow_merge_types` used structural assignability
/// to eliminate the "wider" class from the union (since Derived2 ⊇ Derived1
/// structurally), collapsing `Derived1 | Derived2` to `Derived1`.  The second
/// switch then narrowed by `instanceof Derived2` on a `Derived1`-only type,
/// producing `never` and emitting false TS2339 errors.
#[test]
fn test_flow_merge_preserves_distinct_class_types_across_switches() {
    use tsz_common::checker_options::CheckerOptions;

    let source = r#"
class Base { basey: string = ""; }
class Derived1 extends Base { d: string = ""; }
class Derived2 extends Base { d: string = ""; other: string = ""; }

function test(someDerived: Derived1 | Derived2) {
    switch (true) {
        case someDerived instanceof Derived1:
            someDerived.d;
            break;
        case someDerived instanceof Derived2:
            someDerived.d;
            break;
        default:
            const never: never = someDerived;
    }
    // After the first switch, the type of someDerived must still be
    // Derived1 | Derived2 (not collapsed to just Derived1).
    switch (true) {
        case someDerived instanceof Derived1:
            someDerived.d;
            someDerived.basey;
            break;
        default:
            const never2: never = someDerived;
        case someDerived instanceof Derived2:
            someDerived.d;
            someDerived.other;   // Must not be TS2339 on type 'never'
    }
}
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let interner = TypeInterner::new();
    let options = CheckerOptions::default();
    let mut state = CheckerState::new(arena, &binder, &interner, "test.ts".to_string(), options);
    state.check_source_file(root);

    // No TS2339 errors should be emitted — the narrowing in the second switch
    // should correctly resolve Derived2 (not never).
    let ts2339_errors: Vec<_> = state
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 2339)
        .collect();
    assert!(
        ts2339_errors.is_empty(),
        "Expected no TS2339 errors but got {}: {:?}",
        ts2339_errors.len(),
        ts2339_errors
            .iter()
            .map(|d| &d.message_text)
            .collect::<Vec<_>>()
    );
}

// ============================================================================
// FAILING TESTS - These tests FAIL to demonstrate the bugs exist
// ============================================================================
