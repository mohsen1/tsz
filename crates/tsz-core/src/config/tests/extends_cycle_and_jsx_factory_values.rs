//! TS18000 (`Circularity detected while resolving configuration: {0}`), a
//! config-loader code that had zero emit sites (#16291), plus follow-on
//! coverage for TS18035 (`Invalid value for 'jsxFragmentFactory'. ...`).
//!
//! TS18000 is a *recovery* rule, not just a message: tsc's `parseConfig`
//! consults its resolution stack on entry, reports the code once, and returns
//! an empty config for the cyclic base, so the surviving part of the chain
//! still merges and the program still loads and checks. tsz previously
//! detected the cycle and aborted the whole config load with a non-tsc error.
//!
//! The TS18035 emit site itself landed separately in #16291's enumeration
//! (see `module_resolution.rs` for the base valid/invalid pair and the TS5052
//! dependency). The cases kept here are the ones that pair it against its
//! `jsxFactory`/TS5067 sibling: both-invalid reports both codes, the negative
//! direction, the value-span substring hazard between the two option keys,
//! and arrival through `extends`.
//!
//! Every expectation below is pinned against the real `typescript@7.0.2`
//! binary, including the unsubstituted `{0}` in TS18000's message, which is
//! what tsc itself emits.
//!
//! Split from `config/mod.rs` to keep each file under the 2000-line limit
//! (§19; ratchet tracked by #8280).

use super::super::*;
use tempfile::tempdir;

fn write(dir: &std::path::Path, name: &str, contents: &str) -> std::path::PathBuf {
    let path = dir.join(name);
    std::fs::write(&path, contents).expect("write config");
    path
}

fn codes(parsed: &ParsedTsConfig) -> Vec<u32> {
    parsed.diagnostics.iter().map(|d| d.code).collect()
}

// ---------------------------------------------------------------------------
// TS18000 — `extends` circularity
// ---------------------------------------------------------------------------

#[test]
fn self_extends_cycle_reports_ts18000_and_recovers() {
    let temp = tempdir().expect("create temp dir");
    let path = write(
        temp.path(),
        "tsconfig.json",
        r#"{"extends": "./tsconfig.json", "files": ["a.ts"]}"#,
    );

    let parsed = load_tsconfig_with_diagnostics(&path).expect("cycle must not abort the load");
    assert!(
        codes(&parsed).contains(&18000),
        "expected TS18000 for a self-extends cycle, got: {:?}",
        codes(&parsed)
    );
    // Recovery: the entry config's own `files` still survives the cycle.
    assert_eq!(
        parsed.config.files.as_deref(),
        Some(&["a.ts".to_string()][..]),
        "the entry config's own options must survive an extends cycle"
    );
}

#[test]
fn two_config_extends_cycle_reports_ts18000_once() {
    let temp = tempdir().expect("create temp dir");
    write(temp.path(), "b.json", r#"{"extends": "./tsconfig.json"}"#);
    let path = write(
        temp.path(),
        "tsconfig.json",
        r#"{"extends": "./b.json", "files": ["a.ts"]}"#,
    );

    let parsed = load_tsconfig_with_diagnostics(&path).expect("cycle must not abort the load");
    assert_eq!(
        codes(&parsed).iter().filter(|&&c| c == 18000).count(),
        1,
        "tsc reports the circularity exactly once, got: {:?}",
        codes(&parsed)
    );
}

#[test]
fn three_config_extends_cycle_reports_ts18000_once() {
    let temp = tempdir().expect("create temp dir");
    write(
        temp.path(),
        "b.json",
        r#"{"extends": "./c.json", "compilerOptions": {"noUnusedLocals": true}}"#,
    );
    write(temp.path(), "c.json", r#"{"extends": "./b.json"}"#);
    let path = write(
        temp.path(),
        "tsconfig.json",
        r#"{"extends": "./b.json", "files": ["a.ts"]}"#,
    );

    let parsed = load_tsconfig_with_diagnostics(&path).expect("cycle must not abort the load");
    assert_eq!(
        codes(&parsed).iter().filter(|&&c| c == 18000).count(),
        1,
        "a longer cycle is still reported once, got: {:?}",
        codes(&parsed)
    );
    // The non-cyclic part of the chain still contributes its options.
    assert_eq!(
        parsed
            .config
            .compiler_options
            .as_ref()
            .and_then(|o| o.no_unused_locals),
        Some(true),
        "options from the surviving part of a cyclic chain must still merge"
    );
}

#[test]
fn extends_cycle_in_an_array_still_merges_the_sibling_base() {
    // tsc applies the surviving entries of an `extends` array normally; only
    // the cyclic entry contributes nothing.
    let temp = tempdir().expect("create temp dir");
    write(temp.path(), "b.json", r#"{"extends": "./tsconfig.json"}"#);
    write(
        temp.path(),
        "good.json",
        r#"{"compilerOptions": {"noUnusedLocals": true}}"#,
    );
    let path = write(
        temp.path(),
        "tsconfig.json",
        r#"{"extends": ["./b.json", "./good.json"], "files": ["a.ts"]}"#,
    );

    let parsed = load_tsconfig_with_diagnostics(&path).expect("cycle must not abort the load");
    assert!(
        codes(&parsed).contains(&18000),
        "expected TS18000 for the cyclic array entry, got: {:?}",
        codes(&parsed)
    );
    assert_eq!(
        parsed
            .config
            .compiler_options
            .as_ref()
            .and_then(|o| o.no_unused_locals),
        Some(true),
        "the non-cyclic sibling base must still be applied"
    );
}

#[test]
fn ts18000_message_keeps_tsc_unsubstituted_placeholder_and_is_not_file_anchored() {
    let temp = tempdir().expect("create temp dir");
    let path = write(
        temp.path(),
        "tsconfig.json",
        r#"{"extends": "./tsconfig.json", "files": ["a.ts"]}"#,
    );

    let parsed = load_tsconfig_with_diagnostics(&path).expect("cycle must not abort the load");
    let diag = parsed
        .diagnostics
        .iter()
        .find(|d| d.code == 18000)
        .expect("TS18000 present");
    // tsc builds this diagnostic without passing the resolution stack, so the
    // placeholder survives into the rendered message. Matching tsc means
    // reproducing that rather than substituting a path.
    assert_eq!(
        diag.message_text,
        "Circularity detected while resolving configuration: {0}"
    );
    // `createCompilerDiagnostic` — global, so it renders with no `file(l,c):`
    // prefix.
    assert!(
        diag.file.is_empty(),
        "TS18000 is a global diagnostic, got file {:?}",
        diag.file
    );
}

#[test]
fn diamond_extends_sharing_one_ancestor_is_not_a_cycle() {
    // The negative case that a naive global "seen" set would break: two bases
    // legitimately reach the same ancestor. `visited` is a path stack popped
    // on the way out, so this must stay clean and still inherit.
    let temp = tempdir().expect("create temp dir");
    write(
        temp.path(),
        "d.json",
        r#"{"compilerOptions": {"noUnusedLocals": true}}"#,
    );
    write(temp.path(), "b.json", r#"{"extends": "./d.json"}"#);
    write(temp.path(), "c.json", r#"{"extends": "./d.json"}"#);
    let path = write(
        temp.path(),
        "tsconfig.json",
        r#"{"extends": ["./b.json", "./c.json"], "files": ["a.ts"]}"#,
    );

    let parsed = load_tsconfig_with_diagnostics(&path).expect("load config");
    assert!(
        !codes(&parsed).contains(&18000),
        "a diamond is not a circularity, got: {:?}",
        codes(&parsed)
    );
    assert_eq!(
        parsed
            .config
            .compiler_options
            .as_ref()
            .and_then(|o| o.no_unused_locals),
        Some(true),
        "the shared ancestor must still be applied"
    );
}

#[test]
fn plain_extends_chain_without_a_cycle_stays_clean() {
    let temp = tempdir().expect("create temp dir");
    write(
        temp.path(),
        "b.json",
        r#"{"compilerOptions": {"noUnusedLocals": true}}"#,
    );
    let path = write(
        temp.path(),
        "tsconfig.json",
        r#"{"extends": "./b.json", "files": ["a.ts"]}"#,
    );

    let parsed = load_tsconfig_with_diagnostics(&path).expect("load config");
    assert!(
        !codes(&parsed).contains(&18000),
        "a plain chain must not report circularity, got: {:?}",
        codes(&parsed)
    );
}

// ---------------------------------------------------------------------------
// TS18035 — `jsxFragmentFactory` paired against its `jsxFactory`/TS5067 sibling
//
// The plain valid/invalid pair and the TS5052 dependency live in
// `module_resolution.rs`; these are the cross-option cases.
// ---------------------------------------------------------------------------

#[test]
fn valid_jsx_fragment_factory_spellings_are_clean() {
    // Bare identifier and dotted qualified name, both legal. The binder name
    // is varied so nothing can key off a particular spelling.
    for value in ["Fragment", "React.Fragment", "Preact.h.Frag", "_$weird1"] {
        let temp = tempdir().expect("create temp dir");
        let path = write(
            temp.path(),
            "tsconfig.json",
            &format!(
                r#"{{"compilerOptions": {{"jsx": "react", "jsxFactory": "h", "jsxFragmentFactory": "{value}"}}}}"#
            ),
        );

        let parsed = load_tsconfig_with_diagnostics(&path).expect("load config");
        assert!(
            !codes(&parsed).contains(&18035),
            "{value:?} is a valid qualified name, got: {:?}",
            codes(&parsed)
        );
    }
}

#[test]
fn both_jsx_factory_options_invalid_report_both_codes_independently() {
    // The two checks are independent branches in tsc, not an either/or.
    let temp = tempdir().expect("create temp dir");
    let path = write(
        temp.path(),
        "tsconfig.json",
        r#"{"compilerOptions": {"jsx": "react", "jsxFactory": "bad name!", "jsxFragmentFactory": "also bad!"}}"#,
    );

    let parsed = load_tsconfig_with_diagnostics(&path).expect("load config");
    let got = codes(&parsed);
    assert!(
        got.contains(&5067) && got.contains(&18035),
        "expected both TS5067 and TS18035, got: {got:?}"
    );
}

#[test]
fn invalid_jsx_factory_alone_does_not_report_ts18035() {
    // The negative direction: the new check must not fire off its sibling's
    // option.
    let temp = tempdir().expect("create temp dir");
    let path = write(
        temp.path(),
        "tsconfig.json",
        r#"{"compilerOptions": {"jsx": "react", "jsxFactory": "bad name!"}}"#,
    );

    let parsed = load_tsconfig_with_diagnostics(&path).expect("load config");
    let got = codes(&parsed);
    assert!(
        got.contains(&5067) && !got.contains(&18035),
        "expected TS5067 without TS18035, got: {got:?}"
    );
}

#[test]
fn ts18035_anchors_at_its_own_value_not_the_jsx_factory_value() {
    // `find_value_offset_in_source` searches the quoted key, so the
    // `"jsxFactory"` needle must not match inside `"jsxFragmentFactory"` —
    // and the two diagnostics must land on different spans even though one
    // key is a substring of the other.
    let temp = tempdir().expect("create temp dir");
    let source = r#"{"compilerOptions": {"jsx": "react", "jsxFactory": "bad name!", "jsxFragmentFactory": "also bad!"}}"#;
    let path = write(temp.path(), "tsconfig.json", source);

    let parsed = load_tsconfig_with_diagnostics(&path).expect("load config");
    let ts5067 = parsed
        .diagnostics
        .iter()
        .find(|d| d.code == 5067)
        .expect("TS5067 present");
    let ts18035 = parsed
        .diagnostics
        .iter()
        .find(|d| d.code == 18035)
        .expect("TS18035 present");

    assert_eq!(
        ts5067.start as usize,
        source.find(r#""bad name!""#).expect("jsxFactory value"),
        "TS5067 must anchor at the jsxFactory value"
    );
    assert_eq!(
        ts18035.start as usize,
        source.find(r#""also bad!""#).expect("fragment value"),
        "TS18035 must anchor at the jsxFragmentFactory value"
    );
}

#[test]
fn invalid_jsx_fragment_factory_inherited_from_a_base_still_reports() {
    // The value can arrive through `extends`; the check runs on the file that
    // literally writes the option, matching the already-wired TS5067 sibling.
    let temp = tempdir().expect("create temp dir");
    write(
        temp.path(),
        "b.json",
        r#"{"compilerOptions": {"jsx": "react", "jsxFactory": "h", "jsxFragmentFactory": "bad name!"}}"#,
    );
    let path = write(
        temp.path(),
        "tsconfig.json",
        r#"{"extends": "./b.json", "files": ["a.ts"]}"#,
    );

    let parsed = load_tsconfig_with_diagnostics(&path).expect("load config");
    assert!(
        codes(&parsed).contains(&18035),
        "expected TS18035 from the base config, got: {:?}",
        codes(&parsed)
    );
}

#[test]
fn absent_jsx_fragment_factory_is_clean() {
    let temp = tempdir().expect("create temp dir");
    let path = write(
        temp.path(),
        "tsconfig.json",
        r#"{"compilerOptions": {"jsx": "react", "jsxFactory": "h"}}"#,
    );

    let parsed = load_tsconfig_with_diagnostics(&path).expect("load config");
    assert!(
        !codes(&parsed).contains(&18035),
        "an absent option must not report, got: {:?}",
        codes(&parsed)
    );
}
