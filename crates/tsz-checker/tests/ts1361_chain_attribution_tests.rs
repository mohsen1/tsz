//! TS1361 vs TS1362 attribution across multi-file alias chains.
//!
//! Invariant: when a value use of `X` is rejected because `X` is type-only,
//! the checker walks the alias chain and attributes the error to the
//! innermost declaration that *directly* uses a `type` keyword —
//! `import type`, `export type`, `import { type X }`, `export { type X }`,
//! or `import type X = require(...)`. A plain `export { Y as X }` or
//! `import { X }` that merely re-exports or re-imports a type-only symbol
//! is not a direct marker — the chain walk must continue past it.
//!
//! Reproduces `conformance/externalModules/typeOnly/chained.ts`.

use std::sync::Arc;
use tsz_binder::BinderState;
use tsz_checker::context::CheckerOptions;
use tsz_checker::module_resolution::build_module_resolution_maps;
use tsz_checker::state::CheckerState;
use tsz_common::common::ModuleKind;
use tsz_parser::parser::ParserState;
use tsz_solver::TypeInterner;

fn compile_module_files(files: &[(&str, &str)], entry_idx: usize) -> Vec<(u32, String)> {
    let mut arenas = Vec::with_capacity(files.len());
    let mut binders = Vec::with_capacity(files.len());
    let mut roots = Vec::with_capacity(files.len());
    let file_names: Vec<String> = files.iter().map(|(name, _)| (*name).to_string()).collect();

    for (name, source) in files {
        let mut parser = ParserState::new((*name).to_string(), (*source).to_string());
        let root = parser.parse_source_file();
        let mut binder = BinderState::new();
        binder.bind_source_file(parser.get_arena(), root);
        arenas.push(Arc::new(parser.get_arena().clone()));
        binders.push(Arc::new(binder));
        roots.push(root);
    }

    let (resolved_module_paths, resolved_modules) = build_module_resolution_maps(&file_names);

    let all_arenas = Arc::new(arenas);
    let all_binders = Arc::new(binders);
    let types = TypeInterner::new();
    let options = CheckerOptions {
        module: ModuleKind::CommonJS,
        ..CheckerOptions::default()
    };

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
    checker.ctx.set_lib_contexts(Vec::new());
    checker
        .ctx
        .set_resolved_module_paths(Arc::new(resolved_module_paths));
    checker.ctx.set_resolved_modules(resolved_modules);

    checker.check_source_file(roots[entry_idx]);

    checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code != 2318)
        .map(|d| (d.code, d.message_text.clone()))
        .collect()
}

/// Chain: `export type {{ A as B }}` → plain `export {{ B as C }}` →
/// `import type {{ C }}` + plain `export {{ C as D }}` → `import {{ D }}; new D()`.
///
/// `tsc` attributes the error to the innermost `import type` (TS1361), not
/// the outermost `export type` (TS1362). The plain re-exports in between
/// are not direct markers.
#[test]
fn chained_type_only_attributes_to_innermost_import_type() {
    let a = r#"
class A { a!: string }
export type { A as B };
export type Z = A;
"#;
    let b = r#"
import { Z as Y } from './a';
export { B as C } from './a';
"#;
    let c = r#"
import type { C } from './b';
export { C as D };
"#;
    let d = r#"
import { D } from './c';
new D();
"#;

    let diagnostics = compile_module_files(
        &[("./a.ts", a), ("./b.ts", b), ("./c.ts", c), ("./d.ts", d)],
        3,
    );

    let ts1361 = diagnostics
        .iter()
        .filter(|(c, _)| *c == 1361)
        .collect::<Vec<_>>();
    let ts1362 = diagnostics
        .iter()
        .filter(|(c, _)| *c == 1362)
        .collect::<Vec<_>>();

    assert_eq!(
        ts1361.len(),
        1,
        "Expected exactly one TS1361 attributing to the `import type` marker. \
         Got TS1361={ts1361:?}, TS1362={ts1362:?}, all={diagnostics:?}",
    );
    assert!(
        ts1362.is_empty(),
        "Did not expect TS1362 — the outer `export type` is not the nearest direct marker. \
         Got TS1362={ts1362:?}, all={diagnostics:?}",
    );
}

/// Sanity check: when the only direct type-only marker in the chain is
/// `export type`, TS1362 is still the correct attribution.
#[test]
fn export_type_only_chain_still_attributes_to_export_type() {
    let a = r#"
class A { a!: string }
export type { A as B };
"#;
    let b = r#"
import { B } from './a';
new B();
"#;

    let diagnostics = compile_module_files(&[("./a.ts", a), ("./b.ts", b)], 1);

    let ts1361 = diagnostics
        .iter()
        .filter(|(c, _)| *c == 1361)
        .collect::<Vec<_>>();
    let ts1362 = diagnostics
        .iter()
        .filter(|(c, _)| *c == 1362)
        .collect::<Vec<_>>();

    assert!(
        ts1361.is_empty(),
        "Did not expect TS1361 when no `import type` appears in the chain. \
         Got TS1361={ts1361:?}, all={diagnostics:?}",
    );
    assert_eq!(
        ts1362.len(),
        1,
        "Expected TS1362 for `export type` chain. \
         Got TS1362={ts1362:?}, all={diagnostics:?}",
    );
}
