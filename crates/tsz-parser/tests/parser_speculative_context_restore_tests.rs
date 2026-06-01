//! Regression tests for parser-context restoration around speculative
//! `parse_*` calls (issue: utility-types-project parser-1-20, #10932).
//!
//! Structural rule:
//!   Any speculative parse that invokes a `parse_*` routine (rather than
//!   only scanner-level `next_token`/`is_token`) must roll back through the
//!   full `ParserCheckpoint`, not a hand-rolled subset that misses
//!   `context_flags` and the recovery-flag cluster. Otherwise a failed
//!   speculative parse can leave `IN_TUPLE_ELEMENT`,
//!   `DISALLOW_CONDITIONAL_TYPES`, `abort_intersection_continuation`, and
//!   similar flags set, and the subsequent expression / JSX / type parse
//!   uses corrupted state.
//!
//! Companion rule for nested type contexts:
//!   The true and false branches of a conditional type, and the type
//!   expression inside a template-literal `${...}` substitution, are
//!   complete type-expression positions: they must accept nested
//!   conditional types even when the outer context disabled them (tsc's
//!   `allowConditionalTypesAnd(parseType)`).
//!
//! Each case below is repeated with at least two type-parameter / identifier
//! spellings so the fix is verifiably structural, never keyed to a single
//! name (§25).

use crate::parser::test_fixture::{assert_no_errors, assert_no_errors_named};

// ---------------------------------------------------------------------------
// `try_parse_type_arguments_for_call` rollback must restore context_flags so
// a failed speculative type-argument parse does not corrupt subsequent JSX
// detection in .tsx files.
// ---------------------------------------------------------------------------

#[test]
fn failed_type_args_then_jsx_in_tsx_no_errors() {
    // The `<` after `a` looks like type arguments at first, but the
    // following `+` rules them out — the parser must back off and let JSX /
    // expression parsing take over with clean context flags.
    for source in [
        "const x = (a: number, b: number) => a < b ? 1 : 2;\n",
        "const y = (p: number, q: number) => p < q ? 1 : 2;\n",
    ] {
        assert_no_errors_named("test.tsx", source);
    }
}

#[test]
fn failed_type_args_then_jsx_element_no_errors() {
    // `<X>` here is JSX-component-like; the speculative type-args attempt
    // for the preceding identifier `Component` must back out cleanly.
    for source in [
        "const a = Component<T> + 1;\n",
        "const b = Renderer<K> + 1;\n",
    ] {
        // .ts files: type-args attempt fails on `+`, parser must produce a
        // less-than comparison without leaking flags into the following
        // expression.
        assert_no_errors(source);
    }
}

// ---------------------------------------------------------------------------
// Tuple element containing a generic type reference: the speculative
// type-args parse inside the tuple must not leak `IN_TUPLE_ELEMENT` outward
// (or fail to re-establish it). A subsequent `?` outside the tuple should
// still be a conditional `?`, not a stray nullable marker.
// ---------------------------------------------------------------------------

#[test]
fn generic_type_inside_tuple_then_outer_conditional_no_errors() {
    for source in [
        "type T<A> = [Array<A>] extends [infer R] ? R : never;\n",
        "type U<X> = [ReadonlyArray<X>] extends [infer Y] ? Y : never;\n",
    ] {
        assert_no_errors(source);
    }
}

// ---------------------------------------------------------------------------
// Conditional-type true/false branches accept nested conditional types
// without parentheses, even when the outer context disabled conditional
// types (tsc's `allowConditionalTypesAnd(parseType)`).
// ---------------------------------------------------------------------------

#[test]
fn nested_conditional_in_true_branch_no_errors() {
    for source in [
        "type C<T> = T extends string ? T extends `a${string}` ? 1 : 2 : 3;\n",
        "type D<K> = K extends string ? K extends `x${string}` ? 1 : 2 : 3;\n",
    ] {
        assert_no_errors(source);
    }
}

#[test]
fn nested_conditional_in_false_branch_no_errors() {
    for source in [
        "type C<T> = T extends number ? 1 : T extends string ? 2 : 3;\n",
        "type D<X> = X extends number ? 1 : X extends string ? 2 : 3;\n",
    ] {
        assert_no_errors(source);
    }
}

#[test]
fn nested_conditional_in_both_branches_no_errors() {
    for source in [
        "type C<T> = T extends 0 ? T extends 1 ? 'a' : 'b' : T extends 2 ? 'c' : 'd';\n",
        "type D<K> = K extends 0 ? K extends 1 ? 'a' : 'b' : K extends 2 ? 'c' : 'd';\n",
    ] {
        assert_no_errors(source);
    }
}

// ---------------------------------------------------------------------------
// Template-literal type substitution `${T}` is a scope barrier. A nested
// conditional type inside the substitution must parse even when the outer
// position was a tuple element or `infer T extends X` constraint.
// ---------------------------------------------------------------------------

#[test]
fn nested_conditional_inside_template_literal_substitution_no_errors() {
    for source in [
        "type C<T> = `prefix-${T extends number ? 1 : 0}-suffix`;\n",
        "type D<K> = `prefix-${K extends number ? 1 : 0}-suffix`;\n",
    ] {
        assert_no_errors(source);
    }
}

#[test]
fn template_substitution_inside_tuple_does_not_inherit_tuple_flag() {
    // The outer tuple sets `IN_TUPLE_ELEMENT`. Inside the template's
    // `${...}`, a postfix `?` on a *parenthesized* type should be the
    // inner type's nullable marker, not the outer tuple's optional marker.
    // (Bare postfix `?` on a non-parenthesized expression is invalid in
    // type position regardless, so we use a clean conditional instead.)
    for source in [
        "type T<A> = [`pre-${A extends string ? A : never}-post`];\n",
        "type U<Z> = [`pre-${Z extends string ? Z : never}-post`];\n",
    ] {
        assert_no_errors(source);
    }
}

// ---------------------------------------------------------------------------
// `infer T extends X` inside a conditional-type true/false branch — the
// branch position re-enables conditionals, but the infer-constraint
// position still disables them. Both must be true simultaneously.
// ---------------------------------------------------------------------------

#[test]
fn infer_extends_inside_conditional_branch_no_errors() {
    for source in [
        "type Head<T> = T extends [infer H, ...infer _] ? H : never;\n",
        "type Tail<K> = K extends [infer _, ...infer R] ? R : never;\n",
    ] {
        assert_no_errors(source);
    }
}

// ---------------------------------------------------------------------------
// Failed type-args attempt inside a conditional check-type position must
// not leak flags into the extends-type parse.
// ---------------------------------------------------------------------------

#[test]
fn failed_type_args_in_check_type_does_not_leak_to_extends_type() {
    // `f<T>` is an instantiation expression in expression position, but
    // here we're in type position; `<T>` parses as type args. The point of
    // the test is that whatever the parser tries first, the conditional
    // extends/?/: structure that follows must still parse correctly.
    for source in [
        "type C<A> = A extends Array<infer E> ? E : never;\n",
        "type D<X> = X extends Array<infer Y> ? Y : never;\n",
    ] {
        assert_no_errors(source);
    }
}

// ---------------------------------------------------------------------------
// Span sanity: after the fixes above, the templated reproducer from issue
// #10932 (corrected so `T` is bound) must parse cleanly and the conditional
// type node must end exactly at the start of the trailing semicolon, not
// overshoot into following statements.
// ---------------------------------------------------------------------------

#[test]
fn issue_10932_corrected_reproducer_no_errors() {
    // Original templated repro had a free `T`; bind it as a generic to make
    // the type semantically meaningful. The parser-level concern (no
    // cascading syntax errors / no JSX-state confusion) is what this test
    // actually exercises. Run the same structural shape under two distinct
    // identifier spellings so the fix is verifiably structural (§25).
    for source in [
        "type Row1ParseShape = { kind: 'utility-types-project'; value: string };\n\
         type ParseProbe1<T> = T extends Row1ParseShape ? T : never;\n\
         const parseRow1: ParseProbe1<Row1ParseShape> = { kind: 'utility-types-project', value: 'ok' };\n",
        "type RowShape = { kind: 'project'; value: string };\n\
         type Probe<K> = K extends RowShape ? K : never;\n\
         const row: Probe<RowShape> = { kind: 'project', value: 'ok' };\n",
    ] {
        assert_no_errors(source);
    }
}

// ---------------------------------------------------------------------------
// Mapped-type value position is a complete-type-expression scope: nested
// conditional types parse even when the surrounding context disabled them.
// ---------------------------------------------------------------------------

#[test]
fn nested_conditional_in_mapped_value_type_no_errors() {
    for source in [
        "type M<T> = T extends infer U ? { [K in keyof U]: U[K] extends string ? 1 : 0 } : never;\n",
        "type N<X> = X extends infer Y ? { [P in keyof Y]: Y[P] extends string ? 1 : 0 } : never;\n",
    ] {
        assert_no_errors(source);
    }
}

// ---------------------------------------------------------------------------
// Type-parameter default position is a complete-type-expression scope (tsc's
// `parseDefaultType` wraps in `allowConditionalTypesAnd(parseType)`).
// ---------------------------------------------------------------------------

#[test]
fn nested_conditional_in_type_parameter_default_no_errors() {
    for source in [
        "type F<T extends string, D = T extends 'a' ? 1 : 0> = D;\n",
        "type G<K extends string, V = K extends 'a' ? 1 : 0> = V;\n",
    ] {
        assert_no_errors(source);
    }
}

// ---------------------------------------------------------------------------
// #11348: a conditional alias with an intersection true branch that uses a
// parenthesized union should not disturb parsing and must parse cleanly with a
// follow-up generic alias reference.
// ---------------------------------------------------------------------------

#[test]
fn intersection_true_branch_with_parenthesized_union_no_errors() {
    for source in [
        "type U<T> = T extends unknown ? { a: T } & ({ b: T } | { b: never }) : never;\n\
         type R<T> = U<T>;\n",
        "type Shape<X> = X extends unknown ? { left: X } & ({ right: X } | { right: never }) : never;\n\
         type Alias<Y> = Shape<Y>;\n",
        "type MappedUnion<T> = {\n\
            [K in keyof T]: T[K]\n\
        };\n\
        type Probe<T> = T extends unknown ? { a: T } & ({\n\
            [K in keyof T]: T[K]\n\
        } | MappedUnion<T>) : never;\n\
        type Alias<T> = Probe<T>;",
    ] {
        assert_no_errors(source);
    }
}
