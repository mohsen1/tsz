//! Locks in TS2345 message rendering for generic calls whose parameter type
//! is a bare type parameter and whose argument has a different primitive base
//! from the (possibly inferred) substitution.
//!
//! tsc's behaviour (verified against `primitiveConstraints2.ts` and
//! `typeInferenceConflictingCandidates.ts`):
//!
//! * When the type parameter has an effective constraint chain that bottoms
//!   out in a primitive (e.g. `<U extends T>` where `T` was already fixed to
//!   `number`), the source and target are rendered as primitives:
//!   `'string' / 'number'`. tsc's `getWidenedLiteralType` runs because the
//!   constraint fallback materialises as a primitive parameter type.
//! * When the type parameter has no primitive constraint (e.g.
//!   `<T>(a: T, b: T)`), the inference candidates' literal types are
//!   preserved in the diagnostic: `'3' / '""'`. tsc keeps the candidate-style
//!   display in this case.
//!
//! Earlier, tsz's `generic_direct_primitive_mismatch_display` only widened
//! for rest parameters. These tests guard against regressions in the
//! non-rest, primitive-constraint path while ensuring the no-constraint path
//! continues to preserve literal candidates.
//!
//! The tests deliberately use two different type-parameter names (`T`/`U`
//! and `K`/`V`) to verify the rule is structural, not bound to a particular
//! identifier — see `.claude/CLAUDE.md` §25.

fn diagnostics(source: &str) -> Vec<(u32, String)> {
    tsz_checker::test_utils::check_source_code_messages(source)
}

fn ts2345_messages(source: &str) -> Vec<String> {
    diagnostics(source)
        .into_iter()
        .filter_map(|(code, msg)| (code == 2345).then_some(msg))
        .collect()
}

// `<U extends T>` where `T` is fixed to a primitive (`number`) via the class
// type argument: tsc renders 'string' / 'number'.
#[test]
fn class_method_constrained_type_param_widens_literal_displays() {
    let source = r#"
class C<T> {
   public bar2<U extends T>(x: T, y: U): T {
      return null as any;
   }
}
const x = new C<number>();
x.bar2(2, "");
"#;
    let msgs = ts2345_messages(source);
    assert_eq!(msgs.len(), 1, "expected one TS2345, got: {msgs:#?}");
    let msg = &msgs[0];
    assert!(
        msg.contains("Argument of type 'string'") && msg.contains("parameter of type 'number'"),
        "TS2345 in class method should widen displays, got: {msg}"
    );
}

// Renaming the bound type parameters must not change behaviour — the rule is
// structural.
#[test]
fn class_method_constrained_type_param_widening_is_structural_under_renaming() {
    let source = r#"
class Box<K> {
   m<V extends K>(p: K, q: V): void {}
}
new Box<number>().m(2, "");
"#;
    let msgs = ts2345_messages(source);
    assert_eq!(msgs.len(), 1);
    assert!(
        msgs[0].contains("Argument of type 'string'")
            && msgs[0].contains("parameter of type 'number'"),
        "renamed type parameters should still widen under primitive constraint, got: {msgs:#?}"
    );
}

// `<U extends number>`: direct primitive constraint also triggers widening.
#[test]
fn direct_primitive_constraint_widens_literal_source() {
    let source = r#"
function f<U extends number>(x: U): void {}
f("");
"#;
    let msgs = ts2345_messages(source);
    assert_eq!(msgs.len(), 1);
    assert!(
        msgs[0].contains("Argument of type 'string'")
            && msgs[0].contains("parameter of type 'number'"),
        "direct primitive constraint should widen, got: {msgs:#?}"
    );
}

// `<T>(a: T, b: T)` with no constraint: tsc preserves the literal candidates.
// The fix must NOT widen here.
#[test]
fn unconstrained_type_param_preserves_literal_displays() {
    let source = r#"
declare function bar<T>(item1: T, item2: T): T;
bar(1, "");
"#;
    let msgs = ts2345_messages(source);
    assert_eq!(msgs.len(), 1, "expected one TS2345, got: {msgs:#?}");
    assert!(
        msgs[0].contains("Argument of type '\"\"'") && msgs[0].contains("parameter of type '1'"),
        "unconstrained type parameter must keep literal candidates, got: {msgs:#?}"
    );
}

#[test]
fn implemented_unconstrained_type_param_widens_conflicting_literal_displays() {
    let source = r#"
function bar<T>(item1: T, item2: T) {}
bar(1, "");
"#;
    let msgs = ts2345_messages(source);
    assert_eq!(msgs.len(), 1, "expected one TS2345, got: {msgs:#?}");
    assert!(
        msgs[0].contains("Argument of type 'string'")
            && msgs[0].contains("parameter of type 'number'"),
        "implemented generic signature should widen conflicting primitive candidates, got: {msgs:#?}"
    );
}

// `<T, U extends T>` (free function) where `T` is fixed from an earlier
// argument: tsc widens the later mismatch to the primitive bases even though
// the declared constraint chain ends at an unconstrained type parameter.
#[test]
fn nested_type_param_constraint_to_inferred_param_widens_literals() {
    let source = r#"
function f<T, U extends T>(x: T, y: U): void {}
f(2, "");
"#;
    let msgs = ts2345_messages(source);
    assert_eq!(msgs.len(), 1, "expected one TS2345, got: {msgs:#?}");
    assert!(
        msgs[0].contains("Argument of type 'string'")
            && msgs[0].contains("parameter of type 'number'"),
        "constraint chain through an inferred type parameter should widen, got: {msgs:#?}"
    );
}

#[test]
fn implemented_return_type_mentions_generic_name_exactly() {
    let source = r#"
interface Test {}
function bar<T>(item1: T, item2: T): Test {
    return {};
}
bar(1, "");
"#;
    let msgs = ts2345_messages(source);
    assert_eq!(msgs.len(), 1, "expected one TS2345, got: {msgs:#?}");
    assert!(
        msgs[0].contains("Argument of type 'string'")
            && msgs[0].contains("parameter of type 'number'"),
        "return type `Test` must not be treated as mentioning generic `T`, got: {msgs:#?}"
    );
}

// Hand-written literal targets (not generic) MUST keep their literal display —
// tsc preserves '3' / '2' when the parameter type was authored as a literal.
// This guards against the helper over-firing on non-generic literal-typed
// parameters (where there is no type parameter substitution in play).
#[test]
fn non_generic_literal_target_preserves_literal_display() {
    let source = r#"
function f(x: 2): void {}
f(3);
"#;
    let msgs = ts2345_messages(source);
    assert!(
        msgs.iter().any(|m| m.contains("'3'") && m.contains("'2'")),
        "literal-typed (non-generic) target must keep literal display, got: {msgs:#?}"
    );
    assert!(
        !msgs.iter().any(|m| m.contains("'number'")),
        "non-generic literal target must not be widened to 'number', got: {msgs:#?}"
    );
}

// When source and target widen to the SAME primitive base, no widening should
// occur — preserves "Argument of type '3' is not assignable to parameter of
// type '2'." which is correct.
#[test]
fn same_primitive_base_does_not_trigger_generic_widening() {
    let source = r#"
function f<T>(x: T, y: T): void {}
f(2, 3);
"#;
    let msgs = ts2345_messages(source);
    // Either no error (if T = 2|3 is inferred) or a literal-preserving error.
    // Either way, the widened "number"/"number" form must NOT appear.
    assert!(
        !msgs
            .iter()
            .any(|m| m.contains("type 'number'") && m.contains("parameter of type 'number'")),
        "same-base generic mismatch should not produce 'number' / 'number', got: {msgs:#?}"
    );
}
