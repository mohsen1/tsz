//! Two instantiations of the same generic interface relate by per-parameter
//! variance, not by a structural member walk, when every type parameter sits in
//! a bivariant position.
//!
//! Structural rule: when a generic interface's type parameter appears *solely*
//! in a bivariant position — e.g. the `extends` clause of a member's conditional
//! return type, which selects a branch but never flows into either branch — two
//! instantiations differing only in that argument relate in either direction.
//! `tsc` relates them via `relateVariances` with all-bivariant marks before any
//! structural expansion; the member's deferred conditional (which differs only
//! in that argument and can never relate structurally) is never reached.
//!
//! Regression witness: the kysely/valibot/zod `T`-not-assignable-to-`T` family,
//! where `tsz` materialized each interface instantiation to an object shape and
//! then false-rejected (`TS2322`/`TS2416`) on the differing deferred conditional
//! member. The fix recovers each side's originating application via display
//! provenance and accepts the all-bivariant pair.
//!
//! These cases vary binder names so the behavior tracks the structural position,
//! not any spelling, and pin the negative controls (covariant / check-position
//! parameters must still discriminate) so the bivariance acceptance is not a
//! blanket "same base ⇒ related" shortcut.

use crate::args::CliArgs;
use clap::Parser;

/// Compile a single-file program with the bundled `es2022` lib and return the
/// non-hint diagnostic codes.
fn check(src: &str) -> Vec<u32> {
    let dir = tempfile::tempdir().expect("temp dir");
    std::fs::write(dir.path().join("main.ts"), src).expect("write source");
    let args = CliArgs::try_parse_from([
        "tsz",
        "--ignoreConfig",
        "--noEmit",
        "--strict",
        "--target",
        "es2022",
        "--lib",
        "es2022",
        "main.ts",
    ])
    .expect("parse args");
    let result = crate::driver::compile(&args, dir.path()).expect("compile");
    result
        .diagnostics
        .iter()
        .map(|d| d.code)
        .filter(|code| code / 100 != 61) // drop unused-symbol hints
        .collect()
}

fn count(codes: &[u32], code: u32) -> usize {
    codes.iter().filter(|c| **c == code).count()
}

/// A parameter that appears only in a conditional `extends` position is
/// bivariant: assigning between two instantiations with unrelated concrete
/// arguments is clean in both directions (`tsc` 6.0: no error).
#[test]
fn extends_position_param_is_bivariant_both_directions() {
    // `Col` appears only in `R extends Col[] ? ...`. The two instantiations use
    // disjoint concrete arguments.
    let forward = check(
        r#"
interface Registry<Schema, Col extends keyof Schema> {
    pick<Row>(row: Row): Row extends Col[] ? number : never;
}
function up(g: Registry<{ a: 1 }, "a">): Registry<{ b: 2 }, "b"> { return g; }
"#,
    );
    assert_eq!(forward, Vec::<u32>::new(), "forward got {forward:?}");

    let backward = check(
        r#"
interface Registry<Schema, Col extends keyof Schema> {
    pick<Row>(row: Row): Row extends Col[] ? number : never;
}
function down(g: Registry<{ b: 2 }, "b">): Registry<{ a: 1 }, "a"> { return g; }
"#,
    );
    assert_eq!(backward, Vec::<u32>::new(), "backward got {backward:?}");
}

/// The `implements` witness, with binder names varied from the kysely original:
/// a base method takes the generic interface, the impl takes a concretely
/// instantiated one. Method parameters relate bivariantly and the interface
/// argument is bivariant, so there is no `TS2416`.
#[test]
fn implements_member_concrete_bivariant_interface_param_is_accepted() {
    let codes = check(
        r#"
interface Builder<Schema, Col extends keyof Schema> {
    pick<Row>(row: Row): Row extends Col[] ? number : never;
}
interface Consumer<Schema, Col extends keyof Schema> {
    run(builder: Builder<Schema, Col>): void;
}
class ConsumerImpl<Schema, Col extends keyof Schema> implements Consumer<Schema, Col> {
    run(builder: Builder<{ a: 1 }, "a">): void {}
}
"#,
    );
    assert_eq!(count(&codes, 2416), 0, "got {codes:?}");
}

/// All type parameters unused (purely phantom): bivariant, relates regardless of
/// arguments.
#[test]
fn fully_unused_params_relate_regardless_of_arguments() {
    let codes = check(
        r#"
interface Tagged<First, Second> { value: number; }
function f(g: Tagged<1, 2>): Tagged<3, 4> { return g; }
"#,
    );
    assert_eq!(codes, Vec::<u32>::new(), "got {codes:?}");
}

/// Negative control: a parameter in an ordinary covariant property position is
/// NOT bivariant. The unsound direction must still be rejected (`tsc`: TS2322).
#[test]
fn covariant_property_param_still_rejects_unsound_direction() {
    let codes = check(
        r#"
interface Cell<Held> { value: Held; }
function f(g: Cell<number>): Cell<1> { return g; }
"#,
    );
    assert_eq!(count(&codes, 2322), 1, "got {codes:?}");
}

/// Negative control: the covariant direction is still accepted, proving the
/// rejection above is variance-driven and not a blanket structural failure.
#[test]
fn covariant_property_param_accepts_sound_direction() {
    let codes = check(
        r#"
interface Cell<Held> { value: Held; }
function f(g: Cell<1>): Cell<number> { return g; }
"#,
    );
    assert_eq!(codes, Vec::<u32>::new(), "got {codes:?}");
}

/// Negative control: a parameter genuinely used in a covariant conditional
/// *branch* is measured (not bivariant); the unsound direction is rejected.
#[test]
fn conditional_branch_param_is_measured_not_bivariant() {
    let codes = check(
        r#"
interface Wrap<Held> { read(): unknown extends never ? never : Held; }
function f(g: Wrap<string>): Wrap<"a"> { return g; }
"#,
    );
    assert_eq!(count(&codes, 2322), 1, "got {codes:?}");
}
