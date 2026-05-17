//! Regression coverage for `recursiveIntersectionTypes.ts`.
//!
//! Recursive intersection aliases such as `T & { next: List<T> }` must expose
//! both the current element properties and recursively linked element
//! properties. Assignability still follows the element shape: a richer element
//! list is assignable to a base element list, but not the reverse.
//!
//! Message-display rule: when a generic Application alias (`LinkedList<Entity>`)
//! evaluates to a non-primitive intersection, TS2322 must show the alias
//! application form, not the expanded structural form.

use tsz_checker::diagnostics::Diagnostic;
use tsz_checker::test_utils::check_source_diagnostics;

fn diagnostics(source: &str) -> Vec<Diagnostic> {
    check_source_diagnostics(source)
}

fn code_count(diagnostics: &[Diagnostic], code: u32) -> usize {
    diagnostics.iter().filter(|diag| diag.code == code).count()
}

fn assert_only_reverse_assignment_ts2322(
    diagnostics: &[Diagnostic],
    forward_start: u32,
    reverse_start: u32,
) {
    let ts2322_starts = diagnostics
        .iter()
        .filter(|diag| diag.code == 2322)
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
        code_count(&diags, 2339),
        0,
        "recursive intersection links must expose current and nested properties, got: {diags:?}"
    );
    assert_eq!(
        code_count(&diags, 2454),
        5,
        "upstream baseline expects use-before-assigned diagnostics for the four entity reads and product assignment, got: {diags:?}"
    );
    assert_only_reverse_assignment_ts2322(&diags, forward_assignment, reverse_assignment);
}

#[test]
fn ts2322_message_shows_alias_application_form_not_structural_expansion() {
    // Structural rule: when a generic Application alias (LinkedList<Entity>) evaluates
    // to a non-primitive intersection, TS2322 message must show the alias application
    // form, not the expanded structural form (Entity & { next: LinkedList<Entity>; }).
    let source = r#"
type LinkedList<T> = T & { next: LinkedList<T> };
interface Entity { name: string; }
interface Product extends Entity { price: number; }
declare var entityList: LinkedList<Entity>;
declare var productList: LinkedList<Product>;
productList = entityList;
"#;
    let diags = check_source_diagnostics(source);
    let ts2322 = diags.iter().find(|d| d.code == 2322);
    assert!(ts2322.is_some(), "expected TS2322, got: {diags:?}");
    let msg = &ts2322.unwrap().message_text;
    assert!(
        msg.contains("LinkedList<Entity>"),
        "TS2322 source should show alias application form 'LinkedList<Entity>', got: {msg}"
    );
    assert!(
        !msg.contains("& {"),
        "TS2322 source must not show the expanded structural form, got: {msg}"
    );
}

#[test]
fn ts2322_message_shows_alias_application_form_renamed_type_param() {
    // Same rule with a different alias name and type-param spelling, proving the fix
    // is structural and not keyed on the specific names "LinkedList" or "T".
    let source = r#"
type Chain<Item> = Item & { tail: Chain<Item> };
interface Base { id: number; }
interface Extended extends Base { label: string; }
declare var b: Chain<Base>;
declare var e: Chain<Extended>;
b = e;
e = b;
"#;
    let diags = check_source_diagnostics(source);
    let ts2322 = diags
        .iter()
        .find(|d| d.code == 2322 && d.message_text.contains("Chain<Base>"));
    assert!(
        ts2322.is_some(),
        "expected TS2322 with Chain<Base> source, got: {diags:?}"
    );
    let msg = &ts2322.unwrap().message_text;
    assert!(
        msg.contains("Chain<Base>"),
        "source should show 'Chain<Base>', got: {msg}"
    );
    assert!(
        !msg.contains("& {"),
        "source must not show the expanded structural form, got: {msg}"
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
        code_count(&diags, 2339),
        0,
        "renamed recursive intersection link must expose nested base properties, got: {diags:?}"
    );
    assert_only_reverse_assignment_ts2322(&diags, forward_assignment, reverse_assignment);
}
