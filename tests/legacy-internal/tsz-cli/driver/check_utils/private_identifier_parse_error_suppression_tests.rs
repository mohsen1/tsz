//! Tests for the private-identifier parser-error family in
//! `is_real_syntax_error` (issue #16817).
//!
//! TS18009 (`Private identifiers cannot be used as parameters.`), TS18029
//! (`Private identifiers are not allowed in variable declarations.`) and TS18030
//! (`An optional chain cannot contain private identifiers.`) are tsc *parser*
//! diagnostics that drive `hasParseDiagnostics()`, short-circuiting the whole
//! program's semantic phase — see the family comment on the `18009 | 18029 |
//! 18030` arm of `is_real_syntax_error`. tsz emits all three from the parser but
//! only classified TS18030, so TS18029/TS18009 files went on to full semantic
//! checking and surfaced diagnostics tsc never reaches. These tests pin the
//! classifier and the end-to-end suppression, plus the controls that must keep
//! full semantic checking. Witnesses pinned against `typescript@7.0.2` (issue
//! #16817; #16279 audit round 6 for the Direction-B evidence).

use super::super::check::{CheckerLibSet, collect_diagnostics};
use super::*;

const TS18009: u32 = 18009;
const TS18029: u32 = 18029;
const TS18030: u32 = 18030;

/// The whole private-identifier-in-a-value-position family is a real syntax
/// error (drives tsc's whole-program semantic short-circuit) and must stay a
/// genuine *parser* diagnostic — a suppression trigger, not a checker-routed
/// grammar code that is itself suppressed.
#[test]
fn private_identifier_value_position_codes_are_real_syntax_errors() {
    for code in [TS18009, TS18029, TS18030] {
        assert!(
            is_real_syntax_error(code),
            "TS{code} is a tsc parser diagnostic (parseErrorAtRange) that drives \
             hasParseDiagnostics(); it must count as a real syntax error so the \
             program's semantic phase is short-circuited, matching tsc."
        );
        assert!(
            !is_parser_grammar_code(code),
            "TS{code} survives Direction B (tsc keeps it alongside an unrelated \
             real syntax error), so it is a parser diagnostic, not a checker-side \
             grammar code."
        );
        assert!(
            !is_non_suppressing_parse_error(code),
            "TS{code} triggers tsc's hasParseDiagnostics() suppression, so it must \
             remain a suppressing parse error."
        );
    }
}

/// Control: the *checker-routed* private-identifier grammar codes (TS18016
/// `outside class bodies`, TS18024 `enum member named with a private
/// identifier`) are raised by tsc via `grammarErrorOnNode`, so they are
/// suppressed *by* a parse error rather than being a trigger of one. They must
/// not be promoted into the real-syntax-error family — that split is the whole
/// point of the fix.
#[test]
fn checker_routed_private_identifier_grammar_codes_stay_non_suppressing() {
    for code in [18016_u32, 18024] {
        assert!(
            !is_real_syntax_error(code),
            "TS{code} is a checker-side grammar check in tsc; promoting it to a \
             real syntax error would wrongly suppress the program's semantics."
        );
        assert!(
            is_parser_grammar_code(code),
            "TS{code} is a checker-routed grammar code and must stay in \
             is_parser_grammar_code."
        );
    }
}

fn collect_diagnostic_codes(files: &[(&str, &str)]) -> Vec<u32> {
    let bind_results: Vec<_> = files
        .iter()
        .map(|(file_name, source)| {
            parallel::parse_and_bind_single((*file_name).to_string(), (*source).to_string())
        })
        .collect();
    let program = parallel::merge_bind_results(bind_results);
    let type_cache_output = std::sync::Mutex::new(FxHashMap::default());

    let mut codes: Vec<u32> = collect_diagnostics(
        &CollectDiagnosticsInput {
            program: &program,
            options: &ResolvedCompilerOptions::default(),
            base_dir: std::path::Path::new("/"),
            reference_path_current_directory: None,
            checker_libs: &CheckerLibSet::default(),
            typescript_dom_replacement_globals: (false, false, false),
            has_deprecation_diagnostics: false,
            collect_compile_stats: false,
        },
        None,
        &type_cache_output,
    )
    .diagnostics
    .iter()
    .map(|diagnostic| diagnostic.code)
    .collect();
    codes.sort_unstable();
    codes
}

/// Each private-identifier parse error suppresses the file's trailing semantics.
/// The TS18029 witness is issue #16817's repro (a `for`-header private-identifier
/// binding next to a mis-annotation and an unresolved name); TS18009 is the
/// parameter sibling. tsc reports only the parse code — the trailing TS2322
/// (mis-annotation) and TS2304 (unresolved name) must be suppressed program-wide.
#[test]
fn private_identifier_parse_error_suppresses_trailing_semantics() {
    let witnesses = [
        (
            TS18029,
            "const arr = [1];\nfor (const #x of arr) {}\nconst bad: number = \"str\";\nnope();\n",
        ),
        (
            TS18009,
            "function f(#x) {}\nconst bad: number = \"str\";\nnope();\n",
        ),
    ];
    for (code, source) in witnesses {
        let codes = collect_diagnostic_codes(&[("repro.ts", source)]);
        assert!(
            codes.contains(&code),
            "expected TS{code} for the private-identifier witness, got {codes:?}"
        );
        assert!(
            !codes.contains(&2322) && !codes.contains(&2304),
            "TS{code} is a parse error; tsc short-circuits the semantic phase, so \
             the trailing TS2322/TS2304 must not appear, got {codes:?}"
        );
    }
}

/// Program-wide fence: a private-identifier parse error in one file suppresses
/// the semantic diagnostics of an *unrelated* clean file in the same program,
/// mirroring tsc's `hasParseDiagnostics()` short-circuit over the whole program.
#[test]
fn private_identifier_parse_error_suppresses_semantics_program_wide() {
    let codes = collect_diagnostic_codes(&[
        ("bad.ts", "const #x = 1;\n"),
        ("clean.ts", "const bad: number = \"str\";\n"),
    ]);
    assert!(
        codes.contains(&TS18029),
        "expected TS18029 from bad.ts, got {codes:?}"
    );
    assert!(
        !codes.contains(&2322),
        "a real syntax error anywhere in the program suppresses semantic \
         diagnostics program-wide, so clean.ts's TS2322 must not appear, got {codes:?}"
    );
}

/// Negative control: a *legal* private identifier inside a class body is not a
/// parse error, so full semantic checking still runs and an unrelated
/// mis-annotation still reports TS2322.
#[test]
fn legal_private_identifier_keeps_full_semantic_checking() {
    let codes = collect_diagnostic_codes(&[(
        "ok.ts",
        "class C { #p = 1; m() { return this.#p; } }\nconst bad: number = \"str\";\n",
    )]);
    assert!(
        !codes.contains(&TS18009) && !codes.contains(&TS18029) && !codes.contains(&TS18030),
        "a legal private field must not raise any private-identifier parse error, got {codes:?}"
    );
    assert!(
        codes.contains(&2322),
        "with no parse error, semantic checking runs and the mis-annotation's \
         TS2322 must be reported, got {codes:?}"
    );
}
