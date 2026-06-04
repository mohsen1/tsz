/// Test that TS2304 is emitted for a variable used outside its block scope.
#[test]
fn test_ts2304_out_of_scope_block_variable() {
    let source = r#"
function test() {
    if (true) {
        let blockScoped = 1;
    }
    return blockScoped;
}
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
    checker.ctx.report_unresolved_imports = true;
    checker.check_source_file(root);

    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&2304),
        "Expected TS2304 for out-of-scope block variable, got: {codes:?}"
    );
}

/// Test that TS2304 is emitted for a typo in a variable name with suggestions (TS2552).
#[test]
fn test_ts2304_typo_with_suggestion() {
    use crate::checker::diagnostics::diagnostic_codes;

    let source = r#"
const myVariable = 5;
const result = myVarible + 1;
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
    checker.ctx.report_unresolved_imports = true;
    checker.check_source_file(root);

    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();
    // Should have either TS2304 or TS2552 (did you mean?)
    let has_cannot_find = codes.contains(&diagnostic_codes::CANNOT_FIND_NAME)
        || codes.contains(&diagnostic_codes::CANNOT_FIND_NAME_DID_YOU_MEAN);
    assert!(
        has_cannot_find,
        "Expected TS2304 or TS2552 for typo in variable name, got: {codes:?}"
    );
}

/// Test that TS2304 is emitted for an undeclared variable in a return statement.
#[test]
fn test_ts2304_undeclared_var_in_return() {
    let source = r#"
function getValue(): number {
    return missingVariable;
}
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
    checker.ctx.report_unresolved_imports = true;
    checker.check_source_file(root);

    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&2304),
        "Expected TS2304 for undeclared variable in return, got: {codes:?}"
    );
}

/// Test that TS2304 is emitted for undeclared variable in array spread.
#[test]
fn test_ts2304_undeclared_var_in_array_spread() {
    let source = r#"
const arr = [1, 2, ...undeclaredArray];
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
    checker.ctx.report_unresolved_imports = true;
    checker.check_source_file(root);

    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&2304),
        "Expected TS2304 for undeclared variable in array spread, got: {codes:?}"
    );
}

/// Test that TS2304 is emitted for undeclared variable in object property value.
#[test]
fn test_ts2304_undeclared_var_in_object_literal() {
    let source = r#"
const obj = {
    name: undeclaredName,
    value: 42
};
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
    checker.ctx.report_unresolved_imports = true;
    checker.check_source_file(root);

    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&2304),
        "Expected TS2304 for undeclared variable in object literal, got: {codes:?}"
    );
}

/// Test that TS2304 is emitted for undeclared variable in conditional (ternary) expression.
#[test]
fn test_ts2304_undeclared_var_in_conditional() {
    let source = r#"
const result = true ? undeclaredTrue : 0;
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
    checker.ctx.report_unresolved_imports = true;
    checker.check_source_file(root);

    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&2304),
        "Expected TS2304 for undeclared variable in conditional, got: {codes:?}"
    );
}

/// Test that TS2304 is emitted for undeclared class in extends clause.
#[test]
fn test_ts2304_undeclared_class_in_extends() {
    let source = r#"
class Child extends MissingParent {}
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
    checker.ctx.report_unresolved_imports = true;
    checker.check_source_file(root);

    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&2304),
        "Expected TS2304 for undeclared class in extends clause, got: {codes:?}"
    );
}

/// Test that TS2304 is emitted for undeclared interface in implements clause.
#[test]
fn test_ts2304_undeclared_interface_in_implements() {
    let source = r#"
class MyClass implements MissingInterface {}
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
    checker.ctx.report_unresolved_imports = true;
    checker.check_source_file(root);

    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&2304),
        "Expected TS2304 for undeclared interface in implements clause, got: {codes:?}"
    );
}

/// Test that TS2304 is emitted for undeclared variable in template literal expression.
#[test]
fn test_ts2304_undeclared_var_in_template_literal() {
    let source = r#"
const msg = `Hello ${undeclaredName}!`;
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
    checker.ctx.report_unresolved_imports = true;
    checker.check_source_file(root);

    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&2304),
        "Expected TS2304 for undeclared variable in template literal, got: {codes:?}"
    );
}
