//! `abstract` before an `export`-prefixed declaration — the gap #16389
//! deliberately left open when it fixed only `abstract export as namespace`.
//!
//! `export` here is a *modifier* on the trailing declaration, so tsc reads one
//! modifier run `[abstract, export]` and `checkGrammarModifiers` reports
//! exactly one diagnostic for it. Which one is chosen by the node kind
//! `export` decorates, which is why this could not be a blanket widening of
//! #16392's `export as namespace` lookahead:
//!
//! | trailing form                                   | outside a Block | inside a Block |
//! | ----------------------------------------------- | --------------- | -------------- |
//! | `class` (`abstract` is legal there)             | TS1029 on `export` | TS1029 on `export` |
//! | `const`/`let`/`var`/`function`/`interface`/`type`/`enum` | TS1242 | TS1184 |
//! | `namespace`/`module`, `export { }`, `export *`  | TS1242          | none — the form's own TS1235/TS1233 wins |
//!
//! Before this fix every one of these degraded to an identifier expression, so
//! tsz reported a spurious **TS2304 `Cannot find name 'abstract'`** on top of
//! the wrong grammar code. Every row below is pinned against a real
//! `typescript@7.0.2` oracle (the version `scripts/conformance/typescript-versions.json`
//! pins as `current`).

use crate::parser::test_fixture::parse_source;
use tsz_common::diagnostics::diagnostic_codes;

fn codes(source: &str) -> Vec<u32> {
    let (parser, _root) = parse_source(source);
    let mut codes: Vec<u32> = parser.get_diagnostics().iter().map(|d| d.code).collect();
    codes.sort_unstable();
    codes.dedup();
    codes
}

/// The four containers whose grammar answer differs, plus the source-file top
/// level. `{S}` is the statement under test.
const CONTAINERS: [&str; 5] = [
    "{S}",
    "function outer() { {S} }",
    "function outer() { { {S} } }",
    "namespace NS { {S} }",
    "class Host { static { {S} } }",
];

/// Whether the container at `index` is a Block body (function body, nested
/// block, class static block) — the split #16368/#16375 introduced.
fn is_block(index: usize) -> bool {
    matches!(index, 1 | 2 | 4)
}

fn in_container(index: usize, statement: &str) -> String {
    CONTAINERS[index].replace("{S}", statement)
}

fn assert_diag_at_abstract(source: &str, code: u32) {
    let (parser, _root) = parse_source(source);
    let diagnostics = parser.get_diagnostics();
    let start = source.find("abstract").unwrap() as u32;
    assert!(
        diagnostics
            .iter()
            .any(|d| d.code == code && d.start == start && d.length == "abstract".len() as u32),
        "expected TS{code} on the `abstract` keyword at {start} for {source:?}, got {diagnostics:?}"
    );
}

fn assert_no_cannot_find_name(source: &str) {
    let (parser, _root) = parse_source(source);
    assert!(
        !parser
            .get_diagnostics()
            .iter()
            .any(|d| d.code == diagnostic_codes::CANNOT_FIND_NAME),
        "`abstract` must stay a modifier, not degrade to an identifier expression, for {source:?}"
    );
}

// -- `abstract export class`: `abstract` is legal on a class, so only the
//    ordering error is reported, and it outranks the container check. --

#[test]
fn abstract_export_class_reports_ts1029_on_the_export_keyword_in_every_container() {
    for index in 0..CONTAINERS.len() {
        let source = in_container(index, "abstract export class D {}");
        let (parser, _root) = parse_source(&source);
        let diagnostics = parser.get_diagnostics();
        let export_start = source.find("export").unwrap() as u32;
        assert!(
            diagnostics.iter().any(
                |d| d.code == diagnostic_codes::MODIFIER_MUST_PRECEDE_MODIFIER
                    && d.start == export_start
                    && d.length == "export".len() as u32
            ),
            "expected TS1029 anchored on `export` at {export_start} for {source:?}, got {diagnostics:?}"
        );
    }
}

#[test]
fn abstract_export_class_does_not_also_report_the_container_modifier_error() {
    // tsc reports exactly one diagnostic for the modifier run: the ordering
    // error. A Block body must NOT additionally gain the TS1184 that a bare
    // `export class D {}` in the same position would produce.
    for index in 0..CONTAINERS.len() {
        let source = in_container(index, "abstract export class D {}");
        assert_eq!(
            codes(&source),
            vec![diagnostic_codes::MODIFIER_MUST_PRECEDE_MODIFIER],
            "unexpected extra diagnostics for {source:?}"
        );
    }
}

#[test]
fn abstract_export_class_binder_name_does_not_change_the_answer() {
    // The predicate is node-kind driven; the declared name must be irrelevant.
    for name in ["D", "abstract", "exportish", "Telemetry"] {
        let source = format!("abstract export class {name} {{}}");
        assert_eq!(
            codes(&source),
            vec![diagnostic_codes::MODIFIER_MUST_PRECEDE_MODIFIER],
            "unexpected diagnostics for {source:?}"
        );
    }
}

// -- `abstract export <declaration that admits no `abstract`>`: the same
//    container split the sibling `abstract const`/`abstract function` path
//    uses. --

#[test]
fn abstract_export_modifier_run_splits_ts1242_and_ts1184_by_container() {
    let statements = [
        "abstract export const zz = 1;",
        "abstract export let zz = 1;",
        "abstract export var zz = 1;",
        "abstract export function ff() {}",
        "abstract export async function gg() {}",
        "abstract export interface II {}",
        "abstract export type TT = number;",
        "abstract export enum EE { A }",
        "abstract export default function f() {}",
        "abstract export default function() {}",
        "abstract export default async function g() {}",
    ];
    for statement in statements {
        for index in 0..CONTAINERS.len() {
            let source = in_container(index, statement);
            let expected = if is_block(index) {
                diagnostic_codes::MODIFIERS_CANNOT_APPEAR_HERE
            } else {
                diagnostic_codes::ABSTRACT_MODIFIER_CAN_ONLY_APPEAR_ON_A_CLASS_METHOD_OR_PROPERTY_DECLARATION
            };
            assert_diag_at_abstract(&source, expected);
            assert_no_cannot_find_name(&source);
        }
    }
}

#[test]
fn abstract_export_modifier_run_reports_exactly_one_diagnostic() {
    // The declaration still parses with the (invalid) modifier run attached —
    // no downstream "declaration or statement expected" recovery noise, and no
    // second modifier error from the `export` half.
    for index in 0..CONTAINERS.len() {
        let source = in_container(index, "abstract export const zz = 1;");
        let expected = if is_block(index) {
            diagnostic_codes::MODIFIERS_CANNOT_APPEAR_HERE
        } else {
            diagnostic_codes::ABSTRACT_MODIFIER_CAN_ONLY_APPEAR_ON_A_CLASS_METHOD_OR_PROPERTY_DECLARATION
        };
        assert_eq!(codes(&source), vec![expected], "for {source:?}");
    }
}

// -- Forms that report their own position error inside a Block, which
//    suppresses the modifier diagnostic there. --

#[test]
fn abstract_export_namespace_yields_to_the_module_position_error_in_a_block() {
    for index in 0..CONTAINERS.len() {
        let source = in_container(index, "abstract export namespace NN {}");
        if is_block(index) {
            // TS1235 itself is a checker-side grammar error, so it does not
            // reach this parser-only harness — the CLI does report it (matrix
            // row `abstract|export-namespace|funcbody`). What this fix owns is
            // that the parser adds no modifier diagnostic of its own here.
            assert_eq!(
                codes(&source),
                Vec::<u32>::new(),
                "expected no parse diagnostic for {source:?}"
            );
        } else {
            assert_diag_at_abstract(
                &source,
                diagnostic_codes::ABSTRACT_MODIFIER_CAN_ONLY_APPEAR_ON_A_CLASS_METHOD_OR_PROPERTY_DECLARATION,
            );
        }
        assert_no_cannot_find_name(&source);
    }
}

#[test]
fn abstract_export_declaration_yields_to_the_export_position_error_in_a_block() {
    for statement in ["abstract export {};", "abstract export * from \"./m\";"] {
        for index in 0..CONTAINERS.len() {
            let source = in_container(index, statement);
            let has_modifier_error = codes(&source).iter().any(|&c| {
                c == diagnostic_codes::ABSTRACT_MODIFIER_CAN_ONLY_APPEAR_ON_A_CLASS_METHOD_OR_PROPERTY_DECLARATION
                    || c == diagnostic_codes::MODIFIERS_CANNOT_APPEAR_HERE
            });
            assert_eq!(
                has_modifier_error,
                !is_block(index),
                "modifier-error presence is wrong for {source:?}: {:?}",
                codes(&source)
            );
            assert_no_cannot_find_name(&source);
        }
    }
}

// -- ASI: `abstract` is a contextual keyword, so a line break before `export`
//    cuts it into its own expression statement, exactly as it does for the
//    sibling `abstract` paths. --

#[test]
fn a_line_break_between_abstract_and_export_is_not_a_modifier_run() {
    let source = "abstract\nexport class D {}";
    assert!(
        !codes(source).contains(&diagnostic_codes::MODIFIER_MUST_PRECEDE_MODIFIER),
        "ASI must cut `abstract` off into its own statement, got {:?}",
        codes(source)
    );
}

// -- Negative controls: shapes this fix must leave exactly as they were. --

#[test]
fn a_valid_export_abstract_class_stays_clean() {
    for source in [
        "export abstract class D {}",
        "namespace NS { export abstract class D {} }",
        "abstract class D {}",
    ] {
        assert_eq!(codes(source), Vec::<u32>::new(), "for {source:?}");
    }
}

#[test]
fn abstract_as_an_identifier_expression_is_untouched() {
    // `abstract` is a contextual keyword; these must not be read as a modifier
    // run just because an `export`-like identifier follows.
    for source in [
        "abstract;",
        "abstract + 1;",
        "const abstract = 1; abstract;",
    ] {
        assert!(
            !codes(source).contains(&diagnostic_codes::MODIFIER_MUST_PRECEDE_MODIFIER)
                && !codes(source).contains(
                    &diagnostic_codes::ABSTRACT_MODIFIER_CAN_ONLY_APPEAR_ON_A_CLASS_METHOD_OR_PROPERTY_DECLARATION
                ),
            "for {source:?}: {:?}",
            codes(source)
        );
    }
}

#[test]
fn abstract_export_as_namespace_still_routes_through_its_own_arm() {
    // #16389's shape must keep reporting TS1184 over the whole statement in
    // every container — the new lookahead must not steal it.
    for index in 0..CONTAINERS.len() {
        let source = in_container(index, "abstract export as namespace Telemetry;");
        assert!(
            codes(&source).contains(&diagnostic_codes::MODIFIERS_CANNOT_APPEAR_HERE),
            "for {source:?}: {:?}",
            codes(&source)
        );
    }
}

#[test]
fn abstract_export_type_only_export_is_not_read_as_a_type_alias() {
    // `export type { x }` / `export type * from "m"` is an export declaration,
    // not the `export type X = Y` alias form — so it must land in the
    // position-error arm, not the modifier-run arm.
    for statement in [
        "abstract export type { zz };",
        "abstract export type * from \"./m\";",
    ] {
        let block = in_container(1, statement);
        assert!(
            !codes(&block).contains(&diagnostic_codes::MODIFIERS_CANNOT_APPEAR_HERE),
            "a type-only export in a Block must not gain TS1184: {block:?} {:?}",
            codes(&block)
        );
    }
}

// -- `abstract export default class`: #16398's follow-up. `export default
//    class C {}` reaches a different parser entry point
//    (`parse_export_declaration`) than the plain `abstract export class`
//    arm above, but `abstract` is legal on a class regardless of `default`,
//    so tsc reads the same modifier run and reports the same single TS1029
//    on `export`, in every container. Oracle-confirmed against
//    `typescript@7.0.2`: named class, anonymous class, and a second,
//    legally-placed `abstract` directly before `class` all produce exactly
//    one TS1029, nothing else. --

#[test]
fn abstract_export_default_class_reports_ts1029_on_the_export_keyword_in_every_container() {
    for statement in [
        "abstract export default class D {}",
        "abstract export default class {}",
    ] {
        for index in 0..CONTAINERS.len() {
            let source = in_container(index, statement);
            let (parser, _root) = parse_source(&source);
            let diagnostics = parser.get_diagnostics();
            let export_start = source.find("export").unwrap() as u32;
            assert!(
                diagnostics.iter().any(|d| d.code
                    == diagnostic_codes::MODIFIER_MUST_PRECEDE_MODIFIER
                    && d.start == export_start
                    && d.length == "export".len() as u32),
                "expected TS1029 anchored on `export` at {export_start} for {source:?}, got {diagnostics:?}"
            );
        }
    }
}

#[test]
fn abstract_export_default_class_does_not_also_report_the_container_modifier_error() {
    for statement in [
        "abstract export default class D {}",
        "abstract export default class {}",
    ] {
        for index in 0..CONTAINERS.len() {
            let source = in_container(index, statement);
            assert_eq!(
                codes(&source),
                vec![diagnostic_codes::MODIFIER_MUST_PRECEDE_MODIFIER],
                "unexpected extra diagnostics for {source:?}"
            );
        }
    }
}

#[test]
fn abstract_export_default_class_binder_name_does_not_change_the_answer() {
    for name in ["D", "abstract", "exportish", "Telemetry"] {
        let source = format!("abstract export default class {name} {{}}");
        assert_eq!(
            codes(&source),
            vec![diagnostic_codes::MODIFIER_MUST_PRECEDE_MODIFIER],
            "unexpected diagnostics for {source:?}"
        );
    }
}

#[test]
fn abstract_export_default_redundant_abstract_before_class_still_reports_exactly_one_ts1029() {
    // A second, legally-placed `abstract` directly before `class` belongs to
    // the correct `export default abstract class` tail — tsc's own answer for
    // this shape is still exactly one TS1029, oracle-confirmed.
    for index in 0..CONTAINERS.len() {
        let source = in_container(index, "abstract export default abstract class D {}");
        assert_eq!(
            codes(&source),
            vec![diagnostic_codes::MODIFIER_MUST_PRECEDE_MODIFIER],
            "unexpected diagnostics for {source:?}"
        );
    }
}

#[test]
fn a_line_break_between_abstract_and_export_default_class_is_not_a_modifier_run() {
    let source = "abstract\nexport default class D {}";
    assert!(
        !codes(source).contains(&diagnostic_codes::MODIFIER_MUST_PRECEDE_MODIFIER),
        "ASI must cut `abstract` off into its own statement, got {:?}",
        codes(source)
    );
}

#[test]
fn a_valid_export_default_class_stays_clean() {
    // Negative controls: the correct order, with and without a legal
    // `abstract`, must be entirely unaffected by this fix.
    for source in [
        "export default class D {}",
        "export default abstract class D {}",
        "export default class {}",
    ] {
        assert_eq!(codes(source), Vec::<u32>::new(), "for {source:?}");
    }
}

#[test]
fn abstract_export_default_non_class_forms_are_not_read_as_the_class_arm() {
    // None of `abstract`'s other `export default <expr>` forms are legal on
    // a class, so this fix must never widen the ordering-violation TS1029
    // classification past `class`, in any container.
    for statement in [
        "abstract export default function f() {}",
        "abstract export default 1;",
        "abstract export default (class {});",
    ] {
        for index in 0..CONTAINERS.len() {
            let source = in_container(index, statement);
            assert!(
                !codes(&source).contains(&diagnostic_codes::MODIFIER_MUST_PRECEDE_MODIFIER),
                "must not report the class-ordering TS1029 for {source:?}: {:?}",
                codes(&source)
            );
        }
    }
}

// -- `abstract export default <expr>`: previously a completely silent parse
//    (`abstract` degraded to an identifier expression with no modifier
//    diagnostic at all, in every container). `function`/`async function`
//    take the ordinary `ModifierRun` container split (covered above, folded
//    into `abstract_export_modifier_run_splits_ts1242_and_ts1184_by_container`);
//    every other expression takes the `ExportAssignment` node's own, wider
//    silencing. --

#[test]
fn abstract_export_default_expression_reports_ts1242_only_outside_a_block_or_namespace() {
    // `export default <expr>` is an `ExportAssignment` node — its own
    // placement diagnostic (TS1258 in a Block, TS1319 in a namespace body,
    // both checker-side and outside this parser-only harness) wins in both
    // containers, so the parser's TS1242 modifier error survives only at
    // the source file's own top level. Oracle-confirmed against
    // `typescript@7.0.2`.
    for statement in [
        "abstract export default 1;",
        "abstract export default (class {});",
    ] {
        for index in 0..CONTAINERS.len() {
            let source = in_container(index, statement);
            if is_block(index) || index == 3 {
                assert_eq!(
                    codes(&source),
                    Vec::<u32>::new(),
                    "expected no modifier diagnostic for {source:?}, got {:?}",
                    codes(&source)
                );
            } else {
                assert_diag_at_abstract(
                    &source,
                    diagnostic_codes::ABSTRACT_MODIFIER_CAN_ONLY_APPEAR_ON_A_CLASS_METHOD_OR_PROPERTY_DECLARATION,
                );
            }
            assert_no_cannot_find_name(&source);
        }
    }
}

// -- `abstract export = <expr>`: #16403's residual. `export = ...` was
//    deliberately excluded from the original lookahead (routed instead to
//    `abstract` degrading to a bare identifier expression, then `export =
//    <expr>` re-parsed as an unrelated, unmodified top-level statement) —
//    silently dropping TS1242 everywhere and, at the source file's own top
//    level, also mis-anchoring the checker's TS1203 at `export` instead of
//    `abstract` (oracle-confirmed against `typescript@7.0.2`; TS1203 itself
//    is checker-side and outside this parser-only harness). `export =` is
//    the same `ExportAssignment` node kind as `export default <expr>`, so it
//    takes the identical container split: TS1242 wins outright at the
//    source file's own top level, and is silenced by the assignment's own
//    placement diagnostic in both a Block (TS1231) and a namespace body
//    (TS1063).
#[test]
fn abstract_export_equals_assignment_reports_ts1242_only_outside_a_block_or_namespace() {
    for index in 0..CONTAINERS.len() {
        let source = in_container(index, "abstract export = 1;");
        if is_block(index) || index == 3 {
            assert_eq!(
                codes(&source),
                Vec::<u32>::new(),
                "expected no modifier diagnostic for {source:?}, got {:?}",
                codes(&source)
            );
        } else {
            assert_diag_at_abstract(
                &source,
                diagnostic_codes::ABSTRACT_MODIFIER_CAN_ONLY_APPEAR_ON_A_CLASS_METHOD_OR_PROPERTY_DECLARATION,
            );
        }
        assert_no_cannot_find_name(&source);
    }
}

#[test]
fn a_line_break_between_abstract_and_export_equals_is_not_a_modifier_run() {
    let source = "abstract\nexport = 1;";
    assert!(
        !codes(source).contains(
            &diagnostic_codes::ABSTRACT_MODIFIER_CAN_ONLY_APPEAR_ON_A_CLASS_METHOD_OR_PROPERTY_DECLARATION
        ),
        "ASI must cut `abstract` off into its own statement, got {:?}",
        codes(source)
    );
}

#[test]
fn a_valid_export_equals_assignment_stays_clean() {
    assert_eq!(codes("export = 1;"), Vec::<u32>::new());
}
