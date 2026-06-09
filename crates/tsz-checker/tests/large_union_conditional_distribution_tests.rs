//! Regression coverage: distributive conditional types over large unions.
//!
//! `distribute_conditional` (and its instantiation-path twin) capped union
//! distribution at `100` members and returned `TypeId::ERROR` past that point.
//! For a key space larger than the cap this silently dropped the conditional's
//! result, so a distributive `Exclude`-style conditional collapsed to `ERROR`
//! and an intersection `Html & Pick<Svg, Exclude<keyof Svg, keyof Html>>`
//! degraded to just `Html`. The downstream effect was a *false negative*:
//! `Html[T]` with `T extends keyof (that intersection)` no longer saw the
//! Svg-only keys, so the `TS2536` ("Type 'T' cannot be used to index type
//! 'Html'") that `tsc` reports was dropped purely because the union crossed the
//! size threshold.
//!
//! Structural rule: distributing a conditional over a union of N members must
//! produce the same result for N just below and just above the old `100` cap.
//! The cap now mirrors `DEFAULT_MAX_MAPPED_KEYS` (native 500) so realistic key
//! spaces (DOM tag maps, generated SDK surfaces, large literal enums) evaluate
//! like `tsc` instead of bailing.
//!
//! The fixtures define their own distributive conditional / mapped helpers
//! (`Excl`, `Pck`) rather than the lib `Exclude`/`Pick` so the test is
//! independent of lib loading (`check_source` runs without lib contexts) while
//! exercising the exact same `distribute_conditional` path. Verified against
//! `tsc` 6.0.3: the >100-member shapes report the same diagnostics as the
//! <100-member shapes.

use tsz_checker::context::CheckerOptions;
use tsz_checker::test_utils::{check_source, diagnostic_codes};

fn codes(source: &str) -> Vec<u32> {
    let options = CheckerOptions {
        strict: true,
        ..CheckerOptions::default()
    };
    diagnostic_codes(&check_source(source, "test.ts", options))
}

fn count(source: &str, code: u32) -> usize {
    codes(source).into_iter().filter(|&c| c == code).count()
}

/// Distributive `Exclude` and homomorphic `Pick`, defined locally so the test
/// does not depend on lib utility types.
const HELPERS: &str = "type Excl<T, U> = T extends U ? never : T;\n\
                       type Pck<T, K extends keyof T> = { [P in K]: T[P] };\n";

/// `interface Html` with `html` keys and `interface Svg` with the same `html`
/// keys plus `svg_only` distinct keys, then the deprecated-`lib.dom` shape
/// `ElMap = Html & Pck<Svg, Excl<keyof Svg, keyof Html>>`.
///
/// `keyof ElMap` is therefore `keyof Html | <svg-only keys>`, a strict superset
/// of `keyof Html`, so `Html[T]` for `T extends keyof ElMap` must report TS2536.
/// The `Excl` here distributes over `keyof Svg` (`html + svg_only` members).
fn element_tag_map_source(html: usize, svg_only: usize) -> String {
    let mut s = String::from(HELPERS);
    s.push_str("interface Html {\n");
    for i in 0..html {
        s.push_str(&format!("  \"h{i}\": {i};\n"));
    }
    s.push_str("}\n");
    s.push_str("interface Svg {\n");
    for i in 0..html {
        s.push_str(&format!("  \"h{i}\": {i};\n"));
    }
    for i in 0..svg_only {
        s.push_str(&format!("  \"s{i}\": {i};\n"));
    }
    s.push_str("}\n");
    s.push_str(
        "type ElMap = Html & Pck<Svg, Excl<keyof Svg, keyof Html>>;\n\
         type Q<T extends keyof ElMap> = Html[T];\n",
    );
    s
}

/// A literal-string union `"m0" | "m1" | ... | "m{n-1}"`.
fn literal_union(n: usize) -> String {
    let mut s = String::from("type Big =\n");
    for i in 0..n {
        s.push_str(&format!("  | \"m{i}\"\n"));
    }
    s.push_str(";\n");
    s
}

#[test]
fn small_union_reports_ts2536_through_pick_exclude() {
    // Control: `keyof Svg` has 50 members, comfortably under the old 100 cap.
    let src = element_tag_map_source(20, 30);
    assert_eq!(
        count(&src, 2536),
        1,
        "small key space must report the indexing TS2536: {:?}",
        codes(&src)
    );
}

#[test]
fn large_union_reports_ts2536_through_pick_exclude() {
    // `keyof Svg` has 170 members, so `Excl<keyof Svg, keyof Html>` exceeded the
    // old `100` cap and collapsed `ElMap` down to `Html`, dropping the TS2536.
    // It must now be reported just like the small case.
    let src = element_tag_map_source(20, 150);
    assert_eq!(
        count(&src, 2536),
        1,
        "large key space must still report the indexing TS2536 \
         (distribution must not collapse to ERROR): {:?}",
        codes(&src)
    );
}

#[test]
fn large_union_exclude_keeps_membership_no_false_positive() {
    // A *valid* use of a distributive conditional over a >100-member union must
    // not invent a diagnostic: every retained member is assignable to the result.
    let mut s = literal_union(150);
    s.push_str("type Rest = ");
    s.push_str("Big extends infer B ? (B extends \"m0\" ? never : B) : never;\n");
    s.push_str("const ok: Rest = \"m149\";\n");
    assert_eq!(
        count(&s, 2322),
        0,
        "valid assignment into a distributed conditional over a large union \
         must not error: {:?}",
        codes(&s)
    );
}

#[test]
fn large_union_exclude_rejects_removed_member() {
    // The complement: the removed member is genuinely gone from the result, so
    // assigning it back is a real TS2322. This guards against the cap silently
    // widening the result to `string`/the original union.
    let mut s = String::from("type Excl<T, U> = T extends U ? never : T;\n");
    s.push_str(&literal_union(150));
    s.push_str("type Rest = Excl<Big, \"m0\">;\n");
    s.push_str("const bad: Rest = \"m0\";\n");
    assert_eq!(
        count(&s, 2322),
        1,
        "assigning the excluded member back must error TS2322: {:?}",
        codes(&s)
    );
}
