//! Object-rest assignment-target lowering for a `super` property inside a
//! scoped static-super context (issue #11756).
//!
//! Structural rule: when a `super.<prop>` / `super["<prop>"]` access appears in
//! the assignment-TARGET position of an object-rest destructuring assignment
//! (`{ ...super.x } = ...`) inside a scoped static-super context that requires
//! the `Reflect.set`/`Reflect.get` rewrite, tsc lowers the target to the
//! setter-descriptor form `({ set value(_a) { Reflect.set(base, "x", _a, recv);
//! } }).value`. Before this fix tsz emitted the read form `Reflect.get(base,
//! "x", recv)` in LHS position, producing a call-expression-as-assignment-target
//! (`Reflect.get(...) = __rest(...)`) — a runtime `SyntaxError` (non-runnable
//! JavaScript).
//!
//! These tests vary the class, base, member, and rest property names so the
//! behaviour is keyed on the structural "super property in object-rest target"
//! shape, not on any particular identifier spelling. The negative case proves a
//! non-super object-rest target is unaffected.

use tsz_common::common::ScriptTarget;
use tsz_emitter::output::printer::PrintOptions;

#[path = "test_support.rs"]
mod test_support;

use test_support::parse_and_lower_print;

fn lower_es2015(source: &str) -> String {
    let opts = PrintOptions {
        target: ScriptTarget::ES2015,
        ..Default::default()
    };
    parse_and_lower_print(source, opts)
}

/// The reported repro shape: `{ ...super.a } = { x: 0 }` in a static field of a
/// derived class. The `super.a` rest target must lower to the setter-descriptor
/// form, never to a `Reflect.get(...)` left-hand side.
#[test]
fn object_rest_super_property_target_lowers_to_setter_descriptor() {
    let source = concat!(
        "declare class B { static a: any; }\n",
        "class C extends B {\n",
        "    static x: any = undefined!;\n",
        "    static z13 = { ...super.a } = { x: 0 };\n",
        "}\n",
    );
    let output = lower_es2015(source);
    assert!(
        output.contains("set value(") && output.contains(".value = "),
        "object-rest super target should lower to the setter-descriptor form.\nOutput:\n{output}"
    );
    // The crux of the correctness fix: no call expression in LHS position.
    assert!(
        !output.contains("Reflect.get(") || !reflect_get_used_as_lhs(&output),
        "object-rest super target must not emit `Reflect.get(...) = ...` (invalid LHS).\nOutput:\n{output}"
    );
    assert!(
        output.contains("__rest("),
        "object-rest lowering should still call the `__rest` helper.\nOutput:\n{output}"
    );
}

/// Same rule with different class/base/member/property names, proving the fix is
/// keyed on the structural shape rather than the `C`/`B`/`a` spelling. Uses an
/// element access (`super["title"]`) as the rest target.
#[test]
fn object_rest_super_element_target_lowers_for_other_names() {
    let source = concat!(
        "declare class Widget { static title: any; }\n",
        "class Panel extends Widget {\n",
        "    static seed: any = undefined!;\n",
        "    static leftover = { ...super[\"title\"] } = { label: 1 };\n",
        "}\n",
    );
    let output = lower_es2015(source);
    assert!(
        output.contains("set value(") && output.contains(".value = "),
        "renamed object-rest super element target should still lower to the setter-descriptor form.\nOutput:\n{output}"
    );
    assert!(
        !reflect_get_used_as_lhs(&output),
        "renamed object-rest super target must not produce a `Reflect.get(...)` assignment LHS.\nOutput:\n{output}"
    );
}

/// A `super.prop` rest target wrapped through a property-access spelling with a
/// distinct member name confirms the property-access path also routes through the
/// setter-descriptor emitter.
#[test]
fn object_rest_super_property_target_property_access_other_member() {
    let source = concat!(
        "declare class Store { static cache: any; }\n",
        "class Shop extends Store {\n",
        "    static init: any = undefined!;\n",
        "    static rest = { ...super.cache } = { hit: 0 };\n",
        "}\n",
    );
    let output = lower_es2015(source);
    assert!(
        output.contains("set value(") && output.contains(".value = "),
        "property-access super rest target should lower to the setter-descriptor form.\nOutput:\n{output}"
    );
    assert!(
        !reflect_get_used_as_lhs(&output),
        "property-access super rest target must not produce a `Reflect.get(...)` assignment LHS.\nOutput:\n{output}"
    );
}

/// Negative / regression case: an object-rest assignment whose rest target is a
/// plain identifier (not a `super` property) is unaffected — it keeps the normal
/// `target = __rest(...)` lowering and never grows a setter descriptor.
#[test]
fn object_rest_plain_identifier_target_is_unaffected() {
    let source = concat!(
        "declare class Base { static a: any; }\n",
        "class Derived extends Base {\n",
        "    static x: any = undefined!;\n",
        "    static plain = (() => { let rest; ({ ...rest } = { y: 0 }); return rest; })();\n",
        "}\n",
    );
    let output = lower_es2015(source);
    assert!(
        output.contains("__rest("),
        "plain identifier object-rest should still call `__rest`.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("set value("),
        "plain identifier object-rest target must not grow a setter descriptor.\nOutput:\n{output}"
    );
}

/// Returns true if any `Reflect.get(` call appears immediately on the left-hand
/// side of an assignment (`Reflect.get(...) = `), which is the invalid
/// call-expression-as-LHS shape this fix removes. Scans for the closing-paren
/// followed by ` = ` after a `Reflect.get(` opener at the same nesting depth.
fn reflect_get_used_as_lhs(output: &str) -> bool {
    let bytes = output.as_bytes();
    let needle = b"Reflect.get(";
    let mut search_from = 0;
    while let Some(rel) = find_subslice(&bytes[search_from..], needle) {
        let open = search_from + rel + needle.len();
        // Walk to the matching close paren of this `Reflect.get(` call.
        let mut depth = 1usize;
        let mut i = open;
        while i < bytes.len() && depth > 0 {
            match bytes[i] {
                b'(' => depth += 1,
                b')' => depth -= 1,
                _ => {}
            }
            i += 1;
        }
        // After the matching `)`, an immediate ` = ` (not `==`/`=>`) means the
        // call expression is being used as an assignment target.
        let rest = &output[i..];
        let trimmed = rest.trim_start();
        if let Some(after_eq) = trimmed.strip_prefix("= ")
            && !after_eq.starts_with('=')
        {
            return true;
        }
        search_from = open;
    }
    false
}

fn find_subslice(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || haystack.len() < needle.len() {
        return None;
    }
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}
