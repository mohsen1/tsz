//! Regression tests for TS4112 ("This member cannot have an 'override'
//! modifier because its containing class does not extend another class") on
//! cross-file class heritage.
//!
//! When a class extends a class imported from another module, the base's full
//! instance type may not be resolvable at the derived class's use site (for
//! example a heavily generic base, as in the Kysely benchmark row). tsz must
//! not conclude from a failed base-type resolution that the class "does not
//! extend another class": TS4112 fires only when there is no class base at all
//! (no `extends` clause, an interface base, or an unresolved name). The gate
//! keys off whether the `extends` target resolves to a class, not off whether
//! the base type happened to resolve.

use std::sync::Arc;

use tsz_binder::BinderState;
use tsz_binder::lib_loader::LibFile;
use tsz_checker::context::{CheckerOptions, LibContext};
use tsz_checker::state::CheckerState;
use tsz_common::common::ModuleKind;
use tsz_parser::parser::ParserState;
use tsz_solver::construction::TypeInterner;

fn check_multi_file(
    files: &[(&str, &str)],
    entry_file: &str,
    options: CheckerOptions,
    lib_files: &[Arc<LibFile>],
) -> Vec<tsz_checker::diagnostics::Diagnostic> {
    let mut arenas = Vec::with_capacity(files.len());
    let mut binders = Vec::with_capacity(files.len());
    let mut roots = Vec::with_capacity(files.len());
    let file_names: Vec<String> = files.iter().map(|(name, _)| (*name).to_string()).collect();

    for (name, source) in files {
        let mut parser = ParserState::new((*name).to_string(), (*source).to_string());
        let root = parser.parse_source_file();
        let mut binder = BinderState::new();
        binder.bind_source_file_with_libs(parser.get_arena(), root, lib_files);
        arenas.push(Arc::new(parser.get_arena().clone()));
        binders.push(Arc::new(binder));
        roots.push(root);
    }

    let entry_idx = file_names
        .iter()
        .position(|name| name == entry_file)
        .unwrap_or_else(|| panic!("entry_file {entry_file:?} not found in files"));
    let (resolved_module_paths, resolved_modules) =
        tsz_checker::module_resolution::build_module_resolution_maps(&file_names);

    let all_arenas = Arc::new(arenas);
    let all_binders = Arc::new(binders);
    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        all_arenas[entry_idx].as_ref(),
        all_binders[entry_idx].as_ref(),
        &types,
        file_names[entry_idx].clone(),
        options,
    );
    checker.ctx.set_all_arenas(Arc::clone(&all_arenas));
    checker.ctx.set_all_binders(Arc::clone(&all_binders));
    checker.ctx.set_current_file_idx(entry_idx);
    let lib_contexts: Vec<LibContext> = lib_files
        .iter()
        .map(|lib| LibContext {
            arena: Arc::clone(&lib.arena),
            binder: Arc::clone(&lib.binder),
        })
        .collect();
    checker.ctx.set_lib_contexts(lib_contexts);
    checker.ctx.set_actual_lib_file_count(lib_files.len());
    checker
        .ctx
        .set_resolved_module_paths(Arc::new(resolved_module_paths));
    checker.ctx.set_resolved_modules(resolved_modules);

    checker.check_source_file(roots[entry_idx]);
    checker.ctx.diagnostics.clone()
}

fn opts() -> CheckerOptions {
    CheckerOptions {
        module: ModuleKind::ESNext,
        strict: true,
        ..CheckerOptions::default()
    }
}

fn has_code(diags: &[tsz_checker::diagnostics::Diagnostic], code: u32) -> bool {
    diags.iter().any(|d| d.code == code)
}

fn codes(diags: &[tsz_checker::diagnostics::Diagnostic]) -> Vec<u32> {
    diags.iter().map(|d| d.code).collect()
}

/// A derived class that extends a generic class imported from another module
/// must not emit TS4112 on its `override` members, even if the base's full
/// instance type cannot be resolved at the use site. Kysely repro shape.
#[test]
fn cross_file_generic_base_override_no_ts4112() {
    let diags = check_multi_file(
        &[
            (
                "./query-creator.ts",
                r#"
export interface KyselyPlugin { name: string }
export class QueryCreator<DB> {
  withPlugin(plugin: KyselyPlugin): QueryCreator<DB> { return this; }
  withoutPlugins(): QueryCreator<DB> { return this; }
}
"#,
            ),
            (
                "./kysely.ts",
                r#"
import { QueryCreator, type KyselyPlugin } from "./query-creator.ts";
export class Kysely<DB> extends QueryCreator<DB> {
  override withPlugin(plugin: KyselyPlugin): Kysely<DB> { return this; }
  override withoutPlugins(): Kysely<DB> { return this; }
}
"#,
            ),
        ],
        "./kysely.ts",
        opts(),
        &[],
    );

    assert!(
        !has_code(&diags, 4112),
        "cross-file generic base override must not emit TS4112, got: {:?}",
        codes(&diags)
    );
}

/// The same rule must hold regardless of the type-parameter name chosen on
/// either the base or the derived class — the gate is structural, not
/// name-based.
#[test]
fn cross_file_generic_base_override_no_ts4112_renamed_params() {
    let diags = check_multi_file(
        &[
            (
                "./base.ts",
                r#"
export class Repo<Row> {
  find(): Row | undefined { return undefined; }
}
"#,
            ),
            (
                "./derived.ts",
                r#"
import { Repo } from "./base.ts";
export class CachedRepo<Entity> extends Repo<Entity> {
  override find(): Entity | undefined { return undefined; }
}
"#,
            ),
        ],
        "./derived.ts",
        opts(),
        &[],
    );

    assert!(
        !has_code(&diags, 4112),
        "renamed-type-parameter cross-file base override must not emit TS4112, got: {:?}",
        codes(&diags)
    );
}

/// Importing an interface (not a class) and using it as the `extends` target is
/// not extending a class, so TS4112 must still fire. Guards against the gate
/// over-suppressing.
#[test]
fn cross_file_interface_base_still_ts4112() {
    let diags = check_multi_file(
        &[
            (
                "./shape.ts",
                r#"
export interface Shape { area(): number }
"#,
            ),
            (
                "./circle.ts",
                r#"
import { Shape } from "./shape.ts";
export class Circle extends Shape {
  override area(): number { return 0; }
}
"#,
            ),
        ],
        "./circle.ts",
        opts(),
        &[],
    );

    assert!(
        has_code(&diags, 4112),
        "extending an imported interface must still emit TS4112, got: {:?}",
        codes(&diags)
    );
}
