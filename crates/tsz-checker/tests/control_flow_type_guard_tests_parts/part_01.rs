/// Test that negated assertion with conditional type predicate narrows correctly.
/// When `assert(!isB(foo))` where `isB` uses a conditional type predicate like
/// `Extract<T, U>`, the false branch should exclude matching union members.
///
/// This exercises the `resolve_type_uncached` path for `Conditional` types in the
/// solver's narrowing context — ensuring inner Lazy types are resolved before
/// the conditional is evaluated/distributed.
#[test]
fn test_negated_assertion_with_conditional_type_predicate() {
    let source = r#"
type Foo = {type: 'A', a: number} | {type: 'B', b: number};
type MyExtract<T, U> = T extends U ? T : never;
declare function isB(x: Foo): x is MyExtract<Foo, {type: 'B'}>;
declare function assert(x: boolean): asserts x;

function test(foo: Foo): {type: 'A', a: number} {
    assert(!isB(foo));
    return foo;
}
"#;

    let (parser, root) = parse_test_source(source);
    assert!(parser.get_diagnostics().is_empty(), "Parse errors");

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let options = CheckerOptions {
        strict: true,
        ..CheckerOptions::default()
    }
    .apply_strict_defaults();

    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        options,
    );

    checker.check_source_file(root);

    let diagnostics: Vec<(u32, String)> = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code != 2318)
        .map(|d| (d.code, d.message_text.clone()))
        .collect();

    // Should NOT have TS2322 — foo is narrowed to {type: 'A', a: number}
    // by excluding Extract<Foo, {type: 'B'}> = {type: 'B', b: number}
    let ts2322: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2322)
        .collect();
    assert!(
        ts2322.is_empty(),
        "Negated assertion with conditional type predicate should narrow correctly: {ts2322:?}"
    );
}

/// Regression test: generic type inference from type predicate literal types
/// should preserve the literal type, not widen it.
///
/// When calling `capture<V>(pred: (x: unknown) => x is V)` with a predicate
/// like `isB: (x: unknown) => x is 'B'`, V should be inferred as `'B'` (literal),
/// not widened to `string`. This matches tsc's behavior where types from type
/// annotations don't carry the `RequiresWidening` flag.
#[test]
fn test_generic_inference_preserves_literal_from_type_predicate() {
    let source = r#"
declare function capture<V>(predicate: (arg: unknown) => arg is V): V;
declare function isB(arg: unknown): arg is 'B';

const result = capture(isB);
const check: 'B' = result;
"#;

    let (parser, root) = parse_test_source(source);
    assert!(parser.get_diagnostics().is_empty(), "Parse errors");

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let options = CheckerOptions {
        strict: true,
        ..CheckerOptions::default()
    }
    .apply_strict_defaults();

    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        options,
    );

    checker.check_source_file(root);

    let diagnostics: Vec<(u32, String)> = checker
        .ctx
        .diagnostics
        .iter()
        .map(|d| (d.code, d.message_text.clone()))
        .collect();

    let relevant: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code != 2318)
        .cloned()
        .collect();

    // TS2322 would mean V was widened from 'B' to string
    let ts2322: Vec<_> = relevant.iter().filter(|(code, _)| *code == 2322).collect();
    assert!(
        ts2322.is_empty(),
        "Generic inference from type predicate should preserve literal 'B', not widen to string: {ts2322:?}"
    );
}

/// Regression test: `this is DatafulFoo<T>` type predicate narrows `this` so
/// that property accesses use the narrowed interface members.
///
/// From conformance test `spreadObjectOrFalsy.ts`:
/// ```ts
/// interface DatafulFoo<T> { data: T; }
/// class Foo<T extends string> {
///     data: T | undefined;
///     bar() {
///         if (this.hasData()) {
///             this.data.toLocaleLowerCase(); // NO TS2532
///         }
///     }
///     hasData(): this is DatafulFoo<T> { return true; }
/// }
/// ```
///
/// After narrowing, `this.data` should be `T` (from `DatafulFoo<T>`),
/// not `T | undefined` (from `Foo<T>`). TS2532 must not be emitted.
#[test]
fn test_this_type_predicate_narrows_property_type() {
    let source = r#"
interface DatafulFoo<T> {
    data: T;
}

class Foo<T extends string> {
    data: T | undefined;
    bar() {
        if (this.hasData()) {
            this.data.toLocaleLowerCase();
        }
    }
    hasData(): this is DatafulFoo<T> {
        return true;
    }
}
"#;

    let (parser, root) = parse_test_source(source);
    assert!(parser.get_diagnostics().is_empty(), "Parse errors");

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let options = CheckerOptions {
        strict: true,
        ..CheckerOptions::default()
    }
    .apply_strict_defaults();

    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        options,
    );

    checker.check_source_file(root);

    let ts2532: Vec<_> = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 2532)
        .collect();

    assert!(
        ts2532.is_empty(),
        "Expected no TS2532 after `this is DatafulFoo<T>` narrows `this`. \
         `this.data` should be `T`, not `T | undefined`. Got: {ts2532:#?}"
    );
}

/// `controlFlowAliasing.ts` C11: alias narrowing must survive a *later*
/// reassignment of a `readonly` auto-property (`this.x = 10` in the else
/// branch), but it must be invalidated for a parameter that is reassigned
/// elsewhere in the same scope.
#[test]
fn alias_narrowing_readonly_property_survives_later_assignment() {
    let source = r#"
class C11 {
    constructor(readonly x: string | number) {
        const thisX_isString = typeof this.x === 'string';
        const xIsString = typeof x === 'string';
        if (thisX_isString && xIsString) {
            let s: string;
            s = this.x; // OK: this.x is a constant reference (readonly)
            s = x;      // TS2322: x is reassigned later in the constructor
        }
        else {
            this.x = 10;
            x = 10;
        }
    }
}
"#;

    let (parser, root) = parse_test_source(source);
    assert!(parser.get_diagnostics().is_empty(), "Parse errors");

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let options = CheckerOptions {
        strict: true,
        ..CheckerOptions::default()
    }
    .apply_strict_defaults();

    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        options,
    );
    checker.check_source_file(root);

    let ts2322: Vec<_> = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 2322)
        .collect();

    assert_eq!(
        ts2322.len(),
        1,
        "Expected exactly one TS2322 (on `s = x;`); the readonly property \
         narrowing on `s = this.x;` must NOT error. Got: {ts2322:#?}"
    );
}

/// `controlFlowAliasing.ts` f27: alias narrowing must NOT apply when the
/// captured chain steps through a *mutable* property, even if no assignment
/// is observed. tsc's `isConstantReference` rejects the chain because
/// `obj` on `outer` is not declared `readonly`.
#[test]
fn alias_narrowing_rejected_for_mutable_property_chain() {
    let source = r#"
function f27(outer: { obj: { kind: 'foo', foo: string } | { kind: 'bar', bar: number } }) {
    const isFoo = outer.obj.kind === 'foo';
    if (isFoo) {
        outer.obj.foo;  // TS2339: not narrowed, `obj` is mutable
    }
    else {
        outer.obj.bar;  // TS2339: not narrowed, `obj` is mutable
    }
}
"#;

    let (parser, root) = parse_test_source(source);
    assert!(parser.get_diagnostics().is_empty(), "Parse errors");

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let options = CheckerOptions {
        strict: true,
        ..CheckerOptions::default()
    }
    .apply_strict_defaults();

    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        options,
    );
    checker.check_source_file(root);

    let ts2339: Vec<_> = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 2339)
        .collect();

    assert_eq!(
        ts2339.len(),
        2,
        "Expected exactly two TS2339 from f27 (one per branch); the alias \
         narrowing must be invalidated because `outer.obj` is mutable. \
         Got: {ts2339:#?}"
    );
}

/// `controlFlowAliasing.ts` f26: alias narrowing applies through a chain
/// of `readonly` property accesses even when the same enclosing scope has
/// other mutating operations on unrelated state. The whole chain must be
/// constant for narrowing to apply.
#[test]
fn alias_narrowing_applies_for_readonly_property_chain() {
    let source = r#"
function f26(outer: { readonly obj: { kind: 'foo', foo: string } | { kind: 'bar', bar: number } }) {
    const isFoo = outer.obj.kind === 'foo';
    if (isFoo) {
        outer.obj.foo;  // OK: `obj` is readonly so the chain is constant
    }
    else {
        outer.obj.bar;  // OK: `obj` is readonly so the chain is constant
    }
}
"#;

    let (parser, root) = parse_test_source(source);
    assert!(parser.get_diagnostics().is_empty(), "Parse errors");

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let options = CheckerOptions {
        strict: true,
        ..CheckerOptions::default()
    }
    .apply_strict_defaults();

    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        options,
    );
    checker.check_source_file(root);

    let ts2339: Vec<_> = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 2339)
        .collect();

    assert!(
        ts2339.is_empty(),
        "Expected no TS2339 in f26 (readonly chain narrows correctly), \
         got: {ts2339:#?}"
    );
}

/// Regression test: switch statement narrowing via destructured discriminant alias.
///
/// `const { kind } = obj; switch (kind) { case 'foo': obj.foo; }` should narrow
/// `obj` to the `{ kind: 'foo', foo: string }` branch — no TS2339 on `obj.foo`.
///
/// Fix: `switch_can_affect_reference` now checks `is_aliased_discriminant_switch_expr`
/// so that switch(alias) where `alias` is `const { kind } = obj` allows entry into
/// per-clause narrowing.
#[test]
fn test_switch_narrowing_via_destructured_discriminant_alias() {
    let source = r#"
function f(obj: { kind: 'foo', foo: string } | { kind: 'bar', bar: number }) {
    const { kind } = obj;
    switch (kind) {
        case 'foo': obj.foo; break;
        case 'bar': obj.bar; break;
    }
}
"#;

    let (parser, root) = parse_test_source(source);
    assert!(parser.get_diagnostics().is_empty(), "Parse errors");

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let options = CheckerOptions {
        strict: true,
        ..CheckerOptions::default()
    }
    .apply_strict_defaults();

    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        options,
    );
    checker.check_source_file(root);

    let ts2339: Vec<_> = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 2339)
        .collect();

    assert!(
        ts2339.is_empty(),
        "Expected no TS2339: switch(kind) should narrow obj via destructured discriminant alias, \
         got: {ts2339:#?}"
    );
}

#[test]
fn destructured_boolean_discriminant_truthiness_narrows_source_object() {
    let diagnostics = strict_diagnostics(
        r#"
function processResult(
    result: { ok: true; value: string } | { ok: false; error: string }
): string {
    const { ok } = result;
    if (ok) {
        return result.value;
    }
    return result.error;
}
"#,
    );

    assert!(
        diagnostics.iter().all(|(code, _)| *code != 2339),
        "Expected destructured boolean discriminant to narrow source object, got: {diagnostics:#?}"
    );
}

#[test]
fn renamed_destructured_boolean_discriminant_truthiness_narrows_source_object() {
    let diagnostics = strict_diagnostics(
        r#"
function readState(
    state: { ready: true; payload: number } | { ready: false; reason: string }
) {
    const { ready: isReady } = state;
    if (isReady) {
        const payload: number = state.payload;
    } else {
        const reason: string = state.reason;
    }
}
"#,
    );

    assert!(
        diagnostics.iter().all(|(code, _)| *code != 2339),
        "Expected renamed destructured discriminant to narrow source object, got: {diagnostics:#?}"
    );
}

#[test]
fn non_const_destructured_discriminant_truthiness_does_not_narrow_source_object() {
    let diagnostics = strict_diagnostics(
        r#"
function processResult(
    result: { ok: true; value: string } | { ok: false; error: string }
) {
    let { ok } = result;
    if (ok) {
        return result.value;
    }
    return result.error;
}
"#,
    );

    let ts2339_count = diagnostics.iter().filter(|(code, _)| *code == 2339).count();
    assert_eq!(
        ts2339_count, 2,
        "Expected non-const destructured discriminant not to narrow source object, got: {diagnostics:#?}"
    );
}

#[test]
fn destructured_discriminant_with_default_does_not_narrow_source_object() {
    let diagnostics = strict_diagnostics(
        r#"
function processResult(
    result: { ok: true; value: string } | { ok: false; error: string }
) {
    const { ok = true } = result;
    if (ok) {
        return result.value;
    }
    return result.error;
}
"#,
    );

    let ts2339_count = diagnostics.iter().filter(|(code, _)| *code == 2339).count();
    assert_eq!(
        ts2339_count, 2,
        "Expected defaulted destructured discriminant not to narrow source object, got: {diagnostics:#?}"
    );
}

/// Regression test: aliased condition with loose equality narrows discriminated union.
///
/// `const isFoo = kind == 'foo'; if (isFoo && obj.foo) { ... }` should narrow `obj`
/// to the `{ kind: 'foo', foo?: string }` branch — no TS2339 on `obj.foo`.
///
/// Fix: `discriminant_comparison` (and `literal_comparison`) are now also called for
/// loose equality `==` comparisons, not just strict `===`.
#[test]
fn test_aliased_loose_equality_condition_narrows_discriminant() {
    let source = r#"
function f(obj: { kind: 'foo', foo?: string } | { kind: 'bar', bar?: number }) {
    const { kind } = obj;
    const isFoo = kind == 'foo';
    if (isFoo && obj.foo) {
        let t: string = obj.foo;
    }
}
"#;

    let (parser, root) = parse_test_source(source);
    assert!(parser.get_diagnostics().is_empty(), "Parse errors");

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let options = CheckerOptions {
        strict: true,
        ..CheckerOptions::default()
    }
    .apply_strict_defaults();

    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        options,
    );
    checker.check_source_file(root);

    let ts2339: Vec<_> = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 2339)
        .collect();
    let ts2322: Vec<_> = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 2322)
        .collect();

    assert!(
        ts2339.is_empty() && ts2322.is_empty(),
        "Expected no errors: aliased loose == condition should narrow discriminated union, \
         ts2339={ts2339:#?}, ts2322={ts2322:#?}"
    );
}

#[test]
fn filter_truthiness_callback_does_not_inherit_type_predicate_overload() {
    let diagnostics = strict_diagnostics_with_libs(
        r#"
const values: (number | null)[] = [1, null, 2];
const filtered: number[] = values.filter(x => !!x);
"#,
    );

    assert!(
        diagnostics.iter().any(|(code, _)| *code == 2322),
        "Expected TS2322 because `!!x` should not infer `x is number`, got: {diagnostics:#?}"
    );
}

#[test]
fn mapped_then_filter_truthiness_callback_does_not_inherit_type_predicate_overload() {
    let diagnostics = strict_diagnostics_with_libs(
        r#"
const values: (number | null)[] = [1, null, 2];
const mapped = values.map(x => x);
const filtered: number[] = mapped.filter(x => !!x);
"#,
    );

    assert!(
        diagnostics.iter().any(|(code, _)| *code == 2322),
        "Expected TS2322 after map/filter because `!!x` should not infer `x is number`, got: {diagnostics:#?}"
    );
}

#[test]
fn filter_null_comparison_callback_still_infers_type_predicate() {
    let diagnostics = strict_diagnostics_with_libs(
        r#"
const values: (number | null)[] = [1, null, 2];
const filtered: number[] = values.filter(x => x !== null);
"#,
    );

    assert!(
        diagnostics.iter().all(|(code, _)| *code != 2322),
        "`x !== null` should infer `x is number` for filter, got: {diagnostics:#?}"
    );
}

#[test]
fn contextual_type_guard_assignment_requires_explicit_or_inferred_predicate() {
    let diagnostics = strict_diagnostics(
        r#"
const truthyGuard: (x: number | null) => x is number = x => !!x;
const nullGuard: (x: number | null) => x is number = x => x !== null;
"#,
    );

    let ts2322: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2322)
        .collect();

    assert_eq!(
        ts2322.len(),
        1,
        "Expected exactly one TS2322 for `x => !!x`; `x => x !== null` should infer a predicate. Got: {diagnostics:#?}"
    );
}

#[test]
fn inferred_type_predicate_narrows_discriminated_union_via_arrow_body() {
    // TS 5.5+ inferred type predicates: an arrow function whose body is a
    // single discriminant comparison should be inferred as a type predicate
    // and narrow the discriminated union at the call site.
    let source = r#"
declare const foobar:
  | { type: "foo"; foo: number }
  | { type: "bar"; bar: string };

const foobarPred = (fb: typeof foobar) => fb.type === "foo";
if (foobarPred(foobar)) {
  foobar.foo;
}
"#;

    let diags = strict_diagnostics(source);
    let ts2339: Vec<&(u32, String)> = diags.iter().filter(|d| d.0 == 2339).collect();
    assert!(
        ts2339.is_empty(),
        "Inferred predicate should narrow `foobar` so `.foo` is allowed; ts2339={ts2339:#?}"
    );
}

#[test]
fn inferred_type_predicate_works_when_iteration_var_is_renamed() {
    // The inference rule must not depend on the spelling of the parameter.
    // Renaming `fb` -> `payload` must produce the same predicate.
    let source = r#"
declare const foobar:
  | { type: "foo"; foo: number }
  | { type: "bar"; bar: string };

const isFoo = (payload: typeof foobar) => payload.type === "foo";
if (isFoo(foobar)) {
  foobar.foo;
}
"#;

    let diags = strict_diagnostics(source);
    let ts2339: Vec<&(u32, String)> = diags.iter().filter(|d| d.0 == 2339).collect();
    assert!(
        ts2339.is_empty(),
        "Inferred predicate should be name-independent; ts2339={ts2339:#?}"
    );
}

#[test]
fn inferred_type_predicate_skipped_when_body_does_not_narrow() {
    // When the body cannot narrow the parameter (e.g. a constant boolean), no
    // predicate is inferred. The discriminated union must remain wide and
    // accessing a variant-only property should still error.
    let source = r#"
declare const foobar:
  | { type: "foo"; foo: number }
  | { type: "bar"; bar: string };

const alwaysTrue = (_fb: typeof foobar) => true;
if (alwaysTrue(foobar)) {
  foobar.foo; // should error - no narrowing
}
"#;

    let diags = strict_diagnostics(source);
    let ts2339: Vec<&(u32, String)> = diags.iter().filter(|d| d.0 == 2339).collect();
    assert!(
        !ts2339.is_empty(),
        "Without a narrowing body, .foo should still error on the union; diags={diags:#?}"
    );
}

#[test]
fn inferred_type_predicate_handles_block_body_with_single_return() {
    // A function with a block body that consists of just `return <guard>;`
    // is also eligible for predicate inference.
    let source = r#"
declare const foobar:
  | { type: "foo"; foo: number }
  | { type: "bar"; bar: string };

function isFooBlock(fb: typeof foobar) {
  return fb.type === "foo";
}
if (isFooBlock(foobar)) {
  foobar.foo;
}
"#;

    let diags = strict_diagnostics(source);
    let ts2339: Vec<&(u32, String)> = diags.iter().filter(|d| d.0 == 2339).collect();
    assert!(
        ts2339.is_empty(),
        "Block body with single return should also infer a predicate; ts2339={ts2339:#?}"
    );
}

#[test]
fn inferred_type_predicate_handles_simple_statements_before_return() {
    let source = r#"
function isString(value: unknown) {
  const ignored = 0;
  ignored;
  return typeof value === "string";
}

declare const flag: boolean;
let input: unknown = flag ? "text" : 1;

if (isString(input)) {
  const asString: string = input;
  const asNumber: number = input;

  asString;
  asNumber;
}
"#;

    let diags = strict_diagnostics(source);
    let ts2322: Vec<_> = diags.iter().filter(|(code, _)| *code == 2322).collect();
    assert!(
        !ts2322.iter().any(|(_, message)| {
            message.contains("Type 'unknown' is not assignable to type 'string'")
        }),
        "Inferred predicate should allow assigning input to string; diags={diags:#?}"
    );
    assert!(
        ts2322.len() == 1,
        "Expected only the remaining number assignment to fail; diags={diags:#?}"
    );
}

#[test]
fn inferred_type_predicate_rejects_non_final_return_path() {
    let source = r#"
function isString(value: unknown, flag: boolean) {
  if (flag) {
    return false;
  }
  return typeof value === "string";
}

declare const flag: boolean;
let input: unknown = flag ? "text" : 1;

if (isString(input, flag)) {
  const asString: string = input;
}
"#;

    let diags = strict_diagnostics(source);
    assert!(
        diags.iter().any(|(code, message)| {
            *code == 2322 && message.contains("Type 'unknown' is not assignable to type 'string'")
        }),
        "A block with an alternate return path must not infer a predicate; diags={diags:#?}"
    );
}

#[test]
fn inferred_type_predicate_handles_typeof_guard() {
    // `(x) => typeof x === "string"` should infer `x is string`.
    let source = r#"
const isString = (x: string | number) => typeof x === "string";
declare let v: string | number;
if (isString(v)) {
  const s: string = v;
}
"#;

    let diags = strict_diagnostics(source);
    let ts2322: Vec<&(u32, String)> = diags.iter().filter(|d| d.0 == 2322).collect();
    assert!(
        ts2322.is_empty(),
        "typeof inference should narrow v to string; ts2322={ts2322:#?}"
    );
}

#[test]
fn inferred_type_predicate_handles_instanceof_object_guard() {
    let source = r#"
function isDate(x: object) {
  return x instanceof Date;
}

declare let value: object;
if (isDate(value)) {
  const date: Date = value;
}
"#;

    let diags = strict_diagnostics(source);
    assert!(
        diags.iter().all(|(code, _)| *code != 2322 && *code != 2740),
        "instanceof predicate inference should narrow object to Date; diags={diags:#?}"
    );
}

#[test]
fn inferred_type_predicate_handles_in_guard_through_non_null_assertion() {
    let source = r#"
type Foo = { foo: string };
type Bar = Foo & { bar: string };

function isBar(x: Foo | Bar | null) {
  return "bar" in x!;
}

declare let value: Foo | Bar;
if (isBar(value)) {
  const bar: Bar = value;
}
"#;

    let diags = strict_diagnostics(source);
    assert!(
        diags.iter().all(|(code, _)| *code != 2322 && *code != 2741),
        "in-operator predicate inference should narrow to the member-bearing type; diags={diags:#?}"
    );
}

#[test]
fn inferred_type_predicate_handles_class_methods() {
    let source = r#"
class Inferrer {
  isNumber(x: number | string) {
    return typeof x === "number";
  }
}

declare let value: number | string;
const inferrer = new Inferrer();
if (inferrer.isNumber(value)) {
  const numberValue: number = value;
} else {
  const stringValue: string = value;
}
"#;

    let diags = strict_diagnostics(source);
    let ts2322: Vec<_> = diags.iter().filter(|(code, _)| *code == 2322).collect();
    assert!(
        ts2322.is_empty(),
        "method predicate inference should narrow both branches; ts2322={ts2322:#?}"
    );
}

#[test]
fn inferred_type_predicate_handles_same_parameter_or_guards() {
    let source = r#"
function isNumberOrString(x: unknown) {
  return typeof x === "number" || typeof x === "string";
}

declare let value: unknown;
if (isNumberOrString(value)) {
  const primitive: number | string = value;
}
"#;

    let diags = strict_diagnostics(source);
    let ts2322: Vec<_> = diags.iter().filter(|(code, _)| *code == 2322).collect();
    assert!(
        ts2322.is_empty(),
        "OR guards for the same parameter should infer a union predicate; ts2322={ts2322:#?}"
    );
}

#[test]
fn inferred_type_predicate_allows_throwing_prefix_path() {
    let source = r#"
function assertAndPredicate(x: string | number | Date) {
  if (x instanceof Date) {
    throw new Error();
  }
  return typeof x === "string";
}

declare let value: string | number | Date;
if (assertAndPredicate(value)) {
  const stringValue: string = value;
}
"#;

    let diags = strict_diagnostics(source);
    let ts2322: Vec<_> = diags.iter().filter(|(code, _)| *code == 2322).collect();
    assert!(
        ts2322.is_empty(),
        "throw-only prefix paths should not block predicate inference; ts2322={ts2322:#?}"
    );
}

#[test]
fn inferred_type_predicate_handles_satisfies_boolean_wrapper() {
    let source = r#"
const numbers = [1, 2, null, 3].filter((x) => (x != null) satisfies boolean);
const accepted: number[] = numbers;
"#;

    let diags = strict_diagnostics(source);
    let ts2322: Vec<_> = diags.iter().filter(|(code, _)| *code == 2322).collect();
    assert!(
        ts2322.is_empty(),
        "satisfies boolean should not hide an inferable predicate from filter; ts2322={ts2322:#?}"
    );
}

#[test]
fn inferred_type_predicate_handles_safe_double_negation_truthiness() {
    let source = r#"
type Item = { value: string };
const items = [{ value: "a" }, undefined].filter((item) => !!item);
const accepted: Item[] = items;
"#;

    let diags = strict_diagnostics(source);
    let ts2322: Vec<_> = diags.iter().filter(|(code, _)| *code == 2322).collect();
    assert!(
        ts2322.is_empty(),
        "double-negation truthiness should infer when the falsy branch is only nullish; ts2322={ts2322:#?}"
    );
}

#[test]
fn inferred_type_predicate_rejects_number_double_negation_truthiness() {
    let source = r#"
const isTruthy = (x: number | null) => !!x;
declare let value: number | null;
if (isTruthy(value)) {
  const accepted: number = value;
}
"#;

    let diags = strict_diagnostics(source);
    assert!(
        diags.iter().any(|(code, message)| {
            *code == 2322
                && message.contains("Type 'number | null' is not assignable to type 'number'")
        }),
        "number|null truthiness must not infer because 0 makes the false branch non-nullish; diags={diags:#?}"
    );
}

#[test]
fn inferred_type_predicate_explicit_annotation_still_wins() {
    // When the user wrote a return type, we must NOT override their
    // intent with an inferred predicate. `: boolean` is an explicit choice.
    let source = r#"
declare const foobar:
  | { type: "foo"; foo: number }
  | { type: "bar"; bar: string };

const annotated = (fb: typeof foobar): boolean => fb.type === "foo";
if (annotated(foobar)) {
  foobar.foo; // should error - annotated boolean prevents predicate inference
}
"#;

    let diags = strict_diagnostics(source);
    let ts2339: Vec<&(u32, String)> = diags.iter().filter(|d| d.0 == 2339).collect();
    assert!(
        !ts2339.is_empty(),
        "Explicit `: boolean` annotation must suppress predicate inference; diags={diags:#?}"
    );
}

// ---------------------------------------------------------------------------
// Symbol.hasInstance-aware instanceof narrowing — checker wiring (issue #8779)
// ---------------------------------------------------------------------------
//
// When a constructor carries `[Symbol.hasInstance](v: ...): v is T`, the
// instanceof check is a user-defined type predicate, not a runtime `instanceof`
// check. The narrowing must use type-predicate semantics rather than instanceof
// semantics so that:
//   * primitive union members that are subtypes of T are KEPT in the true branch
//   * primitive union members that are subtypes of T are EXCLUDED in the false branch
// These tests prove the structural rule using multiple variable-name variants
// (per CLAUDE.md §25: the fix must not be keyed to a specific parameter name).

const fn sym_preamble() -> &'static str {
    "interface SymbolConstructor { readonly hasInstance: unique symbol; }\ndeclare var Symbol: SymbolConstructor;\n"
}

/// True branch: `unknown` narrows to the predicate type.
/// Adjacent case 1: parameter named `v`.
#[test]
fn instanceof_has_instance_unknown_source_narrows_to_predicate_type_v() {
    let source = format!(
        r#"{}
class MyArrayLike {{
    static [Symbol.hasInstance](v: unknown): v is MyArrayLike {{ return true; }}
}}
declare var x: unknown;
if (x instanceof MyArrayLike) {{
    x;
}} else {{
    x;
}}
"#,
        sym_preamble()
    );

    let diags = strict_diagnostics(&source);
    // No diagnostics expected — narrowing unknown to MyArrayLike in true branch is clean.
    let ts2339: Vec<_> = diags.iter().filter(|d| d.0 == 2339).collect();
    assert!(
        ts2339.is_empty(),
        "instanceof + hasInstance on unknown should not produce TS2339: {diags:#?}"
    );
}

/// True branch: `unknown` narrows to predicate type.
/// Adjacent case 2: parameter named `value` (proves the rule is not name-keyed).
#[test]
fn instanceof_has_instance_unknown_source_narrows_to_predicate_type_value() {
    let source = format!(
        r#"{}
class Wrapper {{
    static [Symbol.hasInstance](value: unknown): value is Wrapper {{ return true; }}
    inner: number;
}}
declare var x: unknown;
if (x instanceof Wrapper) {{
    x.inner;
}}
"#,
        sym_preamble()
    );

    let diags = strict_diagnostics(&source);
    let ts2339: Vec<_> = diags.iter().filter(|d| d.0 == 2339).collect();
    assert!(
        ts2339.is_empty(),
        "instanceof + hasInstance (param=value) on unknown should not produce TS2339: {diags:#?}"
    );
}

/// The core bug from issue #8779: when the hasInstance predicate type is a
/// primitive (`v is string`), the true branch must KEEP the primitive union
/// member instead of incorrectly excluding it via instanceof's primitive-
/// exclusion rule.
#[test]
fn instanceof_has_instance_primitive_predicate_keeps_primitive_in_true_branch() {
    let source = format!(
        r#"{}
interface StringChecker {{
    [Symbol.hasInstance](v: unknown): v is string;
}}
declare var IsString: StringChecker;
declare var x: string | number;
if (x instanceof IsString) {{
    x;  // x should be `string`
}} else {{
    x;  // x should be `number`
}}
"#,
        sym_preamble()
    );

    // No diagnostics expected. The test validates narrowing by checking that
    // property access on the narrowed type is type-correct.
    let diags = strict_diagnostics(&source);
    let ts_errors: Vec<_> = diags
        .iter()
        .filter(|d| matches!(d.0, 2339 | 2322))
        .collect();
    assert!(
        ts_errors.is_empty(),
        "instanceof + hasInstance (primitive predicate) should not produce type errors: {diags:#?}"
    );
}

/// Adjacent case: parameter named `x` (third name variant, per §25 matrix).
#[test]
fn instanceof_has_instance_primitive_predicate_param_x() {
    let source = format!(
        r#"{}
interface NumChecker {{
    [Symbol.hasInstance](x: unknown): x is number;
}}
declare var IsNum: NumChecker;
declare var val: string | number | boolean;
if (val instanceof IsNum) {{
    val;  // val should be `number`
}} else {{
    val;  // val should be `string | boolean`
}}
"#,
        sym_preamble()
    );

    let diags = strict_diagnostics(&source);
    let ts_errors: Vec<_> = diags
        .iter()
        .filter(|d| matches!(d.0, 2339 | 2322))
        .collect();
    assert!(
        ts_errors.is_empty(),
        "instanceof + hasInstance (param=x, number predicate) should not produce type errors: {diags:#?}"
    );
}

/// False branch: when hasInstance predicate is `v is MyClass`, a primitive union
/// member that is NOT a subtype of `MyClass` must be preserved in the false branch.
/// (Regression: instanceof's primitive-keep rule was wrong for hasInstance
/// predicates whose type is a class, since `string` is not a `MyClass` so it
/// correctly stays in the false branch anyway — this test ensures the class
/// case is not broken by the fix.)
#[test]
fn instanceof_has_instance_class_predicate_false_branch_preserves_non_matching() {
    let source = format!(
        r#"{}
class Tag {{
    static [Symbol.hasInstance](val: unknown): val is Tag {{ return true; }}
    kind: "tag";
}}
declare var x: string | Tag;
if (x instanceof Tag) {{
    x.kind;
}} else {{
    x;  // should be `string`
}}
"#,
        sym_preamble()
    );

    let diags = strict_diagnostics(&source);
    let ts2339: Vec<_> = diags
        .iter()
        .filter(|d| d.0 == 2339)
        .filter(|d| d.1.contains("'kind'"))
        .collect();
    assert!(
        ts2339.is_empty(),
        "instanceof + hasInstance class predicate: true branch should see Tag.kind: {diags:#?}"
    );
}

/// `instanceof` without `[Symbol.hasInstance]` must still use instanceof
/// semantics (primitive-exclusion) — the fix must not change this.
#[test]
fn instanceof_without_has_instance_still_excludes_primitives() {
    let source = r#"
class Foo { x: number; }
declare var y: string | Foo;
if (y instanceof Foo) {
    y.x;
} else {
    y;
}
"#;

    let diags = strict_diagnostics(source);
    let ts2339: Vec<_> = diags
        .iter()
        .filter(|d| d.0 == 2339 && d.1.contains("'x'"))
        .collect();
    assert!(
        ts2339.is_empty(),
        "regular instanceof (no hasInstance) true branch should see Foo.x: {diags:#?}"
    );
}

// ── Destructured discriminant narrows source binding (issue #8780) ───────────

/// When `const { kind } = s` is destructured and then `kind === "a"` is checked,
/// the source binding `s` should be narrowed to the matching discriminated-union
/// variant.  tsc narrows `s` so `s.a` is reachable without TS2339.
#[test]
fn destructured_discriminant_equality_narrows_source_binding() {
    let diagnostics = strict_diagnostics(
        r#"
type S = { kind: "a"; a: number } | { kind: "b"; b: string };
function f(s: S) {
    const { kind } = s;
    if (kind === "a") {
        const _x: number = s.a;
    } else {
        const _y: string = s.b;
    }
}
"#,
    );
    let ts2339: Vec<_> = diagnostics.iter().filter(|(c, _)| *c == 2339).collect();
    let ts2322: Vec<_> = diagnostics.iter().filter(|(c, _)| *c == 2322).collect();
    assert!(
        ts2339.is_empty() && ts2322.is_empty(),
        "Destructured discriminant `kind === \"a\"` must narrow source binding `s`; \
         ts2339={ts2339:#?}, ts2322={ts2322:#?}"
    );
}

/// Renamed destructuring: `const { kind: k } = s; if (k === "a") { s.a; }` must
/// narrow `s` just like the shorthand form.
#[test]
fn renamed_destructured_discriminant_equality_narrows_source_binding() {
    let diagnostics = strict_diagnostics(
        r#"
type S = { kind: "a"; a: number } | { kind: "b"; b: string };
function f(s: S) {
    const { kind: k } = s;
    if (k === "a") {
        const _x: number = s.a;
    } else {
        const _y: string = s.b;
    }
}
"#,
    );
    let ts2339: Vec<_> = diagnostics.iter().filter(|(c, _)| *c == 2339).collect();
    let ts2322: Vec<_> = diagnostics.iter().filter(|(c, _)| *c == 2322).collect();
    assert!(
        ts2339.is_empty() && ts2322.is_empty(),
        "Renamed destructured discriminant `k === \"a\"` must narrow source binding `s`; \
         ts2339={ts2339:#?}, ts2322={ts2322:#?}"
    );
}

/// Rest binding on the same top-level destructure must not hide the discriminant
/// alias relationship between the extracted property and the source binding.
#[test]
fn destructured_discriminant_with_rest_narrows_source_binding() {
    let diagnostics = strict_diagnostics(
        r#"
type S = { kind: "a"; a: number } | { kind: "b"; b: string };
function f(s: S) {
    const { kind, ...rest } = s;
    rest;
    if (kind === "a") {
        const _x: number = s.a;
    } else {
        const _y: string = s.b;
    }
}
"#,
    );
    let ts2339: Vec<_> = diagnostics.iter().filter(|(c, _)| *c == 2339).collect();
    let ts2322: Vec<_> = diagnostics.iter().filter(|(c, _)| *c == 2322).collect();
    assert!(
        ts2339.is_empty() && ts2322.is_empty(),
        "Rest binding must not prevent destructured discriminant narrowing of source binding; \
         ts2339={ts2339:#?}, ts2322={ts2322:#?}"
    );
}

/// Non-const destructured discriminant must NOT narrow the source binding.
/// `let { kind } = s; if (kind === "a") { s.a; }` — `s` remains `S`, TS2339 expected.
#[test]
fn non_const_destructured_discriminant_equality_does_not_narrow_source_binding() {
    let diagnostics = strict_diagnostics(
        r#"
type S = { kind: "a"; a: number } | { kind: "b"; b: string };
function f(s: S) {
    let { kind } = s;
    if (kind === "a") {
        s.a;
    }
}
"#,
    );
    let ts2339_count = diagnostics.iter().filter(|(c, _)| *c == 2339).count();
    assert_eq!(
        ts2339_count, 1,
        "Non-const destructured discriminant must not narrow source binding; \
         got: {diagnostics:#?}"
    );
}

/// Nested destructuring: `const { s: { kind } } = outer; if (kind === "a") { outer.s.a; }`
/// must not narrow `outer.s`; current `tsc` still reports TS2339 for both branches.
#[test]
fn nested_destructured_discriminant_does_not_narrow_root_binding() {
    let diagnostics = strict_diagnostics(
        r#"
type S = { kind: "a"; a: number } | { kind: "b"; b: string };
function f(outer: { s: S }) {
    const { s: { kind } } = outer;
    if (kind === "a") {
        const _x: number = outer.s.a;
    } else {
        const _y: string = outer.s.b;
    }
}
"#,
    );
    let ts2339: Vec<_> = diagnostics.iter().filter(|(c, _)| *c == 2339).collect();
    assert_eq!(
        ts2339.len(),
        2,
        "Nested destructured discriminant must not narrow `outer.s`; got {diagnostics:#?}"
    );
}

/// Nested destructuring with a renamed leaf: `const { s: { kind: k } } = outer`
/// must not narrow `outer.s`.
#[test]
fn nested_destructured_renamed_discriminant_does_not_narrow_root_binding() {
    let diagnostics = strict_diagnostics(
        r#"
type S = { kind: "a"; a: number } | { kind: "b"; b: string };
function f(outer: { s: S }) {
    const { s: { kind: k } } = outer;
    if (k === "a") {
        const _x: number = outer.s.a;
    } else {
        const _y: string = outer.s.b;
    }
}
"#,
    );
    let ts2339: Vec<_> = diagnostics.iter().filter(|(c, _)| *c == 2339).collect();
    assert_eq!(
        ts2339.len(),
        2,
        "Renamed nested destructured discriminant must not narrow `outer.s`; got {diagnostics:#?}"
    );
}

/// Three-level nesting: `const { a: { b: { kind } } } = d; if (kind === "x") { d.a.b.x; }`
/// must not narrow `d.a.b`.
#[test]
fn triple_nested_destructured_discriminant_does_not_narrow_root_binding() {
    let diagnostics = strict_diagnostics(
        r#"
type Leaf = { kind: "x"; x: number } | { kind: "y"; y: string };
type Deep = { a: { b: Leaf } };
function f(d: Deep) {
    const { a: { b: { kind } } } = d;
    if (kind === "x") {
        const _x: number = d.a.b.x;
    } else {
        const _y: string = d.a.b.y;
    }
}
"#,
    );
    let ts2339: Vec<_> = diagnostics.iter().filter(|(c, _)| *c == 2339).collect();
    assert_eq!(
        ts2339.len(),
        2,
        "Triple-nested destructured discriminant must not narrow root binding; got {diagnostics:#?}"
    );
}
