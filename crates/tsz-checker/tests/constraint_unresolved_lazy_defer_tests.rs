//! Regression tests for #14337: the generic type-argument constraint check
//! (`validate_type_args_against_params`, TS2344) must not fail closed against a
//! constraint it could not fully resolve.
//!
//! In the ts-rest canary row, `Record<number, ContractAnyType>` (with
//! `ContractAnyType` a deeply-recursive `zod` schema graph) produced a false
//! `TS2344: Type 'number' does not satisfy the constraint 'string | number |
//! symbol'`. Tracing showed the instantiated key constraint stayed an
//! unresolved `Lazy(DefId)` for `PropertyKey` (`keyof any`) — the cross-file
//! lazy-resolution budget was exhausted while the sibling deeply-recursive
//! value argument was evaluated — so the reflexively-true `number <:
//! string | number | symbol` relation came back `false` against the opaque
//! reference. The fix makes one more genuine resolution attempt and, if the
//! constraint stays opaque, defers rather than emitting TS2344 (matching tsc,
//! which always has the resolved constraint here).
//!
//! The graph-scale trigger itself is exercised by the ts-rest canary
//! project-compile row in CI. These unit tests lock the *acceptance* direction
//! the bug violated (valid `PropertyKey` keys must be accepted) and guard
//! against the fix over-deferring: genuine constraint violations, whose
//! constraint resolves normally, must still emit TS2344.

use tsz_binder::BinderState;
use tsz_checker::context::CheckerOptions;
use tsz_checker::state::CheckerState;
use tsz_checker::test_utils::diagnostic_code_messages;
use tsz_parser::parser::ParserState;
use tsz_solver::construction::TypeInterner;

fn ts2344_count(source: &str) -> usize {
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        CheckerOptions::default(),
    );
    checker.check_source_file(root);

    diagnostic_code_messages(checker.ctx.diagnostics)
        .iter()
        .filter(|(code, _)| *code == 2344)
        .count()
}

/// A valid `number` key against a `PropertyKey` (`string | number | symbol`)
/// constraint must be accepted — the direction the #14337 false positive broke.
#[test]
fn number_key_satisfies_property_key_constraint() {
    let src = r#"
type PropertyKey = string | number | symbol;
type Rec<K extends PropertyKey, V> = { [P in K]: V };
type X = Rec<number, { a: number }>;
declare const x: X;
export { x };
"#;
    assert_eq!(
        ts2344_count(src),
        0,
        "number satisfies string | number | symbol"
    );
}

/// Same acceptance, name-agnostic: the rule must not key on the identifier
/// spelling of the key parameter or the alias.
#[test]
fn valid_key_accepted_with_renamed_binders() {
    let src = r#"
type Prop = string | number | symbol;
type Dictionary<TheKey extends Prop, TheValue> = { [Entry in TheKey]: TheValue };
type Y = Dictionary<string, { z: boolean }>;
declare const y: Y;
export { y };
"#;
    assert_eq!(
        ts2344_count(src),
        0,
        "string satisfies the renamed PropertyKey constraint"
    );
}

/// A resolvable-constraint violation must still emit TS2344 — the fix defers
/// only for an *unresolved* constraint, never for a genuine mismatch.
#[test]
fn object_key_violates_property_key_constraint_still_errors() {
    let src = r#"
type PropertyKey = string | number | symbol;
type Rec<K extends PropertyKey, V> = { [P in K]: V };
type Bad = Rec<{ a: 1 }, string>;
declare const bad: Bad;
export { bad };
"#;
    assert_eq!(
        ts2344_count(src),
        1,
        "an object type does not satisfy string | number | symbol"
    );
}

/// `boolean` is not a `PropertyKey`; a resolvable constraint must still reject
/// it (no over-deferral).
#[test]
fn boolean_key_violates_property_key_constraint_still_errors() {
    let src = r#"
type PropertyKey = string | number | symbol;
type Rec<K extends PropertyKey, V> = { [P in K]: V };
type Bad = Rec<boolean, string>;
declare const bad: Bad;
export { bad };
"#;
    assert_eq!(
        ts2344_count(src),
        1,
        "boolean does not satisfy string | number | symbol"
    );
}

/// A concrete primitive constraint (`string`) with a mismatched concrete
/// argument (`number`) must still error — guards the same modified path.
#[test]
fn concrete_primitive_constraint_violation_still_errors() {
    let src = r#"
type Wrap<K extends string> = { value: K };
type Bad = Wrap<number>;
declare const bad: Bad;
export { bad };
"#;
    assert_eq!(ts2344_count(src), 1, "number does not satisfy string");
}
