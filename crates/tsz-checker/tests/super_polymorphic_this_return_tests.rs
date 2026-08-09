//! Tests for `super.method()` preserving the base method's polymorphic `this`
//! return type (issue #16960).
//!
//! Structural rule:
//!   When a base method's return type is the polymorphic `this` type (whether
//!   inferred from a bare `return this;` or written `): this`), accessing that
//!   method through `super.` must bind `this` to the *enclosing* class's
//!   `this`-type, exactly like tsc's
//!   `getTypeWithThisArgument(baseType, enclosingClassThisType)`. JS's
//!   `super.foo()` never receives a fresh base-class instance — the runtime
//!   receiver stays the current instance — so the result must carry the derived
//!   receiver, not the base class.
//!
//! tsz previously baked `super.method()`'s `this` return to the *base* instance
//! type (the receiver of the `super` lookup), producing a spurious `TS2339`
//! (`Property 'x' does not exist on type 'Base'`) and, for a multi-level
//! `super` chain, collapsing every hop to the root base class.
//!
//! The fix keeps the base member's `this` polymorphic at the `super.` access
//! site, so it threads through inference and is rebound to the actual receiver
//! at the eventual direct-access site. Binder names are varied so no fix can
//! key on a specific identifier.

use tsz_checker::test_utils::{check_source_strict_codes, check_source_strict_messages};

/// Diagnostics other than TS2318 (missing global types in the no-stdlib unit
/// harness), which is noise here.
fn codes(source: &str) -> Vec<u32> {
    check_source_strict_codes(source)
        .into_iter()
        .filter(|&code| code != 2318)
        .collect()
}

/// TS2322 messages (used to witness *which* class name the inferred type
/// resolved to).
fn ts2322_messages(source: &str) -> Vec<String> {
    check_source_strict_messages(source)
        .into_iter()
        .filter(|(code, _)| *code == 2322)
        .map(|(_, message)| message)
        .collect()
}

// ── Primary repro: inferred `return this` threaded through `super` ──────────

#[test]
fn super_inferred_this_return_threads_derived_member_access() {
    // `pull()` returns `this` (inferred). The override forwards it through
    // `super.pull()`; the forwarded result must be the derived receiver, whose
    // `tag()` member exists — so the whole program is clean, like tsc.
    let source = r#"
class Vessel {
    pull() { return this; }
}
class Skiff extends Vessel {
    tag() { return 0; }
    pull() { return super.pull(); }
}
new Skiff().pull().tag();
"#;
    assert_eq!(
        codes(source),
        Vec::<u32>::new(),
        "super.pull() must yield the derived receiver (Skiff), whose tag() exists"
    );
}

#[test]
fn super_inferred_this_return_never_witness_is_not_base() {
    // Assigning the `super` result to `never` witnesses the inferred type. The
    // base's `draw()` infers a polymorphic `this` return, so inside the derived
    // method `super.draw()` is `this` (the derived receiver) — never the base
    // class. The witnessing TS2322 must therefore fire and must NOT name the
    // base class `Cistern` (the pre-fix behavior baked it to the base).
    let source = r#"
class Cistern {
    draw() { return this; }
}
class Wellspring extends Cistern {
    draw() {
        let sink: never = super.draw();
        return super.draw();
    }
}
"#;
    let messages = ts2322_messages(source);
    assert!(
        !messages.is_empty(),
        "super.draw() assigned to `never` must raise TS2322"
    );
    assert!(
        !messages.iter().any(|m| m.contains("Cistern")),
        "super.draw() must NOT resolve to the base class Cistern, got: {messages:?}"
    );
}

// ── Multi-level `super` chain threads the original receiver ────────────────

#[test]
fn super_this_return_threads_through_multi_level_chain() {
    // `A -> B -> C`, each overriding `hop()` as `return super.hop()`. The
    // `this` must thread through as the *original* call's receiver (C), not
    // collapse to B at each hop. `new C().hop().onlyOnC()` is therefore clean.
    let source = r#"
class Rung {
    hop() { return this; }
}
class Middle extends Rung {
    hop() { return super.hop(); }
}
class Summit extends Middle {
    onlyOnSummit() { return 0; }
    hop() { return super.hop(); }
}
new Summit().hop().onlyOnSummit();
"#;
    assert_eq!(
        codes(source),
        Vec::<u32>::new(),
        "multi-level super chain must thread `this` to the outermost receiver (Summit)"
    );
}

// ── Explicit `: this` annotation triggers the same path ────────────────────

#[test]
fn super_explicit_this_return_annotation_threads_derived() {
    let source = r#"
class Beacon {
    signal(): this { return this; }
}
class Lighthouse extends Beacon {
    glow() { return 0; }
    signal(): this { return super.signal(); }
}
new Lighthouse().signal().glow();
"#;
    assert_eq!(
        codes(source),
        Vec::<u32>::new(),
        "an explicit `: this` base return must thread through super just like an inferred one"
    );
}

// ── Regression guard: `this`-typed parameter through `super` still accepted ─

#[test]
fn super_this_typed_param_call_still_accepted() {
    // The `this`-parameter path must stay callable: passing the enclosing
    // `this` into `super.addChild(c: this)` is valid.
    let source = r#"
class Branch {
    children: this[] = [];
    addChild(c: this): void { this.children.push(c); }
}
class Twig extends Branch {
    addChild(c: this): void {
        this.children.push(c);
        super.addChild(c);
    }
}
"#;
    assert!(
        !codes(source).contains(&2345),
        "passing `this` into super.addChild(c: this) must not raise TS2345"
    );
}

// ── Regression guard: a non-`this` base return is unaffected ────────────────

#[test]
fn super_non_this_return_is_unchanged() {
    // `label()` returns a plain string, not `this`. Threading logic must not
    // touch it: accessing a string-only member is fine, a bogus one still errors.
    let clean = r#"
class Marker {
    label(): string { return ""; }
}
class Signpost extends Marker {
    label(): string { return super.label(); }
    read() { const s: string = this.label(); return s; }
}
"#;
    assert_eq!(
        codes(clean),
        Vec::<u32>::new(),
        "a non-this base return must flow through super unchanged"
    );
}

// ── Anti-hardcoding: direct inherited call (no override) still resolves ─────

#[test]
fn direct_inherited_this_return_still_resolves_to_derived() {
    // Regression guard for the path that already worked: a subclass that does
    // NOT override still resolves the inherited `this` return to itself.
    let source = r#"
class Anchor {
    moor() { return this; }
}
class Dock extends Anchor {
    berth() { return 0; }
}
new Dock().moor().berth();
"#;
    assert_eq!(
        codes(source),
        Vec::<u32>::new(),
        "inherited (non-super) `this` return must still resolve to the derived class"
    );
}
