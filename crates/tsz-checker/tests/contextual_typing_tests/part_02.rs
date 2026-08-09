//! Contiguous test shard split out of the parent module to satisfy the
//! source-file line cap.

use super::*;

// Regression tests for contextual typing of array literals against an
// *ambiguous* union of array shapes (issue family: distributive-conditional
// tuple-union evaluation becomes over-constrained, #12175).
//
// When the contextual type is a union of two or more distinct applicable array
// shapes, the per-element contextual type used to be cleared so closures would
// fall back to implicit-any (TS7006). That also widened a nested array literal
// (`[1]` -> `number[]`), so an inner element no longer matched a tuple-element
// arm and produced a spurious TS2322. The fix threads the union of the arms'
// element types back to the element (tsc's `mapType` over a union contextual
// type), which fixes the nested-literal cases while preserving the closure
// implicit-any behavior. Each test varies a structural axis (arity, readonly,
// aliasing, mixed function/tuple arms, nesting depth) to guard against a
// single-shape fix.

fn ts2322(diagnostics: &[Diagnostic]) -> Vec<&Diagnostic> {
    diagnostics.iter().filter(|d| d.code == 2322).collect()
}

#[test]
fn union_of_tuple_arrays_nested_literal_no_false_ts2322() {
    // `[1]` must be contextually typed as the tuple `[number]` (from the first
    // arm's element type), not widened to `number[]`.
    let source = r#"
const a: [number][] | [string, boolean][] = [[1]];
const b: [number][] | [string, boolean][] = [["x", true]];
"#;
    let diagnostics = check_default(source);
    assert!(
        ts2322(&diagnostics).is_empty(),
        "union-of-tuple-arrays element should keep tuple shape, got: {diagnostics:?}"
    );
}

#[test]
fn union_of_tuple_arrays_via_distributive_conditional() {
    // The conditional evaluates correctly to `[number][] | [string, boolean][]`;
    // the contextual element typing of `[[1]]` against it must not widen.
    let source = r#"
type ToArr<T> = T extends any ? T[] : never;
type R = ToArr<[number] | [string, boolean]>;
const a: R = [[1]];
const b: R = [["x", true]];
"#;
    let diagnostics = check_default(source);
    assert!(
        ts2322(&diagnostics).is_empty(),
        "distributive-conditional tuple-union element typing should not widen, got: {diagnostics:?}"
    );
}

#[test]
fn union_of_readonly_tuple_arrays_nested_literal() {
    // `readonly` arms must be unwrapped for element extraction.
    let source = r#"
const a: readonly [number][] | readonly [string, boolean][] = [[1]];
"#;
    let diagnostics = check_default(source);
    assert!(
        ts2322(&diagnostics).is_empty(),
        "readonly union-of-tuple-arrays element should keep tuple shape, got: {diagnostics:?}"
    );
}

#[test]
fn union_of_tuple_arrays_different_arity() {
    let source = r#"
const a: [number, string, boolean][] | [1][] = [[1]];
const b: [number][] | [string][] | [boolean][] = [[true]];
"#;
    let diagnostics = check_default(source);
    assert!(
        ts2322(&diagnostics).is_empty(),
        "differing-arity union arms should still type the element per-arm, got: {diagnostics:?}"
    );
}

#[test]
fn union_of_aliased_tuple_arrays_renamed_binders() {
    // Same shape via aliases with renamed type names: the fix must be structural,
    // not keyed on any identifier.
    let source = r#"
type Pair = [number];
type Trip = [string, boolean];
const a: Pair[] | Trip[] = [[1]];
type AltPair = [number];
type AltTrip = [string, boolean];
const b: AltPair[] | AltTrip[] = [[1]];
"#;
    let diagnostics = check_default(source);
    assert!(
        ts2322(&diagnostics).is_empty(),
        "aliased/renamed union arms should behave identically, got: {diagnostics:?}"
    );
}

#[test]
fn deeply_nested_union_of_tuple_arrays() {
    // The widening bug recurses; the contextual union must thread through each
    // nesting level.
    let source = r#"
const x: [number][][] | [string][][] = [[[1]]];
"#;
    let diagnostics = check_default(source);
    assert!(
        ts2322(&diagnostics).is_empty(),
        "nested union-of-tuple-arrays should not widen at any depth, got: {diagnostics:?}"
    );
}

#[test]
fn union_mixed_function_and_tuple_array_picks_tuple_arm() {
    // A nested tuple literal must still pick the tuple arm even when another arm
    // is a function-typed array.
    let source = r#"
const x: ((a: number) => void)[] | [string, number][] = [["a", 1]];
"#;
    let diagnostics = check_default(source);
    assert!(
        ts2322(&diagnostics).is_empty(),
        "mixed fn|tuple union should let a tuple literal pick the tuple arm, got: {diagnostics:?}"
    );
}

#[test]
fn union_mixed_function_and_tuple_array_closure_picks_fn_arm() {
    // A closure element must pick the function arm (param `a: number`), so no
    // implicit-any is reported.
    let source = r#"
const x: ((a: number) => void)[] | [string, number][] = [a => {}];
"#;
    let diagnostics = check_with_options(
        source,
        CheckerOptions {
            strict: true,
            no_implicit_any: true,
            ..Default::default()
        },
    );
    let relevant: Vec<_> = diagnostics
        .iter()
        .filter(|d| matches!(d.code, 7006 | 2322))
        .collect();
    assert!(
        relevant.is_empty(),
        "closure should adopt the function arm's parameter type, got: {relevant:?}"
    );
}

#[test]
fn union_of_conflicting_function_arrays_still_reports_implicit_any() {
    // When the only applicable arms are function arrays whose parameters genuinely
    // conflict, the closure parameter stays implicit-any (TS7006), matching tsc.
    let source = r#"
const x: ((a: string) => void)[] | ((a: number) => void)[] = [a => {}];
"#;
    let diagnostics = check_with_options(
        source,
        CheckerOptions {
            strict: true,
            no_implicit_any: true,
            ..Default::default()
        },
    );
    assert!(
        diagnostics.iter().any(|d| d.code == 7006),
        "conflicting function-array arms should still emit TS7006, got: {diagnostics:?}"
    );
}

#[test]
fn union_of_object_arrays_nested_literal_matches_arm() {
    let source = r#"
const a: { a: number }[] | { b: string }[] = [{ a: 1 }];
const b: { a: number }[] | { b: string }[] = [{ b: "x" }];
"#;
    let diagnostics = check_default(source);
    assert!(
        ts2322(&diagnostics).is_empty(),
        "union-of-object-arrays element should match the right arm, got: {diagnostics:?}"
    );
}

// Regression tests for #14171: when a generic class is constructed in a
// contextual-return position, the contextual class type must seed the
// construct-signature's type-parameter inference. The construct argument here
// only constrains `S` (via the present `schema` property); `T` is reachable
// only through the *omitted* optional `refiner` member, so round-1 argument
// inference must not falsely mark `T` as covered and skip contextual-return
// seeding — otherwise `T` falls back to `unknown` (spurious TS2322).
fn strict() -> CheckerOptions {
    CheckerOptions {
        strict: true,
        ..Default::default()
    }
}

#[test]
fn generic_new_seeds_type_param_from_contextual_return() {
    let source = r#"
class Box<T, S> {
  value!: T;
  schema: S;
  set: (v: T) => void;
  constructor(props: { schema: S; refiner?: (value: T) => boolean }) {
    this.schema = props.schema;
    this.set = () => {};
  }
}
function define(): Box<string, null> {
  return new Box({ schema: null });
}
"#;
    let diagnostics = check_with_options(source, strict());
    assert!(
        ts2322(&diagnostics).is_empty(),
        "contextual return type Box<string, null> should infer T = string, got: {diagnostics:?}"
    );
}

#[test]
fn generic_new_contextual_return_renamed_binders() {
    // Structural, not identifier-keyed: rename every type parameter and property.
    let source = r#"
class Container<Elem, Sch> {
  val!: Elem;
  cfg: Sch;
  apply: (v: Elem) => void;
  constructor(opts: { cfg: Sch; refine?: (value: Elem) => boolean }) {
    this.cfg = opts.cfg;
    this.apply = () => {};
  }
}
function make(): Container<number, boolean> {
  return new Container({ cfg: true });
}
"#;
    let diagnostics = check_with_options(source, strict());
    assert!(
        ts2322(&diagnostics).is_empty(),
        "renamed-binder construct should infer Elem = number from contextual return, got: {diagnostics:?}"
    );
}

// Minimal generic class whose constructor argument only constrains `S`; `T` is
// reachable solely through the omitted optional `refiner` member. Shared by the
// alias / arrow-body / negative-control variants below.
const BOX_CLASS: &str = r#"
class Box<T, S> {
  value!: T;
  schema: S;
  constructor(props: { schema: S; refiner?: (value: T) => boolean }) {
    this.schema = props.schema;
  }
}
"#;

#[test]
fn generic_new_contextual_return_through_alias() {
    // The contextual return type is an alias of the generic application.
    let source = format!(
        "{BOX_CLASS}\ntype Aliased = Box<string, null>;\n\
         function define(): Aliased {{ return new Box({{ schema: null }}); }}"
    );
    let diagnostics = check_with_options(&source, strict());
    assert!(
        ts2322(&diagnostics).is_empty(),
        "aliased contextual return type should seed T = string, got: {diagnostics:?}"
    );
}

#[test]
fn generic_new_contextual_return_arrow_body() {
    // Arrow expression body is also a contextual-return position.
    let source = format!(
        "{BOX_CLASS}\nconst define = (): Box<number, null> => new Box({{ schema: null }});"
    );
    let diagnostics = check_with_options(&source, strict());
    assert!(
        ts2322(&diagnostics).is_empty(),
        "arrow-body contextual return should seed T = number, got: {diagnostics:?}"
    );
}

#[test]
fn generic_new_argument_inference_still_overrides_contextual_return() {
    // Negative control: when the argument *does* constrain T (via the provided
    // `refiner`), argument inference must win over the contextual return type,
    // so the genuine mismatch is still reported (T = number vs declared string).
    let source = format!(
        "{BOX_CLASS}\nfunction define(): Box<string, null> {{\n  \
         return new Box({{ schema: null, refiner: (value: number) => true }});\n}}"
    );
    let diagnostics = check_with_options(&source, strict());
    assert!(
        !ts2322(&diagnostics).is_empty(),
        "argument-supplied T = number must override contextual return and still report TS2322, got: {diagnostics:?}"
    );
}

// Regression tests for #14822: the surviving callback-argument variant of
// #14171. When the construct argument supplies the optional `refiner` callback
// with an *unannotated* parameter, `T` is reachable only through that
// callback's contravariant parameter position. Round-1 argument inference must
// not treat that as pinning `T` (the parameter is contextually typed, not an
// inference source), so the contextual return type still seeds the class type
// parameter. Otherwise `T` falls back to `unknown` -> spurious TS2322.
const BOX_WITH_TYPE: &str = r#"
declare class Box<S, T> {
  readonly TYPE: T;
  schema: S;
  constructor(props: { schema: S; refiner?: (value: T) => boolean });
}
"#;

#[test]
fn generic_new_callback_arg_seeds_type_param_from_contextual_return() {
    // Return-annotation position with an unannotated callback parameter.
    let source = format!(
        "{BOX_WITH_TYPE}\nfunction viaNew<T>(): Box<unknown, T> {{\n  \
         return new Box({{ schema: null, refiner(value) {{ return true; }} }});\n}}"
    );
    let diagnostics = check_with_options(&source, strict());
    assert!(
        ts2322(&diagnostics).is_empty(),
        "unannotated callback arg must not pin T; contextual return Box<unknown, T> should seed it, got: {diagnostics:?}"
    );
}

#[test]
fn generic_new_callback_arg_seeds_type_param_via_variable_annotation() {
    // Variable-annotation position is also a contextual-return site.
    let source = format!(
        "{BOX_WITH_TYPE}\nfunction wrap<T>(): void {{\n  \
         const x: Box<unknown, T> = new Box({{ schema: null, refiner(value) {{ return true; }} }});\n  \
         void x;\n}}"
    );
    let diagnostics = check_with_options(&source, strict());
    assert!(
        ts2322(&diagnostics).is_empty(),
        "variable-annotated contextual return should seed T through the callback-arg construct, got: {diagnostics:?}"
    );
}

#[test]
fn generic_new_callback_arg_contextual_return_renamed_binders() {
    // Structural, not identifier-keyed: rename every type parameter and member.
    let source = r#"
declare class Holder<Cfg, Elem> {
  readonly OUT: Elem;
  cfg: Cfg;
  constructor(opts: { cfg: Cfg; refine?: (item: Elem) => boolean });
}
function build<Elem>(): Holder<unknown, Elem> {
  return new Holder({ cfg: null, refine(item) { return true; } });
}
"#;
    let diagnostics = check_with_options(source, strict());
    assert!(
        ts2322(&diagnostics).is_empty(),
        "renamed-binder callback-arg construct should seed Elem from contextual return, got: {diagnostics:?}"
    );
}

// Regression tests: under `exactOptionalPropertyTypes`, the contextual type
// fed to a *present* optional object-literal property or tuple element value
// must be the property/element's bare declared type (e.g. `number` for
// `y?: number`), not the read-side type with `undefined` unioned in. That
// contextual type seeds generic inference for an unannotated callback
// argument placed in the slot (e.g. `match(y => ...)`), so getting it wrong
// pins the inferred type parameter to `T | undefined` and produces spurious
// TS18048/TS2322/TS2379 inside and around the callback. `false` (the
// default) is unaffected: the read-side `T | undefined` is correct there.
fn exact_optional() -> CheckerOptions {
    CheckerOptions {
        exact_optional_property_types: true,
        ..CheckerOptions::default()
    }
}

const MATCH_DECL: &str = "declare function match<T>(cb: (value: T) => boolean): T;";

#[test]
fn exact_optional_object_property_generic_inference_drops_sugar_undefined() {
    let source = format!(
        "{MATCH_DECL}\n\
         declare function foo(pos: {{ x?: number; y?: number }}): boolean;\n\
         foo({{ y: match(y => y > 0) }});"
    );
    let diagnostics = check_with_options(&source, exact_optional());
    assert!(
        diagnostics.is_empty(),
        "exactOptionalPropertyTypes should type the callback param as plain `number` for a sugar-optional `y?: number` slot, got: {diagnostics:?}"
    );
}

#[test]
fn exact_optional_object_property_generic_inference_keeps_undefined_without_exact_flag() {
    // Same source, default (non-exact) options: the callback param must stay
    // `number | undefined`, so the possibly-undefined comparison still fires.
    let source = format!(
        "{MATCH_DECL}\n\
         declare function foo(pos: {{ x?: number; y?: number }}): boolean;\n\
         foo({{ y: match(y => y > 0) }});"
    );
    let diagnostics = check_with_options(&source, CheckerOptions::default());
    assert_eq!(
        diagnostics.iter().filter(|d| d.code == 18048).count(),
        1,
        "non-exact optional property should keep `undefined` in the callback's contextual param type, got: {diagnostics:?}"
    );
}

#[test]
fn exact_optional_tuple_element_generic_inference_drops_sugar_undefined() {
    let source = format!(
        "{MATCH_DECL}\n\
         declare function foo2(point: [number?]): boolean;\n\
         foo2([match(y => y > 0)]);"
    );
    let diagnostics = check_with_options(&source, exact_optional());
    assert!(
        diagnostics.is_empty(),
        "exactOptionalPropertyTypes should type the callback param as plain `number` for a sugar-optional tuple slot, got: {diagnostics:?}"
    );
}

#[test]
fn exact_optional_tuple_element_generic_inference_keeps_undefined_without_exact_flag() {
    let source = format!(
        "{MATCH_DECL}\n\
         declare function foo2(point: [number?]): boolean;\n\
         foo2([match(y => y > 0)]);"
    );
    let diagnostics = check_with_options(&source, CheckerOptions::default());
    assert_eq!(
        diagnostics.iter().filter(|d| d.code == 18048).count(),
        1,
        "non-exact optional tuple element should keep `undefined` in the callback's contextual param type, got: {diagnostics:?}"
    );
}

#[test]
fn exact_optional_required_property_generic_inference_unaffected() {
    // A required (non-optional) property's contextual type is unaffected by
    // `exactOptionalPropertyTypes` either way — negative control.
    let source = format!(
        "{MATCH_DECL}\n\
         declare function foo(pos: {{ y: number }}): boolean;\n\
         foo({{ y: match(y => y > 0) }});"
    );
    let diagnostics = check_with_options(&source, exact_optional());
    assert!(
        diagnostics.is_empty(),
        "required property's contextual type must stay plain `number` regardless of exactOptionalPropertyTypes, got: {diagnostics:?}"
    );
}

#[test]
fn exact_optional_object_property_generic_inference_renamed_binders() {
    // Structural, not identifier-keyed: rename the generic function, type
    // parameter, callback parameter, and target property/function names.
    let source = "declare function pick<Val>(select: (candidate: Val) => boolean): Val;\n\
         declare function build(opts: { count?: number }): boolean;\n\
         build({ count: pick(candidate => candidate > 0) });";
    let diagnostics = check_with_options(source, exact_optional());
    assert!(
        diagnostics.is_empty(),
        "renamed-binder sugar-optional property should still drop `undefined` from the callback's contextual param, got: {diagnostics:?}"
    );
}

// Reviewer-reported gap (mohsen1, PR #17054): the object-literal-property fix
// above only reached an *inline* object contextual type (a call parameter's
// `{ y?: T }`). A *named* interface/type-alias target is a `Lazy(DefId)`
// reference that the assignment-type lookup does not resolve on its own, so
// the override never found the property and the bug persisted for a
// variable-declaration-annotated literal (`const s: S = { y: ... }`).

#[test]
fn exact_optional_named_interface_property_generic_inference_drops_sugar_undefined() {
    let source = format!(
        "{MATCH_DECL}\n\
         interface S {{ y?: number }}\n\
         const s: S = {{ y: match(y => y > 0) }};"
    );
    let diagnostics = check_with_options(&source, exact_optional());
    assert!(
        diagnostics.is_empty(),
        "a named interface's sugar-optional property must drop `undefined` from the callback's contextual param just like an inline object type does, got: {diagnostics:?}"
    );
}

#[test]
fn exact_optional_named_interface_property_generic_inference_keeps_undefined_without_exact_flag() {
    let source = format!(
        "{MATCH_DECL}\n\
         interface S {{ y?: number }}\n\
         const s: S = {{ y: match(y => y > 0) }};"
    );
    let diagnostics = check_with_options(&source, CheckerOptions::default());
    assert_eq!(
        diagnostics.iter().filter(|d| d.code == 18048).count(),
        1,
        "non-exact named-interface optional property should keep `undefined` in the callback's contextual param type, got: {diagnostics:?}"
    );
}

#[test]
fn exact_optional_named_type_alias_property_generic_inference_drops_sugar_undefined() {
    let source = format!(
        "{MATCH_DECL}\n\
         type S = {{ y?: number }};\n\
         const s: S = {{ y: match(y => y > 0) }};"
    );
    let diagnostics = check_with_options(&source, exact_optional());
    assert!(
        diagnostics.is_empty(),
        "a named type alias's sugar-optional property must drop `undefined` from the callback's contextual param, got: {diagnostics:?}"
    );
}

#[test]
fn exact_optional_named_tuple_alias_element_generic_inference_drops_sugar_undefined() {
    // The tuple side already resolved `Lazy` aliases correctly before this
    // fix (its evaluator has its own Lazy-evaluation arm); this is a
    // regression guard, not a new behavior change.
    let source = format!(
        "{MATCH_DECL}\n\
         type Tup = [number?];\n\
         const t: Tup = [match(y => y > 0)];"
    );
    let diagnostics = check_with_options(&source, exact_optional());
    assert!(
        diagnostics.is_empty(),
        "a named tuple type alias's sugar-optional element must drop `undefined` from the callback's contextual param, got: {diagnostics:?}"
    );
}
