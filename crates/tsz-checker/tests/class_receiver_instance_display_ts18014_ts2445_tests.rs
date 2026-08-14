//! Regression pins for #17480: a private/protected access diagnostic must render
//! the class's **instance** type (`C`), never its static/constructor side
//! (`typeof C`), when the receiver resolves to an in-flight self-reference while
//! the class's own constructor is still being built.
//!
//! Background. #17453 widened the in-flight-member re-entry guard to un-annotated
//! method / getter bodies. A body that constructs `new C()` (e.g.
//! `#bar() { new C().#baz; }`) therefore requests a fresh instance build while
//! C's own constructor is in flight. #17476 then skipped the
//! `symbol_instance_types` snapshot on that path and fell through to a bare lazy
//! class reference, which two consumers mishandled on the receiver of a
//! private/protected access:
//!   * display rendered `typeof C` where tsc renders `C`
//!     (`privateNameNestedMethodAccess.ts`, TS18014); and
//!   * the unresolved receiver mis-filed the base class, so a legal protected
//!     access from a subclass reported a false TS2445
//!     (`classWithProtectedProperty.ts`).
//!
//! #17476's target (#17456) was already fixed by #17467 (a fields-only
//! provisional no longer clobbers a complete instance), so #17476 was redundant
//! and was reverted by #17483 — restoring the instance snapshot on the re-entry
//! path, which both consumers handle as the instance. The revert landed with no
//! test that pins the *rendered* instance/static distinction it restores, which
//! is exactly why #17476 shipped in the first place: nothing in its diff could
//! distinguish before from after. These tests are that missing guard.
//!
//! Binder names are varied on purpose across cases: the rule keys off structure
//! (instance vs. static side), never an identifier.

use tsz_checker::context::CheckerOptions;
use tsz_checker::diagnostics::Diagnostic;
use tsz_checker::test_utils::{
    check_source, diagnostic_code_message_refs, diagnostics_with_code, has_diagnostic_code,
};
use tsz_common::common::ScriptTarget;

const TS2339: u32 = 2339;
const TS2445: u32 = 2445;
const TS18014: u32 = 18014;

fn check(source: &str) -> Vec<Diagnostic> {
    check_source(
        source,
        "test.ts",
        CheckerOptions {
            target: ScriptTarget::ES2015,
            ..Default::default()
        },
    )
}

/// Assert exactly one TS18014 fired and that its rendered receiver type matches
/// `rendered` (e.g. `"on type 'C'"` for the instance side, `"on type 'typeof
/// Base'"` for the static side).
fn assert_single_ts18014_renders(diags: &[Diagnostic], rendered: &str) {
    let shadow = diagnostics_with_code(diags, TS18014);
    assert_eq!(
        shadow.len(),
        1,
        "expected exactly one TS18014, got: {:?}",
        diagnostic_code_message_refs(diags)
    );
    assert!(
        shadow[0].message_text.contains(rendered),
        "receiver must render `{rendered}`: {}",
        shadow[0].message_text
    );
}

/// The `privateNameNestedMethodAccess.ts` conformance shape. The receiver
/// `new C()` inside `D`'s constructor is reached while `C`'s constructor is in
/// flight (the un-annotated `#bar()` body constructs `new C()`), so its type is a
/// self-reference. The shadowed `#bar` access must render the receiver as the
/// **instance** `C`, not `typeof C`.
#[test]
fn private_shadow_access_renders_instance_side() {
    let source = r#"
class C {
    #foo = 42;
    #bar() { new C().#baz; }
    get #baz() { return 42; }

    m() {
        return class D {
            #bar() {}
            constructor() {
                new C().#foo;
                new C().#bar; // TS18014: shadowed by D's own #bar
                new C().#baz;
                new D().#bar;
            }
        }
    }
}
"#;
    assert_single_ts18014_renders(&check(source), "on type 'C'");
}

/// Same structure with every binder renamed (anti-hardcoding): the instance-side
/// rendering must hold on names the fix never sees.
#[test]
fn private_shadow_access_renders_instance_side_renamed() {
    let source = r#"
class Outer {
    #alpha = 1;
    #beta() { new Outer().#gamma; }
    get #gamma() { return 1; }

    build() {
        return class Inner {
            #beta() {}
            constructor() {
                new Outer().#alpha;
                new Outer().#beta; // TS18014: shadowed by Inner's own #beta
                new Outer().#gamma;
            }
        }
    }
}
"#;
    assert_single_ts18014_renders(&check(source), "on type 'Outer'");
}

/// Static/instance distinction preserved: an outer **static** `#x` shadowed by an
/// inner **instance** `#x`, accessed on the class value `Base` itself, still
/// renders the receiver as the constructor side `typeof Base`. The fix narrows to
/// lazy *instance* references and must not flatten a genuine constructor receiver.
#[test]
fn static_receiver_still_renders_typeof() {
    let source = r#"
class Base {
    static #x() { }
    constructor() {
        class Derived {
            #x() { }
            check() {
                Base.#x; // TS18014: inner instance #x shadows the outer static #x
            }
        }
    }
}
"#;
    assert_single_ts18014_renders(&check(source), "on type 'typeof Base'");
}

/// The `classWithProtectedProperty.ts` conformance shape: a subclass method
/// reads protected instance members through a subclass instance. That is legal —
/// tsc reports nothing — so no false TS2445 may fire even though building the
/// receiver re-enters the class construction.
#[test]
fn protected_instance_access_from_subclass_is_clean() {
    let source = r#"
class C {
    protected x;
    protected a = '';
    protected b: string = '';
    protected c() { return '' }
    protected d = () => '';
    protected static e;
    protected static f() { return '' }
    protected static g = () => '';
}

class D extends C {
    method() {
        var d = new D();
        var r1: string = d.x;
        var r2: string = d.a;
        var r3: string = d.b;
        var r4: string = d.c();
        var r5: string = d.d();
        var r6: string = C.e;
        var r7: string = C.f();
        var r8: string = C.g();
    }
}
"#;
    let diags = check(source);
    assert!(
        !has_diagnostic_code(&diags, TS2445),
        "protected instance access from a subclass is legal; no TS2445 expected, got: {:?}",
        diagnostic_code_message_refs(&diags)
    );
}

/// Negative control: a genuinely-absent private member still reports TS2339, so
/// the instance-side receiver rendering does not fabricate members or swallow the
/// real "does not exist" diagnostic.
#[test]
fn genuinely_absent_private_member_still_ts2339() {
    let source = r#"
class C {
    #present = 1;
    #probe() { new C().#present; }

    m() {
        return class D {
            n(x: any) {
                x.#missing; // TS2339: #missing declared nowhere
            }
        }
    }
}
"#;
    let diags = check(source);
    assert!(
        has_diagnostic_code(&diags, TS2339),
        "an undeclared private member must still report TS2339, got: {:?}",
        diagnostic_code_message_refs(&diags)
    );
}
