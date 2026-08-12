//! #17235: `export = J` where `J` is `import type J = require("./c")` and
//! `./c` `export =`s a value-carrying merged namespace must report **TS1283**
//! ("resolves to a type-only declaration"), not TS1282 ("only refers to a
//! type").
//!
//! Structural rule: a whole-module `import X = require("mod")` targets the
//! module's `export =` assignment, not a member named `X`. `check_vms_export_equals`
//! resolved the *local alias name* as the export name, so the target's value
//! was only found when the alias name coincidentally matched the target's own
//! name. A renamed type-only alias therefore missed the value and mis-picked
//! TS1282. The fix resolves the `export=` key for whole-module require aliases.
//!
//! Oracle: `typescript@7.0.2`, `verbatimModuleSyntaxNoElisionCJS.ts`.

use crate::context::CheckerOptions;
use crate::diagnostics::Diagnostic;
use crate::test_utils::check_multi_file_with_global_index;
use tsz_common::common::ModuleKind;

const EXPORT_EQUALS_ONLY_A_TYPE_TS1282: u32 = 1282;
const EXPORT_EQUALS_REAL_VALUE_TS1283: u32 = 1283;

fn codes(files: &[(&str, &str)], entry: &str) -> Vec<u32> {
    let opts = CheckerOptions {
        module: ModuleKind::CommonJS,
        strict: true,
        verbatim_module_syntax: true,
        ..CheckerOptions::default()
    };
    let mut v: Vec<u32> = check_multi_file_with_global_index(files, entry, opts)
        .iter()
        .map(|d: &Diagnostic| d.code)
        .collect();
    v.sort_unstable();
    v
}

/// A merged interface+namespace whose `export =` therefore carries a runtime
/// value, imported type-only under a *renamed* alias, then re-`export =`d.
const VALUE_NS_MODULE: &str =
    "interface I {}\nnamespace I {\n    export const x = 1;\n}\nexport = I;\n";

#[test]
fn type_only_require_of_value_namespace_export_equals_reports_ts1283() {
    let d = "import I = require(\"./c\");\nimport type J = require(\"./c\");\nexport = J;\n";
    let got = codes(&[("/c.ts", VALUE_NS_MODULE), ("/d.ts", d)], "/d.ts");
    assert!(
        got.contains(&EXPORT_EQUALS_REAL_VALUE_TS1283),
        "expected TS1283 on `export = J`, got: {got:?}"
    );
    assert!(
        !got.contains(&EXPORT_EQUALS_ONLY_A_TYPE_TS1282),
        "must not mis-pick TS1282 when the export= target carries a value, got: {got:?}"
    );
}

// The rule must not depend on the alias name matching the target name: rename
// both the module member and the alias so no coincidence can mask the fix.
#[test]
fn renamed_type_only_require_alias_still_reports_ts1283() {
    let m = "interface Shape {}\nnamespace Shape {\n    export const y = 2;\n}\nexport = Shape;\n";
    let d = "import type Alias = require(\"./m\");\nexport = Alias;\n";
    let got = codes(&[("/m.ts", m), ("/d.ts", d)], "/d.ts");
    assert!(
        got.contains(&EXPORT_EQUALS_REAL_VALUE_TS1283),
        "expected TS1283 for a renamed type-only require alias, got: {got:?}"
    );
    assert!(
        !got.contains(&EXPORT_EQUALS_ONLY_A_TYPE_TS1282),
        "renamed alias must not fall back to TS1282, got: {got:?}"
    );
}

// A class `export =` target is unambiguously a value; same TS1283 outcome.
#[test]
fn type_only_require_of_class_export_equals_reports_ts1283() {
    let m = "class C {}\nexport = C;\n";
    let d = "import type K = require(\"./m\");\nexport = K;\n";
    let got = codes(&[("/m.ts", m), ("/d.ts", d)], "/d.ts");
    assert!(
        got.contains(&EXPORT_EQUALS_REAL_VALUE_TS1283),
        "expected TS1283 for a type-only require of a class export=, got: {got:?}"
    );
}

// Control: when the `export =` target is a *pure type* (interface only, no
// value), the type-only require must still pick TS1282, not TS1283.
#[test]
fn type_only_require_of_pure_type_export_equals_reports_ts1282() {
    let m = "interface P {}\nexport = P;\n";
    let d = "import type Q = require(\"./m\");\nexport = Q;\n";
    let got = codes(&[("/m.ts", m), ("/d.ts", d)], "/d.ts");
    assert!(
        got.contains(&EXPORT_EQUALS_ONLY_A_TYPE_TS1282),
        "a pure-type export= target must stay TS1282, got: {got:?}"
    );
    assert!(
        !got.contains(&EXPORT_EQUALS_REAL_VALUE_TS1283),
        "no value target means no TS1283, got: {got:?}"
    );
}
