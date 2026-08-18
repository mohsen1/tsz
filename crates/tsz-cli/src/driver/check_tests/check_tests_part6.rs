    // ------------------------------------------------------------------
    // Syntactic-phase gating of the semantic phase (tsc driver sequencing).
    //
    // tsc's `emitFilesAndReportErrorsWorker` reports semantic (bind + check)
    // diagnostics only when the syntactic phase produced nothing, so ANY parse
    // diagnostic in ANY root file drops every binder/checker diagnostic
    // program-wide. Two tsz classifier gaps broke this:
    //
    // 1. The scanner-emitted numeric-literal family (TS1125/TS1177/TS1178/
    //    TS1352/TS1353/TS1489/TS6188/TS6189) was missing from
    //    `is_real_syntax_error`, so those parse errors never armed the gate.
    // 2. The binder strict-mode family (TS1102/TS1212/TS1213/TS1214) was
    //    missing from `is_checker_routed_ts1xxx_grammar`, so tsz's
    //    checker-emitted copies survived an armed gate (`code < 2000`).
    //
    // Every witness below is pinned against typescript@6.0.2: the construct
    // alone reports its own code, and the same construct next to an unrelated
    // parse error in a sibling file reports nothing but the parse error.
    // ------------------------------------------------------------------

    /// Each scanner-emitted numeric-literal parse error must suppress an
    /// unrelated semantic diagnostic (TS2322) in a *different* file.
    #[test]
    fn numeric_literal_scanner_error_suppresses_cross_file_semantic_diagnostics() {
        let cases: &[(u32, &str)] = &[
            (1125, "var hex = 0x;\n"),
            (1177, "var bin = 0b;\n"),
            (1178, "var oct = 0o9;\n"),
            (1352, "var bigExp = 1e2n;\n"),
            (1353, "var bigFrac = 1.2n;\n"),
            (1489, "\"use strict\";\nvar lead = 009;\n"),
            (6188, "var sep = 1_;\n"),
            (6189, "var doubled = 1__2;\n"),
        ];
        for (code, source) in cases {
            let diagnostics = collect_test_diagnostics(&[
                ("/literal.ts", *source),
                ("/other.ts", "const mismatched: string = 42;\n"),
            ]);
            let codes: Vec<u32> = diagnostics.iter().map(|d| d.code).collect();
            assert!(
                codes.contains(code),
                "TS{code} itself must survive the syntactic phase: {diagnostics:?}"
            );
            assert!(
                !codes.contains(&2322),
                "TS{code} in one file must suppress the sibling file's TS2322 \
                 (tsc skips the semantic phase program-wide): {diagnostics:?}"
            );
        }
    }

    /// Positive control: with no parse error anywhere, the sibling file's
    /// TS2322 must be reported. Guards against the gate arming spuriously.
    #[test]
    fn semantic_diagnostics_survive_when_no_parse_error_exists() {
        let diagnostics = collect_test_diagnostics(&[
            ("/clean.ts", "var fine = 0o7;\n"),
            ("/other.ts", "const mismatched: string = 42;\n"),
        ]);
        let codes: Vec<u32> = diagnostics.iter().map(|d| d.code).collect();
        assert!(
            codes.contains(&2322),
            "a well-formed octal literal must not arm the syntactic gate: {diagnostics:?}"
        );
    }

    /// The binder strict-mode `delete`-on-identifier pair (TS1102 + TS2703) is
    /// semantic-phase in tsc, so a parse error in a sibling file drops both.
    #[test]
    fn strict_mode_delete_pair_suppressed_by_sibling_parse_error() {
        let strict_delete = "\"use strict\";\nvar victim = 1;\ndelete victim;\n";
        let alone = collect_test_diagnostics(&[("/strict.ts", strict_delete)]);
        let alone_codes: Vec<u32> = alone.iter().map(|d| d.code).collect();
        assert!(
            alone_codes.contains(&1102) && alone_codes.contains(&2703),
            "positive control: TS1102 + TS2703 must both fire without a parse error: {alone:?}"
        );

        let gated = collect_test_diagnostics(&[
            ("/strict.ts", strict_delete),
            ("/broken.ts", "var incomplete = ;\n"),
        ]);
        let gated_codes: Vec<u32> = gated.iter().map(|d| d.code).collect();
        assert!(
            gated_codes.contains(&1109),
            "the sibling's own parse error must still be reported: {gated:?}"
        );
        assert!(
            !gated_codes.contains(&1102) && !gated_codes.contains(&2703),
            "TS1102/TS2703 are semantic-phase in tsc and must be dropped when \
             any root file has a parse error: {gated:?}"
        );
    }

    /// TS1214 (reserved word as identifier in a module) is emitted by tsc's
    /// binder (`checkStrictModeIdentifier`), so it also vanishes when the
    /// syntactic gate arms — while still firing on its own.
    #[test]
    fn module_reserved_word_ts1214_suppressed_by_sibling_parse_error() {
        let module_reserved = "export var yield = 10;\n";
        let alone = collect_test_diagnostics(&[("/modulefile.ts", module_reserved)]);
        let alone_codes: Vec<u32> = alone.iter().map(|d| d.code).collect();
        assert!(
            alone_codes.contains(&1214),
            "positive control: TS1214 must fire without a parse error: {alone:?}"
        );

        let gated = collect_test_diagnostics(&[
            ("/modulefile.ts", module_reserved),
            ("/broken.ts", "var incomplete = ;\n"),
        ]);
        let gated_codes: Vec<u32> = gated.iter().map(|d| d.code).collect();
        assert!(
            !gated_codes.contains(&1214),
            "TS1214 is binder-emitted in tsc and must be dropped when any root \
             file has a parse error: {gated:?}"
        );
    }

    // ------------------------------------------------------------------
    // Class self-reference in a property-initializer type-parameter
    // constraint of a default-exported class (#17570 rows 5/6).
    //
    // Structural rule: a class identifier referenced from a type-parameter
    // constraint inside the class's own property-initializer function
    // expression denotes the class INSTANCE type. For `export default class`
    // the class node's symbol is the default-export binding (no `CLASS`
    // flag), and the eagerly-published value result (`typeof C`) sat in every
    // def-body/symbol cache when the deferred self-reference resolved — so
    // the constraint was judged against the static side and reported a
    // spurious TS2344 that tsc does not. The deferred `Lazy` must be minted
    // from the CLASS-flagged symbol and must never resolve to a
    // constructor-shaped value body.
    //
    // Every clean row is pinned against typescript@6.0.2. Binder names vary
    // across rows so the outcome tracks the structure, not a spelling.
    // ------------------------------------------------------------------

    /// Rows that must be clean: instance arrow property, property `function`
    /// expression, renamed binders, `InstanceType[typeof C]` constraint
    /// spelling, plain-class control, and the annotation-position control.
    #[test]
    fn export_default_class_self_constraint_in_property_initializer_is_clean() {
        let options = project_mode_es2015_strict_options();
        let base = std::path::Path::new("/");
        let rows: &[(&str, &str)] = &[
            (
                "instance-arrow",
                r#"
type MarkOf<R extends { readonly mark: unknown }> = R["mark"];
export default class Schema {
  readonly mark!: number;
  go = <R extends Schema>(b: (x: number) => MarkOf<R>): R => null as any;
}
"#,
            ),
            (
                "function-expression",
                r#"
type MarkOf<R extends { readonly mark: unknown }> = R["mark"];
export default class Schema {
  readonly mark!: number;
  go = function <R extends Schema>(b: (x: number) => MarkOf<R>): R { return null as any; };
}
"#,
            ),
            (
                "renamed-binders",
                r#"
type PickM<Zq extends { readonly tag: unknown }> = Zq["tag"];
export default class Widget {
  readonly tag!: string;
  run = <Zq extends Widget>(cb: (n: number) => PickM<Zq>): Zq => null as any;
}
"#,
            ),
            // (The `InstanceType<typeof Schema>` constraint spelling is also
            // clean, but `InstanceType` is a lib alias and this harness runs
            // lib-less — that row is oracle-verified in the PR's CLI matrix.)
            (
                "generic-class",
                r#"
type MarkOf<R extends { readonly mark: unknown }> = R["mark"];
export default class Schema<T> {
  readonly mark!: T;
  go = <R extends Schema<number>>(b: (x: number) => MarkOf<R>): R => null as any;
}
"#,
            ),
            (
                "plain-class-control",
                r#"
type MarkOf<R extends { readonly mark: unknown }> = R["mark"];
class Schema {
  readonly mark!: number;
  go = <R extends Schema>(b: (x: number) => MarkOf<R>): R => null as any;
}
export default Schema;
"#,
            ),
            (
                "annotation-position-control",
                r#"
type MarkOf<R extends { readonly mark: unknown }> = R["mark"];
export default class Schema {
  readonly mark!: number;
  go: <R extends Schema>(b: (x: number) => MarkOf<R>) => R = null as any;
}
"#,
            ),
        ];
        for (label, source) in rows {
            let diagnostics =
                collect_test_diagnostics_with_options(&[("/case.ts", *source)], &options, base);
            assert!(
                diagnostics.is_empty(),
                "row `{label}` must be tsc-clean; got: {diagnostics:#?}"
            );
        }
    }

    /// Negative control: a constraint the class genuinely violates (no `mark`
    /// member) must keep its real TS2344 — the fix defers only the
    /// under-construction window, it does not disable the check.
    #[test]
    fn export_default_class_self_constraint_genuine_violation_keeps_ts2344() {
        let options = project_mode_es2015_strict_options();
        let base = std::path::Path::new("/");
        let diagnostics = collect_test_diagnostics_with_options(
            &[(
                "/case.ts",
                r#"
type MarkOf<R extends { readonly mark: unknown }> = R["mark"];
export default class Schema {
  readonly other!: number;
  go = <R extends Schema>(b: (x: number) => MarkOf<R>): R => null as any;
}
"#,
            )],
            &options,
            base,
        );
        let codes: Vec<u32> = diagnostics.iter().map(|d| d.code).collect();
        assert!(
            codes.contains(&2344),
            "a genuinely violated constraint must still report TS2344: {diagnostics:#?}"
        );
    }

    /// The constructor's `prototype` property must also resolve to the
    /// instance type for a default-exported class — it shares the deferred
    /// self-reference mint with the constraint path.
    #[test]
    fn export_default_class_prototype_member_resolves_to_instance() {
        let options = project_mode_es2015_strict_options();
        let base = std::path::Path::new("/");
        let diagnostics = collect_test_diagnostics_with_options(
            &[(
                "/case.ts",
                r#"
export default class Schema {
  readonly mark!: number;
}
const m: number = Schema.prototype.mark;
"#,
            )],
            &options,
            base,
        );
        assert!(
            diagnostics.is_empty(),
            "`Schema.prototype.mark` must type as the instance member; got: {diagnostics:#?}"
        );
    }
