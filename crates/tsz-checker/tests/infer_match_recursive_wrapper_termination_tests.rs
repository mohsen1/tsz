//! Termination guards for #10662: `tsz` must always terminate when
//! infer-pattern matching expands a recursive generic-wrapper application.
//!
//! Background: when a conditional's `extends` clause is (or expands to) an
//! object pattern containing `infer`, and the check type is an `Application` of
//! a generic wrapper, matching expands the source application in a *fresh*
//! sub-evaluator (the matching helpers only hold `&self`). Each fresh evaluator
//! resets its own recursion guard, depth counter, and per-`DefId` depth, so a
//! recursive wrapper can re-enter that expansion at ever-deeper nesting through
//! a brand-new evaluator each level — none of the per-evaluator guards fire
//! because they all live on a single evaluator. On the real Zod fixture this
//! hung past 240s with zero output. The cross-evaluator infer-match expansion
//! budget (`MAX_INFER_MATCH_EXPANSION_DEPTH` in `tsz-solver`) cuts the recursion
//! off so the compile always terminates; the guard primitive itself is unit
//! tested in `tsz-solver`.
//!
//! These checker tests exercise the recursive-wrapper infer-extraction family
//! and assert it terminates and stays free of spurious depth diagnostics. The
//! authoritative non-termination reproduction is the Zod project-corpus row
//! (`benchmark_set: required`), which CI runs against the full fixture.

use tsz_checker::test_utils::check_source_codes;

/// A self-referential wrapper whose `_output` member is computed by the very
/// same `infer`-extracting conditional that is matching it. Expanding the
/// source application to read `_output` re-enters the conditional, which
/// expands the source again — through a fresh sub-evaluator each level.
#[test]
fn self_referential_output_infer_extraction_terminates() {
    let source = r#"
interface Schema<Output> {
  _output: Output;
}

interface Lazy<T extends Schema<any>> extends Schema<InferOutput<Lazy<T>>> {
  inner: T;
}

type InferOutput<T> = T extends Schema<infer O> ? O : never;

declare const s: Lazy<Schema<number>>;
type Out = InferOutput<typeof s>;
declare const out: Out;

export {};
"#;
    // The assertion that matters is that this returns at all. A hang times out
    // the entire test binary.
    let _codes = check_source_codes(source);
}

/// Same structural rule with every user-chosen name changed (wrapper, inner
/// field, type parameters, iteration variable) to prove the fix keys on the
/// type *shape*, not on a spelling.
#[test]
fn self_referential_output_infer_extraction_renamed_terminates() {
    let source = r#"
interface Carrier<Payload> {
  carried: Payload;
}

interface Deferred<Inner extends Carrier<any>> extends Carrier<Extract2<Deferred<Inner>>> {
  child: Inner;
}

type Extract2<X> = X extends Carrier<infer P> ? P : never;

declare const d: Deferred<Carrier<string>>;
type Result = Extract2<typeof d>;
declare const result: Result;

export {};
"#;
    let _codes = check_source_codes(source);
}

/// Zod-shaped recursive schema: `ZodObject`/`ZodOptional` wrappers whose
/// `_output` is computed through mapped types, conditionals, and `infer`, with
/// a self-referential lazy schema that makes the output evaluation unbounded —
/// the structure that hung the Zod benchmark row.
#[test]
fn zod_shaped_recursive_schema_infer_terminates() {
    let source = r#"
interface ZodTypeDef {}

interface ZodType<Output = any, Def extends ZodTypeDef = ZodTypeDef> {
  _output: Output;
  _def: Def;
}

interface ZodLazyDef<T extends ZodType = ZodType> extends ZodTypeDef {
  getter: () => T;
}

interface ZodLazy<T extends ZodType = ZodType>
  extends ZodType<InferOutput<ZodLazy<T>>, ZodLazyDef<T>> {
  schema: T;
}

type baseObjectOutputType<Shape extends Record<string, ZodType>> = {
  [k in keyof Shape]: InferOutput<Shape[k]>;
};

interface ZodObjectDef<
  T extends Record<string, ZodType> = Record<string, ZodType>
> extends ZodTypeDef {
  shape: () => T;
}

interface ZodObject<
  T extends Record<string, ZodType> = Record<string, ZodType>
> extends ZodType<baseObjectOutputType<T>, ZodObjectDef<T>> {
  shape: T;
}

type InferOutput<T> = T extends ZodType<infer O> ? O : never;

declare const schema: ZodObject<{ a: ZodLazy<ZodType<number>> }>;
type Out = InferOutput<typeof schema>;
declare const out: Out;

export {};
"#;
    let _codes = check_source_codes(source);
}

/// A deep but finite recursive wrapper must still terminate and must not emit a
/// spurious TS2589 — the bound only fires on genuinely unbounded nesting.
#[test]
fn deep_finite_wrapper_infer_no_ts2589() {
    let source = r#"
interface Box<T> {
  value: T;
}

type Unbox<T> = T extends Box<infer U> ? Unbox<U> : T;

declare const b: Box<Box<Box<Box<Box<number>>>>>;
type R = Unbox<typeof b>;
declare const r: R = 0;

export {};
"#;
    let codes = check_source_codes(source);
    assert!(
        !codes.contains(&2589),
        "finite Box nesting must not produce TS2589. Got: {codes:?}"
    );
}
