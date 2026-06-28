//! Regression tests for issue #14750.
//!
//! When a generic function has a type parameter that appears *only* in return
//! position and carries a default equal to its constraint
//! (`<D extends C = C>(...): R & D`), tsc infers `D` from the contextual
//! assignment target (`inferToMultipleTypes`, single naked type-variable
//! conjunct of an intersection target). tsz previously recorded only an upper
//! bound for `D`, so resolution treated "default + upper-bounds-only" as "no
//! inference happened" and fell back to the default — yielding a wrong result
//! type and a false `TS2322`/`TS2741`. The `mobx` canary row hit this across the
//! whole `createDecoratorAnnotation` surface.
//!
//! Binder names (type-parameter, property, and alias spellings) deliberately
//! vary between cases so the fix cannot be a name- or text-scoped fast path.

use tsz_checker::test_utils::check_source_strict_codes;

fn assignment_error_count(source: &str) -> usize {
    check_source_strict_codes(source)
        .into_iter()
        // TS2322 (not assignable) / TS2741 (missing property) are the
        // false-positive families this issue produces.
        .filter(|code| *code == 2322 || *code == 2741)
        .count()
}

#[test]
fn minimal_object_constraint_inferred_from_variable_annotation() {
    // Reported minimal repro (with a renamed binder + properties).
    let source = r#"
declare function build<Slot extends { marker: string } = { marker: string }>(): { payload: number } & Slot
interface Target { payload: number; marker: "a"; extra: boolean }
const value: Target = build()
"#;
    assert_eq!(
        assignment_error_count(source),
        0,
        "return-only defaulted `Slot` must be inferred from the contextual `Target`, not its default"
    );
}

#[test]
fn mobx_faithful_decorator_union_constraint() {
    // mobx `createDecoratorAnnotation` shape: a return-only `Deco` whose default
    // equals a union constraint, assigned to an intersection target.
    let source = r#"
interface MethodCtx<This, Value> { kind_: "method" }
interface FieldCtx<This, Value> { kind_: "field" }
type MethodDeco<This = any, Value extends (...p: any[]) => any = any> = (value: Value, context: MethodCtx<This, Value>) => Value | void
type FieldDeco<This = any, Value extends (...p: any[]) => any = any> = (value: Value, context: FieldCtx<This, Value>) => Value | void
type AnyDeco = MethodDeco | FieldDeco
interface Marker { annotationType_: string }
type PropDeco = (target: object, key: PropertyKey) => void
declare function makeAnnotation<Deco extends AnyDeco = AnyDeco>(a: Marker): PropDeco & Marker & Deco
declare const marker: Marker
const bound: Marker & PropDeco & MethodDeco & FieldDeco = makeAnnotation(marker)
"#;
    assert_eq!(
        assignment_error_count(source),
        0,
        "return-only defaulted `Deco` (union constraint) must be inferred from the contextual intersection target"
    );
}

#[test]
fn inferred_from_satisfies_target() {
    let source = r#"
declare function assemble<Piece extends { tag: string } = { tag: string }>(): { count: number } & Piece
const out = assemble() satisfies { count: number; tag: "x"; flag: boolean }
"#;
    assert_eq!(
        assignment_error_count(source),
        0,
        "a `satisfies` target must seed the contextual return candidate the same way a variable annotation does"
    );
}

#[test]
fn inferred_from_function_return_context() {
    let source = r#"
declare function spawn<Cell extends { id: string } = { id: string }>(): { seq: number } & Cell
interface Result { seq: number; id: "z"; live: boolean }
function produce(): Result {
  return spawn()
}
"#;
    assert_eq!(
        assignment_error_count(source),
        0,
        "a contextual return statement must seed the contextual return candidate"
    );
}

#[test]
fn default_still_applies_without_contextual_type() {
    // Negative/fallback control: with NO contextual type at the call site, `Slot`
    // must fall back to its default `{ marker: string }`. Assigning that defaulted
    // result to a wider target must still report the missing-property error — both
    // tsc and tsz reject this.
    let source = r#"
declare function build<Slot extends { marker: string } = { marker: string }>(): { payload: number } & Slot
const loose = build()
const widened: { payload: number; marker: string; extra: boolean } = loose
"#;
    assert_eq!(
        assignment_error_count(source),
        1,
        "with no contextual type the default must still apply, leaving `extra` missing"
    );
}

#[test]
fn non_generic_intersection_source_still_rejected() {
    // Control: writing the source non-generically (no inference at all) keeps the
    // union-in-intersection rejection that both tsc and tsz produce. The fix lives
    // entirely on the generic-call inference path, so it must not silence this
    // genuine mismatch. Plain, structurally distinct object brands avoid the
    // `any`-bivariance of function decorators so the mismatch is unambiguous.
    let source = r#"
interface MethodDeco { __method: true }
interface FieldDeco { __field: true }
type AnyDeco = MethodDeco | FieldDeco
interface Marker { annotationType_: string }
interface PropDeco { __prop: true }
declare const src: PropDeco & Marker & AnyDeco
const bound: Marker & PropDeco & MethodDeco & FieldDeco = src
"#;
    assert!(
        assignment_error_count(source) >= 1,
        "non-generic union-in-intersection source must remain rejected (parity with tsc)"
    );
}

#[test]
fn plain_object_decorator_brands_generic_path_clean() {
    // Positive flip mirroring the mobx shape with plain, structurally distinct
    // object brands (no function/`any` bivariance): the generic return-only
    // `Deco` must be inferred from the contextual intersection target.
    let source = r#"
interface MethodDeco { __method: true }
interface FieldDeco { __field: true }
type AnyDeco = MethodDeco | FieldDeco
interface Marker { annotationType_: string }
interface PropDeco { __prop: true }
declare function makeAnnotation<Deco extends AnyDeco = AnyDeco>(a: Marker): PropDeco & Marker & Deco
declare const marker: Marker
const bound: Marker & PropDeco & MethodDeco & FieldDeco = makeAnnotation(marker)
"#;
    assert_eq!(
        assignment_error_count(source),
        0,
        "the generic return-only `Deco` must be inferred from the contextual target even with plain object brands"
    );
}
