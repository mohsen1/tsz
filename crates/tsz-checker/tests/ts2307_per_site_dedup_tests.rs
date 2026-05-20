//! Ensure TS2307 "Cannot find module" is emitted per import-equals site, not
//! deduplicated by module name across the whole file.
//!
//! Reproduces `importDeclRefereingExternalModuleWithNoResolve.ts`, where the
//! same unresolved specifier `"externalModule"` appears in two distinct
//! `import X = require(...)` statements — one at file level, one inside a
//! `declare module "m1"` augmentation. tsc reports TS2307 at both sites. tsz
//! was deduplicating by module name across the entire file and only reported
//! the first.

use tsz_binder::BinderState;
use tsz_checker::CheckerState;
use tsz_parser::parser::ParserState;
use tsz_solver::TypeInterner;

#[test]
fn ts2307_dedup_set_is_cleared_at_start_of_each_import_equals_check() {
    // Drives the core invariant behind the fix for
    // `importDeclRefereingExternalModuleWithNoResolve.ts`: each
    // `import X = require("...")` must not be suppressed by a previous
    // statement's TS2307 emission of the same module name. We observe
    // this by seeding `modules_with_ts2307_emitted` before check, letting
    // the checker run, and asserting the seed entry is gone — proving the
    // dedup is cleared on each statement entry. Full integration coverage
    // (two TS2307 emissions at distinct positions) lives in the
    // conformance test `importDeclRefereingExternalModuleWithNoResolve`.
    let source = r#"import b = require("externalModule");
declare module "m1" {
    import im2 = require("externalModule");
}
"#;
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
    checker
        .ctx
        .modules_with_ts2307_emitted
        .insert("externalModule".to_string());
    assert!(
        checker
            .ctx
            .modules_with_ts2307_emitted
            .contains("externalModule"),
        "seed should be present before check"
    );

    checker.check_source_file(root);

    assert!(
        !checker
            .ctx
            .modules_with_ts2307_emitted
            .contains("externalModule"),
        "check_import_equals_declaration should clear the per-module dedupe entry for each statement, so the set must no longer contain `externalModule` after checking"
    );
}
