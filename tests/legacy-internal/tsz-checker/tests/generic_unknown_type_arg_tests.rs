use crate::test_utils::check_source_diagnostics;

#[test]
fn unknown_type_arg_constraint_uses_keyword_syntax_with_trivia() {
    let diagnostics = check_source_diagnostics(
        r#"
type Need<T extends string> = T;
type Bad = Need</* preserved trivia */ unknown>;
"#,
    );

    let matches = diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.code == 2344)
        .collect::<Vec<_>>();
    assert_eq!(
        matches.len(),
        1,
        "expected exactly one TS2344 for unknown type argument, got: {diagnostics:?}"
    );
    assert!(
        matches[0]
            .message_text
            .contains("Type 'unknown' does not satisfy the constraint 'string'."),
        "expected TS2344 to report the unknown keyword and string constraint, got: {:?}",
        matches[0]
    );
}

#[test]
fn unknown_type_arg_satisfies_indexed_access_constraint_reducing_to_top_type() {
    // `A[number]` with `A = unknown[]` reduces to `unknown`; the top-type
    // `unknown` argument satisfies it (`unknown ⊑ unknown`), matching tsc.
    // The unreduced indexed-access form `unknown[][number]` must not drive a
    // false TS2344. Covers a bare index, a literal index, a nested
    // indexed-access constraint, and renamed binders (not name-driven).
    let diagnostics = check_source_diagnostics(
        r#"
interface P<A extends unknown[], B extends A[number]> { x: B; }
type R = P<unknown[], unknown>;

interface PL<A extends unknown[], B extends A[0]> { x: B; }
type RL = PL<unknown[], unknown>;

interface PN<A extends unknown[][], B extends A[number][number]> { x: B; }
type RN = PN<unknown[][], unknown>;

interface Qq<Zz extends unknown[], Yy extends Zz[number]> { x: Yy; }
type RQ = Qq<unknown[], unknown>;
"#,
    );

    let matches = diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.code == 2344)
        .collect::<Vec<_>>();
    assert!(
        matches.is_empty(),
        "expected no TS2344 when an indexed-access constraint reduces to a top type, got: {matches:?}"
    );
}

#[test]
fn unknown_type_arg_satisfies_non_reducible_unknown_top_type_constraint() {
    // `{} | null | undefined` (TypeScript's `NonReducibleUnknown` idiom) is
    // structurally equal to `unknown`, so the top-type `unknown` argument
    // satisfies it (`unknown ⊑ {} | null | undefined`), matching tsc. Covers the
    // canonical form, a reordered union, and renamed binders (not name-driven).
    let diagnostics = check_source_diagnostics(
        r#"
type NonReducibleUnknown = {} | null | undefined;
type Box<T extends NonReducibleUnknown> = { value: T };
type A = Box<unknown>;

type Reordered<Q extends null | {} | undefined> = { v: Q };
type B = Reordered<unknown>;
"#,
    );

    let matches = diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.code == 2344)
        .collect::<Vec<_>>();
    assert!(
        matches.is_empty(),
        "expected no TS2344 when the constraint is a top type ({{}} | null | undefined), got: {matches:?}"
    );
}

#[test]
fn unknown_type_arg_still_fails_near_top_constraints_missing_null_or_undefined() {
    // `{}` and `{} | null` are NOT top types (they exclude `null`/`undefined` or
    // `undefined`), so `unknown` is not assignable to them and TS2344 must still
    // fire — the top-type acceptance is precise, not a blanket unknown pass.
    let diagnostics = check_source_diagnostics(
        r#"
type Obj<T extends {}> = { v: T };
type Bad1 = Obj<unknown>;

type ObjN<U extends {} | null> = { v: U };
type Bad2 = ObjN<unknown>;
"#,
    );

    let matches = diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.code == 2344)
        .collect::<Vec<_>>();
    assert_eq!(
        matches.len(),
        2,
        "expected TS2344 for both non-top constraints ({{}} and {{}} | null), got: {diagnostics:?}"
    );
}

#[test]
fn unknown_type_arg_still_fails_indexed_access_constraint_reducing_to_proper_type() {
    // `A[number]` with `A = string[]` reduces to `string`; the top-type
    // `unknown` argument is not assignable to `string`, so TS2344 must still
    // fire — and report tsc's reduced constraint `'string'`, not the
    // unreduced `'string[][number]'`.
    let diagnostics = check_source_diagnostics(
        r#"
interface P<A extends unknown[], B extends A[number]> { x: B; }
type Bad = P<string[], unknown>;
"#,
    );

    let matches = diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.code == 2344)
        .collect::<Vec<_>>();
    assert_eq!(
        matches.len(),
        1,
        "expected exactly one TS2344, got: {diagnostics:?}"
    );
    assert!(
        matches[0]
            .message_text
            .contains("Type 'unknown' does not satisfy the constraint 'string'."),
        "expected the reduced constraint 'string' in the TS2344 message, got: {:?}",
        matches[0]
    );
}
