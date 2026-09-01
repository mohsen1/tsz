//! #16426's documented residual: `declare export default class {}` / `declare
//! export default function f(): void;` inside a namespace body reported
//! TS1029 ("'export' modifier must precede 'declare' modifier.") *and*
//! TS1319 ("A default export can only be used in an ECMAScript-style
//! module."), where tsc reports TS1029 alone.
//!
//! This is a distinct mechanism from the whole-file `has_parse_errors` gate
//! `export_assignment_default_namespace_parse_error_gate_tests.rs` covers:
//! TS1029 is a grammar-only diagnostic (`is_parser_grammar_code`, not
//! `is_real_syntax_error`, in `tsz-cli`'s `check_utils.rs`), so it never
//! flips the whole-file `has_parse_errors` flag. tsc still suppresses TS1319
//! here because its `checkGrammarModifiers` already reported an error on this
//! exact node's modifier list and returns early, skipping the sibling
//! `checkESModuleMarker`-style check that would otherwise emit TS1319. tsz's
//! parser emits TS1029 before the class/function's `ExportDeclData` wrapper
//! even exists, so `check_export_declaration`
//! (`state/state_checking_members/statement_callback_bridge.rs`) re-derives
//! "did it already fire" from `all_parse_error_positions` instead of an AST
//! field.
//!
//! Uses [`check_source_codes_with_grammar_only_parse_health`], not
//! [`crate::test_utils::check_source_codes_with_parse_health`]: the coarse
//! helper sets `has_parse_errors = true` for ANY parser diagnostic, including
//! TS1029, which would trip the pre-existing whole-file gate and hide this
//! bug behind a false negative (see that helper's own doc comment). The
//! grammar-only helper matches production's actual split: `has_parse_errors`
//! stays `false`, `all_parse_error_positions` still carries TS1029's
//! position.
//!
//! All expectations measured directly against the pinned `typescript@7.0.2`
//! oracle (`scripts/conformance/typescript-versions.json`),
//! `--noEmit --strict --pretty false --target es2022 --module es2022`.

use crate::test_utils::check_source_codes_with_grammar_only_parse_health;

const MODIFIER_MUST_PRECEDE_MODIFIER: u32 = 1029;
const DEFAULT_EXPORT_ONLY_IN_ECMASCRIPT_MODULE: u32 = 1319;

/// oracle: TS1029 alone.
#[test]
fn declare_export_default_class_in_namespace_reports_only_ts1029() {
    let codes = check_source_codes_with_grammar_only_parse_health(
        "namespace N { declare export default class {} }",
    );
    assert!(
        codes.contains(&MODIFIER_MUST_PRECEDE_MODIFIER),
        "expected TS1029 for the misordered `declare export`; got {codes:?}"
    );
    assert!(
        !codes.contains(&DEFAULT_EXPORT_ONLY_IN_ECMASCRIPT_MODULE),
        "tsc's checkGrammarModifiers already reported on this node and returns \
         early, so TS1319 must not additionally fire; got {codes:?}"
    );
}

/// Same declaration-clause branch, a named class — `default_keyword_pos`
/// anchoring must not depend on the class being anonymous.
#[test]
fn declare_export_default_named_class_in_namespace_reports_only_ts1029() {
    let codes = check_source_codes_with_grammar_only_parse_health(
        "namespace N { declare export default class C {} }",
    );
    assert!(codes.contains(&MODIFIER_MUST_PRECEDE_MODIFIER));
    assert!(!codes.contains(&DEFAULT_EXPORT_ONLY_IN_ECMASCRIPT_MODULE));
}

/// Same shape, a function signature (ambient function bodies are a separate,
/// documented residual — TS1183 — not this fix's concern).
#[test]
fn declare_export_default_function_in_namespace_reports_only_ts1029() {
    let codes = check_source_codes_with_grammar_only_parse_health(
        "namespace N { declare export default function f(): void; }",
    );
    assert!(codes.contains(&MODIFIER_MUST_PRECEDE_MODIFIER));
    assert!(!codes.contains(&DEFAULT_EXPORT_ONLY_IN_ECMASCRIPT_MODULE));
}

/// A nested namespace — the position-range check must not depend on nesting
/// depth.
#[test]
fn declare_export_default_class_in_nested_namespace_reports_only_ts1029() {
    let codes = check_source_codes_with_grammar_only_parse_health(
        "namespace Outer { namespace Inner { declare export default class {} } }",
    );
    assert!(codes.contains(&MODIFIER_MUST_PRECEDE_MODIFIER));
    assert!(!codes.contains(&DEFAULT_EXPORT_ONLY_IN_ECMASCRIPT_MODULE));
}

/// Negative control: ordinary `export default class {}` in a namespace (no
/// `declare`, no modifier-order violation) must still report TS1319 — the
/// new suppression must not fire when there is nothing to deduplicate
/// against.
#[test]
fn plain_export_default_class_in_namespace_still_reports_ts1319() {
    let codes = check_source_codes_with_grammar_only_parse_health(
        "namespace N { export default class {} }",
    );
    assert!(
        codes.contains(&DEFAULT_EXPORT_ONLY_IN_ECMASCRIPT_MODULE),
        "no modifier-order diagnostic exists here, so TS1319 must still fire; got {codes:?}"
    );
    assert!(!codes.contains(&MODIFIER_MUST_PRECEDE_MODIFIER));
}

/// Negative control: `declare export default class {}` at top level (not in
/// a namespace) is unaffected by this check — TS1029 alone, matching tsc,
/// same as before this fix (the outer `is_inside_namespace_declaration` gate
/// never lets execution reach the new suppression logic here).
#[test]
fn declare_export_default_class_at_top_level_reports_only_ts1029() {
    let codes =
        check_source_codes_with_grammar_only_parse_health("declare export default class {}");
    assert!(codes.contains(&MODIFIER_MUST_PRECEDE_MODIFIER));
    assert!(!codes.contains(&DEFAULT_EXPORT_ONLY_IN_ECMASCRIPT_MODULE));
}
