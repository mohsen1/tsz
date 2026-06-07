use crate::context::CheckerOptions;
use crate::test_utils::check_source;

fn diagnostics_for(source: &str) -> Vec<crate::diagnostics::Diagnostic> {
    check_source(source, "test.ts", CheckerOptions::default())
}

/// A generic interface reference written with a free type-parameter argument
/// must display as that reference (`Holder<Elem>`), not as its evaluated
/// instance shape with a member-derived, over-instantiated argument
/// (`Holder<Wrap<Elem>>`). Regression for the
/// `infiniteExpansionThroughInstantiation` recursive-types family (#10867).
/// Binder names are deliberately non-canonical so the assertion checks
/// structure, not spellings.
#[test]
fn generic_interface_source_display_preserves_as_written_type_argument() {
    let diagnostics = diagnostics_for(
        r#"
interface Wrap<A> { a: A; }
interface Holder<U> { item: Wrap<U>; }
function f<Elem>(o: Holder<Elem>) {
    const x: number = o;
}
"#,
    );

    let diag = diagnostics
        .iter()
        .find(|d| d.code == 2322)
        .expect("expected TS2322");
    assert!(
        diag.message_text
            .contains("Type 'Holder<Elem>' is not assignable"),
        "generic interface source should display its as-written reference, got: {diag:?}"
    );
    assert!(
        !diag.message_text.contains("Holder<Wrap<Elem>>"),
        "generic interface source must not over-instantiate the type argument from its member, got: {diag:?}"
    );
}

/// The interface heritage variant of the same family: `Owner<U>` extends
/// `Linked<Linked<U>>` must still display `Owner<T>`, not
/// `Owner<Linked<T>>`. This mirrors the heritage shape of the canonical
/// conformance witness.
#[test]
fn generic_interface_heritage_source_display_is_not_over_instantiated() {
    let diagnostics = diagnostics_for(
        r#"
interface Linked<T> { data: T; }
interface Owner<U> extends Linked<Linked<U>> { name: string; }
function f<Item>(o: Owner<Item>) {
    const x: number = o;
}
"#,
    );

    let diag = diagnostics
        .iter()
        .find(|d| d.code == 2322)
        .expect("expected TS2322");
    assert!(
        diag.message_text
            .contains("Type 'Owner<Item>' is not assignable"),
        "heritage-bearing generic interface source should display its as-written reference, got: {diag:?}"
    );
    assert!(
        !diag.message_text.contains("Owner<Linked<Item>>"),
        "heritage instantiation must not leak into the displayed type argument, got: {diag:?}"
    );
}

/// Concrete (fully-resolved) generic interface arguments are unaffected: the
/// structural/widening display path already matches tsc, so `Holder<number>`
/// keeps rendering with its concrete argument.
#[test]
fn concrete_generic_interface_source_display_is_unchanged() {
    let diagnostics = diagnostics_for(
        r#"
interface Wrap<A> { a: A; }
interface Holder<U> { item: Wrap<U>; }
declare const o: Holder<number>;
const x: string = o;
"#,
    );

    let diag = diagnostics
        .iter()
        .find(|d| d.code == 2322)
        .expect("expected TS2322");
    assert!(
        diag.message_text
            .contains("Type 'Holder<number>' is not assignable"),
        "concrete generic interface source should keep its concrete argument, got: {diag:?}"
    );
}
