//! Regression coverage for `recursiveIntersectionTypes.ts`.
//!
//! Recursive intersection aliases such as `T & { next: List<T> }` must expose
//! both the current element properties and recursively linked element
//! properties. Assignability still follows the element shape: a richer element
//! list is assignable to a base element list, but not the reverse.

use tsz_checker::diagnostics::Diagnostic;
use tsz_checker::test_utils::{check_source_diagnostics, diagnostic_count, diagnostics_with_code};

fn diagnostics(source: &str) -> Vec<Diagnostic> {
    check_source_diagnostics(source)
}

fn assert_only_reverse_assignment_ts2322(
    diagnostics: &[Diagnostic],
    forward_start: u32,
    reverse_start: u32,
) {
    let ts2322_starts = diagnostics_with_code(diagnostics, 2322)
        .iter()
        .map(|diag| diag.start)
        .collect::<Vec<_>>();
    assert_eq!(
        ts2322_starts,
        vec![reverse_start],
        "only the reverse assignment from base list to richer list should fail, got: {diagnostics:?}"
    );
    assert!(
        !ts2322_starts.contains(&forward_start),
        "forward assignment from richer list to base list must stay assignable, got: {diagnostics:?}"
    );
}

fn ts2322_message(diagnostics: &[Diagnostic]) -> Option<String> {
    diagnostics_with_code(diagnostics, 2322)
        .first()
        .map(|d| d.message_text.clone())
}

#[test]
fn recursive_intersection_list_preserves_member_access_and_assignment_direction() {
    let source = r#"
type LinkedList<T> = T & { next: LinkedList<T> };

interface Entity {
    name: string;
}

interface Product extends Entity {
    price: number;
}

var entityList: LinkedList<Entity>;
var s0 = entityList.name;
var s1 = entityList.next.name;
var s2 = entityList.next.next.name;
var s3 = entityList.next.next.next.name;

var productList: LinkedList<Product>;
entityList = productList;
productList = entityList;
"#;

    let diags = diagnostics(source);
    let forward_assignment = source.find("entityList = productList").unwrap() as u32;
    let reverse_assignment = source.find("productList = entityList").unwrap() as u32;
    assert_eq!(
        diagnostic_count(&diags, 2339),
        0,
        "recursive intersection links must expose current and nested properties, got: {diags:?}"
    );
    assert_eq!(
        diagnostic_count(&diags, 2454),
        5,
        "upstream baseline expects use-before-assigned diagnostics for the four entity reads and product assignment, got: {diags:?}"
    );
    assert_only_reverse_assignment_ts2322(&diags, forward_assignment, reverse_assignment);
    let msg = ts2322_message(&diags).unwrap_or_default();
    assert!(
        msg.contains("LinkedList<Entity>") && msg.contains("LinkedList<Product>"),
        "TS2322 message must use alias names, not expanded intersections; got: {msg}"
    );
    assert!(
        !msg.contains("& {"),
        "TS2322 message must not show raw intersection form; got: {msg}"
    );
}

#[test]
fn recursive_intersection_renamed_links_preserve_shape_rule() {
    let source = r#"
type Chain<Item> = Item & { child: Chain<Item> };

interface BaseNode {
    label: string;
}

interface DecoratedNode extends BaseNode {
    weight: number;
}

declare let baseChain: Chain<BaseNode>;
let a = baseChain.label;
let b = baseChain.child.label;
let c = baseChain.child.child.label;

declare let decoratedChain: Chain<DecoratedNode>;
baseChain = decoratedChain;
decoratedChain = baseChain;
"#;

    let diags = diagnostics(source);
    let forward_assignment = source.find("baseChain = decoratedChain").unwrap() as u32;
    let reverse_assignment = source.find("decoratedChain = baseChain").unwrap() as u32;
    assert_eq!(
        diagnostic_count(&diags, 2339),
        0,
        "renamed recursive intersection link must expose nested base properties, got: {diags:?}"
    );
    assert_only_reverse_assignment_ts2322(&diags, forward_assignment, reverse_assignment);
    let msg = ts2322_message(&diags).unwrap_or_default();
    assert!(
        msg.contains("Chain<BaseNode>") && msg.contains("Chain<DecoratedNode>"),
        "TS2322 message must use alias names for renamed recursive alias; got: {msg}"
    );
}

#[test]
fn intersection_alias_with_different_type_param_names_uses_alias_name() {
    // Verify the alias-name rule is not tied to a specific type-parameter spelling
    // (e.g. `T` vs `U` vs `Element`).
    let source = r#"
type Node<Element> = Element & { next: Node<Element> };

interface Small { x: number; }
interface Large extends Small { y: string; }

declare let small: Node<Small>;
declare let large: Node<Large>;
small = large;
large = small;
"#;

    let diags = diagnostics(source);
    let reverse_start = source.find("large = small").unwrap() as u32;
    let ts2322 = diagnostics_with_code(&diags, 2322);
    assert_eq!(
        ts2322.len(),
        1,
        "only the reverse assignment should fail; got: {diags:?}"
    );
    assert_eq!(ts2322[0].start, reverse_start);
    let msg = &ts2322[0].message_text;
    assert!(
        msg.contains("Node<Small>") && msg.contains("Node<Large>"),
        "alias name must not depend on type-parameter spelling; got: {msg}"
    );
}

#[test]
fn recursive_intersection_var_no_accesses_message_check() {
    // Exact original conformance test source
    let source = r#"
type LinkedList<T> = T & { next: LinkedList<T> };

interface Entity {
    name: string;
}

interface Product extends Entity {
    price: number;
}

var entityList: LinkedList<Entity>;
var s0 = entityList.name;
var s1 = entityList.next.name;
var s2 = entityList.next.next.name;
var s3 = entityList.next.next.next.name;

var productList: LinkedList<Product>;
entityList = productList;
productList = entityList;
"#;
    let diags = diagnostics(source);
    let ts2322 = diagnostics_with_code(&diags, 2322);
    assert_eq!(
        ts2322.len(),
        1,
        "should have exactly one TS2322; got: {diags:?}"
    );
    let msg = &ts2322[0].message_text;
    assert!(
        msg.contains("LinkedList<Entity>") && msg.contains("LinkedList<Product>"),
        "message must use alias names; got: {msg}"
    );
}

#[test]
fn structural_intersection_index_signature_keeps_structural_display() {
    let source = r#"
let x: { [x: string]: { a: 0 } } & { [x: string]: { b: 0 } };

x = { y: { a: 0 } };
x = { y: { a: 0, b: 0 } };
x = { y: { a: 0, b: 0, c: 0 } };
"#;
    let diags = diagnostics(source);
    let ts2322 = diagnostics_with_code(&diags, 2322);
    let ts2353 = diagnostics_with_code(&diags, 2353);
    assert_eq!(ts2322.len(), 1, "expected one TS2322, got: {diags:?}");
    assert_eq!(ts2353.len(), 1, "expected one TS2353, got: {diags:?}");
    assert!(
        ts2322[0].message_text.contains("'{ a: 0; } & { b: 0; }'"),
        "plain structural intersections should keep structural display, got: {}",
        ts2322[0].message_text
    );
    assert!(
        ts2353[0].message_text.contains("'{ a: 0; } & { b: 0; }'"),
        "excess-property diagnostic should keep structural display, got: {}",
        ts2353[0].message_text
    );
}
