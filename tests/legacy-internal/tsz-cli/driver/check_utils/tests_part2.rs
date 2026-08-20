//! Unit tests for `check_utils`, part 2. Split out of `tests.rs` to keep
//! the file under the 2000-line limit (#16733).

use super::*;

#[test]
fn filtered_parse_diagnostics_suppresses_await_ts1359_when_ts1109_present() {
    use tsz::parser::ParseDiagnostic;

    let diagnostics = vec![
        ParseDiagnostic {
            start: 100,
            length: 5,
            message: "Identifier expected. 'await' is a reserved word that cannot be used here."
                .to_string(),
            code: 1359,
            related: None,
        },
        ParseDiagnostic {
            start: 200,
            length: 1,
            message: "Expression expected.".to_string(),
            code: 1109,
            related: None,
        },
    ];

    let filtered = filtered_parse_diagnostics(&diagnostics, false);
    let codes: Vec<u32> = filtered.iter().map(|d| d.code).collect();
    assert!(
        !codes.contains(&1359),
        "TS1359 for 'await' should be suppressed when TS1109 is present, got: {codes:?}"
    );
    assert!(
        codes.contains(&1109),
        "TS1109 should still be present, got: {codes:?}"
    );
}

#[test]
fn filtered_parse_diagnostics_keeps_await_ts1359_with_unrelated_parse_errors() {
    use tsz::parser::ParseDiagnostic;

    let diagnostics = vec![
        ParseDiagnostic {
            start: 100,
            length: 5,
            message: "Identifier expected. 'await' is a reserved word that cannot be used here."
                .to_string(),
            code: 1359,
            related: None,
        },
        ParseDiagnostic {
            start: 10,
            length: 6,
            message: "A module cannot have multiple default exports.".to_string(),
            code: 2528,
            related: None,
        },
    ];

    let filtered = filtered_parse_diagnostics(&diagnostics, false);
    let codes: Vec<u32> = filtered.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&1359),
        "TS1359 for 'await' should survive unrelated parse diagnostics, got: {codes:?}"
    );
}

#[test]
fn filtered_parse_diagnostics_keeps_await_ts1359_when_alone() {
    use tsz::parser::ParseDiagnostic;

    let diagnostics = vec![ParseDiagnostic {
        start: 100,
        length: 5,
        message: "Identifier expected. 'await' is a reserved word that cannot be used here."
            .to_string(),
        code: 1359,
        related: None,
    }];

    let filtered = filtered_parse_diagnostics(&diagnostics, false);
    let codes: Vec<u32> = filtered.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&1359),
        "TS1359 for 'await' should be kept when it's the only diagnostic, got: {codes:?}"
    );
}

#[test]
fn filtered_parse_diagnostics_suppresses_ts1028_when_real_parse_error_present() {
    use tsz::parser::ParseDiagnostic;

    // multipleClassPropertyModifiersErrors.ts: `public public p1;` would emit
    // TS1028, but the file also contains `static static p3;` which yields a real
    // parse error (TS1434). tsc emits TS1028 via grammarErrorOnNode, which is
    // suppressed by hasParseDiagnostics(sourceFile) when any real parse error
    // exists, so only TS1434 survives.
    let diagnostics = vec![
        ParseDiagnostic {
            start: 18,
            length: 6,
            message: "Accessibility modifier already seen.".to_string(),
            code: 1028,
            related: None,
        },
        ParseDiagnostic {
            start: 50,
            length: 6,
            message: "Unexpected keyword or identifier.".to_string(),
            code: 1434,
            related: None,
        },
    ];

    let filtered = filtered_parse_diagnostics(&diagnostics, false);
    let codes: Vec<u32> = filtered.iter().map(|d| d.code).collect();
    assert!(
        !codes.contains(&1028),
        "TS1028 should be suppressed when a real parse error (TS1434) is present, got: {codes:?}"
    );
    assert!(
        codes.contains(&1434),
        "TS1434 (real parse error) should survive, got: {codes:?}"
    );
}

#[test]
fn filtered_parse_diagnostics_keeps_ts1028_when_alone() {
    use tsz::parser::ParseDiagnostic;

    // parserMemberVariableDeclaration1.ts: `public public Foo;` with no other
    // parse error. hasParseDiagnostics is false in tsc, so the grammar error is
    // reported. tsz must keep its parser-emitted TS1028 in this case.
    let diagnostics = vec![ParseDiagnostic {
        start: 18,
        length: 6,
        message: "Accessibility modifier already seen.".to_string(),
        code: 1028,
        related: None,
    }];

    let filtered = filtered_parse_diagnostics(&diagnostics, false);
    let codes: Vec<u32> = filtered.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&1028),
        "TS1028 should be kept when it is the only diagnostic, got: {codes:?}"
    );
}

#[test]
fn filtered_parse_diagnostics_suppresses_ts1101_when_real_parse_error_present() {
    use tsz::parser::ParseDiagnostic;

    // `with` inside a class body or module top level is parser-emitted TS1101
    // in tsz (the parser knows the syntactic auto-strict context without the
    // checker). tsc's checkStrictModeWithStatement is a binder check
    // (file.bindDiagnostics), suppressed program-wide by hasParseDiagnostics
    // whenever a real structural parse error exists — verified against the
    // pinned tsc oracle: `with (o) { ... }` plus an unrelated `function f( {}`
    // reports only TS1005, dropping both TS1101 and the checker's TS2410.
    let diagnostics = vec![
        ParseDiagnostic {
            start: 30,
            length: 4,
            message: "'with' statements are not allowed in strict mode.".to_string(),
            code: 1101,
            related: None,
        },
        ParseDiagnostic {
            start: 80,
            length: 1,
            message: "')' expected.".to_string(),
            code: 1005,
            related: None,
        },
    ];

    let filtered = filtered_parse_diagnostics(&diagnostics, false);
    let codes: Vec<u32> = filtered.iter().map(|d| d.code).collect();
    assert!(
        !codes.contains(&1101),
        "TS1101 should be suppressed when a real parse error (TS1005) is present, got: {codes:?}"
    );
    assert!(
        codes.contains(&1005),
        "TS1005 (real parse error) should survive, got: {codes:?}"
    );
}

#[test]
fn filtered_parse_diagnostics_keeps_ts1101_when_alone() {
    use tsz::parser::ParseDiagnostic;

    let diagnostics = vec![ParseDiagnostic {
        start: 30,
        length: 4,
        message: "'with' statements are not allowed in strict mode.".to_string(),
        code: 1101,
        related: None,
    }];

    let filtered = filtered_parse_diagnostics(&diagnostics, false);
    let codes: Vec<u32> = filtered.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&1101),
        "TS1101 should be kept when it is the only diagnostic, got: {codes:?}"
    );
}

#[test]
fn filtered_parse_diagnostics_keeps_ts1101_with_non_real_parse_error() {
    use tsz::parser::ParseDiagnostic;

    // TS1014 (rest parameter must be last) does not corrupt the AST and is
    // intentionally excluded from `is_real_syntax_error` — verified against
    // the pinned tsc oracle: `with` plus `function f(...a, b) {}` in the same
    // file still reports both TS1101 and TS1014/TS7019/TS7006 together.
    let diagnostics = vec![
        ParseDiagnostic {
            start: 30,
            length: 4,
            message: "'with' statements are not allowed in strict mode.".to_string(),
            code: 1101,
            related: None,
        },
        ParseDiagnostic {
            start: 90,
            length: 1,
            message: "A rest parameter must be last in a parameter list.".to_string(),
            code: 1014,
            related: None,
        },
    ];

    let filtered = filtered_parse_diagnostics(&diagnostics, false);
    let codes: Vec<u32> = filtered.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&1101),
        "TS1101 should survive alongside a non-real parse error (TS1014), got: {codes:?}"
    );
}

/// Build the parse-diagnostic pair a bad getter + bad setter in one member list
/// produces. `setter_code`/`setter_message` vary the setter's grammar failure so
/// the whole `checkGrammarAccessor` family is covered by one helper rather than
/// four hand-copied vectors.
fn accessor_grammar_pair(
    setter_code: u32,
    setter_message: &str,
) -> Vec<tsz::parser::ParseDiagnostic> {
    use tsz::parser::ParseDiagnostic;

    vec![
        ParseDiagnostic {
            start: 18,
            length: 2,
            message: "A 'get' accessor cannot have parameters.".to_string(),
            code: 1054,
            related: None,
        },
        ParseDiagnostic {
            start: 52,
            length: 2,
            message: setter_message.to_string(),
            code: setter_code,
            related: None,
        },
    ]
}

#[test]
fn filtered_parse_diagnostics_keeps_getter_ts1054_alongside_setter_ts1049() {
    // #16277. Both codes come from tsc's single `checkGrammarAccessor`, so a
    // setter's TS1049 must not suppress a getter's TS1054 in the same member
    // list. Pinned against tsc 7.0.2 (`--noEmit --strict --pretty false`) in all
    // three accessor containers — class, object literal and type-member list —
    // each of which reports TS1054 and TS1049 together.
    let diagnostics =
        accessor_grammar_pair(1049, "A 'set' accessor must have exactly one parameter.");

    let filtered = filtered_parse_diagnostics(&diagnostics, false);
    let codes: Vec<u32> = filtered.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&1054),
        "TS1054 must survive alongside a sibling TS1049, got: {codes:?}"
    );
    assert!(
        codes.contains(&1049),
        "TS1049 must survive alongside a sibling TS1054, got: {codes:?}"
    );
}

#[test]
fn filtered_parse_diagnostics_keeps_getter_ts1054_alongside_setter_ts1051() {
    // Same family, optional-parameter arm. tsc reports TS1054 + TS1051 together.
    let diagnostics =
        accessor_grammar_pair(1051, "A 'set' accessor cannot have an optional parameter.");

    let filtered = filtered_parse_diagnostics(&diagnostics, false);
    let codes: Vec<u32> = filtered.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&1054),
        "TS1054 must survive alongside a sibling TS1051, got: {codes:?}"
    );
    assert!(
        codes.contains(&1051),
        "TS1051 must survive alongside a sibling TS1054, got: {codes:?}"
    );
}

#[test]
fn filtered_parse_diagnostics_keeps_getter_ts1054_alongside_setter_ts1095() {
    // Positive control that already passed before the fix: TS1095 was the one
    // accessor code already listed as a grammar code, so it never triggered the
    // sibling-suppression path. It pins the arm that must NOT change.
    let diagnostics = accessor_grammar_pair(
        1095,
        "A 'set' accessor cannot have a return type annotation.",
    );

    let filtered = filtered_parse_diagnostics(&diagnostics, false);
    let codes: Vec<u32> = filtered.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&1054),
        "TS1054 must survive alongside a sibling TS1095, got: {codes:?}"
    );
    assert!(
        codes.contains(&1095),
        "TS1095 must survive alongside a sibling TS1054, got: {codes:?}"
    );
}

#[test]
fn filtered_parse_diagnostics_keeps_repeated_setter_ts1049_without_a_getter() {
    // Negative control for the fix's own risk: listing TS1049 as a grammar code
    // must not make it suppress itself or vanish when it is the only kind of
    // diagnostic in the file. Two bad setters alone keep both TS1049s.
    use tsz::parser::ParseDiagnostic;

    let diagnostics = vec![
        ParseDiagnostic {
            start: 18,
            length: 2,
            message: "A 'set' accessor must have exactly one parameter.".to_string(),
            code: 1049,
            related: None,
        },
        ParseDiagnostic {
            start: 52,
            length: 2,
            message: "A 'set' accessor must have exactly one parameter.".to_string(),
            code: 1049,
            related: None,
        },
    ];

    let filtered = filtered_parse_diagnostics(&diagnostics, false);
    let codes: Vec<u32> = filtered.iter().map(|d| d.code).collect();
    assert_eq!(
        codes,
        vec![1049, 1049],
        "both TS1049s must survive when no other diagnostic kind is present, got: {codes:?}"
    );
}

#[test]
fn filtered_parse_diagnostics_suppresses_accessor_grammar_family_when_real_parse_error_present() {
    // The direction the fix newly introduces, and it is tsc's, not a side
    // effect: `checkGrammarAccessor` reports through `grammarErrorOnNode`, which
    // returns without reporting once `hasParseDiagnostics(sourceFile)` holds.
    // Verified against tsc 7.0.2 — a class carrying `set sd(vd: number, wd:
    // number) {}` plus a structural parse error reports ONLY the structural
    // errors (TS1440/TS1109/TS1128); the TS1049 is gone. Same for TS1051.
    use tsz::parser::ParseDiagnostic;

    let diagnostics = vec![
        ParseDiagnostic {
            start: 18,
            length: 2,
            message: "A 'get' accessor cannot have parameters.".to_string(),
            code: 1054,
            related: None,
        },
        ParseDiagnostic {
            start: 52,
            length: 2,
            message: "A 'set' accessor must have exactly one parameter.".to_string(),
            code: 1049,
            related: None,
        },
        ParseDiagnostic {
            start: 70,
            length: 2,
            message: "A 'set' accessor cannot have an optional parameter.".to_string(),
            code: 1051,
            related: None,
        },
        ParseDiagnostic {
            start: 90,
            length: 1,
            message: "Expression expected.".to_string(),
            code: 1109,
            related: None,
        },
    ];

    let filtered = filtered_parse_diagnostics(&diagnostics, false);
    let codes: Vec<u32> = filtered.iter().map(|d| d.code).collect();
    assert_eq!(
        codes,
        vec![1109],
        "the whole accessor grammar family must be suppressed by a real parse error, got: {codes:?}"
    );
}

// TS1018/1020/1025 are siblings of TS1017/1019/1021/1096 in tsc's index-signature
// grammar check (`checkGrammarIndexSignature`), all parser-emitted in tsz from
// `parse_index_signature_with_modifiers`. #16279 is the general shape (a
// partially-listed `checkGrammar*` family self-suppresses); this is the second
// confirmed instance after #16278's accessor family. Verified against the pinned
// tsc@7.0.2 oracle: `[public key: string]: number;` plus an unrelated real parse
// error (`let x: = 1;`, TS1110) reports only TS1110, dropping TS1018 — and, before
// this fix, TS1018 being unlisted made it count as a "real" parse error itself,
// so `interface Foo { [public key: string]: number; } function f() { foo: while
// (true) { foo: while (true) { break; } } }` (no real syntax error, an unrelated
// duplicate label elsewhere in the file) dropped the already-listed TS1114
// entirely in tsz while tsc reports both TS1018 and TS1114.

#[test]
fn filtered_parse_diagnostics_suppresses_ts1018_when_real_parse_error_present() {
    use tsz::parser::ParseDiagnostic;

    let diagnostics = vec![
        ParseDiagnostic {
            start: 20,
            length: 6,
            message: "An index signature parameter cannot have an accessibility modifier."
                .to_string(),
            code: 1018,
            related: None,
        },
        ParseDiagnostic {
            start: 60,
            length: 1,
            message: "Type expected.".to_string(),
            code: 1110,
            related: None,
        },
    ];

    let filtered = filtered_parse_diagnostics(&diagnostics, false);
    let codes: Vec<u32> = filtered.iter().map(|d| d.code).collect();
    assert!(
        !codes.contains(&1018),
        "TS1018 should be suppressed when a real parse error (TS1110) is present, got: {codes:?}"
    );
    assert!(
        codes.contains(&1110),
        "TS1110 (real parse error) should survive, got: {codes:?}"
    );
}

#[test]
fn filtered_parse_diagnostics_suppresses_ts1020_when_real_parse_error_present() {
    use tsz::parser::ParseDiagnostic;

    let diagnostics = vec![
        ParseDiagnostic {
            start: 20,
            length: 1,
            message: "An index signature parameter cannot have an initializer.".to_string(),
            code: 1020,
            related: None,
        },
        ParseDiagnostic {
            start: 60,
            length: 1,
            message: "Type expected.".to_string(),
            code: 1110,
            related: None,
        },
    ];

    let filtered = filtered_parse_diagnostics(&diagnostics, false);
    let codes: Vec<u32> = filtered.iter().map(|d| d.code).collect();
    assert!(
        !codes.contains(&1020),
        "TS1020 should be suppressed when a real parse error (TS1110) is present, got: {codes:?}"
    );
    assert!(
        codes.contains(&1110),
        "TS1110 (real parse error) should survive, got: {codes:?}"
    );
}

#[test]
fn filtered_parse_diagnostics_suppresses_ts1025_when_real_parse_error_present() {
    use tsz::parser::ParseDiagnostic;

    let diagnostics = vec![
        ParseDiagnostic {
            start: 20,
            length: 1,
            message: "An index signature cannot have a trailing comma.".to_string(),
            code: 1025,
            related: None,
        },
        ParseDiagnostic {
            start: 60,
            length: 1,
            message: "Type expected.".to_string(),
            code: 1110,
            related: None,
        },
    ];

    let filtered = filtered_parse_diagnostics(&diagnostics, false);
    let codes: Vec<u32> = filtered.iter().map(|d| d.code).collect();
    assert!(
        !codes.contains(&1025),
        "TS1025 should be suppressed when a real parse error (TS1110) is present, got: {codes:?}"
    );
    assert!(
        codes.contains(&1110),
        "TS1110 (real parse error) should survive, got: {codes:?}"
    );
}

#[test]
fn filtered_parse_diagnostics_keeps_ts1018_ts1020_ts1025_when_alone() {
    use tsz::parser::ParseDiagnostic;

    for (code, message) in [
        (
            1018,
            "An index signature parameter cannot have an accessibility modifier.",
        ),
        (
            1020,
            "An index signature parameter cannot have an initializer.",
        ),
        (1025, "An index signature cannot have a trailing comma."),
    ] {
        let diagnostics = vec![ParseDiagnostic {
            start: 20,
            length: 1,
            message: message.to_string(),
            code,
            related: None,
        }];

        let filtered = filtered_parse_diagnostics(&diagnostics, false);
        let codes: Vec<u32> = filtered.iter().map(|d| d.code).collect();
        assert!(
            codes.contains(&code),
            "TS{code} should be kept when it is the only diagnostic, got: {codes:?}"
        );
    }
}

#[test]
fn filtered_parse_diagnostics_ts1018_does_not_self_suppress_listed_sibling() {
    use tsz::parser::ParseDiagnostic;

    // Before the fix, TS1018 was unlisted in `is_parser_grammar_code`, so it
    // counted as a "real" non-grammar parse error under
    // `has_non_grammar_parse_error` and silently deleted every *listed* sibling
    // in the same file — here, the already-listed TS1114 (duplicate label).
    let diagnostics = vec![
        ParseDiagnostic {
            start: 20,
            length: 6,
            message: "An index signature parameter cannot have an accessibility modifier."
                .to_string(),
            code: 1018,
            related: None,
        },
        ParseDiagnostic {
            start: 80,
            length: 3,
            message: "Duplicate label 'foo'.".to_string(),
            code: 1114,
            related: None,
        },
    ];

    let filtered = filtered_parse_diagnostics(&diagnostics, false);
    let codes: Vec<u32> = filtered.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&1018),
        "TS1018 should survive when it is the only non-grammar-looking diagnostic, got: {codes:?}"
    );
    assert!(
        codes.contains(&1114),
        "TS1114 must not be self-suppressed by unlisted TS1018, got: {codes:?}"
    );
}

#[test]
fn filtered_parse_diagnostics_suppresses_16279_audit_codes_when_real_parse_error_present() {
    use tsz::parser::ParseDiagnostic;

    // #16279 audit round: TS1079/1092/1094/1098/1099/1120/1242/1246/1247/1491/1495
    // were confirmed checker-suppressible against a real `typescript@7.0.2`
    // oracle (a genuine unrelated syntax error in the same file drops each of
    // these, matching the already-listed families they belong to). Before this
    // fix, each was unlisted and so both went unsuppressed on its own AND
    // silently deleted every listed sibling in the same file.
    for code in [
        1079, 1092, 1094, 1098, 1099, 1120, 1242, 1246, 1247, 1491, 1495,
    ] {
        let diagnostics = vec![
            ParseDiagnostic {
                start: 0,
                length: 1,
                message: "candidate".to_string(),
                code,
                related: None,
            },
            ParseDiagnostic {
                start: 10,
                length: 1,
                message: "Expression expected.".to_string(),
                code: 1109,
                related: None,
            },
        ];
        let filtered = filtered_parse_diagnostics(&diagnostics, false);
        let codes: Vec<u32> = filtered.iter().map(|d| d.code).collect();
        assert!(
            !codes.contains(&code),
            "TS{code} should be suppressed when a real parse error (TS1109) is present, got: {codes:?}"
        );
        assert!(
            codes.contains(&1109),
            "TS1109 (real parse error) should survive for the TS{code} case, got: {codes:?}"
        );
    }
}

#[test]
fn filtered_parse_diagnostics_keeps_16279_audit_codes_when_alone() {
    use tsz::parser::ParseDiagnostic;

    for code in [
        1079, 1092, 1094, 1098, 1099, 1120, 1242, 1246, 1247, 1491, 1495,
    ] {
        let diagnostics = vec![ParseDiagnostic {
            start: 0,
            length: 1,
            message: "candidate".to_string(),
            code,
            related: None,
        }];
        let filtered = filtered_parse_diagnostics(&diagnostics, false);
        let codes: Vec<u32> = filtered.iter().map(|d| d.code).collect();
        assert!(
            codes.contains(&code),
            "TS{code} should be kept when it is the only diagnostic, got: {codes:?}"
        );
    }
}

#[test]
fn filtered_parse_diagnostics_suppresses_ts18016_when_real_parse_error_present() {
    use tsz::parser::ParseDiagnostic;

    // #16279 audit round 8: TS18016 ("Private identifiers are not allowed
    // outside class bodies.") is checker-emitted in tsc via
    // `checkGrammarPrivateIdentifierExpression`'s `grammarErrorOnNode` call,
    // but tsz's parser emits it directly for a private-identifier-keyed
    // interface/type-literal/object-literal member. Oracle-confirmed against
    // `typescript@7.0.2`: `interface I { #foo: number }` plus an unrelated
    // real syntax error (`let x: = 1;`) drops TS18016 entirely on the real
    // compiler. Before this fix it was unlisted, so it not only survived on
    // its own but also silently deleted every listed sibling in the same file.
    let diagnostics = vec![
        ParseDiagnostic {
            start: 0,
            length: 1,
            message: "Private identifiers are not allowed outside class bodies.".to_string(),
            code: 18016,
            related: None,
        },
        ParseDiagnostic {
            start: 10,
            length: 1,
            message: "Expression expected.".to_string(),
            code: 1109,
            related: None,
        },
    ];
    let filtered = filtered_parse_diagnostics(&diagnostics, false);
    let codes: Vec<u32> = filtered.iter().map(|d| d.code).collect();
    assert!(
        !codes.contains(&18016),
        "TS18016 should be suppressed when a real parse error (TS1109) is present, got: {codes:?}"
    );
    assert!(
        codes.contains(&1109),
        "TS1109 (real parse error) should survive, got: {codes:?}"
    );
}

#[test]
fn filtered_parse_diagnostics_keeps_ts18016_when_alone() {
    use tsz::parser::ParseDiagnostic;

    let diagnostics = vec![ParseDiagnostic {
        start: 0,
        length: 1,
        message: "Private identifiers are not allowed outside class bodies.".to_string(),
        code: 18016,
        related: None,
    }];
    let filtered = filtered_parse_diagnostics(&diagnostics, false);
    let codes: Vec<u32> = filtered.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&18016),
        "TS18016 should be kept when it is the only diagnostic, got: {codes:?}"
    );
}

#[test]
fn filtered_parse_diagnostics_ts18016_does_not_self_suppress_listed_sibling() {
    use tsz::parser::ParseDiagnostic;

    // Before TS18016 was listed, its presence (an unlisted grammar code)
    // made `has_non_grammar_parse_error` true and suppressed every *listed*
    // sibling in the same file — even with no real structural syntax error.
    // `interface I { #foo: number }` next to a class with a parameterless
    // `set` accessor: tsc keeps both TS18016 and TS1049; tsz kept only
    // TS18016.
    let diagnostics = vec![
        ParseDiagnostic {
            start: 0,
            length: 1,
            message: "Private identifiers are not allowed outside class bodies.".to_string(),
            code: 18016,
            related: None,
        },
        ParseDiagnostic {
            start: 10,
            length: 1,
            message: "A 'set' accessor must have exactly one parameter.".to_string(),
            code: 1049,
            related: None,
        },
    ];
    let filtered = filtered_parse_diagnostics(&diagnostics, false);
    let codes: Vec<u32> = filtered.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&18016),
        "TS18016 should survive alongside a listed sibling, got: {codes:?}"
    );
    assert!(
        codes.contains(&1049),
        "TS1049 must not be self-suppressed by the unlisted TS18016, got: {codes:?}"
    );
}

#[test]
fn filtered_parse_diagnostics_keeps_ts1433_and_ts1436_alongside_real_parse_error() {
    use tsz::parser::ParseDiagnostic;

    // Oracle-tested and rejected for `is_parser_grammar_code`: unlike their
    // modifier/decorator-shaped neighbors above, tsc keeps TS1433 ("Neither
    // decorators nor modifiers may be applied to 'this' parameters.") and
    // TS1436 ("Decorators must precede the name...") even when a real
    // structural syntax error (TS1109) exists elsewhere in the file. They are
    // real tsc parser diagnostics, not checker-side grammar checks, so they
    // must never be added to the suppression list.
    for code in [1433, 1436] {
        let diagnostics = vec![
            ParseDiagnostic {
                start: 0,
                length: 1,
                message: "candidate".to_string(),
                code,
                related: None,
            },
            ParseDiagnostic {
                start: 10,
                length: 1,
                message: "Expression expected.".to_string(),
                code: 1109,
                related: None,
            },
        ];
        let filtered = filtered_parse_diagnostics(&diagnostics, false);
        let codes: Vec<u32> = filtered.iter().map(|d| d.code).collect();
        assert!(
            codes.contains(&code),
            "TS{code} must survive alongside a real parse error (confirmed real tsc parser diagnostic), got: {codes:?}"
        );
    }
}

#[test]
fn js_parse_allowlist_keeps_plain_js_binder_strict_codes() {
    for code in [1214, 18012] {
        assert!(
            is_ts1xxx_allowed_in_js(code),
            "plain JS binder parse diagnostic TS{code} should be reported in JavaScript files"
        );
    }
}

#[test]
fn js_parse_allowlist_keeps_ts2657() {
    assert!(
        is_ts1xxx_allowed_in_js(2657),
        "TS2657 should be preserved for JS JSX recovery diagnostics"
    );
}

#[test]
fn js_parse_allowlist_keeps_ts17002() {
    assert!(
        is_ts1xxx_allowed_in_js(17002),
        "TS17002 should be preserved for JS JSX closing-tag mismatch diagnostics"
    );
}

#[test]
fn js_parse_allowlist_keeps_ts17014() {
    assert!(
        is_ts1xxx_allowed_in_js(17014),
        "TS17014 should be preserved for JS JSX fragment recovery diagnostics"
    );
}

#[test]
fn js_parse_allowlist_keeps_ts1163() {
    assert!(
        is_ts1xxx_allowed_in_js(1163),
        "TS1163 should be preserved for JS yield-outside-generator diagnostics"
    );
}

// ---------------------------------------------------------------
// Export signature tests: CLI path via build_export_signature_input
// ---------------------------------------------------------------

/// Helper: compute export signature from source via the CLI pipeline
/// (`parse_and_bind_single` → merge → `build_export_signature_input` → `from_input`).
fn cli_export_signature(source: &str) -> tsz_lsp::export_signature::ExportSignature {
    let bind_result = parallel::parse_and_bind_single("test.ts".to_string(), source.to_string());
    let program = parallel::merge_bind_results(vec![bind_result]);
    let file = &program.files[0];
    compute_export_signature(&program, file, 0)
}

/// Helper: compute CLI export signature input (for structural inspection).
fn cli_export_input(source: &str) -> tsz_lsp::export_signature::ExportSignatureInput {
    let bind_result = parallel::parse_and_bind_single("test.ts".to_string(), source.to_string());
    let program = parallel::merge_bind_results(vec![bind_result]);
    let file = &program.files[0];
    build_export_signature_input(&program, file, 0)
}

#[test]
fn body_only_edit_preserves_signature() {
    let before = "export function foo() { return 1; }";
    let after = "export function foo() { return 42; }";
    assert_eq!(
        cli_export_signature(before),
        cli_export_signature(after),
        "body-only edit must not change export signature"
    );
}

#[test]
fn comment_only_edit_preserves_signature() {
    let before = "// original comment\nexport const x = 1;";
    let after = "// modified comment with extra words\nexport const x = 1;";
    assert_eq!(
        cli_export_signature(before),
        cli_export_signature(after),
        "comment-only edit must not change export signature"
    );
}

#[test]
fn private_symbol_edit_preserves_signature() {
    let before = "const priv = 1;\nexport const pub_val = priv;";
    let after = "const priv = 999;\nconst priv2 = 2;\nexport const pub_val = priv;";
    assert_eq!(
        cli_export_signature(before),
        cli_export_signature(after),
        "private symbol additions/edits must not change export signature"
    );
}

#[test]
fn adding_export_changes_signature() {
    let before = "export const x = 1;";
    let after = "export const x = 1;\nexport const y = 2;";
    assert_ne!(
        cli_export_signature(before),
        cli_export_signature(after),
        "adding a new export must change the signature"
    );
}

#[test]
fn removing_export_changes_signature() {
    let before = "export const x = 1;\nexport const y = 2;";
    let after = "export const x = 1;";
    assert_ne!(
        cli_export_signature(before),
        cli_export_signature(after),
        "removing an export must change the signature"
    );
}

#[test]
fn re_export_edit_changes_signature() {
    let before = "export { foo } from './other';";
    let after = "export { foo, bar } from './other';";
    assert_ne!(
        cli_export_signature(before),
        cli_export_signature(after),
        "adding a named re-export must change the signature"
    );
}

#[test]
fn wildcard_re_export_changes_signature() {
    let before = "export const x = 1;";
    let after = "export const x = 1;\nexport * from './other';";
    assert_ne!(
        cli_export_signature(before),
        cli_export_signature(after),
        "adding a wildcard re-export must change the signature"
    );
}

#[test]
fn augmentation_edit_changes_signature() {
    let before = "export const x = 1;";
    let after = "export const x = 1;\ndeclare global { interface Window { foo: string; } }";
    assert_ne!(
        cli_export_signature(before),
        cli_export_signature(after),
        "adding a global augmentation must change the signature"
    );
}

#[test]
fn export_input_captures_exports() {
    let input = cli_export_input("export const x = 1;\nexport function foo() {}");
    let names: Vec<&str> = input.exports.iter().map(|(n, _, _)| n.as_str()).collect();
    assert!(names.contains(&"x"), "should contain x export: {names:?}");
    assert!(
        names.contains(&"foo"),
        "should contain foo export: {names:?}"
    );
}

#[test]
fn export_input_captures_re_exports() {
    let input = cli_export_input("export { bar } from './other';");
    let re_names: Vec<&str> = input
        .named_reexports
        .iter()
        .map(|(n, _, _)| n.as_str())
        .collect();
    assert!(
        re_names.contains(&"bar"),
        "should contain bar re-export: {re_names:?}"
    );
}

#[test]
fn export_input_captures_wildcard_re_exports() {
    let input = cli_export_input("export * from './other';");
    assert_eq!(
        input.wildcard_reexports.len(),
        1,
        "should have one wildcard re-export"
    );
    assert_eq!(input.wildcard_reexports[0].0, "./other");
}

#[test]
fn export_input_ignores_private_symbols() {
    let input = cli_export_input("const priv = 1;\nexport const pub_val = priv;");
    let names: Vec<&str> = input.exports.iter().map(|(n, _, _)| n.as_str()).collect();
    assert!(
        !names.contains(&"priv"),
        "private symbols must not appear in export input"
    );
    assert!(names.contains(&"pub_val"));
}

#[test]
fn regex_flag_errors_do_not_suppress_semantic_diagnostics() {
    // TS1499 (unknown regex flag) should not set has_syntax_parse_errors,
    // so TS2339 (property does not exist) should still be emitted.
    assert!(
        is_non_suppressing_parse_error(1499),
        "TS1499 (Unknown regex flag) should be non-suppressing"
    );
    assert!(
        is_non_suppressing_parse_error(1500),
        "TS1500 (Duplicate regex flag) should be non-suppressing"
    );
    assert!(
        is_non_suppressing_parse_error(1502),
        "TS1502 (Incompatible u/v flags) should be non-suppressing"
    );
}

#[test]
fn index_signature_arity_error_does_not_suppress_grammar_diagnostics() {
    // TS1096 (An index signature must have exactly one parameter) is a check-time
    // grammar error in tsc (checkGrammarIndexSignatureParameters) on a well-formed
    // AST, so it must not set has_syntax_parse_errors — otherwise a stray `[a, b]`
    // would suppress unrelated check-time grammar diagnostics elsewhere in the file
    // (e.g. TS1036 in an ambient namespace, and nearby TS1021).
    assert!(
        is_non_suppressing_parse_error(1096),
        "TS1096 (index signature arity) should be non-suppressing"
    );
    assert!(
        !is_real_syntax_error(1096),
        "TS1096 must not be a real syntax error (would still poison the flag)"
    );
    assert!(
        !is_structural_parse_error(1096),
        "TS1096 must not be a structural parse error (would still poison the flag)"
    );
}

/// Helper: parse a single file and collect noCheck path diagnostics.
fn collect_no_check_diags(file_name: &str, source: &str) -> Vec<Diagnostic> {
    let mut parse_results =
        parallel::parse_files_parallel(vec![(file_name.to_string(), source.to_string())]);
    let result = parse_results.remove(0);
    let options = ResolvedCompilerOptions::default();
    let program_has_real_syntax_errors = result
        .parse_diagnostics
        .iter()
        .any(|d| is_real_syntax_error(d.code));
    collect_no_check_parse_diagnostics_for_file(
        &result.file_name,
        &result.arena,
        result.source_file,
        &result.parse_diagnostics,
        &options,
        program_has_real_syntax_errors,
    )
}

#[test]
fn no_check_path_emits_ts8010_for_js_parameter_type_annotation() {
    // Issue #3692: `--noCheck` previously skipped TS8xxx grammar
    // diagnostics that tsc reports from its parser. Confirm that a
    // type-annotated JS parameter still produces TS8010 here.
    let diagnostics = collect_no_check_diags("a.js", "function f(x: number) {}\n");
    assert!(
        diagnostics.iter().any(|d| d.code == 8010),
        "expected TS8010 in JS noCheck output, got: {diagnostics:#?}"
    );
}

#[test]
fn no_check_path_emits_ts8010_for_js_variable_type_annotation() {
    // Variable declarations with TS-only type annotations also surface.
    let diagnostics = collect_no_check_diags("a.js", "let x: number;\n");
    assert!(
        diagnostics.iter().any(|d| d.code == 8010),
        "expected TS8010 in JS noCheck output for `let x: number`, got: {diagnostics:#?}"
    );
}

#[test]
fn no_check_path_does_not_emit_ts8010_for_typescript_files() {
    // The grammar walker must not fire on TypeScript files.
    let diagnostics = collect_no_check_diags("a.ts", "function f(x: number) {}\n");
    assert!(
        !diagnostics.iter().any(|d| d.code == 8010),
        "TS8010 must not fire on TypeScript files, got: {diagnostics:#?}"
    );
}

#[test]
fn no_check_ts_expect_error_does_not_suppress_parse_error() {
    // Under `--noCheck`, `@ts-expect-error` must not suppress parse errors
    // (TS1109 "Expression expected"). tsc reports parse diagnostics from
    // `getSyntacticDiagnostics` which bypasses directive suppression.
    let source = "// @ts-expect-error\nconst broken = ;\n";
    let diagnostics = collect_no_check_diags("a.ts", source);
    assert!(
        diagnostics.iter().any(|d| d.code == 1109),
        "TS1109 must not be suppressed by @ts-expect-error in --noCheck, got: {diagnostics:#?}"
    );
    assert!(
        !diagnostics.iter().any(|d| d.code == 2578),
        "TS2578 must not be emitted under --noCheck, got: {diagnostics:#?}"
    );
}

#[test]
fn no_check_ts_ignore_does_not_suppress_parse_error() {
    // `@ts-ignore` must also not suppress parse errors under `--noCheck`.
    let source = "// @ts-ignore\nconst broken = ;\n";
    let diagnostics = collect_no_check_diags("a.ts", source);
    assert!(
        diagnostics.iter().any(|d| d.code == 1109),
        "TS1109 must survive @ts-ignore in --noCheck, got: {diagnostics:#?}"
    );
}

#[test]
fn no_check_ts_expect_error_does_not_suppress_js_grammar_error() {
    // Under `--noCheck`, `@ts-expect-error` must not suppress JS grammar
    // errors (TS8010 "Type annotations can only be used in TypeScript files").
    let source = "// @ts-expect-error\nlet x: number;\n";
    let diagnostics = collect_no_check_diags("a.js", source);
    assert!(
        diagnostics.iter().any(|d| d.code == 8010),
        "TS8010 must not be suppressed by @ts-expect-error in --noCheck JS, got: {diagnostics:#?}"
    );
    assert!(
        !diagnostics.iter().any(|d| d.code == 2578),
        "TS2578 must not be emitted under --noCheck, got: {diagnostics:#?}"
    );
}

#[test]
fn no_check_ts_expect_error_on_clean_line_does_not_emit_ts2578() {
    // Under `--noCheck`, an @ts-expect-error directive above a line with
    // no diagnostics must not produce TS2578 ("Unused '@ts-expect-error'").
    // tsc does not run type-checking in --noCheck mode so every directive
    // is effectively unreachable; none should be penalized.
    let source = "// @ts-expect-error\nconst x = 5;\n";
    let diagnostics = collect_no_check_diags("a.ts", source);
    assert!(
        !diagnostics.iter().any(|d| d.code == 2578),
        "TS2578 must not be emitted under --noCheck for unused directive, got: {diagnostics:#?}"
    );
}

#[test]
fn no_check_multiple_expect_error_directives_do_not_emit_ts2578() {
    // Multiple @ts-expect-error directives under --noCheck must all be
    // silently ignored rather than producing a wave of TS2578 reports.
    let source = concat!(
        "// @ts-expect-error\nconst a = 1;\n",
        "// @ts-expect-error\nconst b = 2;\n",
    );
    let diagnostics = collect_no_check_diags("a.ts", source);
    assert!(
        !diagnostics.iter().any(|d| d.code == 2578),
        "TS2578 must not fire for multiple unused directives under --noCheck, got: {diagnostics:#?}"
    );
}

fn check_directive_suppression(source: &str, codes_in: &[u32]) -> Vec<Diagnostic> {
    let line_starts = line_starts_of(source);
    let line1_start = line_starts.get(1).copied().unwrap_or(0);
    let mut diagnostics: Vec<Diagnostic> = codes_in
        .iter()
        .map(|&code| {
            Diagnostic::error(
                "test.ts".to_string(),
                line1_start,
                1,
                format!("diag {code}"),
                code,
            )
        })
        .collect();
    apply_ts_directive_suppression("test.ts", source, &mut diagnostics, false);
    diagnostics
}

#[test]
fn apply_suppression_never_suppresses_real_syntax_errors() {
    // TS1109 (Expression expected) is a real syntax error and must survive
    // directive suppression even in the full-check path. It still marks
    // @ts-expect-error as used, matching tsc's TS2578 behavior.
    let source = "// @ts-expect-error\nconst broken = ;\n";
    let remaining = check_directive_suppression(source, &[1109]);
    assert!(
        remaining.iter().any(|d| d.code == 1109),
        "TS1109 must not be suppressed, got: {remaining:#?}"
    );
    assert!(
        !remaining.iter().any(|d| d.code == 2578),
        "TS2578 must not be emitted when directive targets a parse error, got: {remaining:#?}"
    );
}

#[test]
fn apply_suppression_never_suppresses_js_only_syntactic_errors() {
    let source = "// @ts-expect-error\nlet x: number;\n";
    let remaining = check_directive_suppression(source, &[8010]);
    assert!(
        remaining.iter().any(|d| d.code == 8010),
        "TS8010 must not be suppressed, got: {remaining:#?}"
    );
    assert!(
        !remaining.iter().any(|d| d.code == 2578),
        "TS2578 must not be emitted when directive targets a JS syntactic diagnostic, got: {remaining:#?}"
    );
}

#[test]
fn apply_suppression_suppresses_semantic_error_but_not_parse_error_on_same_line() {
    // When a parse error (TS1109) and a semantic error (TS2322) both exist
    // on the target line, the semantic error is suppressed and the parse
    // error survives. The directive is marked as used, so no TS2578.
    let source = "// @ts-expect-error\nconst x: string = ;\n";
    let remaining = check_directive_suppression(source, &[1109, 2322]);
    assert!(
        remaining.iter().any(|d| d.code == 1109),
        "TS1109 must survive directive suppression, got: {remaining:#?}"
    );
    assert!(
        !remaining.iter().any(|d| d.code == 2322),
        "TS2322 must be suppressed by @ts-expect-error, got: {remaining:#?}"
    );
    assert!(
        !remaining.iter().any(|d| d.code == 2578),
        "TS2578 must not fire when directive suppressed a semantic error, got: {remaining:#?}"
    );
}

#[test]
fn apply_suppression_real_syntax_error_codes_are_never_suppressed() {
    // Verify several codes from is_real_syntax_error are all immune.
    let real_syntax_codes: &[u32] = &[1002, 1003, 1005, 1006, 1007, 1109, 1110, 1126, 1127];
    let source = "// @ts-expect-error\ncode_on_line_2;\n";
    for &code in real_syntax_codes {
        let remaining = check_directive_suppression(source, &[code]);
        assert!(
            remaining.iter().any(|d| d.code == code),
            "TS{code} must not be suppressed by @ts-expect-error, got: {remaining:#?}"
        );
    }
}
