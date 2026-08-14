//! Regression guards for #17456: a class must relate to itself.
//!
//! While a class's own constructor type is being built, the constructor builder
//! publishes a declared-properties-only *partial* instance into
//! `symbol_instance_types` (so `C<any>`-style type references resolve). #17453
//! widened `class_build_reenters_in_flight_member` to also fire on un-annotated
//! method / getter bodies, so resolving such a body mid-constructor-build
//! reached the self-reference deferral guard, which snapshotted that partial
//! (methods missing) as the construct-signature return type. The member-less
//! shape became the cached `new C()` result, so the class failed to relate to
//! itself — a false `TS2740` reporting `C` as missing its own methods from `C`.
//!
//! The fix skips that snapshot while the class's own constructor is on
//! `class_constructor_resolution_set`, returning a lazy self-reference that
//! resolves to the completed instance instead. Binder names (class, field,
//! method) are varied on purpose: the fix keys off structure, never an
//! identifier.

use tsz_checker::context::CheckerOptions;
use tsz_checker::diagnostics::Diagnostic;
use tsz_checker::test_utils::check_source;
use tsz_common::common::ScriptTarget;

fn collect_diagnostics(source: &str) -> Vec<Diagnostic> {
    check_source(
        source,
        "test.ts",
        CheckerOptions {
            target: ScriptTarget::ES2015,
            ..Default::default()
        },
    )
}

fn ts2740_count(source: &str) -> usize {
    collect_diagnostics(source)
        .iter()
        .filter(|d| d.code == 2740)
        .count()
}

/// A class whose annotated method constructs and returns a fresh instance,
/// alongside un-annotated sibling methods that re-reference the class during
/// the member-check rebuild, must not report `TS2740` at `return`. Each variant
/// uses distinct binder names so the guard cannot pass by an identifier match.
#[test]
fn self_returning_class_relates_to_itself() {
    let cases: &[&str] = &[
        // Annotated self-returning clone + un-annotated `return this` / `new`.
        r#"
class Cursor {
    items: number[] = [];
    head: number = 0;
    public copy(): Cursor {
        var c = new Cursor();
        c.items = this.items.map((v) => { return v; });
        c.head = this.head;
        return c;
    }
    public advance() { this.head++; return this; }
    public rewind() { this.head = 0; return this; }
    public spawn() { return new Cursor(); }
    public peek() { return this.items[this.head]; }
    public size() { return this.items.length; }
}
"#,
        // Different names; nested in a namespace like the original fixture.
        r#"
namespace Graph {
    export class Walker {
        stack: number[] = [];
        depth: number = -1;
        public duplicate(): Walker {
            var w = new Walker();
            w.stack = this.stack.slice();
            w.depth = this.depth;
            return w;
        }
        public descend() { this.depth++; return this; }
        public ascend() { this.depth--; return this; }
        public branch() { return new Walker(); }
        public top() { return this.stack[this.depth]; }
    }
    export class WalkerPool {
        walkers: Walker[] = [];
    }
}
"#,
    ];
    for (i, src) in cases.iter().enumerate() {
        assert_eq!(
            ts2740_count(src),
            0,
            "case {i}: a class must relate to itself (no self TS2740)"
        );
    }
}

/// The fix must not fabricate members: a genuinely missing property on an
/// otherwise self-consistent class assignment still reports its own diagnostic
/// rather than being silently accepted.
#[test]
fn missing_member_still_reported_after_self_build() {
    let src = r#"
class Box {
    value: number = 0;
    public wrap(): Box {
        var b = new Box();
        b.value = this.value;
        return b;
    }
}
var partial: Box = { value: 1 };
"#;
    // The object literal is missing the `wrap` method; tsc reports TS2741
    // (property missing) here, never TS2740-from-itself on the class body.
    let diags = collect_diagnostics(src);
    assert!(
        diags.iter().any(|d| d.code == 2741 || d.code == 2740),
        "an object literal missing the class's method is still rejected, got: {:?}",
        diags.iter().map(|d| d.code).collect::<Vec<_>>()
    );
}

/// A genuine self-referential field cycle must terminate (the lazy
/// self-reference fallback resolves once the build completes) and must not
/// report a spurious self-`TS2740`.
#[test]
fn genuine_self_field_cycle_terminates_without_self_ts2740() {
    let src = r#"
class Recur {
    child: Recur = new Recur();
    sibling(): Recur { return this.child; }
    make() { return new Recur(); }
}
var r: Recur = new Recur();
"#;
    assert_eq!(
        ts2740_count(src),
        0,
        "a self-referential class still relates to itself"
    );
}
