//! Regression coverage for #14511: a class get-accessor with an *inferred*
//! return type (no explicit annotation) must contribute that inferred type as
//! its property type at the access site, so assignability checks (TS2322) fire
//! exactly as they do for an explicitly-annotated getter. tsz previously typed
//! the inferred-getter property as `any` at the access site (false negative).

use tsz_checker::test_utils::check_source_strict_codes;

fn ts2322_count(source: &str) -> usize {
    check_source_strict_codes(source)
        .into_iter()
        .filter(|code| *code == 2322)
        .count()
}

#[test]
fn inferred_getter_number_flows_to_property_access() {
    // `count` infers `number`; assigning it to `string` must error (tsc: TS2322).
    let source = r#"
class C {
  _v = 0;
  get count() { return this._v; }
}
const wrong: string = new C().count;
"#;
    assert_eq!(
        ts2322_count(source),
        1,
        "inferred-getter `count: number` assigned to `string` must report TS2322"
    );
}

#[test]
fn inferred_getter_number_rejects_narrower_literal() {
    // `count` infers `number` (widened), not the literal `123`.
    let source = r#"
class C {
  _v = 0;
  get count() { return this._v; }
}
const probe: 123 = new C().count;
"#;
    assert_eq!(
        ts2322_count(source),
        1,
        "inferred-getter `count: number` assigned to literal `123` must report TS2322"
    );
}

#[test]
fn inferred_getter_valid_assignment_stays_clean() {
    // Negative control: a valid assignment must not error.
    let source = r#"
class C {
  _v = 0;
  get count() { return this._v; }
}
const ok: number = new C().count;
"#;
    assert_eq!(
        ts2322_count(source),
        0,
        "assigning the inferred `number` getter to `number` must stay clean"
    );
}

#[test]
fn inferred_getter_literal_return_flows_to_access() {
    // A getter that returns a literal infers the widened base in a value
    // position; assigning to an incompatible type must error.
    let source = r#"
class Flag {
  get ready() { return true; }
}
const wrong: string = new Flag().ready;
"#;
    assert_eq!(
        ts2322_count(source),
        1,
        "inferred-getter `ready: boolean` assigned to `string` must report TS2322"
    );
}

#[test]
fn inferred_getter_setter_pair_uses_getter_read_type() {
    // With a getter+setter pair, the read (property) type is the getter return
    // type. The inferred getter returns `number`, so reading into `string` errors.
    let source = r#"
class Box {
  private _v = 0;
  get value() { return this._v; }
  set value(n: number) { this._v = n; }
}
const wrong: string = new Box().value;
"#;
    assert_eq!(
        ts2322_count(source),
        1,
        "the read type of an inferred getter+setter pair is the getter return type"
    );
}

#[test]
fn inferred_getter_renamed_binders_stay_structural() {
    // The fix must be name-agnostic: a renamed class/field/accessor reproduces.
    let source = r#"
class Widget {
  private depth = 0;
  get extent() { return this.depth; }
}
const wrong: string = new Widget().extent;
"#;
    assert_eq!(
        ts2322_count(source),
        1,
        "inferred getter resolution must be structural, not tied to identifier names"
    );
}

#[test]
fn inferred_getter_chained_getter_flows() {
    // A getter whose body reads another inferred getter still resolves to a
    // concrete type at the access site.
    let source = r#"
class Chain {
  _v = 0;
  get inner() { return this._v; }
  get outer() { return this.inner; }
}
const wrong: string = new Chain().outer;
"#;
    assert_eq!(
        ts2322_count(source),
        1,
        "an inferred getter reading another inferred getter must still type-check at access"
    );
}
