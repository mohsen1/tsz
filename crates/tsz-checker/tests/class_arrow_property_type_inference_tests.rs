//! Regression tests for #9619 — class members whose value comes from an
//! arrow-function initializer (e.g. `prop = (x: T) => U`) must keep that
//! signature as the property's type. Two stacked bugs combined to lose it:
//!
//! 1. **Recursive class type rebuild during the property's own compute.**
//!    `class_property_arrow_lexical_this_type` (called from
//!    `get_type_of_function_impl` to capture lexical `this`) eagerly built
//!    the class instance type. During member checking the
//!    `class_instance_type_cache` is intentionally cleared, so the lookup
//!    triggered a full re-build. That re-build asked
//!    `get_type_of_node(prop.initializer)` for the arrow — but the arrow
//!    was already on `node_resolution_stack` from the outer compute, so
//!    the circular-reference guard returned `TypeId::ERROR`, which got
//!    stored as the property's type. Every subsequent read of `c.prop`
//!    came back as ERROR, silently passing every assignability check.
//!
//! 2. **Inner failure rendering looked up the wrong anchor.**
//!    `render_failure_reason` for `ParameterTypeMismatch` used the
//!    `AssignmentSource` display role with the outer anchor node, which
//!    re-resolved the type of the enclosing RHS expression (the class
//!    instance) instead of the inner mismatched function type. The
//!    `_` catchall arm already had a `depth > 0 ⇒ structural formatter`
//!    guard; the `ParameterTypeMismatch` arm didn't, so it leaked the
//!    enclosing class name into the inner elaboration.
//!
//! These tests pin both fixes at the structural-rule level: vary names,
//! the property kind (arrow / function expression), and the cross-file
//! shape so the regression is caught regardless of incidental spelling.

use tsz_checker::diagnostics::Diagnostic;
use tsz_checker::test_utils::check_source_strict;
use tsz_checker::test_utils::check_source_strict_messages as check_strict;

fn diagnostic_codes(diags: &[(u32, String)]) -> Vec<u32> {
    diags.iter().map(|(c, _)| *c).collect()
}

fn join_messages(diags: &[(u32, String)]) -> String {
    diags
        .iter()
        .map(|(c, m)| format!("TS{c}: {m}"))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Flatten a diagnostic and all of its `related_information` messages into
/// one searchable string. Inner-elaboration tests need to inspect the
/// related messages, not just the top-level.
fn full_messages(diags: &[Diagnostic]) -> String {
    let mut out = String::new();
    for d in diags {
        out.push_str(&format!("TS{}: {}\n", d.code, d.message_text));
        for related in &d.related_information {
            out.push_str(&format!("  TS{}: {}\n", related.code, related.message_text));
        }
    }
    out
}

/// The reported repro from #9619: a class with an arrow-function property
/// must carry the arrow's signature on the property, not collapse to
/// `any` / `error`. Assigning a function to `string` must error.
#[test]
fn class_arrow_property_function_value_rejects_string_assignment() {
    let source = r#"
class C {
  prop = (db: number): number => db;
}
declare const c: C;
const wrong: string = c.prop;
"#;
    let diags = check_strict(source);
    assert!(
        diagnostic_codes(&diags).contains(&2322),
        "expected TS2322 for function-to-string assignment, got: {}",
        join_messages(&diags),
    );
}

/// Calling the property with the wrong arity must surface TS2554 —
/// proves the parameter list, not just the return type, survives.
#[test]
fn class_arrow_property_keeps_parameter_count() {
    let source = r#"
class C {
  prop = (db: number): number => db;
}
declare const c: C;
c.prop();
"#;
    let diags = check_strict(source);
    assert!(
        diagnostic_codes(&diags).contains(&2554),
        "expected TS2554 for arity mismatch, got: {}",
        join_messages(&diags),
    );
}

/// Renamed type-parameter / property axis: the rule must not be keyed on
/// the spelling `prop` / `db`. Per §25, structural fixes survive renaming.
#[test]
fn class_arrow_property_signature_survives_rename() {
    let source = r#"
class WithAdapter {
  acquireLock = (handle: number): number => handle;
}
declare const w: WithAdapter;
const wrong: string = w.acquireLock;
"#;
    let diags = check_strict(source);
    assert!(
        diagnostic_codes(&diags).contains(&2322),
        "expected TS2322 for renamed class/property, got: {}",
        join_messages(&diags),
    );
}

/// Function-expression initializer (not arrow) must follow the same rule.
/// Function expressions don't capture lexical `this`, so they exercise the
/// non-arrow branch of `get_type_of_function_impl`.
#[test]
fn class_function_expression_property_keeps_signature() {
    let source = r#"
class C {
  prop = function (db: number): number { return db; };
}
declare const c: C;
const wrong: string = c.prop;
"#;
    let diags = check_strict(source);
    assert!(
        diagnostic_codes(&diags).contains(&2322),
        "expected TS2322 for function-expression property, got: {}",
        join_messages(&diags),
    );
}

/// Class with arrow property satisfying an interface property of the same
/// shape — the property type must survive into structural comparison.
/// This is the simplest expression of the Kysely `MssqlAdapter` symptom
/// from #9619 where a class method was being related as the class itself.
#[test]
fn class_arrow_property_satisfies_compatible_interface_property() {
    let source = r#"
interface Lock {
  acquire: (handle: number) => number;
}
class WithLock {
  acquire = (handle: number): number => handle;
}
const x: Lock = new WithLock();
"#;
    let diags = check_strict(source);
    assert!(
        diags.is_empty(),
        "no diagnostics expected when shapes match — got: {}",
        join_messages(&diags),
    );
}

/// Inner-elaboration rendering: when a class with an arrow property is
/// assigned to an interface whose same-named property has a different
/// parameter type, the inner "Type X is not assignable to type Y" must
/// render the *function* type on the left, not the enclosing class name.
///
/// This is the second bug in #9619 ("source rendered as `MssqlAdapter`
/// instead of the method signature").
#[test]
fn class_arrow_property_inner_elaboration_shows_function_signature_not_class() {
    let source = r#"
interface Lock {
  acquire: (handle: number, options: string) => number;
}
class WithLock {
  acquire = (handle: number, options: number): number => handle;
}
const x: Lock = new WithLock();
"#;
    let diags = check_source_strict(source);
    let joined = full_messages(&diags);
    let codes: Vec<u32> = diags.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&2322),
        "expected TS2322 for incompatible property, got: {joined}",
    );
    // The inner elaboration must surface the source's function signature,
    // not the enclosing class name (`WithLock`). Use the smallest spelling
    // that pins the structural rule without hardcoding to one class name.
    assert!(
        joined.contains("(handle: number, options: number) => number"),
        "inner elaboration must render the source's function signature; got: {joined}",
    );
    assert!(
        !joined.contains(
            "Type 'WithLock' is not assignable to type '(handle: number, options: string) => number'"
        ),
        "inner elaboration must not collapse the source function type to the enclosing class name; got: {joined}",
    );
}

/// Negative case: the same shape with matching parameter types must
/// still be accepted, proving the fix doesn't over-reject.
#[test]
fn class_arrow_property_matching_signature_still_accepted() {
    let source = r#"
interface Lock {
  acquire: (handle: number, options: number) => number;
}
class WithLock {
  acquire = (handle: number, options: number): number => handle;
}
const x: Lock = new WithLock();
"#;
    let diags = check_strict(source);
    assert!(
        diags.is_empty(),
        "matching signatures must not produce diagnostics, got: {}",
        join_messages(&diags),
    );
}

/// Method-signature target (`acquire(h, o): R`) vs arrow-property source.
/// Method signatures are bivariant in parameter types, so this still
/// fails because `number` and `string` are not comparable either way.
/// The point of this test is to verify the *display* still shows the
/// source as the function signature even when the target is a method.
#[test]
fn class_arrow_property_vs_interface_method_signature_renders_signature() {
    let source = r#"
interface Lock {
  acquire(handle: number, options: string): number;
}
class WithLock {
  acquire = (handle: number, options: number): number => handle;
}
const x: Lock = new WithLock();
"#;
    let diags = check_source_strict(source);
    let joined = full_messages(&diags);
    let codes: Vec<u32> = diags.iter().map(|d| d.code).collect();
    assert!(codes.contains(&2322), "expected TS2322, got: {joined}");
    assert!(
        joined.contains("(handle: number, options: number) => number"),
        "source must render as function signature, got: {joined}",
    );
}
