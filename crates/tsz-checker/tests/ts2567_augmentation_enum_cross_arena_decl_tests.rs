//! TS2567 "Enum declarations can only merge with namespace or other enum
//! declarations" — partner diagnostic anchoring across module arenas.
//!
//! When a module augmentation's `export enum Foo { }` resolves to an existing
//! class/function/interface via a wildcard-re-export chain, tsc anchors the
//! partner TS2567 at the *original* declaration (e.g., the `Foo` identifier
//! of `export class Foo` in the source file), not only at the augmentation
//! site. tsz was failing to emit the partner diagnostic because its node
//! lookup used the re-export-chain-resolved file's arena instead of the
//! arena actually owning the declaration nodes (`symbol.decl_file_idx`).
//!
//! This test doesn't exercise the full cross-file conformance runner path
//! (the harness is single-file), but asserts the no-regression case:
//! augmentation declared against a local class in a single file still emits
//! TS2567 correctly.

use tsz_binder::BinderState;
use tsz_checker::CheckerState;
use tsz_parser::parser::ParserState;
use tsz_solver::TypeInterner;

fn get_diagnostic_codes(source: &str) -> Vec<u32> {
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
        Default::default(),
    );

    checker.check_source_file(root);

    checker.ctx.diagnostics.iter().map(|d| d.code).collect()
}

#[test]
fn augmentation_enum_merged_with_class_still_emits_ts2567() {
    // Single-file baseline: the augmentation path still works when the
    // original class and the augmentation are in the same file. (The
    // cross-file variant is exercised by the
    // `moduleAugmentationEnumClassMergeOfReexportIsError` conformance test.)
    let source = r#"
export class Foo {}
declare module "./test" {
    export enum Foo { A, B, C }
}
"#;
    let codes = get_diagnostic_codes(source);
    assert!(
        codes.contains(&2567),
        "augmentation-enum merging with an existing class must emit TS2567; got: {codes:?}"
    );
}
