//! Without `strictNullChecks`, tsc's non-null check is **narrowed, not
//! suppressed**.
//!
//! Structural rule: `checkNonNullTypeWithReporter` computes
//!
//! ```text
//! const kind = (strictNullChecks ? getFalsyFlags(type) : type.flags) & TypeFlags.Nullable;
//! if (kind) { reportError(node, kind); ... }
//! ```
//!
//! so turning `strictNullChecks` off swaps the operand's *falsy facts* for the
//! operand's *own flags*. An operand that IS `null`/`undefined` still reports the
//! whole family — TS18047/TS18048 through
//! `reportObjectPossiblyNullOrUndefinedError`, TS2721/TS2722 through
//! `reportCannotInvokePossiblyNullOrUndefinedError` — while a merely-nullable
//! union stops reporting because a union's own flags are `TypeFlags.Union`.
//! tsz gated the entire mirror on `strictNullChecks`, so every one of these rows
//! reported nothing, and a nullish callee reported the wrong diagnostic (TS2349
//! "not callable") instead of TS2721.
//!
//! Two exclusions are load bearing and both fall out of `type.flags`:
//!
//! - `void` is `TypeFlags.Void`, never `TypeFlags.Nullable`, so a `void` operand
//!   keeps falling through to the position's own structural check in both modes.
//!   `split_nullish_type` normalizes `void` to `undefined` for narrowing, so the
//!   non-null check has to exclude it explicitly.
//! - A union never triggers the non-strict arm, so `T | null` stays clean — which
//!   is what made the suppression look correct: without `strictNullChecks` almost
//!   everything either widens (`let z = null` → `any`) or is a union, and an
//!   explicit `null`/`undefined` annotation is the shape that survives.
//!
//! Oracle: `tsc` 7.0.2, `--noEmit --strict false --target es2015 --pretty false`.
//! Every expectation below is pinned against a real run, in both modes.

use crate::test_utils::{check_source_non_strict_codes as non_strict, check_source_strict_codes};

const TS18047: u32 = 18047; // '<x>' is possibly 'null'.
const TS18048: u32 = 18048; // '<x>' is possibly 'undefined'.
const TS2721: u32 = 2721; // Cannot invoke an object which is possibly 'null'.
const TS2722: u32 = 2722; // Cannot invoke an object which is possibly 'undefined'.
const TS2349: u32 = 2349; // This expression is not callable.

fn count(codes: &[u32], code: u32) -> usize {
    codes.iter().filter(|&&c| c == code).count()
}

/// The nullish diagnostics this check owns, in every position it reaches.
const NULLISH_FAMILY: [u32; 4] = [TS18047, TS18048, TS2721, TS2722];

fn nullish_codes(codes: &[u32]) -> Vec<u32> {
    codes
        .iter()
        .copied()
        .filter(|c| NULLISH_FAMILY.contains(c))
        .collect()
}

// -------------------------------------------------------------------------
// A `null`-typed operand: every position reports without strictNullChecks.
// Binders are varied so nothing keys on identifier text.
// -------------------------------------------------------------------------

#[test]
fn null_typed_property_access_reports_ts18047_without_strict_null_checks() {
    for binder in ["on", "probe", "receiver"] {
        let source = format!("declare const {binder}: null;\n{binder}.foo;");
        let codes = non_strict(&source);
        assert_eq!(
            count(&codes, TS18047),
            1,
            "expected TS18047 for a `null` receiver (binder {binder}), got: {codes:?}"
        );
    }
}

#[test]
fn null_typed_element_access_reports_ts18047_without_strict_null_checks() {
    for binder in ["on", "probe", "receiver"] {
        let source = format!("declare const {binder}: null;\n{binder}[0];");
        let codes = non_strict(&source);
        assert_eq!(
            count(&codes, TS18047),
            1,
            "expected TS18047 for a `null` element-access receiver (binder {binder}), got: {codes:?}"
        );
    }
}

#[test]
fn null_typed_callee_reports_ts2721_not_ts2349_without_strict_null_checks() {
    // The whole point of the call arm: tsz reported TS2349 "This expression is
    // not callable" here, which is a *wrong* diagnostic, not a missing one.
    for binder in ["on", "probe", "callee"] {
        let source = format!("declare const {binder}: null;\n{binder}();");
        let codes = non_strict(&source);
        assert_eq!(
            count(&codes, TS2721),
            1,
            "expected TS2721 for a `null` callee (binder {binder}), got: {codes:?}"
        );
        assert_eq!(
            count(&codes, TS2349),
            0,
            "a `null` callee must not report TS2349 (binder {binder}), got: {codes:?}"
        );
    }
}

#[test]
fn null_typed_in_operand_reports_ts18047_without_strict_null_checks() {
    let codes = non_strict("declare const on: null;\n\"\" in on;");
    assert_eq!(
        count(&codes, TS18047),
        1,
        "expected TS18047 for a `null` `in` RHS, got: {codes:?}"
    );
}

#[test]
fn null_typed_unary_operand_reports_ts18047_without_strict_null_checks() {
    let codes = non_strict("declare const on: null;\n~on;");
    assert_eq!(
        count(&codes, TS18047),
        1,
        "expected TS18047 for a `null` unary operand, got: {codes:?}"
    );
}

// -------------------------------------------------------------------------
// The `undefined` sibling: same rule, the other `TypeFlags.Nullable` member.
// -------------------------------------------------------------------------

#[test]
fn undefined_typed_operands_report_ts18048_and_ts2722_without_strict_null_checks() {
    let access = non_strict("declare const ou: undefined;\nou.bar;");
    assert_eq!(
        count(&access, TS18048),
        1,
        "expected TS18048 for an `undefined` receiver, got: {access:?}"
    );

    let element = non_strict("declare const ou: undefined;\nou[1];");
    assert_eq!(
        count(&element, TS18048),
        1,
        "expected TS18048 for an `undefined` element-access receiver, got: {element:?}"
    );

    let call = non_strict("declare const ou: undefined;\nou();");
    assert_eq!(
        count(&call, TS2722),
        1,
        "expected TS2722 for an `undefined` callee, got: {call:?}"
    );
    assert_eq!(
        count(&call, TS2349),
        0,
        "an `undefined` callee must not report TS2349, got: {call:?}"
    );
}

// -------------------------------------------------------------------------
// Alias, wrapper and nesting forms reach the same predicate.
// -------------------------------------------------------------------------

#[test]
fn aliased_null_type_reports_like_the_written_annotation() {
    let source =
        "type NullAlias = null;\ndeclare const viaAlias: NullAlias;\nviaAlias.member;\nviaAlias();";
    let codes = non_strict(source);
    assert_eq!(
        count(&codes, TS18047),
        1,
        "an aliased `null` annotation must report TS18047, got: {codes:?}"
    );
    assert_eq!(
        count(&codes, TS2721),
        1,
        "an aliased `null` callee must report TS2721, got: {codes:?}"
    );
}

#[test]
fn nested_null_property_reports_on_the_property_receiver() {
    let source = "declare const nested: { p: null };\nnested.p.q;\nnested.p();";
    let codes = non_strict(source);
    assert_eq!(
        count(&codes, TS18047),
        1,
        "a nested `null` property receiver must report TS18047, got: {codes:?}"
    );
    assert_eq!(
        count(&codes, TS2721),
        1,
        "a nested `null` property callee must report TS2721, got: {codes:?}"
    );
}

#[test]
fn static_undefined_member_reports_on_the_qualified_receiver() {
    let source = "class Holder { static m: undefined; }\nHolder.m.x;";
    let codes = non_strict(source);
    assert_eq!(
        count(&codes, TS18048),
        1,
        "a static `undefined` member receiver must report TS18048, got: {codes:?}"
    );
}

// -------------------------------------------------------------------------
// Controls. These are what made the blanket suppression look correct, and every
// one of them must stay clean — they are the false-positive surface of the fix.
// -------------------------------------------------------------------------

#[test]
fn widened_null_initializer_stays_clean_without_strict_null_checks() {
    // `let z = null` widens to `any` without strictNullChecks, so nothing here
    // carries a nullish flag at all.
    let codes = non_strict("let z = null;\nz.foo;\nz();\nlet w = undefined;\nw.foo;");
    assert!(
        nullish_codes(&codes).is_empty(),
        "widened null/undefined initializers must stay clean, got: {codes:?}"
    );
}

#[test]
fn nullable_union_annotation_stays_clean_without_strict_null_checks() {
    // A union's own flags are `TypeFlags.Union`, never `Nullable`, so the
    // non-strict arm does not trigger — this is the narrowing, and it is the
    // whole reason the fix is not a corpus-wide false-positive risk.
    let codes = non_strict("declare const un: { a: number } | null;\nun.a;\nun;");
    assert!(
        nullish_codes(&codes).is_empty(),
        "a `T | null` annotation must stay clean without strictNullChecks, got: {codes:?}"
    );
}

#[test]
fn void_operands_stay_clean_in_both_modes() {
    // `void` is `TypeFlags.Void`, not `TypeFlags.Nullable`. tsc reports the
    // position's own structural error (TS2339/TS7053/TS2349/TS2322) and never
    // the nullish family, under both settings — so this is the one arm of the
    // change that also corrects strict mode, where tsz reported TS18048 for
    // `v[0]` / `"" in v` and TS2722 for `v()`.
    let source = "declare const v: void;\nv.foo;\nv[0];\nv();\n\"\" in v;";

    let lax = non_strict(source);
    assert!(
        nullish_codes(&lax).is_empty(),
        "`void` operands must not report the nullish family without strictNullChecks, got: {lax:?}"
    );

    let strict = check_source_strict_codes(source);
    assert!(
        nullish_codes(&strict).is_empty(),
        "`void` operands must not report the nullish family under strict either, got: {strict:?}"
    );
}

#[test]
fn empty_destructuring_patterns_stay_strict_only() {
    // The empty-binding-pattern check is tsc's `checkNonNullNonVoidType`, a
    // different function from the `checkNonNullTypeWithReporter` mirror this
    // change narrows — and its reporting is observably strict-only. Oracle, tsc
    // 7.0.2: `declare const v: void; const {} = v;` and
    // `declare const n: null; const {} = n;` are BOTH clean without
    // `strictNullChecks` (`void` is not even in `TypeFlags.Nullable`, and
    // `checkNonNullNonVoidType` adds it back on top), and report TS2532 / TS2531
    // under strict. Narrowing the shared reporter's gate must not reach here.
    let source = "declare const v: void;\nconst {} = v;\ndeclare const n: null;\nconst {} = n;\nfunction f({}: void) {}";

    let lax = non_strict(source);
    assert_eq!(
        lax.iter().filter(|c| **c == 2531 || **c == 2532).count(),
        0,
        "empty destructuring must stay clean without strictNullChecks, got: {lax:?}"
    );

    let strict = check_source_strict_codes(source);
    assert_eq!(
        count(&strict, 2532),
        2,
        "strict mode must keep TS2532 for the two `void` patterns, got: {strict:?}"
    );
    assert_eq!(
        count(&strict, 2531),
        1,
        "strict mode must keep TS2531 for the `null` pattern, got: {strict:?}"
    );
}

#[test]
fn uninitialized_and_any_and_plain_operands_stay_clean_without_strict_null_checks() {
    let codes = non_strict(
        "let uninit;\nuninit.foo;\ndeclare const anyv: any;\nanyv.foo;\nanyv();\ndeclare const s: string;\ns.length;",
    );
    assert!(
        nullish_codes(&codes).is_empty(),
        "implicit-any, `any` and non-nullish operands must stay clean, got: {codes:?}"
    );
}

#[test]
fn optional_chain_on_a_null_receiver_now_reports_the_non_strict_family() {
    // `on?.foo` / `on?.()` on a `null` receiver used to report TS2339 on
    // `never` in both modes, because tsz's chain-root nullish stripping
    // (mirroring tsc's `getNonNullableType`) fired unconditionally. That
    // strip is strict-only in tsc, so without `strictNullChecks` this now
    // falls through to the same TS18047/TS2721 family as `on.foo` / `on()`.
    // Full matrix (property/element/call, undefined, nesting, unions) lives
    // in `optional_chain_root_nullish_strict_only_tests.rs`; this row stays
    // here as the one this gate's docstring used to call out as unreached.
    let source = "declare const on: null;\non?.foo;\non?.();";
    let lax = nullish_codes(&non_strict(source));
    assert_eq!(
        lax,
        vec![TS18047, TS2721],
        "the optional-chain rows must report the non-strict nullish family, got: {lax:?}"
    );
    assert!(
        nullish_codes(&check_source_strict_codes(source)).is_empty(),
        "strict mode keeps reporting TS2339/TS2349 (outside this family), not TS18047/TS2721"
    );
}

// -------------------------------------------------------------------------
// The strict arm is unchanged: it keeps using the falsy-facts trigger, so the
// union rows that the non-strict arm skips still report there.
// -------------------------------------------------------------------------

#[test]
fn strict_mode_keeps_reporting_the_union_and_widened_rows() {
    let widened = check_source_strict_codes("let z = null;\nz.foo;\nz();");
    assert_eq!(
        count(&widened, TS18047),
        1,
        "strict mode must keep TS18047 on a `null`-initialized binding, got: {widened:?}"
    );
    assert_eq!(
        count(&widened, TS2721),
        1,
        "strict mode must keep TS2721 on a `null`-initialized callee, got: {widened:?}"
    );

    let union = check_source_strict_codes("declare const un: { a: number } | null;\nun.a;");
    assert_eq!(
        count(&union, TS18047),
        1,
        "strict mode must keep TS18047 on a `T | null` receiver, got: {union:?}"
    );
}

// -------------------------------------------------------------------------
// A `null | undefined` union ANNOTATION is a distinct case from a union with
// a non-nullish member: tsc's type-node resolution collapses it to a bare
// `null` type without `strictNullChecks` (not a suppression, and not the
// `T | null` narrowing above), so it reports the same single-cause family
// TS18047/TS2721 as a plain `null` annotation. Oracle: `tsc` 7.0.2,
// `--strict false`, order-independent — `undefined | null` collapses the
// same way as `null | undefined`.
// -------------------------------------------------------------------------

#[test]
fn null_or_undefined_union_annotation_collapses_to_null_without_strict_null_checks() {
    for binder in ["onu", "probe", "receiver"] {
        let source =
            format!("declare const {binder}: null | undefined;\n{binder}.foo;\n{binder}();");
        let codes = non_strict(&source);
        assert_eq!(
            count(&codes, TS18047),
            1,
            "a `null | undefined` receiver (binder {binder}) must report TS18047, got: {codes:?}"
        );
        assert_eq!(
            count(&codes, TS2721),
            1,
            "a `null | undefined` callee (binder {binder}) must report TS2721, got: {codes:?}"
        );
        assert_eq!(
            count(&codes, 18049),
            0,
            "the non-strict answer must not be the strict two-cause TS18049 (binder {binder}), got: {codes:?}"
        );
    }
}

#[test]
fn undefined_or_null_union_annotation_collapses_the_same_way_regardless_of_order() {
    let codes = non_strict("declare const a: undefined | null;\na.foo;");
    assert_eq!(
        count(&codes, TS18047),
        1,
        "`undefined | null` must collapse to `null` exactly like `null | undefined`, got: {codes:?}"
    );
}

#[test]
fn uniform_undefined_union_annotation_stays_undefined_not_null() {
    // A regression the naive "all members are null/undefined -> null" rule
    // would introduce: `undefined | undefined` has no `null` member at all,
    // so it must resolve to `undefined`, not be dragged to `null`.
    let codes = non_strict("declare const un2: undefined | undefined;\nun2.foo;");
    assert_eq!(
        count(&codes, TS18048),
        1,
        "a uniform `undefined | undefined` annotation must report TS18048, got: {codes:?}"
    );
    assert_eq!(
        count(&codes, TS18047),
        0,
        "a uniform `undefined | undefined` annotation must not report TS18047, got: {codes:?}"
    );
}

#[test]
fn null_or_undefined_union_parameter_and_type_alias_collapse_the_same_way() {
    let param = non_strict("function f(p: null | undefined) { p.foo; }");
    assert_eq!(
        count(&param, TS18047),
        1,
        "a `null | undefined` parameter annotation must report TS18047, got: {param:?}"
    );

    let alias = non_strict(
        "type NullOrUndefined = null | undefined;\ndeclare const y: NullOrUndefined;\ny.foo;",
    );
    assert_eq!(
        count(&alias, TS18047),
        1,
        "a `null | undefined` type alias must report TS18047, got: {alias:?}"
    );
}

#[test]
fn null_or_undefined_union_interface_member_collapses_the_same_way() {
    // Third sibling of the type-node-resolution collapse. An interface
    // property signature's type node resolves through the interface
    // fast-path (`simple_local_interface.rs`'s
    // `try_lower_simple_local_interface_object`) into
    // `get_type_from_type_node_in_type_literal` — a call path distinct from
    // both the direct variable/param annotation path (`type_operators.rs`)
    // and the type-alias/type-literal path (`type_node.rs`), which the
    // class-field/type-literal control test confirms were already correct.
    // The `UNION_TYPE` branch there already had the collapse (this file's
    // earlier tests exercise it directly); the reason it never fired for an
    // interface member is that `null` in type position lowers to a
    // `TYPE_REFERENCE` node (identifier text `"null"`, same shape as
    // `undefined`), and the fast-path's own eligibility gate
    // (`is_simple_local_interface_primitive_type_reference`) allowed
    // `"undefined"` but not `"null"` — so a `null | undefined` member never
    // reached the fast path at all and fell through to `tsz-lowering`'s
    // `TypeLowering`, whose independent union-lowering has no such gate on
    // this shape (see the strict-mode control below for why that path
    // cannot host this fix). Adding `"null"` alongside `"undefined"` routes
    // the member through the already-correct checker path.
    let codes =
        non_strict("interface I { m: null | undefined }\ndeclare const i: I;\ni.m.foo;\ni.m();");
    assert_eq!(
        count(&codes, TS18047),
        1,
        "an interface member typed `null | undefined` must report TS18047, got: {codes:?}"
    );
    assert_eq!(
        count(&codes, TS2721),
        1,
        "an interface member typed `null | undefined` callee must report TS2721, got: {codes:?}"
    );
    assert_eq!(
        count(&codes, 18049),
        0,
        "the non-strict answer must not be the strict two-cause TS18049, got: {codes:?}"
    );
}

#[test]
fn strict_mode_keeps_the_two_cause_answer_for_an_interface_member() {
    // Regression control (caught by review on the first revision of this
    // fix): `strictNullChecks` must NOT collapse the union — an earlier
    // draft added the collapse to `tsz-lowering`'s `TypeLowering`, whose
    // `strict_null_checks` field is not reliably wired to the real compiler
    // option across its ~37 construction sites in `tsz-checker` and so
    // silently defaulted to `false`, collapsing the union even under
    // `--strict`. The fix now stays entirely inside `tsz-checker`, which
    // does have the real option.
    let source = "interface I { m: null | undefined }\ndeclare const i: I;\ni.m.foo;\ni.m();";
    let strict = check_source_strict_codes(source);
    assert_eq!(
        count(&strict, 18049),
        1,
        "strict mode must keep the two-cause TS18049 for an interface member, got: {strict:?}"
    );
    assert_eq!(
        count(&strict, 2723),
        1,
        "strict mode must keep the two-cause TS2723 for an interface member callee, got: {strict:?}"
    );
    assert_eq!(
        count(&strict, TS18047),
        0,
        "strict mode must not collapse to the single-cause TS18047, got: {strict:?}"
    );
}

#[test]
fn interface_index_signature_value_reports_the_reduced_single_cause() {
    // #16620: previously a documented residual. An index-signature member
    // never reaches the interface fast path
    // (`try_lower_simple_local_interface_object` rejects any
    // non-`PROPERTY_SIGNATURE` member), so it falls through to
    // `tsz-lowering`'s `TypeLowering`, which did not apply the non-strict
    // pure-nullish collapse. The general interface-lowering site now opts in
    // via `with_nonstrict_nullish_union_reduction`, wiring the real
    // `strictNullChecks`, so a `null | undefined` index value collapses to a
    // bare `null` exactly like the property-signature paths above — reporting
    // the single-cause TS18047/TS2721 without `strictNullChecks` and the
    // two-cause TS18049/TS2723 under it. Binders are varied so nothing keys
    // on identifier text.
    for (iface, member) in [("I", "m"), ("Bag", "entry"), ("Store", "slot")] {
        let source = format!(
            "interface {iface} {{ [k: string]: null | undefined }}\ndeclare const i: {iface};\ni.{member}.foo;\ni.{member}();"
        );
        let codes = non_strict(&source);
        assert_eq!(
            count(&codes, TS18047),
            1,
            "an index-signature value typed `null | undefined` must report TS18047 (interface {iface}), got: {codes:?}"
        );
        assert_eq!(
            count(&codes, TS2721),
            1,
            "an index-signature value typed `null | undefined` callee must report TS2721 (interface {iface}), got: {codes:?}"
        );
        assert_eq!(
            count(&codes, 18049),
            0,
            "the non-strict answer must not be the strict two-cause TS18049 (interface {iface}), got: {codes:?}"
        );
    }
}

#[test]
fn interface_number_index_signature_value_collapses_the_same_way() {
    // Sibling of the string index signature: a `[k: number]` value union of
    // pure `null | undefined` collapses identically, observed through element
    // access. The receiver `g[0]` is an ElementAccessExpression, not an
    // entity name, so tsc reports the *object* form TS2531 ("Object is
    // possibly 'null'") rather than the named TS18047 — exactly the
    // entity-name distinction this file already documents for a `new C().m`
    // base. Either code proves the collapse fired: before the fix `g[0]` was a
    // surviving `null | undefined` union (own flags `Union`, non-strict arm
    // silent) and neither code appeared. The callee `g[0]()` still reports
    // TS2721, whose reporter does not depend on the entity-name form.
    let source = "interface Grid { [k: number]: null | undefined }\ndeclare const g: Grid;\ng[0].foo;\ng[0]();";
    let codes = non_strict(source);
    assert_eq!(
        count(&codes, 2531),
        1,
        "a number-index-signature value typed `null | undefined` accessed via `g[0]` must report TS2531, got: {codes:?}"
    );
    assert_eq!(
        count(&codes, TS2721),
        1,
        "a number-index-signature value typed `null | undefined` callee must report TS2721, got: {codes:?}"
    );
}

#[test]
fn interface_index_signature_uniform_undefined_stays_undefined_not_null() {
    // Regression guard mirroring the annotation case: `undefined | undefined`
    // has no `null` member, so an index value written that way must resolve to
    // `undefined` (TS18048), never be dragged to `null` (TS18047).
    let source =
        "interface I { [k: string]: undefined | undefined }\ndeclare const i: I;\ni.m.foo;";
    let codes = non_strict(source);
    assert_eq!(
        count(&codes, TS18048),
        1,
        "a uniform `undefined | undefined` index value must report TS18048, got: {codes:?}"
    );
    assert_eq!(
        count(&codes, TS18047),
        0,
        "a uniform `undefined | undefined` index value must not report TS18047, got: {codes:?}"
    );
}

#[test]
fn heritage_bearing_interface_member_collapses_through_the_general_path() {
    // A property-signature interface that ALSO carries heritage is rejected by
    // the simple-interface fast path (`RejectHeritageExtends`) and lowers
    // through the same general `tsz-lowering` path as the index-signature
    // cases — so the fix must reach it too, proving the seam is the general
    // object/interface lowering rather than index signatures specifically.
    let source = "interface Base {}\ninterface Holder extends Base { m: null | undefined }\ndeclare const h: Holder;\nh.m.foo;\nh.m();";
    let codes = non_strict(source);
    assert_eq!(
        count(&codes, TS18047),
        1,
        "a heritage-bearing interface member typed `null | undefined` must report TS18047, got: {codes:?}"
    );
    assert_eq!(
        count(&codes, TS2721),
        1,
        "a heritage-bearing interface member typed `null | undefined` callee must report TS2721, got: {codes:?}"
    );
}

#[test]
fn strict_mode_keeps_the_two_cause_answer_for_an_index_signature() {
    // Regression control: `strictNullChecks` must NOT collapse the index-value
    // union — the opt-in wires the real option, so strict mode keeps the
    // two-cause TS18049/TS2723 exactly as it already did.
    let source =
        "interface I { [k: string]: null | undefined }\ndeclare const i: I;\ni.m.foo;\ni.m();";
    let strict = check_source_strict_codes(source);
    assert_eq!(
        count(&strict, 18049),
        1,
        "strict mode must keep the two-cause TS18049 for an index signature, got: {strict:?}"
    );
    assert_eq!(
        count(&strict, 2723),
        1,
        "strict mode must keep the two-cause TS2723 for an index-signature callee, got: {strict:?}"
    );
    assert_eq!(
        count(&strict, TS18047),
        0,
        "strict mode must not collapse an index value to the single-cause TS18047, got: {strict:?}"
    );
}

#[test]
fn null_or_undefined_union_class_field_and_object_literal_type_already_collapsed() {
    // Controls: these two sibling paths were already correct before this fix
    // (a class field's own type-node resolution and an object type literal
    // used via a type alias both already routed through the patched
    // `type_operators.rs` / `type_node.rs` union constructors) — confirming
    // the interface-member path above was the one genuinely missing arm, not
    // a symptom visible everywhere member types are declared.
    // `c` is a plain identifier so the property access stays a "dotted name"
    // reference, matching the other TS18047 rows above; a `new C().m` base
    // (not a simple identifier) instead reports the generic TS2531 "Object is
    // possibly 'null'" — that is tsc's own real divergence, not a bug.
    let class_field =
        non_strict("class C { m: null | undefined = null; }\nconst c = new C();\nc.m.foo;");
    assert_eq!(
        count(&class_field, TS18047),
        1,
        "a class field typed `null | undefined` must report TS18047, got: {class_field:?}"
    );

    let type_literal =
        non_strict("type T = { m: null | undefined };\ndeclare const t: T;\nt.m.foo;");
    assert_eq!(
        count(&type_literal, TS18047),
        1,
        "an object-type-literal member typed `null | undefined` must report TS18047, got: {type_literal:?}"
    );
}

#[test]
fn null_or_undefined_union_with_a_non_nullish_member_stays_clean() {
    // The collapse is specific to a PURE null/undefined union — a union that
    // also carries a real member is the already-correct `T | null` narrowing
    // (its own flags are `TypeFlags.Union`), unaffected by this change.
    let codes = non_strict("declare const un: { a: number } | null | undefined;\nun.a;\nun;");
    assert!(
        nullish_codes(&codes).is_empty(),
        "a `T | null | undefined` annotation must stay clean without strictNullChecks, got: {codes:?}"
    );
}

#[test]
fn strict_mode_keeps_the_two_cause_answer_for_a_null_or_undefined_union() {
    let source = "declare const onu: null | undefined;\nonu.foo;\nonu();";
    let strict = check_source_strict_codes(source);
    assert_eq!(
        count(&strict, 18049),
        1,
        "strict mode must keep the two-cause TS18049 for `null | undefined`, got: {strict:?}"
    );
    assert_eq!(
        count(&strict, 2723),
        1,
        "strict mode must keep the two-cause TS2723 for a `null | undefined` callee, got: {strict:?}"
    );
    assert_eq!(
        count(&strict, TS18047),
        0,
        "strict mode must not collapse to the single-cause TS18047, got: {strict:?}"
    );
}

#[test]
fn strict_mode_rows_are_unchanged_for_the_directly_nullish_operands() {
    let source = "declare const on: null;\non.foo;\non();\non[0];\n\"\" in on;\n~on;";
    let strict = check_source_strict_codes(source);
    assert_eq!(
        count(&strict, TS18047),
        4,
        "strict mode must keep four TS18047 rows, got: {strict:?}"
    );
    assert_eq!(
        count(&strict, TS2721),
        1,
        "strict mode must keep TS2721 for the callee, got: {strict:?}"
    );
}
