/// `const x = identifier(...)` — synthetic `CANNOT_FIND_NAME` for unbound call target.
/// Structural rule: a `const/let/var` declaration whose initializer starts with a
/// call expression `identifier(args)` should emit `CANNOT_FIND_NAME` for `identifier`
/// when it is not locally bound, so the import-fix fallback can suggest it.
/// This covers the `importFixesWithPackageJsonInSideAnotherPackage` scenario where
/// the real checker may not yet produce TS2304 for this pattern.
#[test]
fn synthetic_missing_name_detects_rhs_call_expression_const() {
    let mut server = make_server();
    // Two different identifier names prove the rule is not hardcoded to `useMemo`.
    for (callee, content, _expected_offset) in [
        (
            "useMemo",
            "const state = useMemo(() => 'Hello', []);",
            14usize,
        ),
        (
            "createSignal",
            "const [v, setV] = createSignal(0);",
            17usize,
        ),
        (
            "computedValue",
            "let result = computedValue(x, y);",
            13usize,
        ),
    ] {
        server.open_files.insert(
            "/lib.d.ts".to_string(),
            format!("export declare function {callee}(): void;"),
        );
        let file = "/app.ts";
        server
            .open_files
            .insert(file.to_string(), content.to_string());

        let mut parser = ParserState::new(file.to_string(), content.to_string());
        let root = parser.parse_source_file();
        let arena = parser.into_arena();
        let mut binder = tsz::binder::BinderState::new();
        binder.bind_source_file(&arena, root);

        let diagnostics =
            server.synthetic_missing_name_expression_diagnostics(file, content, &binder);

        assert!(
            diagnostics.iter().any(|d| {
                d.code == tsz_checker::diagnostics::diagnostic_codes::CANNOT_FIND_NAME
                    && d.message_text.contains(callee)
            }),
            "expected CANNOT_FIND_NAME for '{callee}' in `{content}`, got {diagnostics:?}"
        );
    }
}

#[test]
fn synthetic_missing_name_rhs_call_skips_locally_bound_callee() {
    let mut server = make_server();
    // `myFunc` is imported and bound, so no synthetic diagnostic expected.
    let content = "import { myFunc } from './mod';\nconst result = myFunc(42);";
    let file = "/app.ts";
    server.open_files.insert(
        "/mod.ts".to_string(),
        "export function myFunc(x: number): void {}".to_string(),
    );
    server
        .open_files
        .insert(file.to_string(), content.to_string());

    let mut parser = ParserState::new(file.to_string(), content.to_string());
    let root = parser.parse_source_file();
    let arena = parser.into_arena();
    let mut binder = tsz::binder::BinderState::new();
    binder.bind_source_file(&arena, root);

    let diagnostics = server.synthetic_missing_name_expression_diagnostics(file, content, &binder);
    assert!(
        !diagnostics
            .iter()
            .any(|d| d.message_text.contains("myFunc")),
        "bound callee 'myFunc' must not produce synthetic CANNOT_FIND_NAME, got {diagnostics:?}"
    );
}

/// Span with no `request_span` and empty diagnostics must return nothing.
#[test]
fn fix_missing_type_annotation_no_span_no_diag_returns_empty() {
    let content = "export const el = <div/>;";
    let file = "/no_span_no_diag.tsx";
    let arena = parse_to_arena(file, content);
    let line_map = LineMap::build(content);

    let fixes = Server::apply_isolated_decl_type_annotation_fix(
        file,
        content,
        &arena,
        &line_map,
        &[], // No diagnostics
        &[9010],
        None, // No span either
    );

    assert!(
        fixes.is_empty(),
        "without both diagnostics and span, no fix should be produced"
    );
}
