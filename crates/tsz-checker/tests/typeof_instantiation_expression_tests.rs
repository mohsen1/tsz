use tsz_checker::context::CheckerOptions;
use tsz_checker::diagnostics::Diagnostic;
use tsz_checker::test_utils::{
    check_source_diagnostics, check_source_with_libs, diagnostic_count,
    diagnostic_messages_with_code, load_default_lib_files,
};

fn diagnostics_with_default_libs(source: &str) -> Vec<Diagnostic> {
    let libs = load_default_lib_files();
    assert!(!libs.is_empty(), "expected default libs to load");
    check_source_with_libs(source, "test.ts", CheckerOptions::default(), &libs)
}

fn assert_no_false_typeof_instantiation_diagnostics(diags: &[Diagnostic]) {
    let unexpected: Vec<_> = diags
        .iter()
        .filter(|diag| matches!(diag.code, 2304 | 2344 | 2503 | 2833))
        .collect();
    assert!(
        unexpected.is_empty(),
        "expected no namespace/constraint diagnostics for typeof instantiation expression, got {diags:#?}"
    );
}

// ── success paths ─────────────────────────────────────────────────────────────

#[test]
fn typeof_property_instantiation_resolves_as_value_property_chain() {
    let diags = check_source_diagnostics(
        r#"
type ReturnOf<T extends (...args: any[]) => any> =
    T extends (...args: any[]) => infer R ? R : never;

declare const ops: {
    convert<T>(value: unknown): T;
};

type Converted = ReturnOf<typeof ops.convert<string>>;
declare const converted: Converted;

const ok: string = converted;
const bad: number = converted;
"#,
    );

    assert_no_false_typeof_instantiation_diagnostics(&diags);
}

#[test]
fn nested_typeof_property_instantiation_resolves_with_renamed_bindings() {
    let diags = check_source_diagnostics(
        r#"
type ReturnOf<T extends (...args: any[]) => any> =
    T extends (...args: any[]) => infer R ? R : never;

declare const services: {
    mapper: {
        pick<U>(value: unknown): U;
    };
};

type Picked = ReturnOf<typeof services.mapper.pick<boolean>>;
declare const picked: Picked;

const ok: boolean = picked;
const bad: string = picked;
"#,
    );

    assert_no_false_typeof_instantiation_diagnostics(&diags);
}

#[test]
fn reported_array_map_typeof_instantiation_does_not_resolve_arr_as_namespace() {
    let diags = diagnostics_with_default_libs(
        r#"
const arr = [1, 2, 3];

type Mapper = typeof arr.map<string>;
"#,
    );

    assert_no_false_typeof_instantiation_diagnostics(&diags);
}

#[test]
fn return_type_of_array_map_instantiation_does_not_resolve_numbers_as_namespace() {
    let diags = diagnostics_with_default_libs(
        r#"
const numbers = [1, 2, 3];

type MapResult = ReturnType<typeof numbers.map<string>>;
declare const mapped: MapResult;

const ok: string[] = mapped;
"#,
    );

    assert_no_false_typeof_instantiation_diagnostics(&diags);
}

// ── TS2635: non-callable types ────────────────────────────────────────────────

#[test]
fn non_callable_object_type_with_type_args_emits_ts2635() {
    let diags = check_source_diagnostics(
        r#"
declare const obj: { x: string; y: string; };
type Bad = typeof obj<string>;
"#,
    );
    assert_eq!(
        diagnostic_count(&diags, 2635),
        1,
        "Expected one TS2635 for non-callable object with type args, got: {diags:?}"
    );
}

#[test]
fn non_callable_object_name_variant_emits_ts2635() {
    let diags = check_source_diagnostics(
        r#"
declare const myValue: { alpha: number; beta: boolean; };
type T = typeof myValue<number>;
"#,
    );
    assert_eq!(
        diagnostic_count(&diags, 2635),
        1,
        "Expected TS2635 regardless of object property names, got: {diags:?}"
    );
}

#[test]
fn intersection_of_non_callable_types_emits_ts2635() {
    let diags = check_source_diagnostics(
        r#"
declare const combined: { x: string } & { y: number };
type Bad = typeof combined<string>;
"#,
    );
    assert_eq!(
        diagnostic_count(&diags, 2635),
        1,
        "Expected TS2635 for intersection of non-callable types, got: {diags:?}"
    );
}

#[test]
fn union_of_non_callable_types_emits_ts2635() {
    let diags = check_source_diagnostics(
        r#"
declare const either: { x: string } | { y: number };
type Bad = typeof either<string>;
"#,
    );
    assert_eq!(
        diagnostic_count(&diags, 2635),
        1,
        "Expected TS2635 for union of non-callable types, got: {diags:?}"
    );
}

// ── TS2635: non-generic functions and wrong-arity overloads ───────────────────

#[test]
fn non_generic_function_with_type_args_emits_ts2635() {
    let diags = check_source_diagnostics(
        r#"
declare const nonGeneric: (a: string, b: number) => string[];
type Bad = typeof nonGeneric<string>;
"#,
    );
    assert_eq!(
        diagnostic_count(&diags, 2635),
        1,
        "Expected TS2635 for non-generic function with type args, got: {diags:?}"
    );
}

#[test]
fn non_generic_function_name_variant_emits_ts2635() {
    let diags = check_source_diagnostics(
        r#"
declare const doWork: (payload: boolean) => void;
type R = typeof doWork<number>;
"#,
    );
    assert_eq!(
        diagnostic_count(&diags, 2635),
        1,
        "Name-independent TS2635 for non-generic function, got: {diags:?}"
    );
}

#[test]
fn non_generic_constructor_function_emits_ts2635() {
    let diags = check_source_diagnostics(
        r#"
declare const Ctor: new (a: string, b: number) => string[];
type Bad = typeof Ctor<string>;
"#,
    );
    assert_eq!(
        diagnostic_count(&diags, 2635),
        1,
        "Expected TS2635 for non-generic constructor with type args, got: {diags:?}"
    );
}

#[test]
fn overloaded_function_no_signature_matches_given_arity_emits_ts2635() {
    let diags = check_source_diagnostics(
        r#"
declare const multi: {
    <T>(x: T): T;
    <T>(x: T, n: number): T;
    <T, U>(t: [T, U]): [T, U];
};
type Bad = typeof multi<string, number, boolean>;
"#,
    );
    assert_eq!(
        diagnostic_count(&diags, 2635),
        1,
        "Expected TS2635 when no overload matches arity 3, got: {diags:?}"
    );
}

#[test]
fn overloaded_function_correct_arity_no_error() {
    let diags = check_source_diagnostics(
        r#"
declare const multi: {
    <T>(x: T): T;
    <T, U>(t: [T, U]): [T, U];
};
type Good1 = typeof multi<string>;
type Good2 = typeof multi<string, number>;
"#,
    );
    assert_eq!(
        diagnostic_count(&diags, 2635),
        0,
        "Correct arities must not emit TS2635, got: {diags:?}"
    );
}

#[test]
fn typeof_single_type_param_function_correct_arity_no_error() {
    let diags = check_source_diagnostics(
        r#"
type RT<T extends (...args: any) => any> = T;
declare const createReducer: <S>(s: S) => S;
type R = RT<typeof createReducer<string>>;
"#,
    );
    assert_eq!(
        diagnostic_count(&diags, 2635),
        0,
        "Correct arity must not emit TS2635, got: {diags:?}"
    );
    assert_eq!(
        diagnostic_count(&diags, 2344),
        0,
        "Correct arity must not emit TS2344, got: {diags:?}"
    );
}

// ── TS2635 + TS2344: failed instantiation as type argument ───────────────────

#[test]
fn failed_instantiation_as_constrained_type_arg_emits_ts2635_and_ts2344() {
    let diags = check_source_diagnostics(
        r#"
type RT<T extends (...args: any) => any> = T;
declare const createCacheReducer: <N extends string, QR>(q: QR) => QR;
type Cache<QR> = {
    queries: {
        [QK in keyof QR]: RT<typeof createCacheReducer<QR>>;
    };
};
"#,
    );
    assert_eq!(
        diagnostic_count(&diags, 2635),
        1,
        "Expected TS2635 at the wrong-arity instantiation site, got: {diags:?}"
    );
    assert_eq!(
        diagnostic_count(&diags, 2344),
        1,
        "Expected TS2344 because failed instantiation does not satisfy callable constraint, got: {diags:?}"
    );
}

#[test]
fn failed_instantiation_renamed_type_params_same_behavior() {
    let diags = check_source_diagnostics(
        r#"
type Wrapper<F extends (...args: any) => any> = F;
declare const builder: <A extends string, B>(x: B) => B;
type Built<B> = {
    items: {
        [K in keyof B]: Wrapper<typeof builder<B>>;
    };
};
"#,
    );
    assert_eq!(
        diagnostic_count(&diags, 2635),
        1,
        "Renamed type-params: still one TS2635, got: {diags:?}"
    );
    assert_eq!(
        diagnostic_count(&diags, 2344),
        1,
        "Renamed type-params: still one TS2344, got: {diags:?}"
    );
}

#[test]
fn failed_instantiation_one_type_arg_for_two_param_fn() {
    let diags = check_source_diagnostics(
        r#"
declare const twoParam: <T, U>(x: T, y: U) => [T, U];
type RT<T extends (...args: any) => any> = T;
type C = RT<typeof twoParam<string>>;
"#,
    );
    assert_eq!(
        diagnostic_count(&diags, 2635),
        1,
        "One type arg for two-param fn must emit TS2635, got: {diags:?}"
    );
    assert_eq!(
        diagnostic_count(&diags, 2344),
        1,
        "Failed instantiation as arg to callable constraint must emit TS2344, got: {diags:?}"
    );
}

// ── TS2344: constraint violations for correct-arity instantiations ────────────

#[test]
fn typeof_instantiation_constraint_violation_emits_ts2344() {
    let diags = check_source_diagnostics(
        r#"
type R<T extends number> = T;
declare const fn1: <U>(x: U) => U;
type Bad = R<typeof fn1<string>>;
"#,
    );
    assert_eq!(
        diagnostic_count(&diags, 2344),
        1,
        "Expected one TS2344 for constraint violation, got: {diags:?}"
    );
}

#[test]
fn typeof_instantiation_valid_constraint_no_ts2344() {
    let diags = check_source_diagnostics(
        r#"
type RT<T extends (...args: any) => any> = T;
declare const transform: <X>(x: X) => X;
type Result = RT<typeof transform<string>>;
"#,
    );
    assert_eq!(
        diagnostic_count(&diags, 2344),
        0,
        "Satisfied constraint must not emit TS2344, got: {diags:?}"
    );
    assert_eq!(
        diagnostic_count(&diags, 2635),
        0,
        "Satisfied constraint+arity must not emit TS2635, got: {diags:?}"
    );
}

// ── TS2635 message text: named callables display as symbol name ───────────────
//
// Rule: When a Callable type has a named symbol (lib interface like
// ArrayConstructor, MapConstructor, etc.), TS2635 messages must display the
// symbol name, NOT the structural callable form and NOT an applied alias like
// "ArrayConstructor<string, number>".
//
// Anonymous user-defined callable types (no symbol) still display structurally.

#[test]
fn ts2635_named_callable_array_shows_symbol_name_in_message() {
    let diags = diagnostics_with_default_libs(
        r#"
const r = Array<string, number>;
"#,
    );
    let messages = diagnostic_messages_with_code(&diags, 2635);
    assert_eq!(messages.len(), 1, "Expected one TS2635, got: {diags:?}");
    assert!(
        messages[0].contains("'ArrayConstructor'"),
        "TS2635 for Array<string, number> must say 'ArrayConstructor', got: {}",
        messages[0]
    );
}

#[test]
fn ts2635_named_callable_array_shows_symbol_name_with_typeof_trigger_in_file() {
    // Regression: when `typeof Array<string, number>` appears elsewhere in the
    // same file, tsz used to follow the display alias and emit the full
    // structural callable form instead of "ArrayConstructor".
    let diags = diagnostics_with_default_libs(
        r#"
const r = Array<string, number>;
type T22 = typeof Array<string, number>;
"#,
    );
    let messages = diagnostic_messages_with_code(&diags, 2635);
    assert!(
        !messages.is_empty(),
        "Expected TS2635 diagnostics, got none"
    );
    for msg in &messages {
        assert!(
            msg.contains("'ArrayConstructor'"),
            "Every TS2635 for Array<T...> must say 'ArrayConstructor' even when typeof Array<T...> appears in the same file, got: {msg}"
        );
        assert!(
            !msg.contains("new (length?: number) =>"),
            "TS2635 must not emit the structural callable form, got: {msg}"
        );
    }
}

#[test]
fn ts2635_named_callable_map_shows_symbol_name_in_message() {
    // Same rule applied to MapConstructor — proves this is not Array-specific.
    let diags = diagnostics_with_default_libs(
        r#"
const m = Map<string, number, boolean>;
type Trigger = typeof Map<string, number, boolean>;
"#,
    );
    let messages = diagnostic_messages_with_code(&diags, 2635);
    assert!(
        !messages.is_empty(),
        "Expected TS2635 diagnostics for Map<string, number, boolean>, got none"
    );
    for msg in &messages {
        assert!(
            msg.contains("'MapConstructor'"),
            "TS2635 for Map<T, U, V> must say 'MapConstructor', got: {msg}"
        );
        assert!(
            !msg.contains("new <K, V>"),
            "TS2635 must not emit the structural callable form for MapConstructor, got: {msg}"
        );
    }
}

#[test]
fn ts2635_generic_named_callable_keeps_instantiated_type_args() {
    let diags = check_source_diagnostics(
        r#"
interface Box<T> {
    <U>(value: U): U;
}

declare const box: Box<number>;
const x = box<string, number>;
"#,
    );
    let messages = diagnostic_messages_with_code(&diags, 2635);
    assert_eq!(messages.len(), 1, "Expected one TS2635, got: {diags:?}");
    assert!(
        messages[0].contains("'Box<number>'"),
        "TS2635 for instantiated generic callable must keep type arguments, got: {}",
        messages[0]
    );
}

#[test]
fn ts2635_typeof_function_alias_uses_structural_display() {
    let diags = check_source_diagnostics(
        r#"
declare function fx<T>(x: T): T;
declare function fx<T>(x: T, n: number): T;
declare function fx<T, U>(t: [T, U]): [T, U];

type T10 = typeof fx<string, number, boolean>;
"#,
    );
    let messages = diagnostic_messages_with_code(&diags, 2635);
    assert_eq!(messages.len(), 1, "Expected one TS2635, got: {diags:?}");
    let msg = messages[0];
    assert!(
        msg.contains("{ <T>(x: T): T;"),
        "TS2635 for typeof function instantiation must use structural function display, got: {msg}"
    );
    assert!(
        !msg.contains("'T10'"),
        "TS2635 must not use the containing type alias name for failed typeof instantiation, got: {msg}"
    );
}

#[test]
fn ts2635_anonymous_callable_still_shows_structural_form() {
    // Anonymous overloaded functions (no symbol name) must still use the
    // structural display — the named-symbol fast path must not affect them.
    let diags = check_source_diagnostics(
        r#"
declare const multi: {
    <T>(x: T): T;
    <T>(x: T, n: number): T;
};
type Bad = typeof multi<string, number, boolean>;
"#,
    );
    let messages = diagnostic_messages_with_code(&diags, 2635);
    assert_eq!(messages.len(), 1, "Expected one TS2635, got: {diags:?}");
    assert!(
        messages[0].contains('<'),
        "TS2635 for anonymous callable must show structural form with '<', got: {}",
        messages[0]
    );
}
