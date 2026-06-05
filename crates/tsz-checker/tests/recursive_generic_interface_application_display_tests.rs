//! Generic nominal (interface/class) instances must display their originating
//! type reference — `OwnerList<T>` — in TS2322 messages, not a one-level
//! heritage expansion such as `OwnerList<List<T>>`.
//!
//! When a generic interface with infinitely-expanding recursive heritage
//! (`interface OwnerList<U> extends List<List<U>>`) is instantiated with an
//! argument that carries a free type parameter, the solver eagerly evaluates it
//! to a structural object. Previously the evaluator dropped the back-reference
//! to the originating `Application(OwnerList, [T])` whenever any argument
//! contained a free type parameter, so the diagnostic layer fell back to a
//! property-based type-argument recovery. For this heritage shape no member
//! exposes the bare parameter `U` (every member wraps it, e.g. `data: List<U>`),
//! so the recovery mis-read an arbitrary instantiated member type (`List<T>`) as
//! the type argument and rendered `OwnerList<List<T>>`.
//!
//! The evaluator now records display provenance for these instances, so display
//! matches `tsc`. Binder names are varied across cases to prove the behavior is
//! structural and not keyed on a fixture identifier; concrete and non-recursive
//! instances are exercised as controls.

use tsz_checker::context::CheckerOptions;
use tsz_common::diagnostics::Diagnostic;

fn check_strict(source: &str) -> Vec<Diagnostic> {
    let options = CheckerOptions {
        strict: true,
        strict_null_checks: true,
        ..Default::default()
    };
    tsz_checker::test_utils::check_source(source, "test.ts", options)
}

fn message_for(diags: &[Diagnostic], code: u32) -> String {
    let matches: Vec<&Diagnostic> = diags.iter().filter(|d| d.code == code).collect();
    assert!(
        !matches.is_empty(),
        "expected a TS{code} diagnostic, got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, &d.message_text))
            .collect::<Vec<_>>()
    );
    matches[0].message_text.clone()
}

/// The conformance fixture shape: a generic interface with infinitely-expanding
/// recursive heritage, instantiated with a free type parameter, must show its
/// bare type argument `T` — not the one-level expansion `List<T>`.
#[test]
fn recursive_heritage_generic_instance_source_displays_bare_type_argument() {
    let diags = check_strict(
        "interface List<T> { data: T; next: List<T>; owner: OwnerList<T>; }\n\
         interface OwnerList<U> extends List<List<U>> { name: string; }\n\
         function other<T>(x: T) { var o!: OwnerList<T>; var s: string = o; }\n",
    );
    let msg = message_for(&diags, 2322);
    assert!(
        msg.contains("Type 'OwnerList<T>' is not assignable to type 'string'"),
        "recursive generic interface source must display its bare type argument; got: {msg}"
    );
    assert!(
        !msg.contains("OwnerList<List<T>>"),
        "recursive generic interface source must not over-instantiate the argument; got: {msg}"
    );
}

/// Same structural shape with every binder renamed — the display must not depend
/// on the chosen interface, property, or type-parameter identifiers.
#[test]
fn recursive_heritage_display_independent_of_binder_names() {
    let diags = check_strict(
        "interface Seq<E> { head: E; tail: Seq<E>; meta: Owner2<E>; }\n\
         interface Owner2<W> extends Seq<Seq<W>> { tag: string; }\n\
         function run<Q>(q: Q) { var o!: Owner2<Q>; var s: string = o; }\n",
    );
    let msg = message_for(&diags, 2322);
    assert!(
        msg.contains("Type 'Owner2<Q>' is not assignable to type 'string'"),
        "renamed recursive generic interface source must display its bare argument; got: {msg}"
    );
    assert!(
        !msg.contains("Owner2<Seq<Q>>"),
        "renamed recursive generic interface source must not over-instantiate; got: {msg}"
    );
}

/// The full fixture assignment (`List<T> = OwnerList<T>`) keeps both nominal
/// names with their original type arguments on the top-level message.
#[test]
fn recursive_heritage_assignment_between_related_interfaces_keeps_arguments() {
    let diags = check_strict(
        "interface List<T> { data: T; next: List<T>; owner: OwnerList<T>; }\n\
         interface OwnerList<U> extends List<List<U>> { name: string; }\n\
         function other<T>() { var list!: List<T>; var owner!: OwnerList<T>; list = owner; }\n",
    );
    let msg = message_for(&diags, 2322);
    assert!(
        msg.contains("Type 'OwnerList<T>' is not assignable to type 'List<T>'"),
        "related recursive interfaces must keep their bare type arguments; got: {msg}"
    );
    assert!(
        !msg.contains("OwnerList<List<T>>"),
        "source must not over-instantiate the recursive argument; got: {msg}"
    );
}

/// Control: a concrete instantiation was already correct and must stay so —
/// the argument is shown verbatim, never expanded one heritage level.
#[test]
fn concrete_recursive_heritage_instance_displays_concrete_argument() {
    let diags = check_strict(
        "interface List<T> { data: T; next: List<T>; owner: OwnerList<T>; }\n\
         interface OwnerList<U> extends List<List<U>> { name: string; }\n\
         var o!: OwnerList<string>; var s: string = o;\n",
    );
    let msg = message_for(&diags, 2322);
    assert!(
        msg.contains("Type 'OwnerList<string>' is not assignable to type 'string'"),
        "concrete recursive interface source must display its concrete argument; got: {msg}"
    );
    assert!(
        !msg.contains("OwnerList<List<string>>"),
        "concrete recursive interface source must not over-instantiate; got: {msg}"
    );
}

/// Control: a non-recursive generic interface with parameterized heritage keeps
/// rendering its bare type argument (it relied on the property-based recovery
/// before and continues to display correctly via the recorded provenance).
#[test]
fn non_recursive_generic_interface_instance_displays_bare_argument() {
    let diags = check_strict(
        "interface Base<B> { b: B; }\n\
         interface Derived<D> extends Base<D[]> { d: D; }\n\
         function gen<T>() { var v!: Derived<T>; var s: string = v; }\n",
    );
    let msg = message_for(&diags, 2322);
    assert!(
        msg.contains("Type 'Derived<T>' is not assignable to type 'string'"),
        "non-recursive generic interface source must display its bare argument; got: {msg}"
    );
}
