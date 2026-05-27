//! Tests for TS2376/TS17009 diagnostic prioritization in derived class constructors.
//!
//! For the narrow single-expression-statement case, `tsc` emits only TS17009 at
//! the `this` site. It still emits TS2376 for declaration initializers and
//! multiple pre-super `this` references in position-sensitive constructors.
//!
//! Issue: <https://github.com/mohsen1/tsz/issues/9678>

use crate::test_utils::check_source_diagnostics;

/// tsc: TS17009 at `this.x` — no TS2376.
/// tsz previously also emitted TS2376 at the `super()` call.
#[test]
fn this_before_super_with_initialized_field_emits_only_ts17009() {
    let diags = check_source_diagnostics(
        r#"
class Base {}
class Derived extends Base {
    x = 1;
    constructor() {
        this.x;
        super();
    }
}
"#,
    );

    let ts17009: Vec<_> = diags.iter().filter(|d| d.code == 17009).collect();
    let ts2376: Vec<_> = diags.iter().filter(|d| d.code == 2376).collect();

    assert!(
        !ts17009.is_empty(),
        "Expected TS17009 for this-before-super; got: {diags:?}"
    );
    assert!(
        ts2376.is_empty(),
        "Expected no TS2376 for the single pre-super this expression statement; got: {diags:?}"
    );
}

/// Same fix applies when the field name and iteration variable differ from the
/// first test — the rule is structural, not identifier-specific.
#[test]
fn this_before_super_different_field_name_emits_only_ts17009() {
    let diags = check_source_diagnostics(
        r#"
class Animal {}
class Dog extends Animal {
    name: string = "rex";
    constructor() {
        console.log(this.name);
        super();
    }
}
"#,
    );

    let ts17009: Vec<_> = diags.iter().filter(|d| d.code == 17009).collect();
    let ts2376: Vec<_> = diags.iter().filter(|d| d.code == 2376).collect();

    assert!(
        !ts17009.is_empty(),
        "Expected TS17009 for this-before-super; got: {diags:?}"
    );
    assert!(
        ts2376.is_empty(),
        "Expected no TS2376 for the single pre-super this expression statement; got: {diags:?}"
    );
}

/// `tsc` keeps TS2376 when the pre-super `this` reference appears inside a
/// declaration initializer rather than as a standalone expression statement.
#[test]
fn this_before_super_in_variable_initializer_keeps_ts2376() {
    let diags = check_source_diagnostics(
        r#"
class Base {}
class Derived extends Base {
    x = 1;
    constructor() {
        const value = this.x;
        super();
    }
}
"#,
    );

    let ts17009: Vec<_> = diags.iter().filter(|d| d.code == 17009).collect();
    let ts2376: Vec<_> = diags.iter().filter(|d| d.code == 2376).collect();

    assert!(
        !ts17009.is_empty(),
        "Expected TS17009 for this-before-super; got: {diags:?}"
    );
    assert!(
        !ts2376.is_empty(),
        "Expected TS2376 when this-before-super is in a variable initializer; got: {diags:?}"
    );
}

/// `tsc` also keeps TS2376 when multiple pre-super `this` references appear
/// before the first root-level `super()` call.
#[test]
fn multiple_this_references_before_super_keep_ts2376() {
    let diags = check_source_diagnostics(
        r#"
class Base {}
class Derived extends Base {
    x = 1;
    y: number;
    constructor() {
        this.x = 2;
        this.y = 3;
        super();
    }
}
"#,
    );

    let ts17009: Vec<_> = diags.iter().filter(|d| d.code == 17009).collect();
    let ts2376: Vec<_> = diags.iter().filter(|d| d.code == 2376).collect();

    assert!(
        !ts17009.is_empty(),
        "Expected TS17009 for this-before-super; got: {diags:?}"
    );
    assert!(
        !ts2376.is_empty(),
        "Expected TS2376 when multiple pre-super this references precede super(); got: {diags:?}"
    );
}

/// When `super.property` is accessed before `super()` but `this` is NOT
/// accessed, TS2376 must fire (super-not-first), and TS17009 must NOT fire
/// (it is only for `this`-before-super).
///
/// NOTE: `Child` must have an initialized property so that the class is
/// "position-sensitive" — without one, `has_position_sensitive_members` is
/// false and tsz (like tsc) skips the TS2376 placement check entirely.
#[test]
fn super_property_before_super_call_emits_ts2376_not_ts17009() {
    let diags = check_source_diagnostics(
        r#"
class Base {
    value = 0;
}
class Child extends Base {
    x = 1;
    constructor() {
        super.value;
        super();
    }
}
"#,
    );

    let ts17009: Vec<_> = diags.iter().filter(|d| d.code == 17009).collect();
    let ts2376: Vec<_> = diags.iter().filter(|d| d.code == 2376).collect();

    assert!(
        ts17009.is_empty(),
        "Expected no TS17009 when only super.property (not this) precedes super(); got: {diags:?}"
    );
    assert!(
        !ts2376.is_empty(),
        "Expected TS2376 when super.property precedes super() in a position-sensitive constructor; got: {diags:?}"
    );
}

/// Mixed case: both `super.property` and `this` appear before `super()`.
/// The single `this` expression-statement still suppresses TS2376 even when a
/// separate pre-super `super.property` expression appears first.
#[test]
fn super_property_and_this_before_super_emits_ts17009_only() {
    let diags = check_source_diagnostics(
        r#"
class Base {
    value = 0;
}
class Mixed extends Base {
    x = 1;
    constructor() {
        super.value;
        this.x;
        super();
    }
}
"#,
    );

    let ts17009: Vec<_> = diags.iter().filter(|d| d.code == 17009).collect();
    let ts2376: Vec<_> = diags.iter().filter(|d| d.code == 2376).collect();

    assert!(
        !ts17009.is_empty(),
        "Expected TS17009 for this-before-super in mixed super.property+this case; got: {diags:?}"
    );
    assert!(
        ts2376.is_empty(),
        "Expected no TS2376 for the mixed single-this expression-statement case; got: {diags:?}"
    );
}

/// A correctly ordered constructor with `this` after `super()` must produce
/// neither TS17009 nor TS2376.
#[test]
fn this_after_super_no_diagnostics() {
    let diags = check_source_diagnostics(
        r#"
class Base {}
class Derived extends Base {
    x = 1;
    constructor() {
        super();
        this.x;
    }
}
"#,
    );

    let ts17009: Vec<_> = diags.iter().filter(|d| d.code == 17009).collect();
    let ts2376: Vec<_> = diags.iter().filter(|d| d.code == 2376).collect();

    assert!(
        ts17009.is_empty() && ts2376.is_empty(),
        "Expected no TS17009/TS2376 when this follows super(); got: {diags:?}"
    );
}
