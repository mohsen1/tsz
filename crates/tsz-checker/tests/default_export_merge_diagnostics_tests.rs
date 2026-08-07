//! Focused coverage for merged-declaration default export diagnostics.
//!
//! A default-exported declaration is distinct from an ordinary exported
//! declaration. Its declaration space may not overlap any non-default
//! declaration space (TS2652). Ordinary exported/local intersections that are
//! not claimed by TS2652 continue to report TS2395.

use tsz_checker::context::CheckerOptions;
use tsz_checker::test_utils::check_source;
use tsz_common::common::ModuleKind;

const TS2300: u32 = 2300;
const TS2323: u32 = 2323;
const TS2395: u32 = 2395;
const TS2451: u32 = 2451;
const TS2652: u32 = 2652;

fn relevant_diagnostics(source: &str) -> Vec<(u32, u32)> {
    let mut diagnostics: Vec<_> = check_source(
        source,
        "test.ts",
        CheckerOptions {
            module: ModuleKind::CommonJS,
            ..CheckerOptions::default()
        },
    )
    .into_iter()
    .filter(|diagnostic| matches!(diagnostic.code, TS2300 | TS2323 | TS2395 | TS2451 | TS2652))
    .map(|diagnostic| (diagnostic.code, diagnostic.start))
    .collect();
    diagnostics.sort_unstable_by_key(|&(code, start)| (start, code));
    diagnostics
}

fn assert_merge_diagnostics(source: &str, declaration_name: &str, expected: &[(u32, usize)]) {
    let name_starts: Vec<u32> = source
        .match_indices(declaration_name)
        .map(|(start, _)| start as u32)
        .collect();
    let mut expected: Vec<(u32, u32)> = expected
        .iter()
        .map(|&(code, occurrence)| (code, name_starts[occurrence]))
        .collect();
    expected.sort_unstable_by_key(|&(code, start)| (start, code));

    assert_eq!(
        relevant_diagnostics(source),
        expected,
        "unexpected merged-declaration diagnostics"
    );
}

#[test]
fn default_function_conflicts_only_with_instantiated_namespace_space() {
    // `defaultExportsCannotMerge01`: functions occupy value space, interfaces
    // type space, and this namespace is instantiated by its exported value.
    let source = r#"
export default function Decl() {}
export interface Decl {
    property: number;
}
export namespace Decl {
    export const value = 1;
}
"#;

    assert_merge_diagnostics(source, "Decl", &[(TS2652, 0), (TS2652, 2)]);
}

#[test]
fn default_class_conflicts_with_interface_but_not_type_only_namespace() {
    // `defaultExportsCannotMerge02`: classes occupy type and value space; the
    // non-instantiated namespace contributes namespace space only.
    let source = r#"
export default class Entity {}
export interface Entity {
    property: number;
}
export namespace Entity {
    interface Nested {}
}
"#;

    assert_merge_diagnostics(source, "Entity", &[(TS2652, 0), (TS2652, 1)]);
}

#[test]
fn default_class_and_local_interface_use_ts2652_not_ts2395() {
    // `defaultExportsCannotMerge03`: the class and local interface overlap in
    // type space, so the default/non-default rule takes precedence over the
    // ordinary export/local rule.
    let source = r#"
export default class Model {}
interface Model {
    property: number;
}
namespace Model {
    interface Nested {}
}
"#;

    assert_merge_diagnostics(source, "Model", &[(TS2652, 0), (TS2652, 1)]);
}

#[test]
fn default_conflicts_and_export_local_conflicts_are_partitioned_by_space() {
    // `defaultExportsCannotMerge04`: value-space contributors receive TS2652,
    // while the disjoint exported/local type-space contributors retain TS2395.
    let source = r#"
export default function Factory() {}
namespace Factory {
    export const value = 1;
}
interface Factory {}
export interface Factory {}
"#;

    assert_merge_diagnostics(
        source,
        "Factory",
        &[(TS2652, 0), (TS2652, 1), (TS2395, 2), (TS2395, 3)],
    );
}

#[test]
fn disjoint_default_function_and_interface_spaces_are_allowed() {
    let source = r#"
export default function Callable() {}
export interface Callable {
    property: number;
}
"#;

    assert_merge_diagnostics(source, "Callable", &[]);
}

#[test]
fn default_class_and_non_instantiated_namespace_spaces_are_allowed() {
    let source = r#"
export default class Container {}
namespace Container {
    interface Nested {}
}
"#;

    assert_merge_diagnostics(source, "Container", &[]);
}

#[test]
fn separate_default_export_does_not_reclassify_local_merge_declarations() {
    let source = r#"
function Separate() {}
namespace Separate {
    export const value = 1;
}
export default Separate;
"#;

    assert_merge_diagnostics(source, "Separate", &[]);
}

#[test]
fn ordinary_function_overloads_keep_existing_ts2395_suppression() {
    let source = r#"
export function overload(value: string): void;
function overload(value: number): void;
function overload(value: string | number): void {}
"#;

    assert_merge_diagnostics(source, "overload", &[]);
}

#[test]
fn all_function_group_never_reports_ts2652_for_default_non_default_overlap() {
    // #16742: pinned oracle `typescript@7.0.2` reports only `TS2383`
    // ("Overload signatures must all be exported or non-exported.") plus
    // `TS2391` (missing implementation, outside this file's filtered code
    // set) for this shape — never `TS2652`. A same-named run of function
    // declarations is one overload group, not a merged declaration; tsc's
    // flag-agreement check owns the default-vs-non-default mismatch here,
    // the same way it already owns the plain exported-vs-local mismatch
    // (`ordinary_function_overloads_keep_existing_ts2395_suppression` above).
    // This test previously pinned tsz's own bug (spurious double `TS2652`)
    // rather than oracle-verified behavior.
    let source = r#"
export default function Execute(): void;
function Execute(): void;
"#;

    assert_merge_diagnostics(source, "Execute", &[]);
}

#[test]
fn ambient_namespace_merge_keeps_existing_ts2395_suppression() {
    let source = r#"
declare namespace Ambient {
    export interface Entry {}
    interface Entry {}
}
"#;

    assert_merge_diagnostics(source, "Entry", &[]);
}
