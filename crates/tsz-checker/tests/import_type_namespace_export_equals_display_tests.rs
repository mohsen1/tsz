//! Regression tests for TS2694 namespace display when navigating `export =`
//! modules via `import("./mod").Bar.Q` import-type expressions.
//!
//! When a TypeScript module uses `export =` (CommonJS-style), import-type
//! access paths must NOT include the synthetic `.export=` hop in the
//! user-facing namespace path.  tsc produces:
//!
//!   `Namespace '"mod".Bar' has no exported member 'Q'.`
//!
//! Without the fix, tsz produced:
//!
//!   `Namespace '"mod".export=.Bar' has no exported member 'Q'.`
//!
//! Root: `import_type_namespace_name_with_segments` was incorrectly
//! appending the `.export=` qualifier even for paths that had already
//! resolved through the export-equals object.  The no-segments variant
//! (`import_type_namespace_name`) correctly retains `.export=` for the
//! case where the FIRST access fails directly on the export-equals object.

use tsz_checker::context::CheckerOptions;
use tsz_common::common::ModuleKind;

fn check_import_type(files: &[(&str, &str)], entry_idx: usize) -> Vec<(u32, String)> {
    let entry_file = files[entry_idx].0;
    tsz_checker::test_utils::check_multi_file(
        files,
        entry_file,
        CheckerOptions {
            module: ModuleKind::CommonJS,
            strict: true,
            strict_null_checks: true,
            ..CheckerOptions::default()
        },
    )
    .into_iter()
    .map(|diag| (diag.code, diag.message_text))
    .collect()
}

/// When a module uses `export =` and an import-type expression navigates
/// through a resolved member (`Bar`) to a missing member (`Q`), the TS2694
/// namespace display must show `"mod".Bar` not `"mod".export=.Bar`.
#[test]
fn import_type_nested_access_on_export_equals_module_omits_export_equals_in_ts2694() {
    let mod_source = r#"
export = {
    Bar: {
        method(): void {},
    },
};
"#;

    let consumer_source = r#"
// `Bar` exists in mod's export= object; `Q` does not exist on Bar.
type Missing = import('./mod').Bar.Q;
"#;

    let diagnostics = check_import_type(
        &[("mod.ts", mod_source), ("consumer.ts", consumer_source)],
        1,
    );

    let ts2694_messages: Vec<&str> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2694)
        .map(|(_, msg)| msg.as_str())
        .collect();

    assert!(
        !ts2694_messages.is_empty(),
        "Expected TS2694 for missing member Q on Bar; got: {diagnostics:?}"
    );

    for msg in &ts2694_messages {
        assert!(
            !msg.contains(".export="),
            "TS2694 namespace must not include synthetic `.export=` hop when \
             member was reached through resolved segments; got: {msg}"
        );
        assert!(
            msg.contains(".Bar"),
            "TS2694 namespace should include the resolved segment 'Bar'; got: {msg}"
        );
    }
}

/// Sibling lock: when the FIRST access fails directly on the export= object
/// (no resolved segments), the `.export=` qualifier IS shown.  This matches
/// tsc's behavior for `import("mod").Thing` where Thing is not a property of
/// the exported object.
#[test]
fn import_type_direct_access_on_export_equals_module_shows_export_equals_in_ts2694() {
    let mod_source = r#"
export = {
    Bar: {
        method(): void {},
    },
};
"#;

    let consumer_source = r#"
// `Thing` does not exist directly on mod's export= object.
type Missing = import('./mod').Thing;
"#;

    let diagnostics = check_import_type(
        &[("mod.ts", mod_source), ("consumer.ts", consumer_source)],
        1,
    );

    let ts2694_messages: Vec<&str> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2694)
        .map(|(_, msg)| msg.as_str())
        .collect();

    assert!(
        !ts2694_messages.is_empty(),
        "Expected TS2694 for missing direct member Thing; got: {diagnostics:?}"
    );

    for msg in &ts2694_messages {
        assert!(
            msg.contains(".export="),
            "TS2694 namespace should include `.export=` when member fails directly \
             on the export-equals object; got: {msg}"
        );
    }
}
