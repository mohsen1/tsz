//! Regression tests for property lookup on a value typed as a class's
//! constructor type (`typeof C`), as opposed to its instance type (`C`).
//!
//! `resolve_class_for_access` (`crates/tsz-checker/src/symbols/symbol_resolver_utils.rs`)
//! resolves an arbitrary expression's receiver class and whether the access is
//! static, for private/protected accessibility checks. Its `this`/`super`
//! branches correctly call `is_constructor_type` to decide static-ness, but
//! the generic fallback branch (any other expression whose type maps to a
//! class declaration) hard-coded `is_static: false` — so a plain variable
//! typed `typeof C` was treated as an *instance* receiver. Accessing a
//! private/protected instance member through it then fired the "is private"
//! diagnostic (the member's declaration was found) instead of tsc's "does not
//! exist" (a constructor type simply has no such member). Fixed by computing
//! `is_static` via `self.is_constructor_type(object_type)`, matching the
//! `this`/`super` branches above it.
//!
//! Oracle: pinned `typescript@7.0.2`, `--strict false`.

use crate::test_utils::check_source_diagnostics;

fn diag_codes(source: &str) -> Vec<u32> {
    check_source_diagnostics(source)
        .into_iter()
        .map(|d| d.code)
        .collect()
}

/// A variable typed `typeof C` accessing `C`'s private instance member must
/// report TS2339 ("does not exist"), not TS2341 ("is private").
#[test]
fn ts2339_not_ts2341_for_private_instance_member_via_constructor_type() {
    let source = "\
class C {
    private canary: number = 1;
    static staticCanary: number = 2;
}
var y: typeof C;
y = C;
y.canary;
";
    let codes = diag_codes(source);
    assert!(
        codes.contains(&2339),
        "Expected TS2339 (property does not exist on constructor type). Got: {codes:?}"
    );
    assert!(
        !codes.contains(&2341),
        "Must not report TS2341 (private) for a constructor-typed receiver. Got: {codes:?}"
    );
}

/// Anti-hardcoding cover: renamed class/member/variable, protected instead of
/// private, and derived class to confirm the rule is structural.
#[test]
fn ts2339_not_ts2445_for_protected_instance_member_via_constructor_type_renamed() {
    let source = "\
class Widget {
    protected secret: string = \"hi\";
    static tag: string = \"w\";
}
class Gadget extends Widget {}
var handle: typeof Gadget;
handle = Gadget;
handle.secret;
";
    let codes = diag_codes(source);
    assert!(
        codes.contains(&2339),
        "Expected TS2339 for protected instance member via constructor type. Got: {codes:?}"
    );
    assert!(
        !codes.contains(&2445),
        "Must not report TS2445 (protected) for a constructor-typed receiver. Got: {codes:?}"
    );
}

/// Control: the same private member accessed through a genuine *instance*-typed
/// variable outside the class must still report TS2341 — the fix must not
/// weaken the existing instance-access check.
#[test]
fn ts2341_still_fires_for_private_instance_member_via_instance_type() {
    let source = "\
class C {
    private canary: number = 1;
}
var inst: C;
inst = new C();
inst.canary;
";
    let codes = diag_codes(source);
    assert!(
        codes.contains(&2341),
        "Expected TS2341 for private access via a genuine instance receiver. Got: {codes:?}"
    );
}

/// Control: a constructor-typed receiver accessing an actual *static* member
/// must remain clean — the fix must not start rejecting legitimate static
/// access as a side effect of flipping `is_static`.
#[test]
fn no_error_for_static_member_via_constructor_type() {
    let source = "\
class C {
    private canary: number = 1;
    static staticCanary: number = 2;
}
var y: typeof C;
y = C;
y.staticCanary;
";
    let codes = diag_codes(source);
    assert!(
        !codes.contains(&2339),
        "Static member access via constructor type must not report TS2339. Got: {codes:?}"
    );
    assert!(
        !codes.contains(&2341),
        "Static member access via constructor type must not report TS2341. Got: {codes:?}"
    );
}

/// Regression: `is_constructor_type` treats an unresolved `Lazy(DefId)`
/// reference to a class symbol as always-constructor (correct for its other
/// callers, e.g. heritage-clause validation). A plain instance-typed
/// parameter can still be represented that way before it is materialized —
/// here, forward-referencing `C5` in `C4`'s own method signature is enough to
/// keep `C4`'s type lazy at the point `C5`'s method checks a `C4`-typed
/// parameter. Without resolving the lazy reference first,
/// `resolve_class_for_access` misclassified `c4` as a *static* receiver and
/// silently skipped the protected-access check entirely (a false negative —
/// tsc rejects this). Reduced from `conformance/classes/mixinAccessModifiers.ts`.
#[test]
fn protected_access_still_denied_when_receiver_class_is_forward_referenced() {
    let source = "\
class Base {
    protected p: string = \"\";
}
class Fwd extends Base {
    f(other: Other) {}
}
class Other {
    f(fwd: Fwd) {
        fwd.p;
    }
}
";
    let codes = diag_codes(source);
    assert!(
        codes.contains(&2445),
        "Expected TS2445 for protected access via a forward-referenced instance receiver. Got: {codes:?}"
    );
}

/// Anti-hardcoding cover for the forward-reference regression: renamed
/// classes/members/methods, private instead of protected.
#[test]
fn private_access_still_denied_when_receiver_class_is_forward_referenced_renamed() {
    let source = "\
class Root {
    private secret: number = 0;
}
class Node extends Root {
    link(peer: Leaf) {}
}
class Leaf {
    visit(node: Node) {
        node.secret;
    }
}
";
    let codes = diag_codes(source);
    assert!(
        codes.contains(&2341),
        "Expected TS2341 for private access via a forward-referenced instance receiver. Got: {codes:?}"
    );
}
