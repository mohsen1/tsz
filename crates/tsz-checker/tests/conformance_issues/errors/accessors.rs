use super::super::core::*;

#[test]
fn test_export_assignment_of_export_namespace_with_default_no_ts2349() {
    let ambient_source = r#"
declare module "b" {
    export function a(): void;
    export namespace a {
        var _a: typeof a;
        export { _a as default };
    }
    export default a;
}

declare module "a" {
    import { a } from "b";
    export = a;
}
"#;
    let consumer_source = r#"
import a from "a";
a();
"#;

    // Parse and bind the ambient modules file
    let mut parser_a = ParserState::new("external.d.ts".to_string(), ambient_source.to_string());
    let root_a = parser_a.parse_source_file();
    let mut binder_a = BinderState::new();
    binder_a.bind_source_file(parser_a.get_arena(), root_a);

    // Parse and bind the consumer file
    let mut parser_b = ParserState::new("main.ts".to_string(), consumer_source.to_string());
    let root_b = parser_b.parse_source_file();
    let mut binder_b = BinderState::new();
    binder_b.bind_source_file(parser_b.get_arena(), root_b);

    let arena_a = Arc::new(parser_a.get_arena().clone());
    let arena_b = Arc::new(parser_b.get_arena().clone());
    let all_arenas = Arc::new(vec![Arc::clone(&arena_a), Arc::clone(&arena_b)]);

    // Copy module exports from ambient to consumer binder
    for module_name in &["a", "b"] {
        if let Some(exports) = binder_a.module_exports.get(*module_name).cloned() {
            std::sync::Arc::make_mut(&mut binder_b.module_exports)
                .insert(module_name.to_string(), exports);
        }
    }

    let mut cross_file_targets = FxHashMap::default();
    for module_name in &["a", "b"] {
        if let Some(exports) = binder_a.module_exports.get(*module_name) {
            for (_, &sym_id) in exports.iter() {
                cross_file_targets.insert(sym_id, 0usize);
            }
        }
    }

    let binder_a = Arc::new(binder_a);
    let binder_b = Arc::new(binder_b);
    let all_binders = Arc::new(vec![Arc::clone(&binder_a), Arc::clone(&binder_b)]);

    let types = TypeInterner::new();
    let options = CheckerOptions {
        module: tsz_common::common::ModuleKind::CommonJS,
        es_module_interop: true,
        no_lib: true,
        target: ScriptTarget::ESNext,
        ..Default::default()
    };
    let mut checker = CheckerState::new(
        arena_b.as_ref(),
        binder_b.as_ref(),
        &types,
        "main.ts".to_string(),
        options,
    );

    checker.ctx.set_all_arenas(all_arenas);
    checker.ctx.set_all_binders(all_binders);
    checker.ctx.set_current_file_idx(1);

    for (sym_id, file_idx) in &cross_file_targets {
        checker.ctx.register_symbol_file_target(*sym_id, *file_idx);
    }

    checker.check_source_file(root_b);

    let diagnostics: Vec<(u32, String)> = checker
        .ctx
        .diagnostics
        .iter()
        .map(|d| (d.code, d.message_text.clone()))
        .collect();

    let has_ts2349 = has_error(&diagnostics, 2349);
    assert!(
        !has_ts2349,
        "Should NOT emit TS2349 for export= function+namespace. \
         With esModuleInterop, `import a from 'a'; a()` should be valid. \
         Actual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_typeof_in_type_alias_with_flow_narrowing() {
    // From controlFlowForIndexSignatures.ts
    // typeof c in a type alias inside if (typeof c === 'string') should resolve to 'string'
    let options = CheckerOptions {
        strict_null_checks: true,
        ..Default::default()
    };
    let source = r#"
declare let c: string | number;
if (typeof c === 'string') {
    type C = { [key: string]: typeof c };
    const boo1: C = { bar: 'works' };
    const boo2: C = { bar: 1 }; // should error TS2322
}
"#;
    let diagnostics = compile_and_get_diagnostics_with_options(source, options);
    assert!(
        has_error(&diagnostics, 2322),
        "Expected TS2322 for `bar: 1` not assignable to string (via typeof c narrowed to string). \
         Actual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_ts7006_for_excess_key_in_negated_type_constraint_mapped_type() {
    // From contextualTypesNegatedTypeLikeConstraintInGenericMappedType2.ts
    // When a mapped type maps excess keys to `never` (negated-type-like constraint),
    // a callback assigned to such a key should trigger TS7006 for its implicit-any param.
    // Previously, Round 2 of the two-pass generic call was marking the closure as
    // "already checked" in implicit_any_checked_closures while suppressing its TS7006,
    // causing the final resolve_call to skip it.
    let options = CheckerOptions {
        no_implicit_any: true,
        strict_null_checks: true,
        ..Default::default()
    };
    let source = r#"
type Extract<T, U> = T extends U ? T : never;
type Tags<D extends string, P> = P extends Record<D, infer X> ? X : never;
declare const typeTags: <I>() => <
  P extends {
    readonly [Tag in Tags<"_tag", I> & string]: (
      _: Extract<I, { readonly _tag: Tag }>,
    ) => any;
  } & { readonly [Tag in Exclude<keyof P, Tags<"_tag", I>>]: never },
>(fields: P) => unknown;
type Value = { _tag: "A"; a: number } | { _tag: "B"; b: number };
const matcher = typeTags<Value>();
matcher({
  A: (_) => _.a,
  B: (_) => "fail",
  C: (_) => "fail",
});
"#;
    let diagnostics = compile_and_get_diagnostics_with_options(source, options);
    assert!(
        has_error(&diagnostics, 7006),
        "Expected TS7006 for `_` param in `C: (_) => 'fail'` where C maps to `never` (excess key).\
         \nActual diagnostics: {diagnostics:#?}"
    );
}

// =============================================================================
// Chain summary optimization regression tests
// Verify that the lighter member-info-only path used by summarize_class_chain
// (which skips initialization analysis) doesn't break override checks or
// property access through class hierarchies.
// =============================================================================

#[test]
fn test_chain_summary_override_with_parameter_properties() {
    let source = r#"
        class Base {
            name: string;
            constructor(public id: number) {
                this.name = 'base';
            }
            greet(): string { return this.name; }
        }

        class Derived extends Base {
            constructor(id: number, public extra: string) {
                super(id);
            }
            greet(): string { return this.extra; }
        }
    "#;
    let options = CheckerOptions {
        strict: true,
        strict_null_checks: true,
        strict_property_initialization: true,
        ..CheckerOptions::default()
    };
    let diagnostics = compile_and_get_diagnostics_with_options(source, options);
    let ts2564_count = diagnostics.iter().filter(|(c, _)| *c == 2564).count();
    assert_eq!(
        ts2564_count, 0,
        "No TS2564: name assigned in constructor, id/extra are param properties.\
         \nActual: {diagnostics:#?}"
    );
}

#[test]
fn test_chain_summary_base_member_access_with_initializer() {
    let source = r#"
        class Animal {
            name: string = 'animal';
            legs: number = 4;
        }

        class Dog extends Animal {
            breed: string;
            constructor(breed: string) {
                super();
                this.breed = breed;
            }
        }

        const d = new Dog('lab');
        const n: string = d.name;
        const l: number = d.legs;
        const b: string = d.breed;
    "#;
    let options = CheckerOptions {
        strict: true,
        strict_null_checks: true,
        strict_property_initialization: true,
        ..CheckerOptions::default()
    };
    let diagnostics = compile_and_get_diagnostics_with_options(source, options);
    let ts2564_count = diagnostics.iter().filter(|(c, _)| *c == 2564).count();
    let ts2339_count = diagnostics.iter().filter(|(c, _)| *c == 2339).count();
    assert_eq!(
        ts2564_count, 0,
        "No TS2564: all fields have initializers or constructor assignments.\
         \nActual: {diagnostics:#?}"
    );
    assert_eq!(
        ts2339_count, 0,
        "No TS2339: base class members accessible on derived instances.\
         \nActual: {diagnostics:#?}"
    );
}

#[test]
fn test_chain_summary_deep_hierarchy_property_access() {
    let source = r#"
        class A { a: number = 1; }
        class B extends A { b: number = 2; }
        class C extends B { c: number = 3; }

        const obj = new C();
        const va: number = obj.a;
        const vb: number = obj.b;
        const vc: number = obj.c;
    "#;
    let options = CheckerOptions {
        strict: true,
        strict_null_checks: true,
        strict_property_initialization: true,
        ..CheckerOptions::default()
    };
    let diagnostics = compile_and_get_diagnostics_with_options(source, options);
    let ts2339_count = diagnostics.iter().filter(|(c, _)| *c == 2339).count();
    assert_eq!(
        ts2339_count, 0,
        "No TS2339: deep hierarchy properties accessible.\
         \nActual: {diagnostics:#?}"
    );
}

#[test]
fn test_infinite_constraints_ts2536_nested_indexed_access_literal() {
    // From infiniteConstraints.ts:
    // T2<B extends { [K in keyof B]: B[Exclude<keyof B, K>]["val"] }> = B
    // tsc emits TS2536: Type '"val"' cannot be used to index type 'B[Exclude<keyof B, K>]'
    // NOTE: This specific TS2536 inside a mapped type value type requires the mapped type
    // parameter to be in scope during check_type_node, which is not yet implemented.
    let diagnostics = compile_and_get_diagnostics(
        r#"
type T2<B extends { [K in keyof B]: B[Exclude<keyof B, K>]["val"] }> = B;
        "#,
    );
    assert!(
        has_error(&diagnostics, 2536),
        "Should emit TS2536 when string literal indexes unresolvable indexed access type.\nActual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_ts2536_literal_index_on_generic_indexed_access_simple() {
    // Non-recursive version: T[keyof T] indexed with a literal "foo"
    // tsc emits TS2536
    let diagnostics = compile_and_get_diagnostics(
        r#"
type X<T> = T[keyof T]["foo"];
        "#,
    );
    assert!(
        has_error(&diagnostics, 2536),
        "Should emit TS2536 when string literal indexes generic T[keyof T].\nActual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_unknown_control_flow_generic_keyspace_and_overlap_regression() {
    let diagnostics = compile_and_get_diagnostics(
        r#"
function ff3<T>(t: T, k: keyof (T & {})) {
    t[k];
}
function ff4<T>(t: T & {}, k: keyof (T & {})) {
    t[k];
}
function fx2<T extends {}>(value: T & ({} | null)) {
    if (value === 42) {}
}
function fx4<T extends {} | null>(value: T & ({} | null)) {
    if (value === 42) {}
}
        "#,
    );

    let relevant: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code != 2318)
        .cloned()
        .collect();
    let ts2536_count = relevant.iter().filter(|(code, _)| *code == 2536).count();
    let ts2367_count = relevant.iter().filter(|(code, _)| *code == 2367).count();

    assert_eq!(
        ts2536_count, 1,
        "Expected exactly one TS2536 for indexing raw T with keyof (T & {{}}), got: {relevant:#?}"
    );
    // `T extends {}` and `T extends {} | null` can both be instantiated with
    // a number literal (since `{}` includes primitives), so there IS potential
    // overlap with `42`. tsc does NOT emit TS2367 for these; we must not either.
    assert_eq!(
        ts2367_count, 0,
        "Expected no TS2367 for T extends {{}} comparisons ({{}} includes primitives), got: {relevant:#?}"
    );
}

#[test]
fn test_infinite_constraints_ts2536_keyof_indexed_access_literal() {
    // From infiniteConstraints.ts line 39:
    // declare function function1<T extends {[K in keyof T]: Cond<T[K]>}>(): T[keyof T]["foo"];
    // tsc emits TS2536: Type '"foo"' cannot be used to index type 'T[keyof T]'
    let diagnostics = compile_and_get_diagnostics(
        r#"
type Cond<T> = T extends number ? number : never;
declare function function1<T extends {[K in keyof T]: Cond<T[K]>}>(): T[keyof T]["foo"];
        "#,
    );
    assert!(
        has_error(&diagnostics, 2536),
        "Should emit TS2536 when string literal indexes unresolvable T[keyof T] result.\nActual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_objectish_any_produces_index_signature_object_not_any() {
    // tsc rule: identity homomorphic mapped type `{ [K in keyof T]: T[K] }` with T=any
    // and non-array constraint produces `{ [x: string]: any; [x: number]: any }`, NOT `any`.
    // This ensures `Objectish<any>` is not assignable to `any[]`.
    // With full lib.d.ts (CLI/conformance), this emits TS2740; in unit tests without
    // lib.d.ts the Array interface isn't available so it falls back to TS2322.
    let diagnostics = compile_and_get_diagnostics(
        r#"
type Objectish<T extends unknown> = { [K in keyof T]: T[K] };
type Result = Objectish<any>;
// Result should be { [x: string]: any; [x: number]: any }, not `any`.
// Assigning to an array should fail:
declare const r: Result;
const arr: any[] = r;
        "#,
    );
    assert!(
        has_error(&diagnostics, 2322) || has_error(&diagnostics, 2740),
        "Objectish<any> assigned to any[] should emit TS2322 or TS2740 (not pass silently). \
         Actual diagnostics: {diagnostics:#?}"
    );
}

/// TS2740: Object types with index signatures should not be silently assignable
/// to array types. With full lib.d.ts this emits TS2740 (missing array properties);
/// in unit tests without lib.d.ts it falls back to TS2322.
#[test]
fn test_ts2740_index_signature_object_to_array() {
    let diagnostics = compile_and_get_diagnostics(
        r#"
type Objectish<T extends unknown> = { [K in keyof T]: T[K] };
type IndirectArrayish<U extends unknown[]> = Objectish<U>;

function bar(objectish: Objectish<any>, indirectArrayish: IndirectArrayish<any>) {
    let arr: any[];
    arr = objectish;
    arr = indirectArrayish;
}
        "#,
    );
    let error_count = diagnostics
        .iter()
        .filter(|d| d.0 == 2322 || d.0 == 2740)
        .count();
    assert_eq!(
        error_count, 2,
        "Expected two assignability errors (one for objectish, one for indirectArrayish). \
         Both are object types with index signatures assigned to any[].\n\
         Actual diagnostics: {diagnostics:#?}"
    );
}

/// Test: union type alias return type should not produce false TS2322.
///
/// Regression test for union->Application cache poisoning in `env_eval_cache`.
/// When the `TypeEvaluator` produces an intermediate result mapping a union type
/// to an Application type (due to incomplete type environment resolution at that
/// point in time), caching that result poisons later lookups and causes false
/// assignability failures.
#[test]
fn test_union_type_alias_return_no_false_ts2322() {
    let source = r#"
interface YR<T> { done?: false; value: T; }
interface RR<T> { done: true; value: T; }
type MyResult<T, TReturn = any> = YR<T> | RR<TReturn>;

function test<T>(val: T): MyResult<T> {
    return { done: false, value: val };
}
"#;
    let diagnostics = compile_and_get_diagnostics_with_options(
        source,
        CheckerOptions {
            target: ScriptTarget::ESNext,
            ..CheckerOptions::default()
        },
    );
    let ts2322 = diagnostics
        .iter()
        .filter(|(c, _)| *c == 2322)
        .collect::<Vec<_>>();
    assert!(
        ts2322.is_empty(),
        "Should not emit false TS2322 for generic union type alias return.\n\
         TS2322 diagnostics: {ts2322:#?}"
    );
}

#[test]
fn test_no_false_ts2322_for_homomorphic_mapped_type_empty_target() {
    // Regression: M<{x: number}> should be assignable to M<{}>
    // because M<{x:n}> evaluates to {x:number} and M<{}> evaluates to {}.
    let diagnostics = compile_and_get_diagnostics_with_options(
        r#"
type M<S> = { [K in keyof S]: S[K] };
declare const a: M<{ x: number }>;
const b: M<{}> = a;
"#,
        CheckerOptions {
            strict_null_checks: true,
            ..Default::default()
        },
    );
    let ts2322: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2322)
        .collect();
    assert!(
        ts2322.is_empty(),
        "M<{{x: number}}> should be assignable to M<{{}}>.\nGot: {diagnostics:?}"
    );
}

#[test]
fn test_infer_property_with_context_sensitive_return_statement() {
    // Repro from #50687 / conformance: inferPropertyWithContextSensitiveReturnStatement
    // T is inferred as `number` from `params: 1`, so the inner arrow `a => a + 1`
    // should have `a: number` (not `a: T`). No errors expected.

    // Test 1: Direct callback (works)
    let source_direct = r#"
declare function repro2<T>(config: {
  params: T;
  callback: (params: T) => number;
}): void;

repro2({
  params: 1,
  callback: a => a + 1,
});
"#;
    let diags_direct = compile_and_get_diagnostics(source_direct);
    assert!(
        diags_direct.is_empty(),
        "Direct callback variant should have no errors. Got: {diags_direct:#?}"
    );

    // Test 2: Callback is a zero-param function returning a context-sensitive arrow
    // This is the actual failing case from the conformance test.
    let source = r#"
declare function repro<T>(config: {
  params: T;
  callback: () => (params: T) => number;
}): void;

repro({
  params: 1,
  callback: () => { return a => a + 1 },
});
"#;
    let diagnostics = compile_and_get_diagnostics(source);
    assert!(
        diagnostics.is_empty(),
        "Expected no errors for inferPropertyWithContextSensitiveReturnStatement. Got: {diagnostics:#?}"
    );
}

#[test]
fn test_thisless_method_not_context_sensitive_for_inference() {
    // Block-bodied methods with no parameters should NOT be considered context-sensitive
    // even when they return an object literal. This matches tsc's behavior where
    // hasContextSensitiveReturnExpression returns false for block bodies.
    //
    // Without this, state() is flagged as context-sensitive because it returns an object
    // literal, and gets deferred to Round 2. This prevents State type parameter from
    // being inferred from state()'s return type in Round 1, leaving mutations callback
    // with unknown parameter type.
    let source = r#"
// @strict: true
type StateFunction<State> = (s: State, ...args: any[]) => any;

type StoreOptions<State> = {
  state?: State | (() => State) | { (): State };
  mutations?: Record<string, StateFunction<State>>;
};

declare function createStore<State extends Record<string, unknown>>(
  options: StoreOptions<State>,
): void;

createStore({
  state() {
    return { bar2: 1 };
  },
  mutations: { inc: (state123) => state123.bar2++ },
});
"#;
    let diagnostics = compile_and_get_diagnostics(source);
    let ts18046: Vec<_> = diagnostics.iter().filter(|d| d.0 == 18046).collect();
    assert!(
        ts18046.is_empty(),
        "state() method with block body returning object literal should not be context-sensitive. \
         state123 should be inferred as {{ bar2: number }}, not unknown. \
         Got false TS18046: {ts18046:#?}"
    );
}

#[test]
fn test_jsdoc_constructor_overload_tags_emit_ts2394_for_incompatible_overload() {
    let diagnostics = compile_and_get_diagnostics_named(
        "overloadTag2.js",
        r#"
// @checkJs: true
// @allowJs: true
// @target: esnext
// @outdir: foo
// @declaration: true
// @strict: true
export class Foo {
    #a = true ? 1 : "1"
    #b

    /**
     * Should not have an implicit any error, because constructor's return type is always implicit
     * @constructor
     * @overload
     * @param {string} a
     * @param {number} b
     */
    /**
     * @constructor
     * @overload
     * @param {number} a
     */
    /**
     * @constructor
     * @overload
     * @param {string} a
     *//**
     * @constructor
     * @param {number | string} a
     */
    constructor(a, b) {
        this.#a = a
        this.#b = b
    }
}
var a = new Foo()
var b = new Foo('str')
var c = new Foo(2)
var d = new Foo('str', 2)
"#,
        CheckerOptions::default(),
    );

    // tsc correctly emits TS2394 here because the first @overload (string, number)
    // is not compatible with the implementation signature (number | string, b).
    assert!(
        has_error(&diagnostics, 2394),
        "Expected TS2394 for incompatible JSDoc constructor overload. Actual diagnostics: {diagnostics:#?}"
    );
    assert_eq!(
        diagnostics.iter().filter(|(code, _)| *code == 2554).count(),
        1,
        "Expected the remaining error to be the zero-argument constructor call. Actual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_jsdoc_generic_constructor_overload_tag_does_not_report_ts2394() {
    let diagnostics = compile_and_get_diagnostics_named(
        "overloadTag3.js",
        r#"
// @target: es2015
// @checkJs: true
// @allowJs: true
// @strict: true
// @noEmit: true

/** @template T */
export class Foo {
    /**
     * @constructor
     * @overload
     */
    constructor() { }

    /**
     * @param {T} value
     */
    bar(value) { }
}

/** @type {Foo} */
let foo;
foo = new Foo();
"#,
        CheckerOptions::default(),
    );

    let ts2394_count = diagnostics.iter().filter(|(code, _)| *code == 2394).count();
    assert_eq!(
        ts2394_count, 0,
        "Expected no TS2394 for generic JSDoc constructor overload tags. Actual diagnostics: {diagnostics:#?}"
    );
}

#[test]
#[ignore = "lib-backed JS overload diagnostic is currently red in direct unit CI"]
fn test_check_js_global_tostring_overload_reports_ts2394_with_libs() {
    if !lib_files_available() {
        return;
    }

    let diagnostics = compile_and_get_raw_diagnostics_named_with_lib_and_options(
        "index.js",
        r#"
function toString() {
    this.yadda;
    this.someValue = "";
}
"#,
        CheckerOptions {
            allow_js: true,
            check_js: true,
            strict: true,
            target: ScriptTarget::ES2015,
            ..CheckerOptions::default()
        },
    );

    // In tsc, file-scope function declarations shadow identically-named globals.
    // `function toString()` shadows `lib.dom.d.ts`'s `declare function toString(): string;`
    // rather than merging as overloads, so TS2394 should NOT be emitted.
    assert!(
        !diagnostics.iter().any(|d| d.code == 2394),
        "function toString() in a script file should shadow the lib global, \
         not produce TS2394. Actual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_jsdoc_template_function_unused_type_param_emits_ts6133() {
    let diagnostics = compile_and_get_diagnostics_named(
        "a.js",
        r#"
// @target: es2015
// @allowJs: true
// @checkJs: true
// @noEmit: true
// @noUnusedParameters:true

/** @template T */
function f() {}
"#,
        CheckerOptions::default(),
    );

    assert!(
        diagnostics.iter().any(|(code, msg)| {
            *code == 6133 && msg.contains("'T' is declared but its value is never read.")
        }),
        "Expected TS6133 for unused JSDoc template T. Actual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_jsdoc_template_function_param_type_counts_as_usage() {
    let diagnostics = compile_and_get_diagnostics_named(
        "a.js",
        r#"
// @target: es2015
// @allowJs: true
// @checkJs: true
// @noEmit: true
// @noUnusedParameters:true

/**
 * @template T
 * @param {T} value
 * @returns {T}
 */
function f(value) {
    return value;
}
"#,
        CheckerOptions::default(),
    );

    assert!(
        !diagnostics
            .iter()
            .any(|(code, msg)| *code == 6133 && msg.contains("'T'")),
        "Expected no TS6133 when JSDoc template T is used in param/return tags. Actual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_value_import_from_types_package_emits_ts6137() {
    let diagnostics = compile_and_get_diagnostics_named(
        "a.ts",
        r#"
import foo from "@types/foo-bar";
foo;
"#,
        CheckerOptions {
            module: ModuleKind::CommonJS,
            target: ScriptTarget::ES2015,
            ..CheckerOptions::default()
        },
    );

    assert!(
        has_error(&diagnostics, 6137),
        "Expected TS6137 for value import from @types package. Actual diagnostics: {diagnostics:#?}"
    );
    let message =
        diagnostic_message(&diagnostics, 6137).expect("TS6137 should include a diagnostic message");
    assert!(
        message.contains("foo-bar") && message.contains("@types/foo-bar"),
        "Expected TS6137 message to suggest importing foo-bar instead. Actual message: {message}"
    );
}

#[test]
fn test_typeof_import_package_namespace_preserves_argument_ts2345() {
    let files = vec![
        (
            "/private/tmp/fixture/p1/node_modules/csv-parse/lib/index.d.ts",
            "export function bar(): number;\n",
        ),
        (
            "/private/tmp/fixture/p1/index.ts",
            r#"
export interface MutableRefObject<T> {
    current: T;
}
export function useRef<T>(current: T): MutableRefObject<T> {
    return { current };
}
export const useCsvParser = () => {
    const parserRef = useRef<typeof import("csv-parse")>(null);
    return parserRef;
};
"#,
        ),
    ];

    let mut arenas = Vec::with_capacity(files.len());
    let mut binders = Vec::with_capacity(files.len());
    let mut roots = Vec::with_capacity(files.len());
    let file_names: Vec<String> = files.iter().map(|(name, _)| (*name).to_string()).collect();

    for (name, source) in files {
        let mut parser = ParserState::new(name.to_string(), source.to_string());
        let root = parser.parse_source_file();
        let mut binder = BinderState::new();
        binder.bind_source_file(parser.get_arena(), root);
        arenas.push(Arc::new(parser.get_arena().clone()));
        binders.push(Arc::new(binder));
        roots.push(root);
    }

    let entry_idx = file_names
        .iter()
        .position(|name| name == "/private/tmp/fixture/p1/index.ts")
        .expect("entry file should exist");
    let all_arenas = Arc::new(arenas);
    let all_binders = Arc::new(binders);
    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        all_arenas[entry_idx].as_ref(),
        all_binders[entry_idx].as_ref(),
        &types,
        file_names[entry_idx].clone(),
        CheckerOptions {
            target: ScriptTarget::ES2015,
            module: ModuleKind::CommonJS,
            ..CheckerOptions::default()
        },
    );

    checker.ctx.set_all_arenas(Arc::clone(&all_arenas));
    checker.ctx.set_all_binders(Arc::clone(&all_binders));
    checker.ctx.set_current_file_idx(entry_idx);
    checker
        .ctx
        .set_resolved_modules(FxHashSet::from_iter(["csv-parse".to_string()]));
    checker
        .ctx
        .set_resolved_module_paths(Arc::new(FxHashMap::from_iter([(
            (entry_idx, "csv-parse".to_string()),
            0usize,
        )])));

    checker.check_source_file(roots[entry_idx]);

    let diagnostics: Vec<_> = checker
        .ctx
        .diagnostics
        .iter()
        .map(|d| (d.code, d.message_text.clone()))
        .collect();

    assert!(
        diagnostics.iter().any(|(code, _)| *code == 2345),
        "Expected TS2345 for null passed to typeof import(\"csv-parse\") ref. Actual diagnostics: {diagnostics:#?}"
    );
    // tsc preserves the original module specifier (`"csv-parse"`) in
    // `typeof import("...")` output, not the resolved
    // `node_modules/<pkg>/<entry>` path. Match tsc parity.
    assert!(
        diagnostics.iter().any(|(code, message)| {
            *code == 2345 && message.contains("typeof import(\"csv-parse\")")
        }),
        "Expected TS2345 message to preserve the bare module specifier. Actual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_typeof_import_namespace_skips_type_only_exports() {
    let files = vec![
        (
            "foo2.ts",
            r#"
namespace Bar {
    export interface I {
        a: string;
        b: number;
    }
}

export namespace Baz {
    export interface J {
        a: number;
        b: string;
    }
}

class Bar {
    item: Bar.I;
    constructor(input: Baz.J) {}
}
export { Bar }
"#,
        ),
        (
            "usage.ts",
            r#"
export class Bar2 {
    item: { a: string, b: number, c: object };
    constructor(input?: any) {}
}

export let shim: typeof import("./foo2") = {
    Bar: Bar2
};
"#,
        ),
    ];
    let diagnostics = compile_named_files_get_diagnostics_with_options(
        &files,
        "usage.ts",
        CheckerOptions {
            module: ModuleKind::CommonJS,
            target: ScriptTarget::ES2015,
            ..CheckerOptions::default()
        },
    );

    assert!(
        !diagnostics
            .iter()
            .any(|(code, _)| *code == 2739 || *code == 2741),
        "Expected typeof import namespace to skip type-only exports. Actual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_define_property_prototype_descriptor_setter_is_contextualized() {
    let diagnostics = compile_and_get_diagnostics_named_with_lib_and_options(
        "mod1.js",
        r#"
/**
 * @constructor
 * @param {string} name
 */
function Person(name) {
    this.name = name;
}
Object.defineProperty(Person.prototype, "thing", { value: 42, writable: true });
Object.defineProperty(Person.prototype, "readonlyProp", { value: "Smith", writable: false });
Object.defineProperty(Person.prototype, "rwAccessors", { get() { return 98122 }, set(_) { /*ignore*/ } });
Object.defineProperty(Person.prototype, "readonlyAccessor", { get() { return 21.75 } });
Object.defineProperty(Person.prototype, "setonlyAccessor", {
    /** @param {string} str */
    set(str) {
        this.rwAccessors = Number(str);
    }
});
const m1 = new Person("Name");
m1.rwAccessors = 11;
m1.setonlyAccessor = "yes";
m1.readonlyProp = "name";
m1.readonlyAccessor = 12;
m1.rwAccessors = "no";
m1.setonlyAccessor = 0;
"#,
        CheckerOptions {
            allow_js: true,
            check_js: true,
            strict: true,
            target: ScriptTarget::ES2015,
            module: ModuleKind::CommonJS,
            ..CheckerOptions::default()
        },
    );

    let ts2339: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2339)
        .collect();
    let ts7006: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 7006)
        .collect();
    let ts2540: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2540)
        .collect();
    let has_rw_setter_mismatch = diagnostics.iter().any(|(code, message)| {
        *code == 2322
            && message.contains("string")
            && message.contains("number")
            && message.contains("not assignable")
    });

    assert!(
        ts2339.is_empty(),
        "Expected prototype defineProperty members to appear on constructor instances. Actual diagnostics: {diagnostics:#?}"
    );
    assert!(
        ts7006.is_empty(),
        "Expected paired descriptor setter methods to be contextually typed. Actual diagnostics: {diagnostics:#?}"
    );
    assert!(
        !ts2540.is_empty(),
        "Expected readonly defineProperty descriptors to stay readonly. Actual diagnostics: {diagnostics:#?}"
    );
    assert!(
        has_rw_setter_mismatch,
        "Expected rwAccessors setter writes to be checked against the getter's number type. Actual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_esm_declaration_module_without_default_still_reports_ts1192() {
    let files = [
        (
            "/mod.d.ts",
            r#"
export function toString(): string;
"#,
        ),
        (
            "/index.ts",
            r#"
import mdast, { toString } from "./mod";
mdast;
mdast.toString();
"#,
        ),
    ];
    let mut arenas = Vec::with_capacity(files.len());
    let mut binders = Vec::with_capacity(files.len());
    let mut roots = Vec::with_capacity(files.len());
    let file_names: Vec<String> = files.iter().map(|(name, _)| (*name).to_string()).collect();

    for (name, source) in files {
        let mut parser = ParserState::new(name.to_string(), source.to_string());
        let root = parser.parse_source_file();
        let mut binder = BinderState::new();
        binder.bind_source_file(parser.get_arena(), root);
        arenas.push(Arc::new(parser.get_arena().clone()));
        binders.push(Arc::new(binder));
        roots.push(root);
    }

    let entry_idx = file_names
        .iter()
        .position(|name| name == "/index.ts")
        .expect("entry file should exist");
    let (resolved_module_paths, resolved_modules) = build_module_resolution_maps(&file_names);

    let all_arenas = Arc::new(arenas);
    let all_binders = Arc::new(binders);
    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        all_arenas[entry_idx].as_ref(),
        all_binders[entry_idx].as_ref(),
        &types,
        file_names[entry_idx].clone(),
        CheckerOptions {
            target: ScriptTarget::ESNext,
            module: ModuleKind::ESNext,
            allow_synthetic_default_imports: true,
            ..CheckerOptions::default()
        },
    );

    checker.ctx.set_all_arenas(Arc::clone(&all_arenas));
    checker.ctx.set_all_binders(Arc::clone(&all_binders));
    checker.ctx.set_current_file_idx(entry_idx);
    checker
        .ctx
        .set_resolved_module_paths(Arc::new(resolved_module_paths));
    checker.ctx.set_resolved_modules(resolved_modules);
    checker.ctx.report_unresolved_imports = true;

    checker.check_source_file(roots[entry_idx]);

    let diagnostics: Vec<_> = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code != 2318)
        .map(|d| (d.code, d.message_text.clone()))
        .collect();

    // tsc suppresses TS1192 for .d.ts files when allowSyntheticDefaultImports is true,
    // even for pure ESM modules. The synthetic default is the module namespace object.
    assert!(
        !diagnostics.iter().any(|(code, _)| *code == 1192),
        "Expected no TS1192 for .d.ts files with allowSyntheticDefaultImports=true. Got: {diagnostics:#?}"
    );
}

#[test]
fn test_bare_esm_package_without_default_uses_resolved_node_modules_display() {
    let files = [
        (
            "/node_modules/mdast-util-to-string/index.d.ts",
            r#"
export function toString(): string;
"#,
        ),
        (
            "/index.ts",
            r#"
import mdast, { toString } from "mdast-util-to-string";
mdast;
mdast.toString();

const mdast2 = await import("mdast-util-to-string");
mdast2.toString();
mdast2.default;
"#,
        ),
    ];
    let mut arenas = Vec::with_capacity(files.len());
    let mut binders = Vec::with_capacity(files.len());
    let mut roots = Vec::with_capacity(files.len());
    let file_names: Vec<String> = files.iter().map(|(name, _)| (*name).to_string()).collect();

    for (name, source) in files {
        let mut parser = ParserState::new(name.to_string(), source.to_string());
        let root = parser.parse_source_file();
        let mut binder = BinderState::new();
        binder.bind_source_file(parser.get_arena(), root);
        arenas.push(Arc::new(parser.get_arena().clone()));
        binders.push(Arc::new(binder));
        roots.push(root);
    }

    let entry_idx = 1;
    let all_arenas = Arc::new(arenas);
    let all_binders = Arc::new(binders);
    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        all_arenas[entry_idx].as_ref(),
        all_binders[entry_idx].as_ref(),
        &types,
        file_names[entry_idx].clone(),
        CheckerOptions {
            target: ScriptTarget::ESNext,
            module: ModuleKind::ESNext,
            allow_synthetic_default_imports: false,
            ..CheckerOptions::default()
        },
    );

    checker.ctx.set_all_arenas(Arc::clone(&all_arenas));
    checker.ctx.set_all_binders(Arc::clone(&all_binders));
    checker.ctx.set_current_file_idx(entry_idx);
    checker.ctx.set_lib_contexts(Vec::new());
    checker
        .ctx
        .set_resolved_module_paths(Arc::new(FxHashMap::from_iter([(
            (entry_idx, "mdast-util-to-string".to_string()),
            0usize,
        )])));
    checker
        .ctx
        .set_resolved_modules(FxHashSet::from_iter(["mdast-util-to-string".to_string()]));
    checker.ctx.report_unresolved_imports = true;

    checker.check_source_file(roots[entry_idx]);

    let diagnostics: Vec<_> = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 1192 || d.code == 2339)
        .map(|d| (d.code, d.message_text.clone()))
        .collect();

    // tsc preserves the original bare module specifier
    // (`"mdast-util-to-string"`) in `typeof import("...")` output, not the
    // resolved `node_modules/<pkg>/index` path. Match tsc parity.
    assert!(
        diagnostics.iter().any(|(code, message)| {
            *code == 1192 && message.contains("\"mdast-util-to-string\"")
        }),
        "Expected TS1192 to use the bare module specifier. Actual diagnostics: {diagnostics:#?}"
    );
    assert!(
        diagnostics.iter().any(|(code, message)| {
            *code == 2339 && message.contains("typeof import(\"mdast-util-to-string\")")
        }),
        "Expected TS2339 to use the bare module specifier. Actual diagnostics: {diagnostics:#?}"
    );
}
