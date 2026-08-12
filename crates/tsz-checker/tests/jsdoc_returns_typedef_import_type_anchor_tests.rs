//! #17193: two JSDoc TS2694 residuals left after #17184/#17194 gave `@type`
//! and `@param` a precise member-token anchor for a failed (non-`typeof`)
//! `import("./mod").Member` reference.
//!
//! Structural rule: every JSDoc tag whose type expression is a bare
//! `import("./mod").Member` reference resolves through
//! `resolve_jsdoc_import_type_member_result` directly and, on failure,
//! anchors TS2694 at the member-name token inside the comment — the same
//! rule `tsc` applies to every tag uniformly. `@returns`/`@return` and
//! `@typedef` still went through the generic `resolve_jsdoc_reference` ->
//! `resolve_jsdoc_import_type_reference` path, which anchors at whatever the
//! shared `jsdoc_typedef_anchor_pos` cell last held — observably a stale
//! value, not the failing tag's own position.
//!
//! A `@typedef` failure had a second, worse defect: `type_from_jsdoc_typedef_inner`
//! treated the resulting `None` body as "not a typedef at all" and skipped
//! registering it, so a later reference to the typedef name fell through to
//! plain name resolution and additionally reported a spurious TS2304 "Cannot
//! find name" — `tsc` keeps the (error-typed) alias visible after a failed
//! base-type resolution.
//!
//! Oracle: typescript@7.0.2.

use tsz_checker::context::CheckerOptions;
use tsz_checker::test_utils::check_multi_file;
use tsz_common::diagnostics::Diagnostic;

fn js_options() -> CheckerOptions {
    CheckerOptions {
        allow_js: true,
        check_js: true,
        strict: true,
        ..Default::default()
    }
}

fn check(files: &[(&str, &str)], entry: &str) -> Vec<Diagnostic> {
    check_multi_file(files, entry, js_options())
}

fn ts2694(files: &[(&str, &str)], entry: &str) -> Vec<(String, u32)> {
    check(files, entry)
        .into_iter()
        .filter(|d| d.code == 2694)
        .map(|d| (d.message_text, d.start))
        .collect()
}

fn all_codes(files: &[(&str, &str)], entry: &str) -> Vec<u32> {
    check(files, entry).into_iter().map(|d| d.code).collect()
}

const TYPES_MOD: (&str, &str) = ("types.js", "export const X = 1;\n");

/// A standalone function's `@param` + `@returns` both reference the same
/// unresolvable `import(...).Missing` member. `tsc` reports two diagnostics,
/// one per tag, each at its own member-name token. Before this fix `@param`
/// anchored correctly (post-#17194) but `@returns` fell through to the
/// coarse `jsdoc_typedef_anchor_pos` fallback and anchored past EOF.
#[test]
fn function_param_and_returns_each_anchor_at_own_member_token() {
    let entry = "/** @param {import('./types.js').Missing} p\n * @returns {import('./types.js').Missing}\n */\nfunction h(p) { return p; }\n";
    let mut msgs = ts2694(&[TYPES_MOD, ("main.js", entry)], "main.js");
    msgs.sort_by_key(|(_, start)| *start);
    let param_offset = entry.find("Missing").unwrap() as u32;
    let returns_offset = entry.rfind("Missing").unwrap() as u32;
    assert_ne!(param_offset, returns_offset);
    assert_eq!(
        msgs,
        vec![
            (
                "Namespace '\"types\"' has no exported member 'Missing'.".to_string(),
                param_offset,
            ),
            (
                "Namespace '\"types\"' has no exported member 'Missing'.".to_string(),
                returns_offset,
            ),
        ],
    );
}

/// The singular `@return` spelling gets the same precise anchor, on a class
/// method (a distinct call site — `call_signature_from_method_internal`).
#[test]
fn method_return_singular_tag_anchors_at_member_token() {
    let entry =
        "class C {\n  /** @return {import('./types.js').Missing}\n   */\n  m() { return 1; }\n}\n";
    let msgs = ts2694(&[TYPES_MOD, ("main.js", entry)], "main.js");
    let offset = entry.find("Missing").unwrap() as u32;
    assert_eq!(
        msgs,
        vec![(
            "Namespace '\"types\"' has no exported member 'Missing'.".to_string(),
            offset,
        )],
    );
}

/// `@typedef {import(...).Missing}` anchors at the member token, not the
/// `/**` comment start.
#[test]
fn typedef_base_type_anchors_at_member_token() {
    let entry = "/** @typedef {import('./types.js').Missing} T */\n/** @type {T} */\nlet b;\n";
    let msgs = ts2694(&[TYPES_MOD, ("main.js", entry)], "main.js");
    let offset = entry.find("Missing").unwrap() as u32;
    assert_eq!(
        msgs,
        vec![(
            "Namespace '\"types\"' has no exported member 'Missing'.".to_string(),
            offset,
        )],
    );
}

/// A qualified (multi-segment) `@typedef` base type gets the same anchor.
#[test]
fn typedef_qualified_base_type_anchors_at_member_token() {
    let entry = "/** @typedef {import('./types.js').Missing.Deep} T */\n/** @type {T} */\nlet b;\n";
    let msgs = ts2694(&[TYPES_MOD, ("main.js", entry)], "main.js");
    let offset = entry.find("Missing").unwrap() as u32;
    assert_eq!(
        msgs,
        vec![(
            "Namespace '\"types\"' has no exported member 'Missing'.".to_string(),
            offset,
        )],
    );
}

/// A failed `@typedef` import-type base type must NOT cascade into a
/// spurious TS2304 on later references to the typedef name — `tsc` keeps
/// the (error-typed) alias visible. Exactly one diagnostic (the TS2694),
/// matching the oracle exit shape.
#[test]
fn typedef_failed_base_type_does_not_report_cannot_find_name() {
    let entry = "/** @typedef {import('./types.js').Missing} T */\n/** @type {T} */\nlet b;\n";
    let codes = all_codes(&[TYPES_MOD, ("main.js", entry)], "main.js");
    assert_eq!(codes, vec![2694]);
}

/// Positive control: a real interface export resolves cleanly through both
/// `@typedef` and `@returns` — no diagnostics, no regression from the new
/// direct-resolution branches.
#[test]
fn resolvable_import_member_reports_nothing() {
    let shape_mod: (&str, &str) = ("shape.ts", "export interface Shape { n: number }\n");
    let entry = "/** @typedef {import('./shape.ts').Shape} T */\n/** @type {T} */\nlet b = { n: 1 };\n\n/** @returns {import('./shape.ts').Shape}\n */\nfunction h() { return { n: 1 }; }\n";
    let codes = all_codes(&[shape_mod, ("main.js", entry)], "main.js");
    assert_eq!(codes, Vec::<u32>::new());
}

/// Renamed binders: the member-token search keys off the tag's own literal
/// text, not any fixed identifier spelling.
#[test]
fn renamed_member_and_module_still_anchor_correctly() {
    let widgets_mod: (&str, &str) = ("widgets.js", "export const Gadget = 1;\n");
    let entry = "/** @returns {import('./widgets.js').Cog}\n */\nfunction spin() { return 1; }\n";
    let msgs = ts2694(&[widgets_mod, ("main.js", entry)], "main.js");
    let offset = entry.find("Cog").unwrap() as u32;
    assert_eq!(
        msgs,
        vec![(
            "Namespace '\"widgets\"' has no exported member 'Cog'.".to_string(),
            offset,
        )],
    );
}
