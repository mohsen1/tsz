//! Regression tests for the head message an unmatched-property failure takes
//! when a base **class** names one side of the missing-property line.
//!
//! Structural rule (every row below oracled against `typescript@7.0.2`, the
//! conformance pin): the missing-property line names the base class that
//! contributes the relevant shape — the class that DECLARES the property on
//! the target side, the base class a member-less source inherits its whole
//! surface from on the source side. When either substitution fires, `tsc`
//! keeps a top-level `TS2322` naming the relation's own endpoints and nests
//! the missing-property line beneath it; the standalone `TS2741` survives only
//! when both named types ARE the endpoints.
//!
//! The discriminator is base-**class** heritage, not visibility and not
//! "the target is a class": `interface T extends B {}` over a base interface
//! keeps `TS2741`, a `private`/`#private` member declared directly on the
//! target keeps `TS2741`, and an interface extending a *class* substitutes.
//!
//! tsz decides this in `error_reporter::render_failure_missing_property`, via
//! `render_failure_missing_property_base_class`, at both `TS2741` construction
//! sites (the singular renderer and the brand-filtered plural renderer).

use crate::diagnostics::Diagnostic;
use crate::test_utils::check_source_diagnostics;
use tsz_common::diagnostics::diagnostic_codes;

const TS2322: u32 = diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE;
const TS2741: u32 = diagnostic_codes::PROPERTY_IS_MISSING_IN_TYPE_BUT_REQUIRED_IN_TYPE;

/// The assignability diagnostic's `(code, message)` plus the messages of any
/// nested elaboration lines, which is exactly what the head rule decides.
fn assignability_shape(source: &str) -> (u32, String, Vec<String>) {
    let diagnostics = check_source_diagnostics(source);
    let matching: Vec<&Diagnostic> = diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.code == TS2322 || diagnostic.code == TS2741)
        .collect();
    assert_eq!(
        matching.len(),
        1,
        "expected exactly one assignability diagnostic; got {:?}",
        diagnostics
            .iter()
            .map(|diagnostic| (diagnostic.code, diagnostic.message_text.clone()))
            .collect::<Vec<_>>()
    );
    let diagnostic = matching[0];
    let nested = diagnostic
        .related_information
        .iter()
        .filter(|info| info.code == TS2741)
        .map(|info| info.message_text.clone())
        .collect();
    (diagnostic.code, diagnostic.message_text.clone(), nested)
}

fn assert_nested_ts2741(source: &str, head: &str, nested: &str) {
    let (code, message, elaborations) = assignability_shape(source);
    assert_eq!(code, TS2322, "head code for:\n{source}");
    assert_eq!(message, head, "head message for:\n{source}");
    assert_eq!(
        elaborations,
        vec![nested.to_string()],
        "nested line for:\n{source}"
    );
}

fn assert_standalone_ts2741(source: &str, message_text: &str) {
    let (code, message, elaborations) = assignability_shape(source);
    assert_eq!(code, TS2741, "head code for:\n{source}");
    assert_eq!(message, message_text, "message for:\n{source}");
    assert!(
        elaborations.is_empty(),
        "standalone TS2741 carries no nested missing-property line; got {elaborations:?}"
    );
}

// --- target side: the class that declares the missing property ------------

#[test]
fn public_member_inherited_from_a_base_class_takes_the_ts2322_head() {
    assert_nested_ts2741(
        "class Base { x: number = 1 }\nclass Derived extends Base {}\ndeclare const s: {};\nconst t: Derived = s;\n",
        "Type '{}' is not assignable to type 'Derived'.",
        "Property 'x' is missing in type '{}' but required in type 'Base'.",
    );
}

/// Visibility is not the discriminator: the same shape with a `private` member
/// takes the same head. Binder names deliberately differ from the public case.
#[test]
fn private_member_inherited_from_a_base_class_takes_the_ts2322_head() {
    assert_nested_ts2741(
        "class Root { private secret = 1 }\nclass Leaf extends Root {}\ndeclare const value: {};\nconst held: Leaf = value;\n",
        "Type '{}' is not assignable to type 'Leaf'.",
        "Property 'secret' is missing in type '{}' but required in type 'Root'.",
    );
}

/// The name is the DECLARING class, not the immediate base.
#[test]
fn a_two_level_class_chain_names_the_declaring_class_not_the_immediate_base() {
    assert_nested_ts2741(
        "class Grand { g: number = 1 }\nclass Mid extends Grand {}\nclass Tip extends Mid {}\ndeclare const s: {};\nconst t: Tip = s;\n",
        "Type '{}' is not assignable to type 'Tip'.",
        "Property 'g' is missing in type '{}' but required in type 'Grand'.",
    );
}

/// An *interface* target over a base class substitutes too — the rule keys on
/// the base being a class, not on the target being one.
#[test]
fn an_interface_extending_a_class_takes_the_ts2322_head() {
    assert_nested_ts2741(
        "class Holder { p: number = 1 }\ninterface Shape extends Holder {}\ndeclare const s: {};\nconst t: Shape = s;\n",
        "Type '{}' is not assignable to type 'Shape'.",
        "Property 'p' is missing in type '{}' but required in type 'Holder'.",
    );
}

/// A fresh object literal source reaches the same head.
#[test]
fn a_fresh_object_literal_source_takes_the_same_head() {
    assert_nested_ts2741(
        "class Owner { p: number = 1 }\nclass Sub extends Owner {}\nconst t: Sub = {};\n",
        "Type '{}' is not assignable to type 'Sub'.",
        "Property 'p' is missing in type '{}' but required in type 'Owner'.",
    );
}

// --- source side: the base class a member-less source inherits from --------

#[test]
fn a_member_less_interface_source_is_named_by_its_base_class() {
    assert_nested_ts2741(
        "class Anchor {}\ninterface Carrier extends Anchor {}\ndeclare const s: Carrier;\ninterface Want { q: number }\nconst t: Want = s;\n",
        "Type 'Carrier' is not assignable to type 'Want'.",
        "Property 'q' is missing in type 'Anchor' but required in type 'Want'.",
    );
}

#[test]
fn a_member_less_class_source_is_named_by_its_base_class() {
    assert_nested_ts2741(
        "class Origin {}\nclass Passthrough extends Origin {}\ndeclare const s: Passthrough;\ninterface Need { q: number }\nconst t: Need = s;\n",
        "Type 'Passthrough' is not assignable to type 'Need'.",
        "Property 'q' is missing in type 'Origin' but required in type 'Need'.",
    );
}

/// Both sides can substitute independently in one message.
#[test]
fn a_private_name_target_and_a_member_less_source_substitute_on_both_readings() {
    assert_nested_ts2741(
        "class Ancestor {}\ninterface Bearer extends Ancestor {}\ndeclare const s: Bearer;\nclass Guard { #tag = 1 }\nconst t: Guard = s;\n",
        "Type 'Bearer' is not assignable to type 'Guard'.",
        "Property '#tag' is missing in type 'Ancestor' but required in type 'Guard'.",
    );
}

// --- negative controls: the standalone TS2741 must survive -----------------

/// Base **interface** heritage is flattened into the endpoint's own name.
#[test]
fn a_member_inherited_from_a_base_interface_keeps_the_standalone_ts2741() {
    assert_standalone_ts2741(
        "interface BaseShape { x: number }\ninterface Wanted extends BaseShape {}\ndeclare const s: {};\nconst t: Wanted = s;\n",
        "Property 'x' is missing in type '{}' but required in type 'Wanted'.",
    );
}

#[test]
fn a_member_declared_on_the_target_itself_keeps_the_standalone_ts2741() {
    assert_standalone_ts2741(
        "interface Direct { x: number }\ndeclare const s: {};\nconst t: Direct = s;\n",
        "Property 'x' is missing in type '{}' but required in type 'Direct'.",
    );
}

/// A `private` member declared directly on the target keeps `TS2741` — the
/// pair to `private_member_inherited_from_a_base_class_takes_the_ts2322_head`.
#[test]
fn a_private_member_declared_on_the_target_itself_keeps_the_standalone_ts2741() {
    assert_standalone_ts2741(
        "class Sealed { private x = 1 }\ndeclare const s: {};\nconst t: Sealed = s;\n",
        "Property 'x' is missing in type '{}' but required in type 'Sealed'.",
    );
}

/// A `#private` member declared directly on the target likewise.
#[test]
fn a_private_name_declared_on_the_target_itself_keeps_the_standalone_ts2741() {
    assert_standalone_ts2741(
        "class Branded { #p = 1 }\ndeclare const s: {};\nconst t: Branded = s;\n",
        "Property '#p' is missing in type '{}' but required in type 'Branded'.",
    );
}

/// A source interface that declares only a METHOD of its own is not
/// member-less, so it neither renames to its base class nor promotes the head.
///
/// This is `compiler/interfaceExtendsClassWithPrivate1.ts` reduced to its
/// line 24 (`d = i;`). Reading "declares nothing of its own" off the resolved
/// property set instead of the declaration's member list misses the method,
/// renames the source `I` to `C`, and promotes a correct `TS2741` into a
/// false-positive `TS2322`.
#[test]
fn a_source_interface_declaring_only_a_method_is_named_by_the_endpoint() {
    assert_standalone_ts2741(
        "class Shared { pass(v: number) { return v; } private tag = 1; }\ninterface Widened extends Shared { extra(v: number): number; }\nclass Full extends Shared implements Widened { extra(v: number) { return v; } only() {} }\ndeclare const w: Widened;\ndeclare let f: Full;\nf = w;\n",
        "Property 'only' is missing in type 'Widened' but required in type 'Full'.",
    );
}

/// The same shape one step simpler: an own method alone blocks the source-side
/// substitution, with no shared base and no `implements` involved.
#[test]
fn an_own_method_alone_blocks_the_source_side_substitution() {
    assert_standalone_ts2741(
        "class Rooted {}\ninterface Speaks extends Rooted { talk(): void }\ndeclare const s: Speaks;\ninterface Demands { talk(): void; listen(): void }\nconst t: Demands = s;\n",
        "Property 'listen' is missing in type 'Speaks' but required in type 'Demands'.",
    );
}

/// A source that declares a member of its own is named by the endpoint even
/// when it also has a base class.
#[test]
fn a_source_with_its_own_member_is_named_by_the_endpoint() {
    assert_nested_ts2741(
        "class Under { private x = 1 }\nclass Over extends Under {}\nclass Given { y: number = 1 }\ndeclare const s: Given;\nconst t: Over = s;\n",
        "Type 'Given' is not assignable to type 'Over'.",
        "Property 'x' is missing in type 'Given' but required in type 'Under'.",
    );
}

/// The missing member being the target's OWN keeps `TS2741` even though the
/// target also inherits from a class — the substitution is per-property.
#[test]
fn an_own_missing_member_keeps_ts2741_on_a_target_that_also_has_a_base_class() {
    assert_standalone_ts2741(
        "class Parent { p: number = 1 }\nclass Child extends Parent { q: number = 1 }\ndeclare const s: { p: number };\nconst t: Child = s;\n",
        "Property 'q' is missing in type '{ p: number; }' but required in type 'Child'.",
    );
}
