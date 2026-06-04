#[test]
fn assignability_normalization_keeps_generic_functions_callable_not_plain_objects() {
    let source = r#"
        declare let f: <T, S extends T>(x: T, y: S) => void;
        declare let g: <T, S>(x: T, y: S) => void;
    "#;

    let kinds = normalized_type_kinds_for_named_bindings(source, &["f", "g"]);
    assert_eq!(
        kinds[0], "Function",
        "expected normalized source to stay a function, got {kinds:?}"
    );
    assert_eq!(
        kinds[1], "Function",
        "expected normalized target to stay a function, got {kinds:?}"
    );
}

#[test]
fn solver_subtype_rejects_stricter_generic_constraints_directly() {
    let source = r#"
        declare let f: <T, S extends T>(x: T, y: S) => void;
        declare let g: <T, S>(x: T, y: S) => void;
    "#;

    let (parser, root) = parse_test_source(source);

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        CheckerOptions::default(),
    );

    checker.ctx.set_lib_contexts(Vec::new());
    checker.check_source_file(root);

    let ids: Vec<_> = ["f", "g"]
        .iter()
        .map(|name| {
            binder
                .file_locals
                .get(name)
                .map(|sym_id| checker.get_type_of_symbol(sym_id))
                .map(|type_id| checker.evaluate_type_for_assignability(type_id))
                .expect("expected binding type")
        })
        .collect();

    assert!(
        !is_fresh_subtype_of(checker.ctx.types, ids[0], ids[1]),
        "boundary subtype unexpectedly accepts stricter generic constraints"
    );
}

#[test]
fn boundary_assignability_rejects_stricter_generic_constraints() {
    let source = r#"
        declare let f: <T, S extends T>(x: T, y: S) => void;
        declare let g: <T, S>(x: T, y: S) => void;
    "#;

    let (parser, root) = parse_test_source(source);

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        CheckerOptions::default(),
    );

    checker.ctx.set_lib_contexts(Vec::new());
    checker.check_source_file(root);

    let ids: Vec<_> = ["f", "g"]
        .iter()
        .map(|name| {
            binder
                .file_locals
                .get(name)
                .map(|sym_id| checker.get_type_of_symbol(sym_id))
                .map(|type_id| checker.evaluate_type_for_assignability(type_id))
                .expect("expected binding type")
        })
        .collect();

    let overrides = CheckerOverrideProvider::new(&checker, None);
    let relation_result = is_assignable_with_overrides(
        &AssignabilityQueryInputs {
            db: checker.ctx.types,
            resolver: &checker.ctx,
            source: ids[0],
            target: ids[1],
            flags: checker.ctx.pack_relation_flags(),
            inheritance_graph: &checker.ctx.inheritance_graph,
            sound_mode: checker.ctx.sound_mode(),
        },
        &overrides,
    );
    assert!(
        !relation_result.is_related(),
        "assignability boundary unexpectedly accepts stricter generic constraints"
    );
}

#[test]
fn js_constructor_property_with_logical_or_is_declaration() {
    // Pattern: `X.Y = X.Y || function() {}` — tsc treats this as a
    // declaration (AssignmentDeclarationKind.Property), not a regular
    // assignment. No TS2322 should be emitted.
    let diagnostics = check_source(
        r#"
var test = {};
test.K = test.K ||
    function () {}

test.K.prototype = {
    add() {}
};

new test.K().add;
"#,
        "test.js",
        CheckerOptions {
            check_js: true,
            no_implicit_any: true,
            ..CheckerOptions::default()
        },
    );

    assert!(
        diagnostics.iter().all(|d| d.code != 2322),
        "JS lazy constructor initialization `X.Y = X.Y || function() {{}}` \
         should be treated as a declaration and not produce TS2322, got: {diagnostics:?}"
    );
    assert!(
        diagnostics.iter().all(|d| d.code != 2351 && d.code != 7009),
        "JS lazy constructor initialization `X.Y = X.Y || function() {{}}` \
         should keep the property constructable, got: {diagnostics:?}"
    );
}

#[test]
fn js_constructor_property_with_nullish_coalescing_is_declaration() {
    // Pattern: `X.Y = X.Y ?? function() {}` — same as above but with `??`.
    let diagnostics = check_source(
        r#"
var test = {};
test.K = test.K ??
    function () {}

test.K.prototype = {
    add() {}
};

new test.K();
"#,
        "test.js",
        CheckerOptions {
            check_js: true,
            no_implicit_any: true,
            ..CheckerOptions::default()
        },
    );

    assert!(
        diagnostics.iter().all(|d| d.code != 2322),
        "JS lazy constructor initialization `X.Y = X.Y ?? function() {{}}` \
         should be treated as a declaration and not produce TS2322, got: {diagnostics:?}"
    );
    assert!(
        diagnostics.iter().all(|d| d.code != 2351 && d.code != 7009),
        "JS lazy constructor initialization `X.Y = X.Y ?? function() {{}}` \
         should keep the property constructable, got: {diagnostics:?}"
    );
}

#[test]
fn import_meta_assignment_emits_ts2364() {
    // import.meta is parsed as PROPERTY_ACCESS_EXPRESSION in tsz, but assigning to
    // import.meta directly should emit TS2364 (not a valid assignment target), matching tsc.
    let diags = diagnostics_for("import.meta = {};");
    assert!(
        diags.iter().any(|d| d.code == 2364),
        "Expected TS2364 for `import.meta = {{}}` but got: {:?}",
        diags.iter().map(|d| d.code).collect::<Vec<_>>()
    );
}

#[test]
fn import_meta_property_assignment_is_valid() {
    // import.meta.foo is a regular property access, so the assignment target is valid.
    // It may still emit a property error, but not TS2364.
    let diags = diagnostics_for("import.meta.foo = 42;");
    assert!(
        !diags.iter().any(|d| d.code == 2364),
        "Should NOT emit TS2364 for `import.meta.foo = 42` but got: {:?}",
        diags.iter().map(|d| d.code).collect::<Vec<_>>()
    );
}

/// `({ } = { x: 0, y: 0 })` is a destructuring assignment with an empty
/// pattern. tsc treats every property on the RHS as excess and emits TS2353
/// for each, even though the empty `{}` target is normally treated as wide
/// for assignability. The variable-declaration form `var { } = { x: 0, y: 0 };`
/// stays silent — only the assignment-expression shape gets the strict check.
#[test]
fn destructuring_assignment_empty_pattern_emits_ts2353_for_each_excess_property() {
    let diags = diagnostics_for(
        r#"
function f() {
    ({ } = { x: 0, y: 0 });
}
"#,
    );
    let ts2353: Vec<_> = diags.iter().filter(|d| d.code == 2353).collect();
    assert_eq!(
        ts2353.len(),
        2,
        "expected exactly two TS2353 (one per RHS property) for empty destructuring pattern; got: {ts2353:?}"
    );
    assert!(
        ts2353.iter().any(|d| d.message_text.contains("'x'")),
        "expected TS2353 for property 'x', got: {ts2353:?}"
    );
    assert!(
        ts2353.iter().any(|d| d.message_text.contains("'y'")),
        "expected TS2353 for property 'y', got: {ts2353:?}"
    );
}

/// `var { } = { x: 0, y: 0 };` (declaration form) must NOT emit TS2353 for
/// excess properties. tsc only applies the strict empty-pattern check to
/// destructuring assignments, not declarations — verifying the new check is
/// scoped correctly to the assignment path.
#[test]
fn destructuring_declaration_empty_pattern_does_not_emit_ts2353() {
    let diags = diagnostics_for(
        r#"
function f() {
    var { } = { x: 0, y: 0 };
}
"#,
    );
    let ts2353: Vec<_> = diags.iter().filter(|d| d.code == 2353).collect();
    assert!(
        ts2353.is_empty(),
        "destructuring declaration with empty pattern must not emit TS2353; got: {ts2353:?}"
    );
}

#[test]
fn tuple_with_homomorphic_passthrough_over_union_no_error() {
    let diagnostics = diagnostics_for(
        r#"
type PassThrough<U> = { [P in keyof U]: U[P] };
type NodeA = { name: string; id: number };
type NodeB = { name: boolean; id: number };
declare const c: [PassThrough<NodeA | NodeB>];
const d: [NodeA | NodeB] = c;
"#,
    );
    assert!(
        diagnostics.iter().all(|d| d.code != 2322),
        "Expected no TS2322 for [PassThrough<NodeA|NodeB>] assigned to [NodeA|NodeB], got: {diagnostics:?}"
    );
}

#[test]
fn tuple_with_homomorphic_passthrough_different_type_param_name_no_error() {
    let diagnostics = diagnostics_for(
        r#"
type Id<K> = { [Q in keyof K]: K[Q] };
type NodeA = { name: string; id: number };
type NodeB = { name: boolean; id: number };
declare const c: [Id<NodeA | NodeB>];
const d: [NodeA | NodeB] = c;
"#,
    );
    assert!(
        diagnostics.iter().all(|d| d.code != 2322),
        "Expected no TS2322 for [Id<NodeA|NodeB>] (renamed type param K) assigned to [NodeA|NodeB], got: {diagnostics:?}"
    );
}

#[test]
fn tuple_with_passthrough_direct_still_no_error() {
    let diagnostics = diagnostics_for(
        r#"
type PassThrough<U> = { [P in keyof U]: U[P] };
type NodeA = { name: string; id: number };
type NodeB = { name: boolean; id: number };
declare const a: PassThrough<NodeA | NodeB>;
const b: NodeA | NodeB = a;
"#,
    );
    assert!(
        diagnostics.iter().all(|d| d.code != 2322),
        "Expected no TS2322 for direct PassThrough<NodeA|NodeB> assigned to NodeA|NodeB, got: {diagnostics:?}"
    );
}

#[test]
fn tuple_with_multi_element_passthrough_no_error() {
    let diagnostics = diagnostics_for(
        r#"
type PassThrough<U> = { [P in keyof U]: U[P] };
type NodeA = { name: string; id: number };
type NodeB = { name: boolean; id: number };
declare const e: [PassThrough<NodeA | NodeB>, string];
const f: [NodeA | NodeB, string] = e;
"#,
    );
    assert!(
        diagnostics.iter().all(|d| d.code != 2322),
        "Expected no TS2322 for multi-element tuple [PassThrough<NodeA|NodeB>, string], got: {diagnostics:?}"
    );
}

#[test]
fn tuple_structural_mismatch_still_fails() {
    let diagnostics = diagnostics_for(
        r#"
type PassThrough<U> = { [P in keyof U]: U[P] };
type NodeA = { name: string; id: number };
type NodeB = { name: boolean; id: number };
type DifferentShape = { x: string; y: number };
declare const c: [PassThrough<NodeA | NodeB>];
const bad: [DifferentShape] = c;
"#,
    );
    assert!(
        diagnostics.iter().any(|d| d.code == 2322),
        "Expected TS2322 for [PassThrough<NodeA|NodeB>] assigned to [DifferentShape], got: {diagnostics:?}"
    );
}

#[test]
fn callable_source_satisfies_callable_union_across_assignment_entrypoints() {
    let diagnostics = diagnostics_for(
        r#"
type CallableNode<TProps> = (props: TProps) => string | number;
type ConstructNode<TProps> = new (props: TProps) => { render(): string };
type Elementish<TProps> = CallableNode<TProps> | ConstructNode<TProps>;
type Props = { label: string };

const Component = (props: Props) => 1;
const initialized: Elementish<Props> = Component;

let assigned!: Elementish<Props>;
assigned = Component;

declare function accepts(value: Elementish<Props>): void;
accepts(Component);
"#,
    );

    assert!(
        diagnostics
            .iter()
            .all(|diag| diag.code != 2322 && diag.code != 2345),
        "Expected callable source to satisfy callable union arm through TS2322 and TS2345 paths, got: {diagnostics:?}"
    );
}

#[test]
fn callable_source_does_not_satisfy_callable_union_arm_with_static_requirements() {
    let diagnostics = diagnostics_for(
        r#"
type CallableWithStatic<TProps> = {
    (props: TProps): number;
    required: string;
};
type ConstructNode<TProps> = new (props: TProps) => { render(): string };
type Elementish<TProps> = CallableWithStatic<TProps> | ConstructNode<TProps>;
type Props = { label: string };

const Component = (props: Props) => 1;
const initialized: Elementish<Props> = Component;

declare function accepts(value: Elementish<Props>): void;
accepts(Component);
"#,
    );

    assert!(
        diagnostics.iter().any(|diag| diag.code == 2322),
        "Expected TS2322 when callable union arm has static requirements, got: {diagnostics:?}"
    );
    assert!(
        diagnostics.iter().any(|diag| diag.code == 2345),
        "Expected TS2345 when callable union arm has static requirements, got: {diagnostics:?}"
    );
}
