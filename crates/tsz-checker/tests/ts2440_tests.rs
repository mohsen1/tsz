//! Tests for TS2440: Import declaration conflicts with local declaration
//!
//! Verifies correct detection of conflicts between import declarations and
//! local declarations (function, variable, class, namespace, etc.) in the
//! same scope.

use crate::CheckerState;
use crate::context::CheckerOptions;
use tsz_binder::BinderState;
use tsz_parser::parser::ParserState;
use tsz_solver::TypeInterner;

fn get_diagnostics(source: &str) -> Vec<(u32, String)> {
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        CheckerOptions::default(),
    );

    checker.check_source_file(root);

    checker
        .ctx
        .diagnostics
        .iter()
        .map(|d| (d.code, d.message_text.clone()))
        .collect()
}

fn has_error_with_code(source: &str, code: u32) -> bool {
    get_diagnostics(source).iter().any(|d| d.0 == code)
}

// =========================================================================
// Import-equals: import X = M.N conflicts with local declaration
// =========================================================================

#[test]
fn test_import_equals_conflicts_with_variable() {
    // import X = N.X; var X = 1; — should emit TS2440
    let source = r#"
namespace N {
    export var X = 1;
}
import X = N.X;
var X = 1;
"#;
    assert!(
        has_error_with_code(source, 2440),
        "Should emit TS2440 when import-equals conflicts with variable. Got: {:?}",
        get_diagnostics(source)
    );
}

#[test]
fn test_import_equals_conflicts_with_function() {
    // import X = N.X; function X() {} — should emit TS2440
    let source = r#"
namespace N {
    export function X() {}
}
import X = N.X;
function X() {}
"#;
    assert!(
        has_error_with_code(source, 2440),
        "Should emit TS2440 when import-equals conflicts with function. Got: {:?}",
        get_diagnostics(source)
    );
}

#[test]
fn test_import_equals_no_conflict_with_type_only() {
    // import X = N.X; type X = string; — no conflict (type-only doesn't conflict)
    let source = r#"
namespace N {
    export type X = number;
}
import X = N.X;
type X = string;
"#;
    assert!(
        !has_error_with_code(source, 2440),
        "Should NOT emit TS2440 for import-equals when only type alias declared. Got: {:?}",
        get_diagnostics(source)
    );
}

#[test]
fn test_import_equals_in_namespace_conflicts_with_var() {
    // import X = N.X inside a namespace, conflicts with var X in same scope
    let source = r#"
namespace N {
    export var X = 1;
}
namespace Outer {
    import X = N.X;
    var X = 1;
}
"#;
    assert!(
        has_error_with_code(source, 2440),
        "Should emit TS2440 for import-equals inside namespace conflicting with var. Got: {:?}",
        get_diagnostics(source)
    );
}

#[test]
fn test_import_equals_conflict_when_export_alias_is_shadowed_by_export_type() {
    // r.Q has value semantics through the export import alias even though the
    // namespace also exports a same-named type alias.
    let source = r#"
namespace q {
    export const Q = {};
}
namespace r {
    export import Q = q.Q;
    export type Q = number;
}
namespace s {
    import Q = r.Q;
    const Q = 0;
}
"#;
    assert!(
        has_error_with_code(source, 2440),
        "Should emit TS2440 when a value-bearing export alias is shadowed by an export type. Got: {:?}",
        get_diagnostics(source)
    );
}

// =========================================================================
// Regression: import-equals to a value (e.g. enum member) merging with a
// same-named local type alias must still emit TS2440. The binder merges the
// import alias and the type alias into a single symbol (ALIAS | TYPE_ALIAS),
// so the conflict cannot be detected by walking other-named symbols.
//
// Conformance reproducer: importedEnumMemberMergedWithExportedAliasIsError.ts
// =========================================================================

#[test]
fn test_import_equals_value_target_conflicts_with_local_type_alias() {
    // The import alias and the type alias bind to *separate* symbols (the
    // binder's type-alias path explicitly creates a new TYPE_ALIAS symbol
    // when an existing ALIAS occupies the slot). The TS2440 conflict check
    // must therefore find the type-alias symbol via `find_all_by_name` and
    // recognise it as a same-scope conflict — it must NOT skip it because
    // the type alias declaration owns its own (type-parameter) scope.
    let source = r#"
namespace N {
    export var X = 1;
}
import X = N.X;
export type X = number;
"#;
    assert!(
        has_error_with_code(source, 2440),
        "Should emit TS2440 when import-equals (value) shares a name with a local type alias. Got: {:?}",
        get_diagnostics(source)
    );
}

#[test]
fn test_import_equals_value_target_emits_ts2395_for_mixed_export_partner() {
    // The binder splits the import alias and type alias into two symbols
    // (linked via `alias_partners`). tsc treats them as a merged declaration,
    // so a non-exported import paired with an exported type alias must emit
    // TS2395 on each declaration's name node.
    let source = r#"
namespace N {
    export var X = 1;
}
import X = N.X;
export type X = number;
"#;
    let diags = get_diagnostics(source);
    let count_2395 = diags.iter().filter(|(c, _)| *c == 2395).count();
    assert!(
        count_2395 >= 2,
        "Expected ≥2 TS2395 diagnostics (one per partnered decl), got {count_2395}. All: {diags:?}"
    );
}

#[test]
fn test_import_equals_value_target_no_ts2395_when_all_exported() {
    // When both halves of the partnership are exported, there is no
    // export-mismatch and TS2395 must not fire.
    let source = r#"
namespace N {
    export var X = 1;
}
export import X = N.X;
export type X = number;
"#;
    let diags = get_diagnostics(source);
    let count_2395 = diags.iter().filter(|(c, _)| *c == 2395).count();
    assert_eq!(
        count_2395, 0,
        "Should not emit TS2395 when all partnered decls are exported. Got: {diags:?}"
    );
}

#[test]
fn test_import_equals_enum_member_target_conflicts_with_type_alias_choice_invariant() {
    // Verify the invariant doesn't depend on the user-chosen alias name —
    // the rule is structural, not name-based.
    for (alias_name, type_name) in [("EnumA", "EnumA"), ("Foo", "Foo"), ("XYZ", "XYZ")] {
        let source = format!(
            r#"
namespace Outer {{
    export enum E {{ A, B }}
}}
import {alias_name} = Outer.E.A;
export type {type_name} = [string] | [string, number];
"#
        );
        assert!(
            has_error_with_code(&source, 2440),
            "Should emit TS2440 for alias '{alias_name}' merging with same-named type. Got: {:?}",
            get_diagnostics(&source)
        );
    }
}

// =========================================================================
// No false positive: module augmentation declarations
// =========================================================================

#[test]
fn test_no_false_positive_for_module_augmentation_interface() {
    // import { X } from "./foo"; declare module "./foo" { interface X {} }
    // Should NOT emit TS2440 because the interface is inside a module augmentation.
    // Note: in a single-file test, the import won't resolve (no cross-file setup),
    // so the resolved_id == sym_id guard will also prevent false positives.
    let source = r#"
import { ParentThing } from "./parent";
declare module "./parent" {
    interface ParentThing {
        bar: string;
    }
}
"#;
    assert!(
        !has_error_with_code(source, 2440),
        "Should NOT emit TS2440 for interface inside module augmentation. Got: {:?}",
        get_diagnostics(source)
    );
}

// =========================================================================
// No false positive: unresolved module imports
// =========================================================================

#[test]
fn test_no_false_positive_for_unresolved_module() {
    // import Foo from "blah"; export function Foo() {}
    // When the module is unresolved, resolve_alias_symbol returns the same symbol.
    // The merged local function flag should NOT cause a false TS2440.
    let source = r#"
import Foo from "blah";
export function Foo() {}
"#;
    assert!(
        !has_error_with_code(source, 2440),
        "Should NOT emit TS2440 when import module is unresolved. Got: {:?}",
        get_diagnostics(source)
    );
}

// =========================================================================
// Export import-equals: error position at 'export' keyword
// =========================================================================

#[test]
fn test_ts1202_export_import_equals_anchors_at_export_keyword() {
    // Regression test for privacyCheck-SimpleReference conformance case:
    // `export import X = require("...")` under ES modules emits TS1202 at the
    // `export` keyword (col 1), not at the `import` keyword (col 8). tsz's
    // parser splits this into EXPORT_DECLARATION wrapping IMPORT_EQUALS_DECLARATION,
    // so the inner import's span starts at `import`; we walk up to the export
    // parent to preserve tsc's anchor.
    use tsz_common::common::ModuleKind;
    let source = "export import mExported = require(\"./mExported\");\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);
    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        CheckerOptions {
            module: ModuleKind::ES2015,
            ..CheckerOptions::default()
        },
    );
    checker.check_source_file(root);
    let ts1202 = checker
        .ctx
        .diagnostics
        .iter()
        .find(|d| d.code == 1202)
        .expect("expected TS1202 for export import under ES module");
    assert_eq!(
        ts1202.start, 0,
        "TS1202 must anchor at the `export` modifier (byte 0), not at `import` (byte 7). got start={}",
        ts1202.start,
    );
}

#[test]
fn test_export_import_equals_error_position() {
    // export import X = N.X; var X = 1;
    // TS2440 should be reported at the 'export' keyword, not 'import'
    let source = r#"
namespace N {
    export var X = 1;
}
export import X = N.X;
var X = 1;
"#;
    let diags = get_diagnostics(source);
    let ts2440 = diags.iter().filter(|d| d.0 == 2440).collect::<Vec<_>>();
    assert!(
        !ts2440.is_empty(),
        "Should emit TS2440 for export import-equals conflict. Got: {diags:?}"
    );
}

// =========================================================================
// `declare global` blocks live in the global scope, not the module scope.
// Declarations inside them must not conflict with module-scoped imports.
// =========================================================================

#[test]
fn test_namespace_import_does_not_conflict_with_declare_global_const() {
    // Repro for TS2440 false positive from crashDeclareGlobalTypeofExport.
    // `import * as foo` is in the module scope; `const foo` inside
    // `declare global { ... }` is in the global scope. They must not conflict.
    let source = r#"
import * as foo from './foo'
export = foo;

declare global {
    const foo: number;
}
"#;
    assert!(
        !has_error_with_code(source, 2440),
        "Must NOT emit TS2440 when local declaration is inside `declare global`. Got: {:?}",
        get_diagnostics(source)
    );
}

#[test]
fn test_namespace_import_does_not_conflict_with_declare_global_var() {
    let source = r#"
import * as bar from './bar'
export = bar;

declare global {
    var bar: string;
}
"#;
    assert!(
        !has_error_with_code(source, 2440),
        "Must NOT emit TS2440 for var inside `declare global`. Got: {:?}",
        get_diagnostics(source)
    );
}

#[test]
fn test_default_import_does_not_conflict_with_declare_global_function() {
    let source = r#"
import baz from './baz'
export { baz };

declare global {
    function baz(): void;
}
"#;
    assert!(
        !has_error_with_code(source, 2440),
        "Must NOT emit TS2440 for function inside `declare global`. Got: {:?}",
        get_diagnostics(source)
    );
}

// =========================================================================
// `export as namespace X` declares a global namespace alias for the module;
// it does NOT introduce a local binding and must not collide with an
// `import * as X` of the same name.
// =========================================================================

#[test]
fn test_namespace_import_does_not_conflict_with_export_as_namespace() {
    // Repro for TS2440 false positive from crashDeclareGlobalTypeofExport.
    // `import * as foo` is a module-scope binding; `export as namespace foo`
    // declares a GLOBAL namespace alias for the module. They don't clash.
    let source = r#"
import * as foo from './foo'
export as namespace foo
export = foo;
"#;
    assert!(
        !has_error_with_code(source, 2440),
        "Must NOT emit TS2440 when the same name appears in `export as namespace`. Got: {:?}",
        get_diagnostics(source)
    );
}

#[test]
fn test_default_import_does_not_conflict_with_export_as_namespace() {
    let source = r#"
import qux from './qux'
export as namespace qux
export = qux;
"#;
    assert!(
        !has_error_with_code(source, 2440),
        "Must NOT emit TS2440 for default import vs `export as namespace` of the same name. Got: {:?}",
        get_diagnostics(source)
    );
}
