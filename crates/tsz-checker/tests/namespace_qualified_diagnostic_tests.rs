//! Tests for namespace-qualified type display in TS2741 diagnostics.
//!
//! When two classes share the same short name but live in different
//! namespaces (e.g. `M.A` and `N.A`), the diagnostic must qualify both
//! names so the reader can tell them apart.
//!
//! Regression: the source side was being taken verbatim from the
//! constructor expression text (e.g. `new N.A()` yielded `N.A`), while
//! the target side went through the type formatter which only emits the
//! class's short name (`A`). The two sides collided at the bare-name
//! level even though the two strings weren't equal, so the existing
//! pair-disambiguation check (comparing `src_str == tgt_str`) never
//! fired and the target was left unqualified.

fn get_diagnostics(source: &str) -> Vec<(u32, String)> {
    tsz_checker::test_utils::check_source_code_messages(source)
}

#[test]
fn ts2741_qualifies_both_sides_when_classes_collide_across_namespaces() {
    // Two namespaces each declare a class named `A` with different required
    // properties. The assignment mentions both via qualified names, and the
    // emitted TS2741 diagnostic must qualify BOTH sides (source and target)
    // so the reader can tell them apart.
    let source = r#"
namespace M { export class A { name: string = ""; } }
namespace N { export class A { id: number = 0; } }
var x: M.A = new N.A();
"#;
    let diags = get_diagnostics(source);

    let ts2741: Vec<_> = diags.iter().filter(|(c, _)| *c == 2741).collect();
    assert_eq!(
        ts2741.len(),
        1,
        "expected exactly one TS2741 diagnostic; got: {diags:?}"
    );
    let msg = &ts2741[0].1;
    assert!(
        msg.contains("'N.A'") && msg.contains("'M.A'"),
        "TS2741 message should qualify both source and target classes with their namespaces, got: {msg:?}"
    );
    assert!(
        !msg.contains("but required in type 'A'."),
        "TS2741 target should be qualified as 'M.A', not 'A'. got: {msg:?}"
    );
}

#[test]
fn ts2741_leaves_unique_short_name_unqualified() {
    // When there is no short-name collision, the diagnostic should continue
    // to use the bare class name — tsc behaviour for the unambiguous case.
    let source = r#"
namespace M { export class A { name: string = ""; } }
class Other { other: string = ""; }
var w: M.A = new Other();
"#;
    let diags = get_diagnostics(source);
    let ts2741: Vec<_> = diags.iter().filter(|(c, _)| *c == 2741).collect();
    assert_eq!(
        ts2741.len(),
        1,
        "expected exactly one TS2741 diagnostic; got: {diags:?}"
    );
    let msg = &ts2741[0].1;
    // The target is `M.A` but nothing else shares the name `A`, so tsc
    // uses the bare `A` in the message (no namespace qualification).
    assert!(
        msg.contains("'Other'") && msg.contains("'A'"),
        "expected bare names in message; got: {msg:?}"
    );
    assert!(
        !msg.contains("'M.A'"),
        "target should not be qualified when there is no collision; got: {msg:?}"
    );
}

#[test]
fn ts2559_assignment_qualifies_weak_type_pair_when_names_collide() {
    let source = r#"
namespace M { export interface A { m?: string; } }
namespace N { export interface A { n?: number; } }
const sourceValue: N.A = {};
const targetValue: M.A = sourceValue;
"#;
    let diags = get_diagnostics(source);

    let ts2559: Vec<_> = diags.iter().filter(|(c, _)| *c == 2559).collect();
    assert_eq!(
        ts2559.len(),
        1,
        "expected exactly one TS2559 diagnostic; got: {diags:?}"
    );
    let msg = &ts2559[0].1;
    assert!(
        msg.contains("'N.A'") && msg.contains("'M.A'"),
        "TS2559 assignment should qualify both weak types, got: {msg:?}"
    );
    assert!(
        !msg.contains("Type 'A' has no properties in common with type 'A'."),
        "TS2559 assignment should not collapse both sides to the same short name, got: {msg:?}"
    );
}

#[test]
fn ts2559_call_argument_qualifies_weak_type_pair_when_names_collide() {
    let source = r#"
namespace M { export interface A { m?: string; } }
namespace N { export interface A { n?: number; } }
declare function take(value: M.A): void;
const sourceValue: N.A = {};
take(sourceValue);
"#;
    let diags = get_diagnostics(source);

    let ts2559: Vec<_> = diags.iter().filter(|(c, _)| *c == 2559).collect();
    assert_eq!(
        ts2559.len(),
        1,
        "expected exactly one TS2559 diagnostic; got: {diags:?}"
    );
    let msg = &ts2559[0].1;
    assert!(
        msg.contains("'N.A'") && msg.contains("'M.A'"),
        "TS2559 call argument should qualify both weak types, got: {msg:?}"
    );
    assert!(
        !msg.contains("Type 'A' has no properties in common with type 'A'."),
        "TS2559 call argument should not collapse both sides to the same short name, got: {msg:?}"
    );
}

#[test]
fn ts2559_generic_constraint_qualifies_weak_type_pair_when_names_collide() {
    let source = r#"
namespace M { export interface A { m?: string; } }
namespace N { export interface A { n?: number; } }
type Box<T extends M.A> = T;
type Bad = Box<N.A>;
"#;
    let diags = get_diagnostics(source);

    let ts2559: Vec<_> = diags.iter().filter(|(c, _)| *c == 2559).collect();
    assert_eq!(
        ts2559.len(),
        1,
        "expected exactly one TS2559 diagnostic; got: {diags:?}"
    );
    let msg = &ts2559[0].1;
    assert!(
        msg.contains("'N.A'") && msg.contains("'M.A'"),
        "TS2559 generic constraint should qualify both weak types, got: {msg:?}"
    );
    assert!(
        !msg.contains("Type 'A' has no properties in common with type 'A'."),
        "TS2559 generic constraint should not collapse both sides to the same short name, got: {msg:?}"
    );
}

fn get_diagnostics_strict(source: &str) -> Vec<(u32, String)> {
    let mut parser =
        tsz_parser::parser::ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = tsz_binder::BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let types = tsz_solver::TypeInterner::new();
    let options = tsz_checker::context::CheckerOptions {
        strict_null_checks: true,
        ..Default::default()
    };
    let mut checker = tsz_checker::state::CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        options,
    );

    checker.check_source_file(root);

    checker
        .ctx
        .diagnostics
        .iter()
        .map(|d| (d.code, d.message_text.clone()))
        .collect()
}

/// Primitive value (e.g. `false`) assigned to an optional property whose
/// declared type is a single weak object must produce TS2559 (not TS2322),
/// with the literal source type (`'false'` not widened to `'boolean'`) and
/// the declared target shape (`'OverridesInput'` not `'OverridesInput |
/// undefined'`). Mirrors the failing nested-elaboration shape from
/// `nestedExcessPropertyChecking.ts` under `// @strict: true`. Regression
/// test for the boundary's `weak_union_violation` flag previously bailing
/// on non-union targets.
#[test]
fn ts2559_for_primitive_assigned_to_weak_object_property() {
    let source = r#"
type OverridesInput = { someProp?: 'A' | 'B' };
interface Unrelated { _?: any }
interface VariablesA { overrides?: OverridesInput }
interface VariablesB { overrides?: OverridesInput }
const foo: Unrelated & { variables: VariablesA & VariablesB } = {
    variables: { overrides: false }
};
"#;
    let diags = get_diagnostics_strict(source);

    let ts2559: Vec<_> = diags.iter().filter(|(c, _)| *c == 2559).collect();
    let ts2322: Vec<_> = diags.iter().filter(|(c, _)| *c == 2322).collect();
    assert!(
        !ts2559.is_empty(),
        "expected at least one TS2559 for `false` against weak `OverridesInput`; got: {diags:?}"
    );
    assert!(
        ts2322.is_empty(),
        "weak-target primitive mismatch should NOT emit TS2322; got: {diags:?}"
    );
    let msg = &ts2559[0].1;
    assert!(
        msg.contains("'false'"),
        "TS2559 source should preserve the literal `false`, not widen to `boolean`: {msg:?}"
    );
    assert!(
        msg.contains("'OverridesInput'") && !msg.contains("'OverridesInput | undefined'"),
        "TS2559 target should show declared type without strict-null `| undefined`: {msg:?}"
    );
}
