//! Regression tests for issue #14216 (mined from purify-ts `Either.ts`/`Maybe.ts`).
//!
//! `export { box as Box }` re-exports a distinct local binding under the name of
//! an existing local declaration (`class Box {}`). tsc keeps the local
//! declaration for every in-module reference (value and type) and re-aliases
//! only at the export boundary, so the public export `Box` resolves to `box`
//! while `new Box()` / `: Box` in-module resolve to the class. tsz used to
//! overwrite the scope/file-local slot with the alias source, producing spurious
//! TS2749 at type sites and TS2552 at value sites. The binder now routes the
//! colliding export through the file's module-export surface, leaving local
//! resolution intact.

use tsz_checker::context::CheckerOptions;
use tsz_common::common::ModuleKind;

fn compile_entry_file(files: &[(&str, &str)], entry_idx: usize) -> Vec<(u32, String)> {
    let entry_file = files[entry_idx].0;
    tsz_checker::test_utils::check_multi_file(
        files,
        entry_file,
        CheckerOptions {
            module: ModuleKind::ESNext,
            strict: true,
            strict_null_checks: true,
            ..CheckerOptions::default()
        },
    )
    .into_iter()
    .filter(|diag| diag.code != 2318)
    .map(|diag| (diag.code, diag.message_text))
    .collect()
}

#[test]
fn export_alias_does_not_clobber_local_class_in_value_and_type_space() {
    // `class Box` creates both a value and a type; `export { box as Box }` must
    // not re-point in-module references to the lowercase function.
    let source = r#"
class Box {
    constructor(public value: number) {}
}
const useType = (b: Box): number => b.value;
const useValue = (): Box => new Box(1);
const box = (n: number) => new Box(n);
export { box as Box };
"#;

    let diagnostics = compile_entry_file(&[("m.ts", source)], 0);

    assert!(
        !diagnostics.iter().any(|(code, _)| *code == 2749),
        "local class used as a type must not report TS2749 after an export alias \
         collides with its name: {diagnostics:#?}"
    );
    assert!(
        !diagnostics.iter().any(|(code, _)| *code == 2552),
        "local class used as a value must not report TS2552 after an export alias \
         collides with its name: {diagnostics:#?}"
    );
}

#[test]
fn export_alias_collision_with_renamed_binders_stays_clean() {
    // Vary the binder names: the rule is structural, not keyed off `Box`/`box`.
    let source = r#"
class Widget {
    constructor(public payload: string) {}
}
const reader = (w: Widget): string => w.payload;
const maker = (): Widget => new Widget("a");
const factory = (s: string) => new Widget(s);
export { factory as Widget };
"#;

    let diagnostics = compile_entry_file(&[("renamed.ts", source)], 0);

    assert!(
        !diagnostics
            .iter()
            .any(|(code, _)| *code == 2749 || *code == 2552),
        "renamed export-alias collision must stay clean (no TS2749/TS2552): {diagnostics:#?}"
    );
}

#[test]
fn cross_file_import_resolves_to_alias_target_not_local_declaration() {
    // Load-bearing: `import { Box }` from another module must resolve to the
    // alias target `box` (a callable returning a `Box` instance), not the local
    // class. Calling `Box(7)` and reading `.value` must type-check.
    let module_source = r#"
class Box {
    constructor(public value: number) {}
}
const useType = (b: Box): number => b.value;
const useValue = (): Box => new Box(1);
const box = (n: number) => new Box(n);
export { box as Box };
"#;

    let consumer_source = r#"
import { Box } from "./m";
const made = Box(7);
const n: number = made.value;
"#;

    let diagnostics = compile_entry_file(
        &[("m.ts", module_source), ("consumer.ts", consumer_source)],
        1,
    );

    assert!(
        diagnostics.is_empty(),
        "cross-file import of the aliased export must resolve to the alias target \
         and type-check cleanly: {diagnostics:#?}"
    );
}
