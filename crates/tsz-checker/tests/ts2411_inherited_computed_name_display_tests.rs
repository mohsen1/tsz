//! TS2411 renders an *inherited* property's name the way `tsc` does: from the
//! property's own declaration, not from the resolved symbol name. A computed
//! key written on the base (`get ["get1"]() {}`) keeps its bracketed source
//! text (`["get1"]`) even when the diagnostic is reported at a derived class's
//! index signature, matching `tsc`'s `getNameOfSymbolAsWritten`
//! (`declarationNameToString`).
//!
//! Regression for the `computedPropertyNames45_ES5/ES6` conformance cluster
//! tracked in #16866: tsz emitted `Property 'get1' ...` where `tsc` emits
//! `Property '["get1"]' ...` (same code TS2411, same position — message only).
//! The own-member path already spelled computed keys correctly; only the
//! inherited-property path in `check_inherited_properties_against_index_signatures`
//! fell back to the bare resolved name. Oracled against pinned
//! `typescript@7.0.2`.
use tsz_checker::test_utils::check_source_diagnostics;

fn ts2411_messages(source: &str) -> Vec<String> {
    check_source_diagnostics(source)
        .iter()
        .filter(|d| d.code == 2411)
        .map(|d| d.message_text.clone())
        .collect()
}

#[test]
fn inherited_computed_string_key_renders_bracketed_name() {
    // `computedPropertyNames45_ES6`: the getter `["get1"]` is declared on the
    // base class `C`; the index signature that rejects it lives on the derived
    // class `D`, so this exercises the inherited-property path.
    let messages = ts2411_messages(
        r#"
class Foo { x }
class Foo2 { x; y }

class C {
    get ["get1"]() { return new Foo }
}

class D extends C {
    [s: string]: Foo2;
    set ["set1"](p: Foo) { }
}
"#,
    );
    assert!(
        messages.contains(
            &"Property '[\"get1\"]' of type 'Foo' is not assignable to 'string' index type 'Foo2'."
                .to_string()
        ),
        "inherited computed key must keep its bracketed source text; got: {messages:?}"
    );
    assert!(
        !messages
            .iter()
            .any(|m| m.contains("Property 'get1'") && !m.contains("[\"get1\"]")),
        "must not spell the inherited computed key as the bare resolved name; got: {messages:?}"
    );
}

#[test]
fn inherited_computed_key_display_is_independent_of_binder_names() {
    // The rule is structural: renaming the classes and the key must not change
    // that the bracketed source text is preserved.
    let messages = ts2411_messages(
        r#"
class Alpha { a }
class Beta { a; b }

class Base {
    get ["wibble"]() { return new Alpha }
}

class Derived extends Base {
    [s: string]: Beta;
}
"#,
    );
    assert!(
        messages.iter().any(|m| m.contains("Property '[\"wibble\"]'")),
        "renamed binders must still preserve the bracketed computed key; got: {messages:?}"
    );
}

#[test]
fn inherited_plain_identifier_key_is_unchanged() {
    // Negative control: a plain identifier property inherited from the base
    // still renders as the bare name — the fix must not add brackets where the
    // source had none.
    let messages = ts2411_messages(
        r#"
class Foo { x }
class Foo2 { x; y }

class C {
    get get1() { return new Foo }
}

class D extends C {
    [s: string]: Foo2;
}
"#,
    );
    assert!(
        messages.iter().any(|m| m
            == "Property 'get1' of type 'Foo' is not assignable to 'string' index type 'Foo2'."),
        "a plain identifier key must render bare, without brackets; got: {messages:?}"
    );
}
