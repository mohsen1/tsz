#[test]
fn test_ts7027_unreachable_code_after_throw() {
    // Test TS7027 for unreachable code after throw
    let source = r#"
function test1(): never {
    throw new Error("error");
    console.log("unreachable");  // Should error: TS7027
}

function test2(): number {
    throw new Error("error");
    return 1;  // Should error: TS7027
}
"#;

    let (parser, root) = parse_test_source(source);
    assert!(parser.get_diagnostics().is_empty());

    let mut binder = BinderState::new();
    merge_shared_lib_symbols(&mut binder);
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let opts = crate::checker::context::CheckerOptions {
        jsx_factory: "React.createElement".to_string(),
        jsx_fragment_factory: "React.Fragment".to_string(),
        allow_unreachable_code: Some(false),
        ..Default::default()
    };
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        opts,
    );
    setup_lib_contexts(&mut checker);
    checker.check_source_file(root);

    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();

    // Should have 2 TS7027 errors
    assert_eq!(
        codes.iter().filter(|&&c| c == 7027).count(),
        2,
        "Expected 2 TS7027 errors for unreachable code after throw, got: {codes:?}"
    );
}

#[test]
fn test_ts7027_unreachable_after_never_expression() {
    // Test TS7027 for unreachable code after never-type expressions
    let source = r#"
declare function fail(): never;

function test1(): number {
    fail();
    return 1;  // Should error: TS7027
}

function test2(): void {
    fail();
    console.log("unreachable");  // Should error: TS7027
}
"#;

    let (parser, root) = parse_test_source(source);
    assert!(parser.get_diagnostics().is_empty());

    let mut binder = BinderState::new();
    merge_shared_lib_symbols(&mut binder);
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let opts = crate::checker::context::CheckerOptions {
        jsx_factory: "React.createElement".to_string(),
        jsx_fragment_factory: "React.Fragment".to_string(),
        allow_unreachable_code: Some(false),
        ..Default::default()
    };
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        opts,
    );
    setup_lib_contexts(&mut checker);
    checker.check_source_file(root);

    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();

    // Should have 2 TS7027 errors
    assert_eq!(
        codes.iter().filter(|&&c| c == 7027).count(),
        2,
        "Expected 2 TS7027 errors for unreachable code after never expression, got: {codes:?}"
    );
}

#[test]
fn test_ts2366_conditional_returns_all_paths() {
    // Test that functions with conditional returns that cover all paths don't error
    let source = r#"
function test1(flag: boolean): number {
    if (flag) {
        return 1;
    } else {
        return 2;
    }
}

function test2(x: number): string {
    if (x > 0) {
        return "positive";
    } else if (x < 0) {
        return "negative";
    } else {
        return "zero";
    }
}

function test3(x: number): number {
    switch (x) {
        case 1:
            return 1;
        case 2:
            return 2;
        default:
            return 0;
    }
}
"#;

    let (parser, root) = parse_test_source(source);
    assert!(parser.get_diagnostics().is_empty());

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

    // Should have no TS2366 errors - all paths return
    assert_eq!(
        codes.iter().filter(|&&c| c == 2366).count(),
        0,
        "Expected 0 TS2366 errors when all paths return, got: {codes:?}"
    );
}

#[test]
fn test_ts2366_early_return() {
    // Test that early returns are handled correctly
    let source = r#"
function test1(x: number): number {
    if (x < 0) {
        return -1;
    }
    return x;  // OK - this is reached when x >= 0
}

function test2(x: number): number {
    if (x < 0) {
        return -1;
    }
    if (x > 0) {
        return 1;
    }
    return 0;  // OK - this is reached when x == 0
}
"#;

    let (parser, root) = parse_test_source(source);
    assert!(parser.get_diagnostics().is_empty());

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

    // Should have no TS2366 errors - all paths return
    assert_eq!(
        codes.iter().filter(|&&c| c == 2366).count(),
        0,
        "Expected 0 TS2366 errors with early returns, got: {codes:?}"
    );
}

#[test]
fn test_ts2366_throw_as_exit() {
    // Test that throw statements are treated as exits
    let source = r#"
function test1(x: number): number {
    if (x < 0) {
        throw new Error("negative");
    }
    return x;
}

function test2(x: number): never {
    throw new Error("always throws");
}

function test3(x: number): number {
    if (x < 0) {
        throw new Error("negative");
    }
    if (x > 100) {
        throw new Error("too large");
    }
    return x;
}
"#;

    let (parser, root) = parse_test_source(source);
    assert!(parser.get_diagnostics().is_empty());

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

    // Should have no TS2366 errors - throw exits the function
    assert_eq!(
        codes.iter().filter(|&&c| c == 2366).count(),
        0,
        "Expected 0 TS2366 errors when throw is used as exit, got: {codes:?}"
    );
}
