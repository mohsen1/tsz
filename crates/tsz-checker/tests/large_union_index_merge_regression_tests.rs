use tsz_checker::diagnostics::diagnostic_codes;
use tsz_checker::test_utils::check_source_strict;

#[test]
fn numeric_index_merge_reports_only_descendant_index_signature_conflict() {
    let diagnostics = check_source_strict(
        r#"
interface Node {
    nodeType: number;
}

interface Element extends Node {
    tagName: string;
}

interface HTMLElement extends Element {
    [index: number]: HTMLElement;
}

interface HTMLFormElement extends HTMLElement {
    [index: number]: Element;
}

interface HTMLSelectElement extends HTMLElement {
    length: number;
}
"#,
    );

    let ts2430 = diagnostics
        .iter()
        .filter(|diag| diag.code == diagnostic_codes::INTERFACE_INCORRECTLY_EXTENDS_INTERFACE)
        .collect::<Vec<_>>();

    assert!(
        ts2430
            .iter()
            .any(|diag| diag.message_text.contains("HTMLFormElement")),
        "expected HTMLFormElement TS2430 from incompatible numeric index, got: {diagnostics:?}"
    );
    assert!(
        !ts2430
            .iter()
            .any(|diag| diag.message_text.contains("HTMLSelectElement")),
        "did not expect HTMLSelectElement TS2430 without its own numeric index, got: {diagnostics:?}"
    );
}

#[test]
fn tag_name_indexed_access_base_constraint_satisfies_element_constraints() {
    let diagnostics = check_source_strict(
        r#"
interface Node {
    nodeType: number;
}

interface Element extends Node {
    tagName: string;
}

interface HTMLElement extends Element {
    [index: number]: HTMLElement;
}

interface HTMLElementTagNameMap {
    div: HTMLElement;
}

interface HTMLElementDeprecatedTagNameMap {
    acronym: HTMLElement;
    applet: HTMLUnknownElement;
}

interface ElementTagNameMap {
    [index: number]: HTMLElement;
}

interface HTMLUnknownElement extends HTMLElement {
    unknown: string;
}

interface HTMLCollectionOf<T extends Element> {
    item(index: number): T;
}

interface QueryRoot {
    getElementsByTagName<K extends keyof HTMLElementTagNameMap>(
        qualifiedName: K
    ): HTMLCollectionOf<HTMLElementTagNameMap[K]>;
    getElementsByDeprecatedTagName<K extends keyof HTMLElementDeprecatedTagNameMap>(
        qualifiedName: K
    ): HTMLCollectionOf<HTMLElementDeprecatedTagNameMap[K]>;
}

function assertNodeTagName<
    T extends keyof ElementTagNameMap,
    U extends ElementTagNameMap[T]
>(node: Node | null, tagName: T): node is U {
    return node !== null && tagName !== undefined;
}
"#,
    );

    assert!(
        !diagnostics
            .iter()
            .any(|diag| diag.code == diagnostic_codes::TYPE_DOES_NOT_SATISFY_THE_CONSTRAINT),
        "did not expect TS2344 for HTMLElementTagNameMap[K] satisfying Element, got: {diagnostics:?}"
    );
    assert!(
        !diagnostics.iter().any(|diag| diag.code
            == diagnostic_codes::A_TYPE_PREDICATES_TYPE_MUST_BE_ASSIGNABLE_TO_ITS_PARAMETERS_TYPE),
        "did not expect TS2677 for ElementTagNameMap[T] predicate, got: {diagnostics:?}"
    );
}

#[test]
fn renamed_indexed_access_base_constraint_satisfies_element_constraints() {
    let diagnostics = check_source_strict(
        r#"
interface Node {
    nodeType: number;
}

interface Element extends Node {
    tagName: string;
}

interface HTMLElement extends Element {
    [index: number]: HTMLElement;
}

interface LegacyTagMap {
    acronym: HTMLElement;
    applet: HTMLUnknownElement;
}

interface HTMLUnknownElement extends HTMLElement {
    unknown: string;
}

interface HTMLCollectionOf<T extends Element> {
    item(index: number): T;
}

interface QueryRoot {
    getElementsByLegacyTagName<K extends keyof LegacyTagMap>(
        qualifiedName: K
    ): HTMLCollectionOf<LegacyTagMap[K]>;
}
"#,
    );

    assert!(
        !diagnostics
            .iter()
            .any(|diag| diag.code == diagnostic_codes::TYPE_DOES_NOT_SATISFY_THE_CONSTRAINT),
        "did not expect TS2344 for renamed tag map indexed access satisfying Element, got: {diagnostics:?}"
    );
}
