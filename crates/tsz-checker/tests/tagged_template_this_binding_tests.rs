//! Tests for `this:`-typed tag functions in tagged template expressions.
//!
//! Structural rule: when a tagged template's tag is a property or element
//! access expression AND the resolved tag function type carries an explicit,
//! non-trivial `this:` constraint, the receiver type must be bound to that
//! `this:` parameter during call resolution — matching the same policy used
//! for decorator call sites.
//!
//! When the tag function has no `this:` constraint (or a trivial one such as
//! `this: any` / `this: void`), the receiver must NOT be threaded into the
//! call resolver, so generic inference is not perturbed.

use tsz_checker::test_utils::check_source_codes;

/// Minimal `TemplateStringsArray` shim so tests don't need the full lib.
const PREAMBLE: &str = r#"
interface ReadonlyArray<T> { [n: number]: T; readonly length: number; }
interface TemplateStringsArray extends ReadonlyArray<string> { readonly raw: ReadonlyArray<string>; }
"#;

fn check(src: &str) -> Vec<u32> {
    check_source_codes(&format!("{PREAMBLE}\n{src}"))
}

// ──────────────────────────────────────────────────────────────────────────────
// Passing cases: explicit this: constraint that matches the receiver
// ──────────────────────────────────────────────────────────────────────────────

#[test]
fn property_access_tag_with_explicit_this_no_error() {
    // Tag function constrained to `this: TagHost`. When the tag is accessed via
    // `host.tag`, the receiver type `TagHost` satisfies the constraint.
    let codes = check(
        r#"
class TagHost {
    tag(this: TagHost, strings: TemplateStringsArray, x: number): string {
        return "";
    }
}

declare const host: TagHost;
const result = host.tag`hello ${42}`;
"#,
    );

    assert!(
        codes.is_empty(),
        "Property-access tag with matching this: should not produce errors; got: {codes:?}"
    );
}

#[test]
fn element_access_tag_with_explicit_this_no_error() {
    // Same structural rule via element access (`host["tag"]`).
    let codes = check(
        r#"
class TagHost {
    tag(this: TagHost, strings: TemplateStringsArray, x: number): string {
        return "";
    }
}

declare const host: TagHost;
const result = host["tag"]`hello ${42}`;
"#,
    );

    assert!(
        codes.is_empty(),
        "Element-access tag with matching this: should not produce errors; got: {codes:?}"
    );
}

#[test]
fn generic_tag_with_explicit_this_infers_type_param() {
    // Generic tag: `tag<T>(this: Provider, strings: TemplateStringsArray, v: T): T`.
    // The receiver binds `this:`, and `T` should be inferred from the substitution.
    let codes = check(
        r#"
class Provider {
    tag<T>(this: Provider, strings: TemplateStringsArray, v: T): T {
        return v;
    }
}

declare const p: Provider;
const x: number = p.tag`prefix ${42}`;
"#,
    );

    assert!(
        codes.is_empty(),
        "Generic tag with explicit this: must infer T without error; got: {codes:?}"
    );
}

// ──────────────────────────────────────────────────────────────────────────────
// Passing cases: trivial / absent this: — receiver must not perturb inference
// ──────────────────────────────────────────────────────────────────────────────

#[test]
fn property_access_tag_no_this_constraint_no_error() {
    // Tag has no `this:` constraint. Even though accessed via property access,
    // the receiver must NOT be threaded into call resolution.
    let codes = check(
        r#"
class Holder {
    tag(strings: TemplateStringsArray, x: number): string {
        return "";
    }
}

declare const holder: Holder;
const result = holder.tag`hello ${42}`;
"#,
    );

    assert!(
        codes.is_empty(),
        "Tag without this: constraint should not produce errors; got: {codes:?}"
    );
}

#[test]
fn property_access_tag_this_any_no_error() {
    // `this: any` is trivial — any receiver is assignable. Same as no constraint.
    let codes = check(
        r#"
class Holder {
    tag(this: any, strings: TemplateStringsArray, x: number): string {
        return "";
    }
}

declare const holder: Holder;
const result = holder.tag`hello ${42}`;
"#,
    );

    assert!(
        codes.is_empty(),
        "Tag with this: any should not produce errors; got: {codes:?}"
    );
}

#[test]
fn property_access_tag_this_void_no_error() {
    // `this: void` is the tsc-standard "no receiver" sentinel — treated as trivial.
    let codes = check(
        r#"
class Holder {
    tag(this: void, strings: TemplateStringsArray, x: number): string {
        return "";
    }
}

declare const holder: Holder;
const result = holder.tag`hello ${42}`;
"#,
    );

    assert!(
        codes.is_empty(),
        "Tag with this: void should not produce errors; got: {codes:?}"
    );
}

#[test]
fn bare_identifier_tag_with_explicit_this_no_error() {
    // When the tag is a bare identifier (not a property access), the receiver
    // is None regardless of the this: constraint.
    let codes = check(
        r#"
function tag(this: unknown, strings: TemplateStringsArray, x: number): string {
    return "";
}
const result = tag`hello ${42}`;
"#,
    );

    assert!(
        codes.is_empty(),
        "Bare identifier tag should not produce receiver-related errors; got: {codes:?}"
    );
}

// ──────────────────────────────────────────────────────────────────────────────
// Name-variation coverage: the rule is structural, not name-dependent
// ──────────────────────────────────────────────────────────────────────────────

#[test]
fn explicit_this_constraint_name_variation_k() {
    // Same structural rule with a renamed tag method (`render` instead of `tag`).
    let codes = check(
        r#"
class Renderer {
    render<K>(this: Renderer, strings: TemplateStringsArray, v: K): K {
        return v;
    }
}

declare const r: Renderer;
const x: string = r.render`prefix ${"hello"}`;
"#,
    );

    assert!(
        codes.is_empty(),
        "Rule must be independent of tag method name; got: {codes:?}"
    );
}
