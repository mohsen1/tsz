//! Regression tests for #16308: a deep, non-cyclic same-file interface
//! `extends` chain silently dropped its `Array<T>` base.
//!
//! Structural rule: when a user interface's `extends` clause resolution
//! nests through several other user-declared interfaces before reaching its
//! ultimate base (here, a lib generic like `Array<T>`), `tsc` merges the
//! full chain regardless of how many interfaces are in it — nesting depth
//! alone is not a cycle. tsz's `merge_interface_heritage_types_inner`
//! (`crates/tsz-checker/src/types/interface_type.rs`) bounds its own
//! recursion with a `heritage_merge_depth` counter; the previous limit of 5
//! bailed to the partially-merged type (own members only, base dropped) on
//! the sixth nested call, with no "incomplete" signal, so the truncated type
//! was then cached and reused by everything above it in the chain — exactly
//! the shape reported against the real mobx corpus row
//! (`IObservableArray<T> extends Array<T>`, reached through mobx's own
//! multi-file interface hierarchy). The fix raises the bound to match the
//! already-battle-tested analogous guard for lib-interface heritage
//! (`LIB_HERITAGE_MERGE_MAX_DEPTH`, 50).
//!
//! Binder names are varied (`Chain`/`Link` prefix vs `I`) so no identifier
//! is load-bearing.

use crate::context::CheckerOptions;
use crate::diagnostics::diagnostic_codes;
use crate::test_utils::{check_source_with_libs, load_default_lib_files};

fn strict_codes_with_libs(source: &str) -> Vec<u32> {
    let libs = load_default_lib_files();
    check_source_with_libs(
        source,
        "test.ts",
        CheckerOptions {
            strict: true,
            ..CheckerOptions::default()
        },
        &libs,
    )
    .into_iter()
    .map(|d| d.code)
    .collect()
}

fn assert_no_missing_property(codes: &[u32], context: &str) {
    assert!(
        !codes.contains(&diagnostic_codes::PROPERTY_DOES_NOT_EXIST_ON_TYPE),
        "{context}: expected inherited Array<T> members to resolve, got {codes:?}",
    );
}

/// Core witness: a six-level same-file interface chain terminating in
/// `extends Array<T>`. Below the fix, the sixth nested heritage-merge call
/// (processing `I6 extends Array<T>`) hit the depth guard and returned only
/// `I6`'s own member, dropping every `Array` member from `I1` on up.
#[test]
fn six_level_chain_to_array_base_resolves_inherited_members() {
    let codes = strict_codes_with_libs(
        r#"
        interface I1<T> extends I2<T> { own1(): void }
        interface I2<T> extends I3<T> { own2(): void }
        interface I3<T> extends I4<T> { own3(): void }
        interface I4<T> extends I5<T> { own4(): void }
        interface I5<T> extends I6<T> { own5(): void }
        interface I6<T> extends Array<T> { own6(): void }

        declare const x: I1<number>;
        x.map((v) => v);
        x.slice(0);
        x.own1();
        "#,
    );
    assert_no_missing_property(&codes, "six-level chain");
}

/// Negative control: five levels sits exactly at the pre-fix boundary (the
/// fifth call, processing `I5 extends Array<T>`, saw a depth of 4 and never
/// tripped the old `>= 5` guard). Must stay clean both before and after the
/// fix — this pins the boundary itself, not just the regression side.
#[test]
fn five_level_chain_to_array_base_resolves_inherited_members() {
    let codes = strict_codes_with_libs(
        r#"
        interface I1<T> extends I2<T> { own1(): void }
        interface I2<T> extends I3<T> { own2(): void }
        interface I3<T> extends I4<T> { own3(): void }
        interface I4<T> extends I5<T> { own4(): void }
        interface I5<T> extends Array<T> { own5(): void }

        declare const x: I1<number>;
        x.map((v) => v);
        x.own1();
        "#,
    );
    assert_no_missing_property(&codes, "five-level chain (pre-fix boundary control)");
}

/// Renamed-binder control: identical shape, different identifiers, so no
/// binder name is load-bearing for the fix.
#[test]
fn six_level_chain_renamed_binders_resolves_inherited_members() {
    let codes = strict_codes_with_libs(
        r#"
        interface ChainRoot<T> extends ChainB<T> { rootMember(): void }
        interface ChainB<T> extends ChainC<T> { bMember(): void }
        interface ChainC<T> extends ChainD<T> { cMember(): void }
        interface ChainD<T> extends ChainE<T> { dMember(): void }
        interface ChainE<T> extends ChainLeaf<T> { eMember(): void }
        interface ChainLeaf<T> extends Array<T> { leafMember(): void }

        declare const y: ChainRoot<string>;
        y.forEach((v) => v);
        y.rootMember();
        "#,
    );
    assert_no_missing_property(&codes, "renamed-binder six-level chain");
}

/// A much deeper (well beyond the new 50-level bound is not exercised here,
/// but comfortably beyond the old 5-level one) chain must still resolve, so
/// the fix is not itself narrowly tuned to exactly six levels.
#[test]
fn twelve_level_chain_to_array_base_resolves_inherited_members() {
    let codes = strict_codes_with_libs(
        r#"
        interface L1<T> extends L2<T> {}
        interface L2<T> extends L3<T> {}
        interface L3<T> extends L4<T> {}
        interface L4<T> extends L5<T> {}
        interface L5<T> extends L6<T> {}
        interface L6<T> extends L7<T> {}
        interface L7<T> extends L8<T> {}
        interface L8<T> extends L9<T> {}
        interface L9<T> extends L10<T> {}
        interface L10<T> extends L11<T> {}
        interface L11<T> extends L12<T> {}
        interface L12<T> extends Array<T> {}

        declare const z: L1<boolean>;
        z.map((v) => v);
        "#,
    );
    assert_no_missing_property(&codes, "twelve-level chain");
}

/// Negative control: a member that genuinely does not exist anywhere in the
/// chain (own members or the inherited `Array<T>` base) must still be
/// reported. The depth-guard fix must not turn off TS2339 wholesale.
#[test]
fn six_level_chain_still_reports_genuinely_missing_member() {
    let codes = strict_codes_with_libs(
        r#"
        interface I1<T> extends I2<T> { own1(): void }
        interface I2<T> extends I3<T> { own2(): void }
        interface I3<T> extends I4<T> { own3(): void }
        interface I4<T> extends I5<T> { own4(): void }
        interface I5<T> extends I6<T> { own5(): void }
        interface I6<T> extends Array<T> { own6(): void }

        declare const x: I1<number>;
        x.thisMemberDoesNotExistAnywhere();
        "#,
    );
    assert!(
        codes.contains(&diagnostic_codes::PROPERTY_DOES_NOT_EXIST_ON_TYPE),
        "expected a genuinely missing member to still report TS2339, got {codes:?}",
    );
}

/// Concrete (non-generic) base control: the same chain shape with a
/// concrete `extends Array<string>` leaf instead of a passed-through type
/// parameter.
#[test]
fn six_level_chain_concrete_array_base_resolves_inherited_members() {
    let codes = strict_codes_with_libs(
        r#"
        interface I1 extends I2 { own1(): void }
        interface I2 extends I3 { own2(): void }
        interface I3 extends I4 { own3(): void }
        interface I4 extends I5 { own4(): void }
        interface I5 extends I6 { own5(): void }
        interface I6 extends Array<string> { own6(): void }

        declare const x: I1;
        x.map((v) => v);
        x.own1();
        "#,
    );
    assert_no_missing_property(&codes, "six-level chain, concrete Array<string> base");
}
