#[test]
fn test_duplicate_identifier_type_alias_2300() {
    use crate::checker::diagnostics::diagnostic_codes;

    let source = r#"
type Foo = { x: number };
type Foo = { y: number };

type Bar = { x: number };
interface Bar { y: number; }
"#;

    let (parser, root) = parse_test_source(source);
    assert!(
        parser.get_diagnostics().is_empty(),
        "Parse errors: {:?}",
        parser.get_diagnostics()
    );

    let mut binder = BinderState::new();
    merge_shared_lib_symbols(&mut binder);
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        crate::checker::context::CheckerOptions::default(),
    );
    setup_lib_contexts(&mut checker);
    checker.check_source_file(root);

    let duplicate_count = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == diagnostic_codes::DUPLICATE_IDENTIFIER)
        .count();
    assert_eq!(
        duplicate_count, 4,
        "Expected TS2300 for type alias conflicts, got: {:?}",
        checker.ctx.diagnostics
    );
}

/// Test TS2300: Duplicate identifier - duplicate enum members
#[test]
fn test_duplicate_identifier_enum_member_2300() {
    use crate::checker::diagnostics::diagnostic_codes;

    let source = r#"
enum Color {
    Red,
    Green,
    Blue,
    // Duplicate should emit TS2300
    Red,
}
"#;

    let (parser, root) = parse_test_source(source);
    assert!(
        parser.get_diagnostics().is_empty(),
        "Parse errors: {:?}",
        parser.get_diagnostics()
    );

    let mut binder = BinderState::new();
    merge_shared_lib_symbols(&mut binder);
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        crate::checker::context::CheckerOptions::default(),
    );
    setup_lib_contexts(&mut checker);
    checker.check_source_file(root);

    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&diagnostic_codes::DUPLICATE_IDENTIFIER),
        "Expected TS2300 for duplicate enum member 'Red', got: {codes:?}"
    );
}

#[test]
fn test_type_alias_with_function_no_duplicate_2300() {
    use crate::checker::diagnostics::diagnostic_codes;

    let source = r#"
type Foo = { x: number };
function Foo() {}
"#;

    let (parser, root) = parse_test_source(source);
    assert!(
        parser.get_diagnostics().is_empty(),
        "Parse errors: {:?}",
        parser.get_diagnostics()
    );

    let mut binder = BinderState::new();
    merge_shared_lib_symbols(&mut binder);
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        crate::checker::context::CheckerOptions::default(),
    );
    setup_lib_contexts(&mut checker);
    checker.check_source_file(root);

    let duplicate_count = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == diagnostic_codes::DUPLICATE_IDENTIFIER)
        .count();
    assert_eq!(
        duplicate_count, 0,
        "Did not expect TS2300 for type alias + function, got: {:?}",
        checker.ctx.diagnostics
    );
}

#[test]
fn test_class_accessor_pair_no_duplicate_2300() {
    use crate::checker::diagnostics::diagnostic_codes;

    let source = r#"
class Rectangle {
    private _width: number = 0;

    get width(): number {
        return this._width;
    }

    set width(value: number) {
        this._width = value;
    }
}
"#;

    let (parser, root) = parse_test_source(source);
    assert!(
        parser.get_diagnostics().is_empty(),
        "Parse errors: {:?}",
        parser.get_diagnostics()
    );

    let mut binder = BinderState::new();
    merge_shared_lib_symbols(&mut binder);
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        crate::checker::context::CheckerOptions::default(),
    );
    setup_lib_contexts(&mut checker);
    checker.check_source_file(root);

    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        !codes.contains(&diagnostic_codes::DUPLICATE_IDENTIFIER),
        "Did not expect TS2300 for getter/setter pair, got: {:?}",
        checker.ctx.diagnostics
    );
}

#[test]
fn test_class_duplicate_getter_2300() {
    use crate::checker::diagnostics::diagnostic_codes;

    let source = r#"
class Rectangle {
    get width(): number {
        return 1;
    }

    get width(): number {
        return 2;
    }
}
"#;

    let (parser, root) = parse_test_source(source);
    assert!(
        parser.get_diagnostics().is_empty(),
        "Parse errors: {:?}",
        parser.get_diagnostics()
    );

    let mut binder = BinderState::new();
    merge_shared_lib_symbols(&mut binder);
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        crate::checker::context::CheckerOptions::default(),
    );
    setup_lib_contexts(&mut checker);
    checker.check_source_file(root);

    let duplicate_count = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == diagnostic_codes::DUPLICATE_IDENTIFIER)
        .count();
    assert_eq!(
        duplicate_count, 1,
        "Expected 1 TS2300 for duplicate getter (only on second occurrence, matching tsc), got: {:?}",
        checker.ctx.diagnostics
    );
}

#[test]
fn test_overload_call_reports_no_overload_matches() {
    use crate::checker::diagnostics::diagnostic_codes;

    let source = r#"
function f(x: string): void;
function f(x: number, y: number): void;
function f(x: any, y?: any) {}
f(true);
"#;

    let (parser, root) = parse_test_source(source);

    let mut binder = BinderState::new();
    merge_shared_lib_symbols(&mut binder);
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        crate::checker::context::CheckerOptions::default(),
    );
    setup_lib_contexts(&mut checker);
    checker.check_source_file(root);

    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();
    // tsc reports TS2345 (not TS2769) when a single overload matches by arity — it picks the
    // best-match overload and reports the specific type mismatch on that signature.
    // TS2769 is only reported when multiple overloads match by arity but all fail.
    assert!(
        codes.contains(&2345) || codes.contains(&diagnostic_codes::NO_OVERLOAD_MATCHES_THIS_CALL),
        "Expected TS2345 or TS2769 for overload call mismatch, got: {codes:?}"
    );
}
