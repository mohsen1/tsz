//! TS1205 vs TS1448 code selection for plain (non-type-only) re-export chains
//! under `isolatedModules` / `verbatimModuleSyntax`.
//!
//! Structural rule (tsc `checkAliasSymbol`): for a plain `export { X } from
//! "..."` specifier, tsc picks the diagnostic code purely on `isType` —
//! whether the alias, resolved through the FULL re-export chain, lands on a
//! declaration carrying no runtime value:
//!
//!   message = isType ? TS1205 : TS1448
//!
//! A plain re-export whose chain ends at a pure type (interface / type alias)
//! is **TS1205 at every hop**, no matter how many re-export hops sit between
//! the specifier and the original declaration. TS1448 ("resolves to a
//! type-only declaration") is reserved for a target that DOES carry a runtime
//! value but was marked type-only (`import type` / `export type`) somewhere in
//! the chain.
//!
//! Regression witness: #17101 — tsz reported TS1205 on the first hop but
//! TS1448 on every hop after it, because the immediate-target fast path only
//! catches the first hop (where the target is the declaration itself) and the
//! deeper hops fell through to an unconditional-TS1448 branch.

use std::sync::Arc;
use tsz_binder::BinderState;
use tsz_checker::context::CheckerOptions;
use tsz_checker::module_resolution::build_module_resolution_maps;
use tsz_checker::state::CheckerState;
use tsz_common::common::ModuleKind;
use tsz_parser::parser::ParserState;
use tsz_solver::construction::TypeInterner;

/// Compile a multi-file project checking `files[entry_idx]` as the entry file
/// and return `(code, message, start)` for each diagnostic. `verbatim`
/// selects `verbatimModuleSyntax`; otherwise `isolatedModules` is used.
fn compile(files: &[(&str, &str)], entry_idx: usize, verbatim: bool) -> Vec<(u32, String, u32)> {
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
        isolated_modules: !verbatim,
        verbatim_module_syntax: verbatim,
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
        .map(|d| (d.code, d.message_text.clone(), d.start))
        .collect()
}

fn codes(diags: &[(u32, String, u32)]) -> Vec<u32> {
    diags.iter().map(|(c, _, _)| *c).collect()
}

/// The three-hop chain from #17101: every hop is a plain re-export of a pure
/// interface. tsc reports TS1205 at every hop; the deep hops must not be
/// TS1448.
fn interface_chain() -> Vec<(&'static str, &'static str)> {
    vec![
        ("/impl.ts", "export interface Foo {}\n"),
        ("/a.ts", "export { Foo } from \"./impl\";\n"),
        ("/b.ts", "export { Foo } from \"./a\";\n"),
        ("/reexport.ts", "export { Foo } from \"./b\";\n"),
    ]
}

#[test]
fn hop1_plain_reexport_of_interface_is_ts1205() {
    let diags = compile(&interface_chain(), 1, false);
    assert!(
        codes(&diags).contains(&1205),
        "hop 1 (a.ts) should report TS1205; got {diags:?}"
    );
    assert!(
        !codes(&diags).contains(&1448),
        "hop 1 (a.ts) must not report TS1448; got {diags:?}"
    );
}

#[test]
fn hop2_plain_reexport_of_interface_is_ts1205_not_ts1448() {
    let diags = compile(&interface_chain(), 2, false);
    assert!(
        codes(&diags).contains(&1205),
        "hop 2 (b.ts) should report TS1205; got {diags:?}"
    );
    assert!(
        !codes(&diags).contains(&1448),
        "hop 2 (b.ts) must not report TS1448 — the chain has no `export type`; got {diags:?}"
    );
}

#[test]
fn hop3_plain_reexport_of_interface_is_ts1205_not_ts1448() {
    let diags = compile(&interface_chain(), 3, false);
    assert!(
        codes(&diags).contains(&1205),
        "hop 3 (reexport.ts) should report TS1205; got {diags:?}"
    );
    assert!(
        !codes(&diags).contains(&1448),
        "hop 3 (reexport.ts) must not report TS1448; got {diags:?}"
    );
}

/// Type-alias variant of the chain — same rule, different type-decl kind,
/// to prove the fix is structural (not interface-specific).
#[test]
fn hop2_plain_reexport_of_type_alias_is_ts1205_not_ts1448() {
    let files = vec![
        ("/impl.ts", "export type Bar = number;\n"),
        ("/a.ts", "export { Bar } from \"./impl\";\n"),
        ("/b.ts", "export { Bar } from \"./a\";\n"),
    ];
    let diags = compile(&files, 2, false);
    assert!(
        codes(&diags).contains(&1205),
        "hop 2 of a type-alias chain should report TS1205; got {diags:?}"
    );
    assert!(
        !codes(&diags).contains(&1448),
        "hop 2 of a type-alias chain must not report TS1448; got {diags:?}"
    );
}

/// Renamed binders at each hop — the fix must not key on the name matching
/// across hops.
#[test]
fn renamed_reexport_chain_is_ts1205_not_ts1448() {
    let files = vec![
        ("/impl.ts", "export interface Original {}\n"),
        ("/a.ts", "export { Original as Mid } from \"./impl\";\n"),
        ("/b.ts", "export { Mid as Outer } from \"./a\";\n"),
    ];
    let diags = compile(&files, 2, false);
    assert!(
        codes(&diags).contains(&1205),
        "renamed hop-2 re-export should report TS1205; got {diags:?}"
    );
    assert!(
        !codes(&diags).contains(&1448),
        "renamed hop-2 re-export must not report TS1448; got {diags:?}"
    );
}

/// Control: a plain re-export chain whose target is a runtime **value**
/// (`class`) is clean at every hop — no TS1205 and no TS1448.
#[test]
fn value_target_reexport_chain_is_clean() {
    let files = vec![
        ("/impl.ts", "export class Foo {}\n"),
        ("/a.ts", "export { Foo } from \"./impl\";\n"),
        ("/b.ts", "export { Foo } from \"./a\";\n"),
    ];
    let diags = compile(&files, 2, false);
    assert!(
        !codes(&diags).contains(&1205) && !codes(&diags).contains(&1448),
        "a value-target re-export chain must be clean; got {diags:?}"
    );
}

/// TS1448 discriminator on a DEEP hop: the resolved target carries a runtime
/// value (`class`), and the chain crosses an explicit `export type` at a hop
/// *before* this specifier's immediate target. Because the immediate target
/// here is a plain re-export alias (not itself `export type`), this reaches
/// the same code-selection branch as the pure-type chain and must resolve to
/// TS1448, proving the branch keys on the fully-resolved target's runtime
/// value, not on the nearest hop.
#[test]
fn deep_value_target_crossing_export_type_is_ts1448() {
    let files = vec![
        ("/impl.ts", "export class Foo {}\n"),
        ("/a.ts", "export type { Foo } from \"./impl\";\n"),
        ("/b.ts", "export { Foo } from \"./a\";\n"),
        ("/c.ts", "export { Foo } from \"./b\";\n"),
    ];
    let diags = compile(&files, 3, false);
    assert!(
        codes(&diags).contains(&1448),
        "a value re-exported plainly two hops after crossing `export type` should be TS1448; \
         got {diags:?}"
    );
    assert!(
        !codes(&diags).contains(&1205),
        "the deep value-crossing-export-type case must not be TS1205; got {diags:?}"
    );
}
