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

/// #17673 item 1: aliases are transparent to tsc's ambiguous-union merge.
/// Every error case below was oracle-pinned against tsc 6.0.2 (`--strict`):
/// tsc reports exactly one TS2322 on the return statement.
fn assert_single_ts2322(body: &str, context: &str) {
    let source = format!("{PRELUDE}\n{body}");
    let diagnostics = check_source_diagnostics(&source);
    let codes: Vec<_> = diagnostics
        .iter()
        .map(|diagnostic| diagnostic.code)
        .collect();
    assert_eq!(codes, vec![2322], "{context}: {diagnostics:#?}");
}

#[test]
fn alias_wrapped_ambiguous_arms_report_the_return_mismatch() {
    assert_single_ts2322(
        r#"
type StrRow = RawBuilder<string>
type NumRow = RawBuilder<number>
function aliasArms(): StrRow | NumRow {
  return sql``
}
"#,
        "both arms alias-wrapped",
    );
}

#[test]
fn mixed_alias_and_direct_ambiguous_arms_report_the_return_mismatch() {
    assert_single_ts2322(
        r#"
type StrRow = RawBuilder<string>
function mixedAliasDirect(): StrRow | RawBuilder<number> {
  return sql``
}
"#,
        "one alias arm plus one direct arm",
    );
}

#[test]
fn generic_alias_ambiguous_arms_report_the_return_mismatch() {
    assert_single_ts2322(
        r#"
type Row<Payload> = RawBuilder<Payload>
function genericAliasArms(): Row<string> | Row<number> {
  return sql``
}
"#,
        "both arms through a generic alias of the base",
    );
}

#[test]
fn tag_declared_to_return_the_alias_application_reports_against_direct_arms() {
    assert_single_ts2322(
        r#"
type Row<Payload> = RawBuilder<Payload>
interface RowTag {
  <Tagged = unknown>(parts: TemplateStringsArray, ...values: unknown[]): Row<Tagged>
}
declare const rowSql: RowTag
function tagReturnsAlias(): RawBuilder<string> | RawBuilder<number> {
  return rowSql``
}
"#,
        "tag's declared return is the generic alias, arms are direct",
    );
}

#[test]
fn ordinary_call_with_alias_arms_reports_the_return_mismatch() {
    assert_single_ts2322(
        r#"
type StrRow = RawBuilder<string>
type NumRow = RawBuilder<number>
function callAliasArms(): StrRow | NumRow {
  return rawCall()
}
"#,
        "ordinary zero-evidence call form with alias arms",
    );
}

#[test]
fn alias_arms_with_undefined_arm_report_the_return_mismatch() {
    assert_single_ts2322(
        r#"
type StrRow = RawBuilder<string>
type NumRow = RawBuilder<number>
function aliasArmsUndef(): StrRow | NumRow | undefined {
  return sql``
}
"#,
        "alias arms plus a nullish arm",
    );
}

#[test]
fn alias_to_a_different_base_does_not_merge_and_stays_clean() {
    assert_clean(
        r#"
interface OtherBuilder<T> { readonly other: T }
type OtherRow = OtherBuilder<number>
function aliasOtherBase(): RawBuilder<string> | OtherRow {
  return sql``
}
"#,
        "alias of a different base keeps the single-arm inference",
    );
}

#[test]
fn single_alias_arm_with_null_still_infers_from_that_arm() {
    assert_clean(
        r#"
type StrRow = RawBuilder<string>
function singleAliasNull(): StrRow | null {
  return sql``
}
"#,
        "single alias arm plus null keeps inferring from the arm",
    );
}

#[test]
fn alias_chain_arm_reports_the_return_mismatch() {
    assert_single_ts2322(
        r#"
type Row<Payload> = RawBuilder<Payload>
type StrRow = Row<string>
type StrRow2 = StrRow
function chainArms(): StrRow2 | RawBuilder<number> {
  return sql``
}
"#,
        "arm through a two-hop alias chain",
    );
}

#[test]
fn tag_declared_through_an_alias_chain_reports_against_direct_arms() {
    assert_single_ts2322(
        r#"
type Row<Payload> = RawBuilder<Payload>
type Row2<Payload> = Row<Payload>
interface Row2Tag {
  <Tagged = unknown>(parts: TemplateStringsArray, ...values: unknown[]): Row2<Tagged>
}
declare const row2Sql: Row2Tag
function chainReturn(): RawBuilder<string> | RawBuilder<number> {
  return row2Sql``
}
"#,
        "tag's declared return is a two-hop alias chain, arms are direct",
    );
}

/// #17673 item 2: the ambiguous-union TS2322 must render the contextually
/// inferred source type, not a context-free re-derivation that lets the tag's
/// type parameter fall back to its `unknown` default. Every message fragment
/// below was oracle-pinned against tsc 6.0.2 (`--strict`).
fn assert_single_ts2322_message(body: &str, fragment: &'static str, context: &str) {
    let source = format!("{PRELUDE}\n{body}");
    let diagnostics = check_source_diagnostics(&source);
    assert_diagnostic_shapes_exactly(
        &source,
        &diagnostics,
        &[DiagnosticShape::code(2322).with_message_fragment(fragment)],
    );
    let _ = context;
}

#[test]
fn ambiguous_union_message_renders_the_inferred_union_source() {
    assert_single_ts2322_message(
        r#"
function twoArm(): RawBuilder<string> | RawBuilder<number> {
  return sql``
}
"#,
        "Type 'RawBuilder<string | number>' is not assignable to type 'RawBuilder<string> | RawBuilder<number>'",
        "two-arm tag source display",
    );
}

#[test]
fn three_arm_ambiguous_union_message_renders_the_full_inferred_union() {
    assert_single_ts2322_message(
        r#"
function threeArm(): RawBuilder<string> | RawBuilder<number> | RawBuilder<boolean[]> {
  return sql``
}
"#,
        "Type 'RawBuilder<string | number | boolean[]>' is not assignable",
        "three-arm tag source display",
    );
}

#[test]
fn renamed_binders_ambiguous_union_message_renders_the_inferred_union_source() {
    assert_single_ts2322_message(
        r#"
interface CrateRow<Payload> {
  readonly slot: Payload | undefined
  readonly sealed: true
}
interface StampTag {
  <Mark = unknown>(parts: TemplateStringsArray, ...values: unknown[]): CrateRow<Mark>
}
declare const stamp: StampTag
function pickCrate(): CrateRow<string> | CrateRow<number> {
  return stamp``
}
"#,
        "Type 'CrateRow<string | number>' is not assignable to type 'CrateRow<string> | CrateRow<number>'",
        "renamed binders source display",
    );
}

#[test]
fn ordinary_call_ambiguous_union_message_renders_the_inferred_union_source() {
    assert_single_ts2322_message(
        r#"
function viaCall(): RawBuilder<string> | RawBuilder<number> {
  return rawCall()
}
"#,
        "Type 'RawBuilder<string | number>' is not assignable to type 'RawBuilder<string> | RawBuilder<number>'",
        "ordinary zero-evidence call source display",
    );
}

#[test]
fn nullish_arm_ambiguous_union_message_renders_the_inferred_union_source() {
    assert_single_ts2322_message(
        r#"
function withUndef(): RawBuilder<string> | RawBuilder<number> | undefined {
  return sql``
}
"#,
        "Type 'RawBuilder<string | number>' is not assignable",
        "nullish arm source display",
    );
}

#[test]
fn alias_arm_ambiguous_union_message_renders_the_inferred_union_source() {
    assert_single_ts2322_message(
        r#"
type StrRow = RawBuilder<string>
type NumRow = RawBuilder<number>
function aliasArms(): StrRow | NumRow {
  return sql``
}
"#,
        "Type 'RawBuilder<string | number>' is not assignable",
        "alias arms source display",
    );
}
// -----------------------------------------------------------------------------
// #17673 item 3: a union arm whose type argument is a FOREIGN (outer-scope)
// type parameter still makes the union ambiguous. The return-context
// substitution must not pin the tag's parameter from the concrete arm alone;
// tsc combines the per-arm candidates (`Tagged := number | O`) and the return
// assignability check reports TS2322.
// -----------------------------------------------------------------------------

#[test]
fn foreign_param_arm_with_concrete_arm_reports_the_return_mismatch() {
    assert_single_ts2322(
        r#"
function genericOuter<O>(): RawBuilder<O> | RawBuilder<number> {
  return sql``
}
"#,
        "foreign outer param arm plus concrete arm",
    );
}

#[test]
fn renamed_foreign_binder_arm_reports_the_return_mismatch() {
    assert_single_ts2322(
        r#"
function pickRow<Elem>(): RawBuilder<Elem> | RawBuilder<string> {
  return sql``
}
"#,
        "renamed foreign binder, string concrete arm",
    );
}

#[test]
fn reversed_arm_order_foreign_param_reports_the_return_mismatch() {
    assert_single_ts2322(
        r#"
function genericOuter<O>(): RawBuilder<number> | RawBuilder<O> {
  return sql``
}
"#,
        "concrete arm first, foreign param arm second",
    );
}

#[test]
fn two_foreign_param_arms_report_the_return_mismatch() {
    assert_single_ts2322(
        r#"
function twoOuter<A, B>(): RawBuilder<A> | RawBuilder<B> {
  return sql``
}
"#,
        "both arms foreign outer params",
    );
}

#[test]
fn alias_wrapped_foreign_param_arm_reports_the_return_mismatch() {
    assert_single_ts2322(
        r#"
type ORow<Payload> = RawBuilder<Payload>
function aliasOuter<O>(): ORow<O> | RawBuilder<number> {
  return sql``
}
"#,
        "foreign param arm through a generic alias of the base",
    );
}

#[test]
fn ordinary_call_foreign_param_arm_reports_the_return_mismatch() {
    assert_single_ts2322(
        r#"
function genericOuter<O>(): RawBuilder<O> | RawBuilder<number> {
  return rawCall()
}
"#,
        "ordinary zero-evidence call form with a foreign param arm",
    );
}

#[test]
fn single_foreign_arm_with_null_still_infers_from_that_arm() {
    assert_clean(
        r#"
function nullableOuter<O>(): RawBuilder<O> | null {
  return sql``
}
"#,
        "single foreign param arm with a nullish arm pins the param",
    );
}

#[test]
fn single_foreign_arm_with_undefined_still_infers_from_that_arm() {
    assert_clean(
        r#"
function undefOuter<O>(): RawBuilder<O> | undefined {
  return sql``
}
"#,
        "single foreign param arm with an undefined arm pins the param",
    );
}

#[test]
fn foreign_arm_with_unknown_argument_arm_stays_clean() {
    assert_clean(
        r#"
function unknownArm<O>(): RawBuilder<O> | RawBuilder<unknown> {
  return sql``
}
"#,
        "combined union collapses into the unknown arm",
    );
}

// -----------------------------------------------------------------------------
// Union-target head-line display: alias-spelled members must not collapse.
//
// The union display's constituent-collapse identity keyed an evaluated generic
// instantiation on its *declaring interface symbol*, so two alias-spelled
// instantiations of the same base (`StrRow = RawBuilder<string>`,
// `NumRow = RawBuilder<number>`) collapsed to one member and the TS2322 head
// rendered `... is not assignable to type 'StrRow'` where tsc renders the full
// written union. The identity now keys on the display-alias provenance (the
// application the evaluated form was produced from), so distinct
// instantiations stay distinct while same-type duplicates keep collapsing.
// Every pinned fragment below is oracle-pinned against tsc 6.0.2 (`--strict`).
// -----------------------------------------------------------------------------

#[test]
fn alias_arm_ambiguous_union_message_renders_the_full_union_target() {
    assert_single_ts2322_message(
        r#"
type StrRow = RawBuilder<string>
type NumRow = RawBuilder<number>
function aliasArms(): StrRow | NumRow {
  return sql``
}
"#,
        "Type 'RawBuilder<string | number>' is not assignable to type 'StrRow | NumRow'",
        "alias arms full union target display",
    );
}

#[test]
fn generic_alias_arm_ambiguous_union_message_renders_the_full_union_target() {
    assert_single_ts2322_message(
        r#"
type Row<Payload> = RawBuilder<Payload>
function genericAliasArms(): Row<string> | Row<number> {
  return sql``
}
"#,
        "Type 'RawBuilder<string | number>' is not assignable to type 'Row<string> | Row<number>'",
        "generic alias arms full union target display",
    );
}

#[test]
fn nullish_alias_arm_ambiguous_union_message_renders_the_full_union_target() {
    assert_single_ts2322_message(
        r#"
type StrRow = RawBuilder<string>
type NumRow = RawBuilder<number>
function aliasArmsUndef(): StrRow | NumRow | undefined {
  return sql``
}
"#,
        "Type 'RawBuilder<string | number>' is not assignable to type 'StrRow | NumRow | undefined'",
        "alias arms plus undefined full union target display",
    );
}

#[test]
fn renamed_binder_alias_arm_message_renders_the_full_union_target() {
    assert_single_ts2322_message(
        r#"
interface CrateRow<Payload> {
  readonly slot: Payload | undefined
  readonly sealed: true
}
interface StampTag {
  <Mark = unknown>(parts: TemplateStringsArray, ...values: unknown[]): CrateRow<Mark>
}
declare const stamp: StampTag
type FirstCrate = CrateRow<string>
type SecondCrate = CrateRow<number>
function pickCrate(): FirstCrate | SecondCrate {
  return stamp``
}
"#,
        "Type 'CrateRow<string | number>' is not assignable to type 'FirstCrate | SecondCrate'",
        "renamed binders alias arms full union target display",
    );
}

// The head line renders both members in the written source order
// (`'StrRow | RawBuilder<number>'`); oracle-reverified against tsc 6.0.2 with
// the arms swapped in source too (`RawBuilder<number> | StrRow`) — tsc's own
// head line and elaboration target are unaffected by which arm was written
// first, and tsz's canonical interner order (string-arg sorts before
// number-arg, `union_member_order.rs`) already agrees, so this is not the
// order residual a prior session's comment claimed.
#[test]
fn mixed_alias_and_direct_arm_message_keeps_both_union_target_members() {
    assert_single_ts2322_message(
        r#"
type StrRow = RawBuilder<string>
function mixedAliasDirect(): StrRow | RawBuilder<number> {
  return sql``
}
"#,
        "is not assignable to type 'StrRow | RawBuilder<number>'",
        "mixed alias and direct arms keep both target members",
    );
}

// Structural rule: `getBestMatchingType`'s `findMatchingTypeReferenceOrTypeAliasReference`
// step runs before property-overlap scoring — when the source is a generic
// type reference, tsc elaborates against the first union member instantiating
// the *same* generic declaration, alias-spelled or not. tsz's
// `select_union_target_best_member` (`explain_union_discriminant.rs`) only had
// the discriminant-match and overlap steps, so an application-shaped source
// vs. a same-base mixed-arm union target fell through
// `explain_union_target_failure`'s application-shaped-comparison branch,
// which gives up with a bare union-head line whenever no member fails with a
// *missing* property (this shape fails on a property *type* mismatch
// instead). Both gaps are fixed: the missing type-reference step, and the
// bare fallback no longer swallowing a genuine structural reason.
//
// Oracle-pinned (tsc 6.0.2 `--strict`): the assignment elaborates against
// `StrRow` (first same-base arm) with the full chain, never against
// `RawBuilder<number>` and never collapsing to the head line alone.
#[test]
fn mixed_alias_and_direct_arm_elaborates_against_first_same_base_arm() {
    let source = format!(
        "{PRELUDE}\ntype StrRow = RawBuilder<string>\ndeclare const src: RawBuilder<string | number>\nconst pinnedTarget: StrRow | RawBuilder<number> = src\n"
    );
    let diagnostics = check_source_diagnostics(&source);
    assert_diagnostic_shapes_exactly(
        &source,
        &diagnostics,
        &[DiagnosticShape::code(2322)
            .with_message_fragment("is not assignable to type 'StrRow | RawBuilder<number>'")
            .with_related_min(2)],
    );
    let full_chain = diagnostics
        .iter()
        .flat_map(|d| {
            d.related_information
                .iter()
                .map(|r| r.message_text.as_str())
        })
        .collect::<Vec<_>>()
        .join(" | ");
    assert!(
        full_chain.contains("is not assignable to type 'StrRow'"),
        "expected the chain to drill into the first same-base arm (StrRow), got: {full_chain}"
    );
    assert!(
        !full_chain.contains("is not assignable to type 'RawBuilder<number>'"),
        "must not elaborate against the later same-base arm, got: {full_chain}"
    );
}

// Same shape with the union arms written in the opposite order: tsc's choice
// of elaboration member does not depend on source spelling order (oracle
// reverified), only on the canonical interner order, so this must match the
// unswapped case above.
#[test]
fn reversed_written_order_still_elaborates_against_first_same_base_arm() {
    let source = format!(
        "{PRELUDE}\ntype StrRow = RawBuilder<string>\ndeclare const src: RawBuilder<string | number>\nconst pinnedTarget: RawBuilder<number> | StrRow = src\n"
    );
    let diagnostics = check_source_diagnostics(&source);
    let full_chain = diagnostics
        .iter()
        .flat_map(|d| {
            d.related_information
                .iter()
                .map(|r| r.message_text.as_str())
        })
        .collect::<Vec<_>>()
        .join(" | ");
    assert!(
        full_chain.contains("is not assignable to type 'StrRow'"),
        "expected the chain to drill into StrRow regardless of written order, got: {full_chain}"
    );
}

// Negative control: renamed alias/interface still resolves the same-base
// match through the alias hop, not through name spelling.
#[test]
fn renamed_alias_still_elaborates_against_first_same_base_arm() {
    let source = format!(
        "{PRELUDE}\ntype FirstRow = RawBuilder<string>\ndeclare const renamedSrc: RawBuilder<string | number>\nconst pinnedRenamed: FirstRow | RawBuilder<number> = renamedSrc\n"
    );
    let diagnostics = check_source_diagnostics(&source);
    let full_chain = diagnostics
        .iter()
        .flat_map(|d| {
            d.related_information
                .iter()
                .map(|r| r.message_text.as_str())
        })
        .collect::<Vec<_>>()
        .join(" | ");
    assert!(
        full_chain.contains("is not assignable to type 'FirstRow'"),
        "expected the chain to drill into the renamed alias arm, got: {full_chain}"
    );
}

// Negative control: a genuinely different generic base (no shared DefId) must
// not be treated as a type-reference match — this still falls through to the
// existing discriminant/overlap selection, unaffected by the new step.
#[test]
fn different_generic_base_arms_still_report_a_mismatch() {
    let source = format!(
        "{PRELUDE}\ninterface OtherBuilder<Output> {{\n  readonly expressionType: Output | undefined\n  readonly isRawBuilder: true\n}}\ndeclare const otherSrc: RawBuilder<string>\nconst pinnedOther: RawBuilder<number> | OtherBuilder<number> = otherSrc\n"
    );
    let diagnostics = check_source_diagnostics(&source);
    assert_diagnostic_shapes_exactly(
        &source,
        &diagnostics,
        &[DiagnosticShape::code(2322).with_message_fragment(
            "is not assignable to type 'OtherBuilder<number> | RawBuilder<number>'",
        )],
    );
}

// Negative control: the same alias written twice is one constituent — the
// per-instantiation identity must not stop genuine same-type duplicates from
// collapsing. tsc renders a single member here (no union in the target
// display, no second member frame).
#[test]
fn duplicate_alias_arm_target_still_collapses_to_one_member() {
    let source = format!(
        "{PRELUDE}\ntype StrRow = RawBuilder<string>\ndeclare const rbBool: RawBuilder<boolean>\nconst dup: StrRow | StrRow = rbBool\n"
    );
    let diagnostics = check_source_diagnostics(&source);
    let codes: Vec<_> = diagnostics
        .iter()
        .map(|diagnostic| diagnostic.code)
        .collect();
    assert_eq!(codes, vec![2322], "duplicate alias arms: {diagnostics:#?}");
    let message = &diagnostics[0].message_text;
    assert!(
        !message.contains('|'),
        "duplicate alias arms must collapse to a single target member, got: {message}"
    );
}
