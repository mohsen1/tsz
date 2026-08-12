//! A JSDoc `@type`/`@param`/`@returns` annotation whose `import('mod').Member`
//! fails to resolve must report **TS2694 exactly once, at the member-name
//! token** — matching `tsc`.
//!
//! tsz resolved the JSDoc type string twice (the comment-scan validation pass
//! AND the lazy type computation of the annotated symbol) and emitted the
//! diagnostic from each, at two coarse/wrong anchors (the `/**` comment start
//! and the declaration). The fix routes the single diagnostic through the
//! comment-scan pass, anchored at each tag's own member token, and silences the
//! lazy re-resolutions (issue #17176).

use tsz_checker::context::CheckerOptions;
use tsz_checker::test_utils::check_multi_file;

const TYPES_JS: &str = "class Widget { size() { return 2; } }\nmodule.exports.Widget = Widget;\n";

fn check(main: &str) -> Vec<(u32, u32)> {
    let options = CheckerOptions {
        strict: true,
        allow_js: true,
        check_js: true,
        ..Default::default()
    };
    check_multi_file(
        &[("types.js", TYPES_JS), ("main.js", main)],
        "main.js",
        options,
    )
    .into_iter()
    .filter(|d| d.code == 2694)
    .map(|d| (d.start, d.length))
    .collect()
}

/// Byte offset + length of the Nth (0-based) `Missing` token in `main`.
fn missing_at(main: &str, nth: usize) -> (u32, u32) {
    let mut from = 0usize;
    for _ in 0..nth {
        from = main[from..].find("Missing").expect("occurrence") + from + 1;
    }
    let pos = main[from..].find("Missing").expect("occurrence") + from;
    (pos as u32, "Missing".len() as u32)
}

/// `@type` on a declaration: one TS2694, at the member token in the comment.
#[test]
fn type_tag_import_member_reported_once_at_member_token() {
    let main = "/** @type {import('./types.js').Missing} */\nlet w;\n";
    assert_eq!(check(main), vec![missing_at(main, 0)]);
}

/// `@param`: one TS2694, at the member token.
#[test]
fn param_tag_import_member_reported_once_at_member_token() {
    let main =
        "/**\n * @param {import('./types.js').Missing} x\n */\nfunction f(x) { return x; }\n";
    assert_eq!(check(main), vec![missing_at(main, 0)]);
}

/// `@returns`: one TS2694, at the member token.
#[test]
fn returns_tag_import_member_reported_once_at_member_token() {
    let main = "/**\n * @returns {import('./types.js').Missing}\n */\nfunction g() { return undefined; }\n";
    assert_eq!(check(main), vec![missing_at(main, 0)]);
}

/// Two separate `@type` annotations: one TS2694 each, at their own member
/// tokens — proving the fix is per-annotation, not deduped to a single site.
#[test]
fn two_type_annotations_report_two_diagnostics_at_distinct_tokens() {
    let main = "/** @type {import('./types.js').Missing} */\nlet a;\n\
                /** @type {import('./types.js').Missing} */\nlet b;\n";
    let mut got = check(main);
    got.sort_unstable();
    let mut want = vec![missing_at(main, 0), missing_at(main, 1)];
    want.sort_unstable();
    assert_eq!(got, want);
}

/// `@param` and `@returns` carrying the identical type in one comment: each is
/// anchored at its own member token (the same-type-in-one-comment case that a
/// coarse comment anchor collapses).
#[test]
fn param_and_returns_same_type_anchor_at_their_own_tokens() {
    let main = "/**\n * @param {import('./types.js').Missing} x\n \
                * @returns {import('./types.js').Missing}\n */\nfunction h(x) { return x; }\n";
    let mut got = check(main);
    got.sort_unstable();
    let mut want = vec![missing_at(main, 0), missing_at(main, 1)];
    want.sort_unstable();
    assert_eq!(got, want);
}

/// Structural, not name-based: a renamed missing member still reports once at
/// its token.
#[test]
fn renamed_missing_member_reported_once() {
    let main = "/** @type {import('./types.js').Nonexistent} */\nlet q;\n";
    let pos = main.find("Nonexistent").unwrap() as u32;
    let got: Vec<(u32, u32)> = {
        let options = CheckerOptions {
            strict: true,
            allow_js: true,
            check_js: true,
            ..Default::default()
        };
        check_multi_file(
            &[("types.js", TYPES_JS), ("main.js", main)],
            "main.js",
            options,
        )
        .into_iter()
        .filter(|d| d.code == 2694)
        .map(|d| (d.start, d.length))
        .collect()
    };
    assert_eq!(got, vec![(pos, "Nonexistent".len() as u32)]);
}
