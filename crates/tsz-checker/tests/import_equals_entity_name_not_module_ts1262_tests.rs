//! An internal `import X = A.B` (entity-name reference) is a namespace alias,
//! not external module syntax, so it must not make its file a module. When it
//! wrongly did, `await`/`yield`-as-identifier at the top level became reserved
//! and tsz over-reported TS1262 on files tsc treats as scripts
//! (`conformance/externalModules/topLevelAwait.2.ts`: oracle emits nothing).
//!
//! Structural rule: tsc's `isAnExternalModuleIndicatorNode` counts an
//! `ImportEqualsDeclaration` only when `isExternalModuleReference` — the
//! `= require("...")` form. `= A.B` is not an indicator. Fixed in the binder's
//! `file_has_module_syntax_indicator`
//! (`crates/tsz-binder/src/state/core.rs`).

use tsz_checker::test_utils::check_source_codes;

const TS1262: u32 = 1262;

/// The reported witness: `import await = foo.await;` in a script keeps `await`
/// a legal top-level identifier — no TS1262.
#[test]
fn import_equals_entity_name_keeps_await_a_script_identifier() {
    let codes = check_source_codes(
        r#"
declare namespace foo { const await: any; }
import await = foo.await;
"#,
    );
    assert!(
        !codes.contains(&TS1262),
        "entity-name `import await = foo.await` must not make the file a module; got {codes:?}"
    );
}

/// The same file, but with an explicit module marker, IS a module — so `await`
/// bound at the top level is reserved and TS1262 fires. Proves the fix keys on
/// module-ness, not on suppressing the check.
#[test]
fn export_marker_alongside_entity_name_import_equals_reserves_await() {
    let codes = check_source_codes(
        r#"
export {};
declare namespace foo { const await: any; }
import await = foo.await;
"#,
    );
    assert!(
        codes.contains(&TS1262),
        "with `export {{}}` the file is a module, so top-level `await` is reserved; got {codes:?}"
    );
}

/// A neutral alias name proves the rule is structural: an entity-name
/// `import =` contributes no module syntax, so a plain top-level `await`
/// binding elsewhere stays a legal script identifier.
#[test]
fn entity_name_import_equals_does_not_reserve_a_sibling_await_binding() {
    let codes = check_source_codes(
        r#"
namespace ns { export const value = 1; }
import alias = ns.value;
var await = alias;
"#,
    );
    assert!(
        !codes.contains(&TS1262),
        "an entity-name `import =` leaves the file a script; `await` stays legal; got {codes:?}"
    );
}
