//! TS2451 vs TS2300 selection for cross-file module-augmentation conflicts.
//!
//! When a `declare module "./target"` augmentation declares an export whose
//! name also exists in the augmentation target, tsc emits TS2451
//! ("Cannot redeclare block-scoped variable") when BOTH declarations are
//! block-scoped variables (const/let), and TS2300 ("Duplicate identifier")
//! otherwise (e.g. a CommonJS `module.exports` property on the target side).
//!
//! Regression: `exportAsNamespace_augment.ts` and
//! `duplicateIdentifierRelatedSpans_moduleAugmentation.ts` both require
//! TS2451 but tsz was unconditionally forcing TS2300 whenever any cross-file
//! targeted-module-augmentation declaration was present in the conflict set.

use tsz_checker::context::CheckerOptions;
use tsz_common::common::ModuleKind;

fn compile_module_files(files: &[(&str, &str)], entry_idx: usize) -> Vec<(u32, String, u32)> {
    let entry_file = files[entry_idx].0;
    tsz_checker::test_utils::check_multi_file(
        files,
        entry_file,
        CheckerOptions {
            module: ModuleKind::CommonJS,
            ..CheckerOptions::default()
        },
    )
    .into_iter()
    .map(|d| (d.code, d.message_text, d.start))
    .collect()
}

/// When BOTH the original and the module-augmentation declarations are
/// `export const` (block-scoped), tsc reports TS2451, not TS2300.
#[test]
fn targeted_augmentation_const_const_uses_ts2451() {
    let a = "export const x = 0;\n";
    let b = "export {};\n\
             declare module \"./a\" {\n    export const x = 0;\n}\n";

    // Check both files — each should see the "x" redeclaration as TS2451.
    for entry in [0usize, 1usize] {
        let diags = compile_module_files(&[("a.ts", a), ("b.ts", b)], entry);
        let for_x: Vec<_> = diags
            .iter()
            .filter(|(_, msg, _)| msg.contains("'x'"))
            .collect();
        assert!(
            !for_x.is_empty(),
            "entry={entry} — expected a duplicate-identifier diagnostic for 'x', got: {diags:?}"
        );
        assert!(
            for_x.iter().all(|(code, _, _)| *code == 2451),
            "entry={entry} — all 'x' diagnostics must be TS2451 when both declarations are \
             block-scoped (const/let). Got: {for_x:?}"
        );
    }
}

/// When the augmentation target exports a non-block-scoped symbol (e.g. a
/// CommonJS `module.exports` property in a JS file) and the augmentation adds
/// an `export const` with the same name, tsc reports TS2300.
///
/// This locks in the invariant that the force-TS2300 override is gated on
/// there being a *genuinely* non-block-scoped declaration somewhere in the
/// conflict set — not just on the mere presence of a cross-file augmentation.
#[test]
fn targeted_augmentation_commonjs_const_uses_ts2300() {
    let test_js = "module.exports = { a: \"ok\" };\n";
    let index_ts = "import { a } from \"./test\";\n\
                    export {};\n\
                    declare module \"./test\" {\n    export const a: number;\n}\n";

    // We assert on the TS consumer's perspective (the augmenting file), where
    // the augmentation's `export const a: number;` is local.
    let diags = compile_module_files(&[("test.js", test_js), ("index.ts", index_ts)], 1);
    let for_a: Vec<_> = diags
        .iter()
        .filter(|(code, msg, _)| matches!(*code, 2300 | 2451) && msg.contains("'a'"))
        .collect();

    assert!(
        !for_a.is_empty(),
        "expected a duplicate-identifier diagnostic for 'a', got: {diags:?}"
    );
    assert!(
        for_a.iter().any(|(code, _, _)| *code == 2300),
        "augmentation-vs-CommonJS-property conflict should emit TS2300 (non-block-scoped \
         target); got: {for_a:?}"
    );
    assert!(
        for_a.iter().all(|(code, _, _)| *code != 2451),
        "augmentation-vs-CommonJS-property conflict must not emit TS2451; got: {for_a:?}"
    );
}

#[test]
fn type_only_reexported_function_conflicts_with_augmentation_const_as_ts2451() {
    let files = [
        (
            "main.ts",
            "import { Row } from \"./index\";\n\
             Row();\n",
        ),
        (
            "a.d.ts",
            "import \"./index\";\n\
             declare module \"./index\" {\n    const Row: () => void;\n}\n",
        ),
        ("index.d.ts", "export type { Row } from \"./common\";\n"),
        ("common.d.ts", "export declare function Row(): void;\n"),
    ];

    let all_diags: Vec<_> = (0..files.len())
        .flat_map(|entry_idx| compile_module_files(&files, entry_idx))
        .collect();
    let row_diags: Vec<_> = all_diags
        .iter()
        .filter(|(_, msg, _)| msg.contains("'Row'") || msg.contains("Row"))
        .collect();
    let ts2451_count = row_diags
        .iter()
        .filter(|(code, _, _)| *code == 2451)
        .count();

    assert!(
        ts2451_count >= 2,
        "expected TS2451 on the augmentation const and type-only reexport specifier; got {row_diags:?}"
    );
    assert!(
        row_diags.iter().all(|(code, _, _)| *code != 2300),
        "function-valued type-only reexport vs augmentation const should not be TS2300; got {row_diags:?}"
    );
    assert!(
        row_diags
            .iter()
            .all(|(code, _, _)| *code != 1362 && *code != 2349),
        "augmentation value should suppress type-only value and non-callable cascades; got {row_diags:?}"
    );
}
