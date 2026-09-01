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

/// A *named* generic conditional alias used as the source of an assignability
/// error must display by its alias application (`Branch<U>`), not collapse to
/// the union of its branches. `tsc` keeps the alias spelling for a deferred
/// conditional that carries its own `aliasSymbol`; only an anonymous deferred
/// conditional renders as the branch union. Binder names are deliberately
/// non-canonical so the assertion checks structure, not spellings. Regression
/// for the `conditionalTypes1` fixture (#14141): `T95<U>` was rendered as
/// `number | boolean`.
#[test]
fn named_conditional_alias_source_display_keeps_alias_not_branch_union() {
    let diagnostics = diagnostics_for(
        r#"
type Target<Q> = Q extends string ? true : 42;
type Branch<Q> = Q extends string ? boolean : number;
function f<U>(value: Branch<U>): Target<U> {
    return value;
}
"#,
    );

    let diag = diagnostics
        .iter()
        .find(|d| d.code == 2322)
        .expect("expected TS2322");
    assert!(
        diag.message_text
            .contains("Type 'Branch<U>' is not assignable to type 'Target<U>'."),
        "named conditional alias source should display its alias application, got: {diag:?}"
    );
    assert!(
        !diag.message_text.contains("number | boolean"),
        "named conditional alias source must not collapse to its branch union, got: {diag:?}"
    );
}

/// Helper: find the TS2322 and return its `(top message, related messages)`.
fn ts2322_with_related(diagnostics: &[crate::diagnostics::Diagnostic]) -> (String, Vec<String>) {
    let diag = diagnostics
        .iter()
        .find(|d| d.code == 2322)
        .expect("expected TS2322");
    (
        diag.message_text.clone(),
        diag.related_information
            .iter()
            .map(|r| r.message_text.clone())
            .collect(),
    )
}

/// Assigning one instantiation of a generic to another (same base, differing
/// type argument) in a **variable declaration** must elaborate the failing
/// type-argument relation under the top-line TS2322, matching tsc:
///
/// ```text
/// Type 'Container<string>' is not assignable to type 'Container<number>'.
///   Type 'string' is not assignable to type 'number'.
/// ```
///
/// Regression: the public-variance prepass used to emit a bare top-line and
/// drop the nested `TypeArgumentMismatch` reason for the var-decl /
/// assignment-expression path, while the return-statement and argument
/// (TS2345) paths kept it. Binder names are deliberately non-canonical so the
/// assertion checks structure, not spellings.
#[test]
fn same_base_generic_var_decl_elaborates_failing_type_argument() {
    let diagnostics = diagnostics_for(
        r#"
interface Container<Payload> { item: Payload; }
declare const source: Container<string>;
const sink: Container<number> = source;
"#,
    );
    let (top, related) = ts2322_with_related(&diagnostics);
    assert!(
        top.contains("Container<string>") && top.contains("Container<number>"),
        "top line should name both same-base instantiations, got: {top:?}"
    );
    assert!(
        related
            .iter()
            .any(|m| m.contains("'string' is not assignable to type 'number'")),
        "must elaborate the failing type argument under the top-line, got related: {related:?}"
    );
}

/// Same divergence through an **assignment expression** (`x = y`) rather than a
/// declaration: the elaboration must still carry the type-argument reason.
#[test]
fn same_base_generic_assignment_expr_elaborates_failing_type_argument() {
    let diagnostics = diagnostics_for(
        r#"
interface Cell<Slot> { slot: Slot; }
declare let target: Cell<number>;
declare const origin: Cell<string>;
target = origin;
"#,
    );
    let (_, related) = ts2322_with_related(&diagnostics);
    assert!(
        related
            .iter()
            .any(|m| m.contains("'string' is not assignable to type 'number'")),
        "assignment-expression path must elaborate the failing type argument, got: {related:?}"
    );
}

/// Two type parameters where only the **second** argument differs: the
/// elaboration must point at the offending argument's relation, not be dropped.
#[test]
fn same_base_two_type_param_application_elaborates_second_argument() {
    let diagnostics = diagnostics_for(
        r#"
interface Couple<First, Second> { left: First; right: Second; }
declare const made: Couple<number, string>;
const want: Couple<number, number> = made;
"#,
    );
    let (top, related) = ts2322_with_related(&diagnostics);
    assert!(
        top.contains("Couple<number, string>") && top.contains("Couple<number, number>"),
        "top line should name both instantiations, got: {top:?}"
    );
    assert!(
        related
            .iter()
            .any(|m| m.contains("'string' is not assignable to type 'number'")),
        "the differing (second) type argument must be elaborated, got: {related:?}"
    );
}
