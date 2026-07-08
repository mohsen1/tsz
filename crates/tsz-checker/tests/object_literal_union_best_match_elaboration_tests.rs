//! Union-target object-literal elaboration: the best-match drill-in gate and
//! the union per-property (`checkTypes`) chain (issue #15403).
//!
//! Structural rule: when a fresh object literal fails against a union target
//! that contains an array-like member and the union's best-matching member
//! (tsc `findBestTypeForObjectLiteral`: the first non-array-like member in
//! relation order) does not carry a failing property, tsc does NOT drill into
//! the property. It reports the outer TS2322/TS2345/TS1360 at the assignment
//! anchor with the `hasExcessProperties` checkTypes chain: `Types of property
//! 'x' are incompatible.` against the union of per-member property-or-index
//! types (members lacking the property contribute `undefined`). Unions without
//! an array-like member keep drilling in (control), and a drilled
//! function-valued property whose return type fits the target anchors at the
//! VALUE expression with a `Did you mean to call this expression?` hint
//! (tsc `elaborateDidYouMeanToCallOrConstruct`).

use tsz_checker::diagnostics::Diagnostic;
use tsz_checker::test_utils::check_source_diagnostics;

fn diagnostics(source: &str) -> Vec<Diagnostic> {
    check_source_diagnostics(source)
}

fn offset_of(source: &str, needle: &str) -> u32 {
    u32::try_from(source.find(needle).expect("needle present")).expect("offset fits")
}

/// The single diagnostic with `code`, panicking (with context) otherwise.
fn sole_diagnostic(diags: &[Diagnostic], code: u32) -> &Diagnostic {
    let mut matching = diags.iter().filter(|diag| diag.code == code);
    let first = matching
        .next()
        .unwrap_or_else(|| panic!("expected a TS{code}; got {diags:?}"));
    assert!(
        matching.next().is_none(),
        "expected exactly one TS{code}; got {diags:?}"
    );
    first
}

fn chain_messages(diag: &Diagnostic) -> Vec<String> {
    diag.related_information
        .iter()
        .map(|info| info.message_text.clone())
        .collect()
}

const JSONISH: &str =
    "type Wire = string | number | boolean | null | Wire[] | { [slot: string]: Wire };\n";

#[test]
fn var_init_reports_outer_with_union_property_chain() {
    let source = format!("{JSONISH}const payload: Wire = {{ fetch: () => 1 }};\n");
    let diags = diagnostics(&source);
    let diag = sole_diagnostic(&diags, 2322);
    assert_eq!(
        diag.start,
        offset_of(&source, "payload"),
        "outer TS2322 anchors at the declaration name; got {diags:?}"
    );
    assert_eq!(
        diag.message_text,
        "Type '{ fetch: () => number; }' is not assignable to type 'Wire'."
    );
    assert_eq!(
        chain_messages(diag),
        vec![
            "Types of property 'fetch' are incompatible.".to_string(),
            "Type '() => number' is not assignable to type 'Wire | undefined'.".to_string(),
        ],
        "got {diags:?}"
    );
}

#[test]
fn nested_literal_recurses_through_the_check_types_chain() {
    let source = format!("{JSONISH}let sink: Wire;\nsink = {{ outer: {{ inner: () => 1 }} }};\n");
    let diags = diagnostics(&source);
    let diag = sole_diagnostic(&diags, 2322);
    assert_eq!(diag.start, offset_of(&source, "sink ="));
    assert_eq!(
        chain_messages(diag),
        vec![
            "Types of property 'outer' are incompatible.".to_string(),
            "Type '{ inner: () => number; }' is not assignable to type 'Wire | undefined'."
                .to_string(),
            "Types of property 'inner' are incompatible.".to_string(),
            "Type '() => number' is not assignable to type 'Wire | undefined'.".to_string(),
        ],
        "got {diags:?}"
    );
}

#[test]
fn call_argument_reports_ts2345_with_full_chain() {
    let source = format!(
        "{JSONISH}declare function send(w: Wire): void;\nsend({{ outer: {{ inner: () => 1 }} }});\n"
    );
    let diags = diagnostics(&source);
    let diag = sole_diagnostic(&diags, 2345);
    assert_eq!(
        chain_messages(diag),
        vec![
            "Types of property 'outer' are incompatible.".to_string(),
            "Type '{ inner: () => number; }' is not assignable to type 'Wire | undefined'."
                .to_string(),
            "Types of property 'inner' are incompatible.".to_string(),
            "Type '() => number' is not assignable to type 'Wire | undefined'.".to_string(),
        ],
        "got {diags:?}"
    );
}

#[test]
fn return_statement_reports_outer_with_chain() {
    let source = format!("{JSONISH}function make(): Wire {{\n  return {{ leaf: () => 1 }};\n}}\n");
    let diags = diagnostics(&source);
    let diag = sole_diagnostic(&diags, 2322);
    // tsc anchors a failing return statement at the `return` keyword.
    assert_eq!(diag.start, offset_of(&source, "return {"));
    assert_eq!(
        chain_messages(diag),
        vec![
            "Types of property 'leaf' are incompatible.".to_string(),
            "Type '() => number' is not assignable to type 'Wire | undefined'.".to_string(),
        ],
        "got {diags:?}"
    );
}

#[test]
fn satisfies_reports_ts1360_with_chain() {
    let source = format!("{JSONISH}const checked = {{ leaf: () => 1 }} satisfies Wire;\n");
    let diags = diagnostics(&source);
    let diag = sole_diagnostic(&diags, 1360);
    assert_eq!(
        chain_messages(diag),
        vec![
            "Types of property 'leaf' are incompatible.".to_string(),
            "Type '() => number' is not assignable to type 'Wire | undefined'.".to_string(),
        ],
        "got {diags:?}"
    );
}

#[test]
fn index_member_chain_reduces_two_member_union_target() {
    // Per-property union `Rec | undefined` reduces to `Rec` for a
    // definitely-non-nullable source (tsc `isRelatedTo` nullable reduction).
    let source = "interface Rec { tag: string }\n\
         type Bag = string | Rec[] | { [key: string]: Rec };\n\
         const wrong: Bag = { item: true };\n";
    let diags = diagnostics(source);
    let diag = sole_diagnostic(&diags, 2322);
    assert_eq!(
        chain_messages(diag),
        vec![
            "Types of property 'item' are incompatible.".to_string(),
            "Type 'boolean' is not assignable to type 'Rec'.".to_string(),
        ],
        "got {diags:?}"
    );
}

#[test]
fn nullish_source_keeps_the_undefined_arm() {
    let source = "interface Rec { tag: string }\n\
         type Bag = string | Rec[] | { [key: string]: Rec };\n\
         const wrong: Bag = { item: null };\n";
    let diags = diagnostics(source);
    let diag = sole_diagnostic(&diags, 2322);
    assert_eq!(
        chain_messages(diag),
        vec![
            "Types of property 'item' are incompatible.".to_string(),
            "Type 'null' is not assignable to type 'Rec | undefined'.".to_string(),
        ],
        "got {diags:?}"
    );
}

#[test]
fn named_property_member_contributes_its_declared_type() {
    let source = "interface Rec { tag: string }\n\
         type Bag = string | Rec[] | { item: number };\n\
         const wrong: Bag = { item: true };\n";
    let diags = diagnostics(source);
    let diag = sole_diagnostic(&diags, 2322);
    assert_eq!(
        chain_messages(diag),
        vec![
            "Types of property 'item' are incompatible.".to_string(),
            "Type 'boolean' is not assignable to type 'number'.".to_string(),
        ],
        "got {diags:?}"
    );
}

#[test]
fn three_member_property_union_with_two_nullish_arms_reduces() {
    let source = "interface Rec { tag: string }\n\
         type Bag = string | Rec[] | { [key: string]: Rec | null };\n\
         const wrong: Bag = { item: true };\n";
    let diags = diagnostics(source);
    let diag = sole_diagnostic(&diags, 2322);
    assert_eq!(
        chain_messages(diag),
        vec![
            "Types of property 'item' are incompatible.".to_string(),
            "Type 'boolean' is not assignable to type 'Rec'.".to_string(),
        ],
        "got {diags:?}"
    );
}

#[test]
fn all_properties_passing_elaborates_against_best_member() {
    // `undefined` satisfies the per-property union, so the fallback elaborates
    // against the best member — the first non-array-like member (`string`).
    let source = "interface Rec { tag: string }\n\
         type Bag = string | Rec[] | { [key: string]: Rec };\n\
         const wrong: Bag = { item: undefined };\n";
    let diags = diagnostics(source);
    let diag = sole_diagnostic(&diags, 2322);
    assert_eq!(
        chain_messages(diag),
        vec!["Type '{ item: undefined; }' is not assignable to type 'string'.".to_string()],
        "got {diags:?}"
    );
}

#[test]
fn nullish_union_member_is_the_best_match_when_it_sorts_first() {
    let source = "interface Rec { tag: string }\n\
         type Bag = null | Rec[] | { [key: string]: Rec };\n\
         const wrong: Bag = { item: undefined };\n";
    let diags = diagnostics(source);
    let diag = sole_diagnostic(&diags, 2322);
    assert_eq!(
        chain_messages(diag),
        vec!["Type '{ item: undefined; }' is not assignable to type 'null'.".to_string()],
        "got {diags:?}"
    );
}

#[test]
fn declaration_order_of_union_members_does_not_change_the_best_match() {
    // Written with the index member first: tsc's relation order still picks
    // the primitive member as the best match.
    let source = "interface Rec { tag: string }\n\
         type Bag = { [key: string]: Rec } | string | Rec[];\n\
         const wrong: Bag = { item: undefined };\n";
    let diags = diagnostics(source);
    let diag = sole_diagnostic(&diags, 2322);
    assert_eq!(
        chain_messages(diag),
        vec!["Type '{ item: undefined; }' is not assignable to type 'string'.".to_string()],
        "got {diags:?}"
    );
}

#[test]
fn nested_excess_property_wins_and_anchors_at_the_nested_property() {
    let source = "interface Rec { tag: string }\n\
         type Bag = string | Rec[] | { [key: string]: Rec };\n\
         const wrong: Bag = { item: { bogus: 1 } };\n";
    let diags = diagnostics(source);
    let diag = sole_diagnostic(&diags, 2322);
    assert_eq!(
        diag.start,
        offset_of(source, "bogus"),
        "chain terminating in a nested excess property anchors at that property; got {diags:?}"
    );
    assert_eq!(
        chain_messages(diag),
        vec![
            "Types of property 'item' are incompatible.".to_string(),
            "Object literal may only specify known properties, and 'bogus' does not exist in type 'Rec'.".to_string(),
        ],
        "got {diags:?}"
    );
}

#[test]
fn nested_excess_property_outranks_a_sibling_property_mismatch() {
    let source = "interface Rec { tag: string }\n\
         type Bag = string | Rec[] | { [key: string]: Rec };\n\
         const wrong: Bag = { item: { tag: 1, bogus: 2 } };\n";
    let diags = diagnostics(source);
    let diag = sole_diagnostic(&diags, 2322);
    assert_eq!(diag.start, offset_of(source, "bogus"), "got {diags:?}");
    assert_eq!(
        chain_messages(diag),
        vec![
            "Types of property 'item' are incompatible.".to_string(),
            "Object literal may only specify known properties, and 'bogus' does not exist in type 'Rec'.".to_string(),
        ],
        "got {diags:?}"
    );
}

#[test]
fn union_without_array_like_member_still_drills_into_the_property() {
    let source = "interface Rec { tag: string }\n\
         type Bag = number | { [key: string]: Rec };\n\
         const wrong: Bag = { item: true };\n";
    let diags = diagnostics(source);
    let diag = sole_diagnostic(&diags, 2322);
    assert_eq!(
        diag.start,
        offset_of(source, "item: true"),
        "no array-like member: keep the inner property drill; got {diags:?}"
    );
    assert_eq!(
        diag.message_text,
        "Type 'boolean' is not assignable to type 'Rec'."
    );
}

#[test]
fn array_member_union_whose_object_member_has_the_property_drills() {
    let source = "type Pair = number[] | { first: number };\n\
         const wrong: Pair = { first: \"s\" };\n";
    let diags = diagnostics(source);
    let diag = sole_diagnostic(&diags, 2322);
    assert_eq!(
        diag.start,
        offset_of(source, "first: \"s\""),
        "best member carries the property: keep the drill; got {diags:?}"
    );
    assert_eq!(
        diag.message_text,
        "Type 'string' is not assignable to type 'number'."
    );
}

#[test]
fn tuple_and_readonly_array_members_count_as_array_like() {
    for union in [
        "[number] | { first: number }",
        "readonly string[] | { first: number }",
    ] {
        let source = format!("type Pair = {union};\nconst wrong: Pair = {{ first: \"s\" }};\n");
        let diags = diagnostics(&source);
        let diag = sole_diagnostic(&diags, 2322);
        assert_eq!(
            diag.start,
            offset_of(&source, "first: \"s\""),
            "best member `{{ first: number }}` still drills for `{union}`; got {diags:?}"
        );
    }
}

#[test]
fn did_you_mean_to_call_anchors_at_the_value_expression() {
    let source = "const holder: { count: number } = { count: () => 1 };\n";
    let diags = diagnostics(source);
    let diag = sole_diagnostic(&diags, 2322);
    assert_eq!(
        diag.start,
        offset_of(source, "() => 1"),
        "function-valued property with a fitting return anchors at the value; got {diags:?}"
    );
    assert!(
        diag.related_information
            .iter()
            .any(|info| info.message_text == "Did you mean to call this expression?"),
        "expected the call hint; got {diags:?}"
    );
}

#[test]
fn did_you_mean_to_call_applies_to_identifier_values() {
    let source = "declare function supply(): number;\n\
         const holder: { count: number } = { count: supply };\n";
    let diags = diagnostics(source);
    let diag = sole_diagnostic(&diags, 2322);
    assert_eq!(diag.start, offset_of(source, "supply }"), "got {diags:?}");
    assert!(
        diag.related_information
            .iter()
            .any(|info| info.message_text == "Did you mean to call this expression?"),
        "expected the call hint; got {diags:?}"
    );
}

#[test]
fn mismatching_return_type_keeps_the_property_name_anchor() {
    let source = "const holder: { count: number } = { count: () => \"s\" };\n";
    let diags = diagnostics(source);
    let diag = sole_diagnostic(&diags, 2322);
    assert_eq!(
        diag.start,
        offset_of(source, "count: () => \"s\""),
        "return type does not fit: anchor stays at the property name; got {diags:?}"
    );
    assert!(
        diag.related_information
            .iter()
            .all(|info| info.message_text != "Did you mean to call this expression?"),
        "no call hint for a non-fitting return; got {diags:?}"
    );
}
