/// Test that array destructuring with non-iterable number type emits TS2488
#[test]
fn test_iterator_array_destructuring_number_emits_ts2488() {
    use crate::binder::BinderState;
    use crate::checker::diagnostics::diagnostic_codes;
    use crate::checker::state::CheckerState;
    use tsz_solver::construction::TypeInterner;

    let source = r#"
const num: number = 42;
const [a, b] = num;
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
    let ts2488_count = codes
        .iter()
        .filter(|&&c| {
            c == diagnostic_codes::TYPE_MUST_HAVE_A_SYMBOL_ITERATOR_METHOD_THAT_RETURNS_AN_ITERATOR
        })
        .count();

    assert_eq!(
        ts2488_count, 1,
        "Expected 1 TS2488 error for array destructuring of number. All codes: {codes:?}"
    );
}

/// Test that array destructuring with valid array type does not emit TS2488
#[test]
fn test_iterator_array_destructuring_array_no_error() {
    use crate::binder::BinderState;
    use crate::checker::diagnostics::diagnostic_codes;
    use crate::checker::state::CheckerState;
    use tsz_solver::construction::TypeInterner;

    let source = r#"
const arr: number[] = [1, 2, 3];
const [a, b] = arr;
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
    let ts2488_count = codes
        .iter()
        .filter(|&&c| {
            c == diagnostic_codes::TYPE_MUST_HAVE_A_SYMBOL_ITERATOR_METHOD_THAT_RETURNS_AN_ITERATOR
        })
        .count();

    assert_eq!(
        ts2488_count, 0,
        "Expected 0 TS2488 errors for array destructuring of array. All codes: {codes:?}"
    );
}

/// Test that array destructuring of a non-iterable number type emits TS2488
#[test]
fn test_array_destructuring_number_emits_ts2488() {
    use crate::binder::BinderState;
    use crate::checker::diagnostics::diagnostic_codes;
    use crate::checker::state::CheckerState;
    use tsz_solver::construction::TypeInterner;

    let source = r#"
const num: number = 42;
const [a, b] = num;  // TS2488: number is not iterable
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
    let ts2488_count = codes
        .iter()
        .filter(|&&c| {
            c == diagnostic_codes::TYPE_MUST_HAVE_A_SYMBOL_ITERATOR_METHOD_THAT_RETURNS_AN_ITERATOR
        })
        .count();

    assert_eq!(
        ts2488_count, 1,
        "Expected 1 TS2488 error for array destructuring of number. All codes: {codes:?}"
    );
}

/// Test that array destructuring of a non-iterable boolean type emits TS2488
#[test]
fn test_array_destructuring_boolean_emits_ts2488() {
    use crate::binder::BinderState;
    use crate::checker::diagnostics::diagnostic_codes;
    use crate::checker::state::CheckerState;
    use tsz_solver::construction::TypeInterner;

    let source = r#"
const flag: boolean = true;
const [x] = flag;  // TS2488: boolean is not iterable
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
    let ts2488_count = codes
        .iter()
        .filter(|&&c| {
            c == diagnostic_codes::TYPE_MUST_HAVE_A_SYMBOL_ITERATOR_METHOD_THAT_RETURNS_AN_ITERATOR
        })
        .count();

    assert_eq!(
        ts2488_count, 1,
        "Expected 1 TS2488 error for array destructuring of boolean. All codes: {codes:?}"
    );
}

/// Test that array destructuring of a non-iterable object type emits TS2488
#[test]
fn test_array_destructuring_object_emits_ts2488() {
    use crate::binder::BinderState;
    use crate::checker::diagnostics::diagnostic_codes;
    use crate::checker::state::CheckerState;
    use tsz_solver::construction::TypeInterner;

    let source = r#"
const obj = { a: 1, b: 2 };
const [x, y] = obj;  // TS2488: object is not iterable
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
    let ts2488_count = codes
        .iter()
        .filter(|&&c| {
            c == diagnostic_codes::TYPE_MUST_HAVE_A_SYMBOL_ITERATOR_METHOD_THAT_RETURNS_AN_ITERATOR
        })
        .count();

    assert_eq!(
        ts2488_count, 1,
        "Expected 1 TS2488 error for array destructuring of object. All codes: {codes:?}"
    );
}

/// Test that array destructuring of an array type does not emit TS2488
#[test]
fn test_array_destructuring_array_no_error() {
    use crate::binder::BinderState;
    use crate::checker::diagnostics::diagnostic_codes;
    use crate::checker::state::CheckerState;
    use tsz_solver::construction::TypeInterner;

    let source = r#"
const arr: number[] = [1, 2, 3];
const [a, b, c] = arr;  // OK: array is iterable
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
    let ts2488_count = codes
        .iter()
        .filter(|&&c| {
            c == diagnostic_codes::TYPE_MUST_HAVE_A_SYMBOL_ITERATOR_METHOD_THAT_RETURNS_AN_ITERATOR
        })
        .count();

    assert_eq!(
        ts2488_count, 0,
        "Expected 0 TS2488 errors for array destructuring of array. All codes: {codes:?}"
    );
}

/// Test that array destructuring of a string type does not emit TS2488
#[test]
fn test_array_destructuring_string_no_error() {
    use crate::binder::BinderState;
    use crate::checker::diagnostics::diagnostic_codes;
    use crate::checker::state::CheckerState;
    use tsz_solver::construction::TypeInterner;

    let source = r#"
const str: string = "hello";
const [a, b, c] = str;  // OK: string is iterable
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
    let ts2488_count = codes
        .iter()
        .filter(|&&c| {
            c == diagnostic_codes::TYPE_MUST_HAVE_A_SYMBOL_ITERATOR_METHOD_THAT_RETURNS_AN_ITERATOR
        })
        .count();

    assert_eq!(
        ts2488_count, 0,
        "Expected 0 TS2488 errors for array destructuring of string. All codes: {codes:?}"
    );
}
