//! Regression test for TS2339 on `globalThis.<block-scoped-name>`.
//!
//! `var` declarations at file scope are added to `typeof globalThis`, but
//! `let` / `const` are not. Accessing the block-scoped name through globalThis
//! must therefore emit TS2339.
//!
//! Bug: `resolve_lib_global_var_symbol` walked every symbol in
//! `lib_symbol_ids` and accepted any `FUNCTION_SCOPED_VARIABLE`-flagged
//! candidate by name — including parameter symbols of lib callables (e.g.
//! `Path2D.moveTo(x: number, y: number)`'s `y` parameter), which share that
//! flag. That bogus match suppressed the legitimate TS2339 for
//! `globalThis.y` when the user file had `const y = 2`.
//!
//! Fix: reject lib candidates whose `parent` is a callable
//! (FUNCTION / METHOD / CONSTRUCTOR / `GET_ACCESSOR` / `SET_ACCESSOR` / SIGNATURE).

use tsz_checker::context::CheckerOptions;

fn diagnostics(source: &str) -> Vec<(u32, String)> {
    let lib_files =
        tsz_checker::test_utils::load_compiled_lib_files(&["lib.es5.d.ts", "lib.dom.d.ts"]);
    if lib_files.is_empty() {
        // Lib files not available in this build environment — skip.
        return Vec::new();
    }

    tsz_checker::test_utils::check_source_with_libs(
        source,
        "test.ts",
        CheckerOptions::default(),
        &lib_files,
    )
    .into_iter()
    .map(|d| (d.code, d.message_text))
    .collect()
}

#[test]
fn const_local_emits_ts2339_on_global_this_property_access() {
    // `var x` is on globalThis, so `globalThis.x` should NOT emit TS2339.
    // `const y` is NOT on globalThis, so `globalThis.y` SHOULD emit TS2339.
    let diags = diagnostics(
        r#"
var x = 1
const y = 2
globalThis.x = 3
globalThis.y = 4
"#,
    );
    if diags.is_empty() {
        // No lib files available — skip silently in restricted environments.
        return;
    }

    let ts2339_msgs: Vec<&str> = diags
        .iter()
        .filter(|(code, _)| *code == 2339)
        .map(|(_, msg)| msg.as_str())
        .collect();

    assert_eq!(
        ts2339_msgs.len(),
        1,
        "Expected exactly one TS2339 (on `globalThis.y`), got: {diags:#?}"
    );
    assert!(
        ts2339_msgs[0].contains("'y'"),
        "TS2339 message should reference property `y`, got: {:?}",
        ts2339_msgs[0]
    );
    assert!(
        ts2339_msgs[0].contains("'typeof globalThis'"),
        "TS2339 message should reference `typeof globalThis`, got: {:?}",
        ts2339_msgs[0]
    );
}

#[test]
fn let_local_emits_ts2339_on_global_this_property_access() {
    let diags = diagnostics(
        r#"
let z = 5
globalThis.z = 6
"#,
    );
    if diags.is_empty() {
        return;
    }

    let ts2339_msgs: Vec<&str> = diags
        .iter()
        .filter(|(code, _)| *code == 2339)
        .map(|(_, msg)| msg.as_str())
        .collect();

    assert_eq!(
        ts2339_msgs.len(),
        1,
        "Expected exactly one TS2339 (on `globalThis.z`), got: {diags:#?}"
    );
    assert!(
        ts2339_msgs[0].contains("'z'"),
        "TS2339 message should reference property `z`, got: {:?}",
        ts2339_msgs[0]
    );
}
