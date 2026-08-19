//! Return-context inference for zero-evidence generic tags and calls.
//!
//! Kysely's `sql<T = unknown>` tag is returned from a generic helper as
//! `RawBuilder<Simplify<ShallowDehydrateObject<O>> | null>`. The contextual
//! return is the only inference source. When the declared and contextual
//! results are aligned applications of the same `RawBuilder` definition,
//! `tsc` binds the tag's tracked `T` to that whole composite outer type.

use tsz_checker::test_utils::{
    DiagnosticShape, assert_diagnostic_shapes_exactly, check_source_diagnostics,
    diagnostic_code_message_refs,
};

const PRELUDE: &str = r#"
interface TemplateStringsArray {
  readonly raw: readonly string[]
}

interface Promise<Value> {
  catch<Result = never>(
    onrejected: (reason: unknown) => Result
  ): Promise<Value | Result>
}

interface RawBuilder<Output> {
  readonly expressionType: Output | undefined
  readonly isRawBuilder: true
}

interface SqlTag {
  <Tagged = unknown>(parts: TemplateStringsArray, ...values: unknown[]): RawBuilder<Tagged>
}

declare const sql: SqlTag
declare function rawCall<Result = unknown>(): RawBuilder<Result>
declare function fromValue<Result = unknown>(value: Result): RawBuilder<Result>

type DrainOuterGeneric<T> = [T] extends [unknown] ? T : never
type Simplify<T> = DrainOuterGeneric<{ [K in keyof T]: T[K] } & {}>
type ShallowDehydrateObject<T> = { [K in keyof T]: T[K] }
"#;

fn check(body: &str) -> Vec<tsz_common::diagnostics::Diagnostic> {
    check_source_diagnostics(&format!("{PRELUDE}\n{body}"))
}

fn assert_clean(body: &str, context: &str) {
    let diagnostics = check(body);
    assert!(
        diagnostics.is_empty(),
        "{context}: expected no diagnostics, got {:#?}",
        diagnostic_code_message_refs(&diagnostics),
    );
}

#[test]
fn generic_tag_uses_composite_outer_return_context() {
    assert_clean(
        r#"
function jsonObjectFrom<Row>(value: Row): RawBuilder<
  Simplify<ShallowDehydrateObject<Row>> | null
> {
  return sql`json(${value})`
}
"#,
        "generic tag with mapped/union/null outer context",
    );
}

#[test]
fn ordinary_zero_evidence_call_uses_outer_return_context() {
    assert_clean(
        r#"
function buildRecord<Entity>(): RawBuilder<Simplify<Entity> | null> {
  return rawCall()
}
"#,
        "ordinary call with renamed outer binder",
    );
}

#[test]
fn direct_substitution_and_explicit_type_argument_stay_clean() {
    assert_clean(
        r#"
function direct<Value>(): RawBuilder<Value> {
  return sql``
}

function explicit<Value>(): RawBuilder<Simplify<Value> | null> {
  return sql<Simplify<Value> | null>``
}
"#,
        "direct and explicit contextual return bindings",
    );
}

#[test]
fn no_context_keeps_default_unknown() {
    let source =
        format!("{PRELUDE}\nconst inferred = sql``\nconst bad: RawBuilder<string> = inferred\n");
    let diagnostics = check_source_diagnostics(&source);
    assert_diagnostic_shapes_exactly(
        &source,
        &diagnostics,
        &[DiagnosticShape::code(2322).with_message_fragment(
            "RawBuilder<unknown>' is not assignable to type 'RawBuilder<string>",
        )],
    );
}

#[test]
fn argument_inference_outranks_conflicting_return_context() {
    let source = format!("{PRELUDE}\nconst bad: RawBuilder<string> = fromValue(123)\n");
    let diagnostics = check_source_diagnostics(&source);
    assert_diagnostic_shapes_exactly(
        &source,
        &diagnostics,
        &[DiagnosticShape::code(2322).with_message_fragment(
            "RawBuilder<number>' is not assignable to type 'RawBuilder<string>",
        )],
    );
}

#[test]
fn nested_generic_extraction_uses_outer_apparent_constraint() {
    assert_clean(
        r#"
interface ContextualBox<Scope, Key extends keyof Scope, Value> {
  readonly expressionType: Value | undefined
}

type ReferenceInput<Scope, Key extends keyof Scope> =
  | (keyof Scope & string)
  | ContextualBox<Scope, Key, any>

type ExtractReferenceValue<Scope, Key extends keyof Scope, Ref> =
  Ref extends ContextualBox<any, any, infer Value> ? Value : unknown

function wrapUnary<
  Scope,
  Key extends keyof Scope,
  Ref extends ReferenceInput<Scope, Key>,
>(expr: Ref): ContextualBox<Scope, Key, boolean> {
  function unary<Ref extends ReferenceInput<Scope, Key>>(
    value: Ref,
  ): ContextualBox<Scope, Key, ExtractReferenceValue<Scope, Key, Ref>> {
    return {} as any
  }

  return unary(expr)
}
"#,
        "same-spelled nested generic extraction",
    );
}

#[test]
fn renamed_sibling_type_parameter_keeps_the_same_apparent_constraint() {
    assert_clean(
        r#"
interface ContextualBox<Scope, Key extends keyof Scope, Value> {
  readonly expressionType: Value | undefined
}

type ReferenceInput<Scope, Key extends keyof Scope> =
  | (keyof Scope & string)
  | ContextualBox<Scope, Key, any>

type ExtractReferenceValue<Scope, Key extends keyof Scope, Ref> =
  Ref extends ContextualBox<any, any, infer Value> ? Value : unknown

function makeOperations<Scope, Key extends keyof Scope>() {
  function unary<Ref extends ReferenceInput<Scope, Key>>(
    value: Ref,
  ): ContextualBox<Scope, Key, ExtractReferenceValue<Scope, Key, Ref>> {
    return {} as any
  }

  function renamed<OuterRef extends ReferenceInput<Scope, Key>>(
    expr: OuterRef,
  ): ContextualBox<Scope, Key, boolean> {
    return unary(expr)
  }

  return { renamed }
}
"#,
        "renamed sibling binder passed to a generic helper",
    );
}

#[test]
fn different_base_and_ambiguous_union_do_not_pin_the_tag() {
    let source = format!(
        r#"{PRELUDE}
interface OtherBuilder<T> {{ readonly other: T }}
declare function other<Result = unknown>(): OtherBuilder<Result>

function wrongBase<Outer>(): RawBuilder<Outer> {{
  return other()
}}

function ambiguous(): RawBuilder<string> | RawBuilder<number> {{
  return sql``
}}
"#
    );
    let diagnostics = check_source_diagnostics(&source);
    let codes: Vec<_> = diagnostics
        .iter()
        .map(|diagnostic| diagnostic.code)
        .collect();
    assert_eq!(
        codes,
        vec![2739, 2322],
        "unexpected diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn three_arm_ambiguous_union_reports_the_return_mismatch() {
    let source = format!(
        r#"{PRELUDE}
function threeArm(): RawBuilder<string> | RawBuilder<number> | RawBuilder<boolean[]> {{
  return sql``
}}
"#
    );
    let diagnostics = check_source_diagnostics(&source);
    let codes: Vec<_> = diagnostics
        .iter()
        .map(|diagnostic| diagnostic.code)
        .collect();
    assert_eq!(
        codes,
        vec![2322],
        "unexpected diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn ambiguous_union_with_nullish_arm_still_reports_the_return_mismatch() {
    let source = format!(
        r#"{PRELUDE}
function withUndef(): RawBuilder<string> | RawBuilder<number> | undefined {{
  return sql``
}}
"#
    );
    let diagnostics = check_source_diagnostics(&source);
    let codes: Vec<_> = diagnostics
        .iter()
        .map(|diagnostic| diagnostic.code)
        .collect();
    assert_eq!(
        codes,
        vec![2322],
        "unexpected diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn single_matching_arm_with_null_still_infers_from_that_arm() {
    assert_clean(
        r#"
function oneArmNull(): RawBuilder<string> | null {
  return sql``
}
"#,
        "single same-base arm plus null keeps inferring from the arm",
    );
}

#[test]
fn mixed_base_union_infers_from_the_matching_arm_only() {
    assert_clean(
        r#"
interface OtherBuilder<T> { readonly other: T }
function mixedBase(): RawBuilder<string> | OtherBuilder<number> {
  return sql``
}
"#,
        "one same-base arm plus a different-base arm stays unambiguous",
    );
}

#[test]
fn agreeing_alias_arms_do_not_report() {
    assert_clean(
        r#"
type AliasedRaw = RawBuilder<string>
function agreeing(): AliasedRaw | RawBuilder<string> {
  return sql``
}
"#,
        "arms that agree on the argument stay clean",
    );
}

#[test]
fn argument_evidence_outranks_ambiguous_union_context() {
    assert_clean(
        r#"
const picked: RawBuilder<string> | RawBuilder<number> = fromValue(123)
"#,
        "a concrete argument decides the parameter; the ambiguous context does not",
    );
}

#[test]
fn renamed_binders_ambiguous_union_reports_the_return_mismatch() {
    let source = format!(
        r#"{PRELUDE}
interface CrateRow<Payload> {{
  readonly slot: Payload | undefined
  readonly sealed: true
}}
interface StampTag {{
  <Mark = unknown>(parts: TemplateStringsArray, ...values: unknown[]): CrateRow<Mark>
}}
declare const stamp: StampTag
function pickCrate(): CrateRow<string> | CrateRow<number> {{
  return stamp``
}}
"#
    );
    let diagnostics = check_source_diagnostics(&source);
    let codes: Vec<_> = diagnostics
        .iter()
        .map(|diagnostic| diagnostic.code)
        .collect();
    assert_eq!(
        codes,
        vec![2322],
        "unexpected diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn ordinary_zero_evidence_call_against_ambiguous_union_reports() {
    let source = format!(
        r#"{PRELUDE}
function viaCall(): RawBuilder<string> | RawBuilder<number> {{
  return rawCall()
}}
"#
    );
    let diagnostics = check_source_diagnostics(&source);
    let codes: Vec<_> = diagnostics
        .iter()
        .map(|diagnostic| diagnostic.code)
        .collect();
    assert_eq!(
        codes,
        vec![2322],
        "unexpected diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn nested_promise_catch_generic_does_not_escape_into_outer_result() {
    assert_clean(
        r#"
declare function makePromise<Value = unknown>(): Promise<Value>

function recover<Outer>(): Promise<Outer> {
  return makePromise().catch((reason): never => { throw reason })
}
"#,
        "nested Promise.catch generic control",
    );
}
