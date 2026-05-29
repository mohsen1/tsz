//! Declaration-emit parenthesization regression tests:
//!
//! * source-parenthesized `infer` operands in conditional types,
//! * intersection members nested inside a union,
//! * template-literal type text re-escaping.
//!
//! Each fix is exercised with at least two distinct bound-variable name
//! choices so the test fails if any fix were keyed on a specific identifier
//! spelling rather than the structural shape.

use super::*;

// ---------------------------------------------------------------------------
// Fix A: a source-parenthesized `infer X` *check* operand of a conditional
// keeps its parens, because the conditional's own `extends` keyword would
// otherwise be reabsorbed as the infer type-parameter's constraint clause.
// This holds whether or not the infer already carries its own constraint.
// ---------------------------------------------------------------------------

#[test]
fn check_operand_parenthesized_infer_keeps_parens_no_constraint() {
    // `(infer A) extends infer B ? ...` must not collapse to
    // `infer A extends infer B ? ...`.
    let output = emit_dts("export type T<X> = (infer A) extends infer B ? infer C : infer D;");
    assert!(
        output.contains("(infer A) extends infer B ?"),
        "parenthesized infer check operand must keep parens: {output}"
    );
}

#[test]
fn check_operand_parenthesized_infer_keeps_parens_renamed() {
    // Same rule, different iteration-variable spelling: proves the fix is
    // not keyed on the name `A`.
    let output = emit_dts("export type T<X> = (infer Z9) extends infer Q ? infer R : infer S;");
    assert!(
        output.contains("(infer Z9) extends infer Q ?"),
        "parenthesized infer check operand must keep parens (renamed): {output}"
    );
}

#[test]
fn nested_check_operand_parenthesized_infer_keeps_parens() {
    // The inner conditional sits in the outer conditional's extends position;
    // its own `(infer U)` check operand must still be parenthesized.
    let output =
        emit_dts("export type T<X> = X extends ((infer U) extends number ? 1 : 0) ? 1 : 0;");
    assert!(
        output.contains("(infer U) extends number ?"),
        "nested parenthesized infer check operand must keep parens: {output}"
    );
}

// ---------------------------------------------------------------------------
// Fix A: a source-parenthesized `infer U extends C` *extends* operand keeps
// its parens, but a bare `(infer U)` extends operand (no constraint) drops
// the redundant parens.
// ---------------------------------------------------------------------------

#[test]
fn extends_operand_parenthesized_infer_with_constraint_keeps_parens() {
    let output =
        emit_dts("export type T<X> = X extends (infer U extends number) ? [X, U] : never;");
    assert!(
        output.contains("(infer U extends number) ?"),
        "constrained infer extends operand must keep parens: {output}"
    );
}

#[test]
fn extends_operand_parenthesized_infer_with_constraint_keeps_parens_renamed() {
    // Renamed iteration variable proves the rule is structural.
    let output =
        emit_dts("export type T<X> = X extends (infer Vee extends string) ? [X, Vee] : never;");
    assert!(
        output.contains("(infer Vee extends string) ?"),
        "constrained infer extends operand must keep parens (renamed): {output}"
    );
}

#[test]
fn extends_operand_parenthesized_infer_without_constraint_drops_redundant_parens() {
    // Negative/fallback case: a bare `(infer U)` extends operand has no
    // constraint clause to capture the conditional's tokens, so tsc drops
    // the redundant parens.
    let output = emit_dts("export type T<X> = X extends (infer U) ? [X, U] : never;");
    assert!(
        output.contains("extends infer U ?") && !output.contains("(infer U) ?"),
        "bare infer extends operand should drop redundant parens: {output}"
    );
}

// ---------------------------------------------------------------------------
// Fix B(a): an intersection member nested in a union is parenthesized so the
// grouping round-trips (`A & B | C` => `(A & B) | C`).
// ---------------------------------------------------------------------------

#[test]
fn intersection_member_in_union_is_parenthesized() {
    let output = emit_dts("export declare function f(x: \"a\" | (string & {})): void;");
    assert!(
        output.contains("\"a\" | (string & {})"),
        "intersection member in union must be parenthesized: {output}"
    );
}

#[test]
fn intersection_member_in_union_is_parenthesized_other_shape() {
    // A different member ordering and a non-empty intersection arm proves the
    // rule is about the intersection shape, not the literal `string & {}`.
    let output = emit_dts("export type U = number | (boolean & { tag: 1 });");
    assert!(
        output.contains("(boolean & {"),
        "intersection member in union must be parenthesized (other shape): {output}"
    );
}

// ---------------------------------------------------------------------------
// Fix B(b): template-literal type text spans are re-escaped for the
// backtick-delimited context (control chars become escape sequences again).
// ---------------------------------------------------------------------------

#[test]
fn template_literal_type_text_is_reescaped() {
    let output = emit_dts("export type T = `${string}:\\t${number}\\r\\n`;");
    assert!(
        output.contains("`${string}:\\t${number}\\r\\n`"),
        "template-literal text must re-escape control chars: {output:?}"
    );
    // The cooked tab/newline must not leak through verbatim.
    assert!(
        !output.contains('\t'),
        "template-literal text must not emit a literal tab: {output:?}"
    );
}

#[test]
fn template_literal_type_text_reescapes_backtick_and_dollar_brace() {
    // A backtick and a `${` in the literal text must be escaped so the text
    // does not prematurely close the template or start a substitution.
    let output = emit_dts("export type T = `a\\`b\\${c}d${string}`;");
    assert!(
        output.contains("a\\`b\\${c}d"),
        "backtick and ${{ in template text must be escaped: {output:?}"
    );
}

// ---------------------------------------------------------------------------
// Fix C: a source-parenthesized array element keeps its parens verbatim in
// the copied annotation (`(T)[]` stays `(T)[]`, not `T[]`). The fix keys on
// the source `PARENTHESIZED_TYPE` array element, never on the element's name.
// ---------------------------------------------------------------------------

#[test]
fn parenthesized_array_element_keeps_parens() {
    // `(Foo)[]` is a redundant-but-user-written grouping that tsc preserves
    // verbatim in the .d.ts.
    let output = emit_dts("export type T = (Foo)[];");
    assert!(
        output.contains("(Foo)[]"),
        "source-parenthesized array element must keep parens: {output}"
    );
}

#[test]
fn parenthesized_array_element_keeps_parens_renamed() {
    // Same rule, different element spelling: proves the fix is not keyed on a
    // particular type-reference name.
    let output = emit_dts("export type T = (Widget9)[];");
    assert!(
        output.contains("(Widget9)[]"),
        "source-parenthesized array element must keep parens (renamed): {output}"
    );
}

#[test]
fn unparenthesized_array_element_stays_unparenthesized() {
    // Negative/fallback case: a bare array element gets no synthesized parens.
    let output = emit_dts("export type T = Foo[];");
    assert!(
        output.contains("Foo[]") && !output.contains("(Foo)[]"),
        "bare array element must stay unparenthesized: {output}"
    );
}

// ---------------------------------------------------------------------------
// Fix C: a *bare* intersection member of a union keeps its source spelling
// (`T | T & undefined` stays unparenthesized), while a *source-parenthesized*
// intersection member keeps its parens. Both branches key on the source
// `PARENTHESIZED_TYPE` wrapper, never on the member's spelling.
// ---------------------------------------------------------------------------

#[test]
fn unparenthesized_intersection_union_member_stays_unparenthesized() {
    // `T | T & undefined` is the verbatim source; tsc preserves the exact
    // unparenthesized grouping rather than synthesizing `T | (T & undefined)`.
    let output = emit_dts("export declare function f<T>(a: T | T & undefined): void;");
    assert!(
        output.contains("a: T | T & undefined"),
        "bare intersection union member must stay unparenthesized: {output}"
    );
    assert!(
        !output.contains("a: T | (T & undefined)"),
        "bare intersection union member must not gain synthesized parens: {output}"
    );
}

#[test]
fn unparenthesized_intersection_union_member_stays_unparenthesized_renamed() {
    // Different type-parameter spelling proves the rule is structural.
    let output = emit_dts("export declare function g<Elem>(a: Elem | Elem & null): void;");
    assert!(
        output.contains("a: Elem | Elem & null"),
        "bare intersection union member must stay unparenthesized (renamed): {output}"
    );
    assert!(
        !output.contains("(Elem & null)"),
        "bare intersection union member must not gain synthesized parens (renamed): {output}"
    );
}

#[test]
fn parenthesized_intersection_union_member_keeps_parens() {
    // When the source parenthesizes the intersection arm, those parens
    // round-trip so the grouping stays unambiguous.
    let output = emit_dts("export declare function f<T>(a: T | (T & undefined)): void;");
    assert!(
        output.contains("a: T | (T & undefined)"),
        "source-parenthesized intersection union member must keep parens: {output}"
    );
}

// ---------------------------------------------------------------------------
// Fix D (refines #11687): a `PARENTHESIZED_TYPE` reached directly by
// `emit_type` (an annotation-like position: type-alias RHS, mapped-type
// value, function/constructor return, type-predicate target, …) preserves
// its source parens when the inner type is a composite that benefits from
// explicit grouping (union, intersection, conditional, mapped/type-literal,
// tuple) or an `infer` carrying a constraint, and strips them when the inner
// is atomic or a function/constructor type. The decision keys on the peeled
// inner node *kind*, never on names or rendered output.
// ---------------------------------------------------------------------------

#[test]
fn alias_rhs_parenthesized_intersection_keeps_parens() {
    // `keyofIntersection` / `parenthesisDoesNotBlockAliasSymbolCreation`:
    // a source-parenthesized intersection in alias-RHS position keeps parens.
    let output = emit_dts("export type T<A, B> = (A & B);");
    assert!(
        output.contains("= (A & B);"),
        "parenthesized intersection in alias-RHS must keep parens: {output}"
    );
}

#[test]
fn alias_rhs_parenthesized_intersection_keeps_parens_renamed() {
    // Renamed type parameters prove the rule is structural, not name-keyed.
    let output = emit_dts("export type Wrap<Left, Right> = (Left & Right);");
    assert!(
        output.contains("= (Left & Right);"),
        "parenthesized intersection in alias-RHS must keep parens (renamed): {output}"
    );
}

#[test]
fn alias_rhs_parenthesized_union_keeps_parens() {
    let output = emit_dts("export type T = (string | number);");
    assert!(
        output.contains("= (string | number);"),
        "parenthesized union in alias-RHS must keep parens: {output}"
    );
}

#[test]
fn alias_rhs_parenthesized_tuple_keeps_parens() {
    // `destructuringInFunctionType`: `type T1 = ([a, b, c]);` keeps parens.
    let output = emit_dts("export type T = ([number, string]);");
    assert!(
        output.contains("= ([number, string]);"),
        "parenthesized tuple in alias-RHS must keep parens: {output}"
    );
}

#[test]
fn predicate_target_parenthesized_union_keeps_parens() {
    // `narrowingUnionToUnion`: `x is (string | 0)` keeps parens.
    let output = emit_dts("export declare function check(x: unknown): x is (string | 0);");
    assert!(
        output.contains("x is (string | 0)"),
        "parenthesized type-predicate target union must keep parens: {output}"
    );
}

#[test]
fn predicate_target_parenthesized_intersection_keeps_parens() {
    // `typeGuardOfFormThisMember`: `this is (Networked & this)` keeps parens.
    let output = emit_dts(
        "export interface Thing { isNet(): this is (Networked & this); }\ninterface Networked {}",
    );
    assert!(
        output.contains("this is (Networked & this)"),
        "parenthesized type-predicate target intersection must keep parens: {output}"
    );
}

#[test]
fn mapped_type_value_parenthesized_conditional_keeps_parens() {
    // `recursiveTypeReferences2`: a source-parenthesized conditional as a
    // mapped-type value keeps parens.
    let output = emit_dts("export type M<T> = { [K in keyof T]?: (T[K] extends string ? 1 : 0) };");
    assert!(
        output.contains("(T[K] extends string ? 1 : 0)"),
        "parenthesized conditional mapped-type value must keep parens: {output}"
    );
}

#[test]
fn function_return_parenthesized_infer_with_constraint_keeps_parens() {
    // `inferTypesWithExtends1` X3: a function-return-position
    // `(infer U extends string)` keeps parens (the trailing `extends` would
    // otherwise re-absorb the enclosing conditional's tokens).
    let output = emit_dts(
        "export type T<F> = F extends (...a: any[]) => (infer U extends string) ? U : never;",
    );
    assert!(
        output.contains("=> (infer U extends string) ?"),
        "parenthesized constrained infer in function return must keep parens: {output}"
    );
}

#[test]
fn function_return_parenthesized_infer_with_constraint_keeps_parens_renamed() {
    // Renamed infer variable proves the rule is structural.
    let output = emit_dts(
        "export type T<F> = F extends (...a: any[]) => (infer Vee extends number) ? Vee : never;",
    );
    assert!(
        output.contains("=> (infer Vee extends number) ?"),
        "parenthesized constrained infer in function return must keep parens (renamed): {output}"
    );
}

#[test]
fn annotation_redundant_parens_around_keyword_are_stripped() {
    // #11687's win: `var x: (string)` → `string`.
    let output = emit_dts("export declare var x: (string);");
    assert!(
        output.contains("var x: string;") && !output.contains("(string)"),
        "redundant parens around a keyword type must be stripped: {output}"
    );
}

#[test]
fn annotation_redundant_parens_around_function_type_are_stripped() {
    // #11687's win: `var f: (() => c)` → `() => c` in direct annotation
    // position. (Function-type parens are *kept* only as union/array members,
    // which are handled by their own structural arms.)
    let output = emit_dts("export declare var f: (() => string);");
    assert!(
        output.contains("var f: () => string;") && !output.contains("(() => string)"),
        "redundant parens around a function type must be stripped: {output}"
    );
}

#[test]
fn annotation_bare_parenthesized_infer_drops_redundant_parens() {
    // Negative/fallback: a bare `(infer U)` with no constraint has nothing to
    // capture trailing tokens, so the redundant parens drop. Exercised via the
    // function-return annotation position so it reaches this arm directly.
    let output = emit_dts("export type T<F> = F extends (...a: any[]) => (infer U) ? U : never;");
    assert!(
        !output.contains("=> (infer U)"),
        "bare infer (no constraint) in annotation position should drop redundant parens: {output}"
    );
}
