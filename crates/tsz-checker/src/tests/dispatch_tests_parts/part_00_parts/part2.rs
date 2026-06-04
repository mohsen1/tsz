#[test]
fn class_property_initializer_this_prescan_includes_accessors() {
    let diags = check_source(
        r#"
declare function needsAccessor(value: { readonly current: number }): void;

export class Model {
    get current(): number {
        return 1;
    }
    value = needsAccessor(this);
}
"#,
        "test.ts",
        CheckerOptions {
            strict: true,
            ..CheckerOptions::default()
        },
    );
    let relevant: Vec<_> = diags
        .iter()
        .filter(|d| matches!(d.code, 2345 | 2739))
        .collect();
    assert_eq!(
        relevant.len(),
        0,
        "Expected class property initializer `this` prescan to include accessors, got: {relevant:?}"
    );
}

#[test]
fn static_property_initializer_this_uses_constructor_owner_during_type_environment() {
    let diags = check_source(
        r#"
type ForwardConstructor = typeof Registry;

export class Registry {
    static build(): number {
        return 1;
    }
    static create = this.build;
}

Registry.create;
"#,
        "test.ts",
        CheckerOptions {
            strict: true,
            ..CheckerOptions::default()
        },
    );
    let relevant: Vec<_> = diags
        .iter()
        .filter(|d| matches!(d.code, 2339 | 2532 | 2683))
        .collect();
    assert_eq!(
        relevant.len(),
        0,
        "Expected static property initializer `this` to use the constructor owner during early type environment construction, got: {relevant:?}"
    );
}

#[test]
fn explicit_this_current_class_does_not_use_any_cached_placeholder() {
    let diags = check_source_diagnostics(
        r#"
const C = class C {
    static getInstance() { return new C(); }
    m(this: C) {
        return this.missing;
    }
};
"#,
    );
    let ts2339: Vec<_> = diags
        .iter()
        .filter(|d| d.code == 2339 && d.message_text.contains("missing"))
        .collect();
    assert!(
        !ts2339.is_empty(),
        "Expected TS2339 for explicit `this: C` missing member access, got: {diags:?}"
    );
}

#[test]
fn jsx_children_contextual_typing_uses_request_path() {
    let diags = check_source(
        r#"
declare namespace JSX {
    interface Element {}
    interface IntrinsicElements {
        div: {};
    }
    interface ElementChildrenAttribute {
        children: {};
    }
}

declare function Panel(props: { children: (s: string) => JSX.Element }): JSX.Element;

<Panel>{s => { s.toUpperCase(); return <div />; }}</Panel>;
"#,
        "test.tsx",
        CheckerOptions {
            jsx_mode: JsxMode::Preserve,
            ..CheckerOptions::default()
        },
    );
    let relevant: Vec<_> = diags
        .iter()
        .filter(|d| matches!(d.code, 7006 | 2339))
        .collect();
    assert_eq!(
        relevant.len(),
        0,
        "Expected JSX children contextual typing to stay on the request path, got: {relevant:?}"
    );
}

#[test]
fn jsdoc_template_and_param_resolution_stay_stable_through_request_path() {
    let diags = check_source(
        r#"
/** @template T
 * @param {(value: T) => T} fn
 * @param {T} value
 */
function apply(fn, value) {
    return fn(value);
}

/** @template T */
class Box {
    /** @param {T} value */
    constructor(value) {
        this.value = value;
    }
}

/** @param {{ text: string }} value */
const useText = (value) => value.text.toUpperCase();

apply(useText, { text: "ok" });
new Box("ok");
"#,
        "test.js",
        CheckerOptions::default(),
    );
    let relevant: Vec<_> = diags
        .iter()
        .filter(|d| matches!(d.code, 7006 | 7031 | 2304 | 2314 | 2339))
        .collect();
    assert_eq!(
        relevant.len(),
        0,
        "Expected JSDoc template/param resolution to stay stable, got: {relevant:?}"
    );
}

#[test]
fn jsdoc_generic_callback_typedef_type_tag_resolves_as_callable() {
    let diags = check_js_source_diagnostics(
        r#"
/**
 * @template T
 * @callback B
 * @returns {T}
 */

/** @type {B<string>} */
let b = {};

b();
b(1);
"#,
    );
    let codes = diagnostic_codes(&diags);
    assert!(
        codes.contains(&2322),
        "Expected TS2322 for assigning {{}} to generic callback typedef, got: {codes:?}"
    );
    assert!(
        diags
            .iter()
            .any(|d| d.code == 2322 && d.message_text.contains("B<string>")),
        "Expected TS2322 to preserve the instantiated JSDoc callback alias in the message, got: {diags:?}"
    );
    assert!(
        codes.contains(&2554),
        "Expected TS2554 for calling instantiated callback typedef with an extra arg, got: {codes:?}"
    );
    assert!(
        !codes.contains(&2349),
        "Expected instantiated callback typedef to stay callable, got: {codes:?}"
    );
}
