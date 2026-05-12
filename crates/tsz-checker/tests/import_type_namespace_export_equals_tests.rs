use tsz_checker::context::CheckerOptions;
use tsz_checker::test_utils::check_multi_file;
use tsz_common::ModuleKind;

fn options() -> CheckerOptions {
    CheckerOptions {
        module: ModuleKind::CommonJS,
        no_lib: true,
        ..CheckerOptions::default()
    }
}

/// Regression: `import("./mod").Bar.Q` should produce
/// `Namespace '"mod".Bar' has no exported member 'Q'.`
/// not `Namespace '"mod".export=.Bar' has no exported member 'Q'.`
#[test]
fn import_type_nested_segment_omits_export_equals_in_namespace_name() {
    let diags = check_multi_file(
        &[
            (
                "/mod.d.ts",
                r#"
declare namespace mod {
    namespace Bar { function method(): void; }
}
export = mod;
"#,
            ),
            (
                "/test.ts",
                r#"
type X = import("./mod").Bar.Q;
"#,
            ),
        ],
        "/test.ts",
        options(),
    );
    let ts2694: Vec<&str> = diags
        .iter()
        .filter(|d| d.code == 2694)
        .map(|d| d.message_text.as_str())
        .collect();
    assert!(
        !ts2694.is_empty(),
        "expected TS2694 for missing member Q, got: {diags:?}"
    );
    assert!(
        ts2694
            .iter()
            .any(|m| m.contains("\"mod\".Bar") && !m.contains("export=")),
        "expected namespace name '\"mod\".Bar' (no .export=), got: {ts2694:?}"
    );
}

/// Direct member access (zero segments) should still include `.export=` to
/// match tsc behaviour for that case.
#[test]
fn import_type_direct_member_preserves_export_equals_in_namespace_name() {
    let diags = check_multi_file(
        &[
            (
                "/mod.d.ts",
                r#"
declare namespace mod {
    namespace Bar { function method(): void; }
}
export = mod;
"#,
            ),
            (
                "/test.ts",
                r#"
type Y = import("./mod").DirectMember;
"#,
            ),
        ],
        "/test.ts",
        options(),
    );
    let ts2694: Vec<&str> = diags
        .iter()
        .filter(|d| d.code == 2694)
        .map(|d| d.message_text.as_str())
        .collect();
    assert!(
        !ts2694.is_empty(),
        "expected TS2694 for missing member DirectMember, got: {diags:?}"
    );
    assert!(
        ts2694.iter().any(|m| m.contains("\"mod\".export=")),
        "expected namespace name '\"mod\".export=' for zero-segment case, got: {ts2694:?}"
    );
}
