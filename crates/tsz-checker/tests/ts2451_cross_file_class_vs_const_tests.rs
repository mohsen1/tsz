//! TS2451 vs TS2300 selection for cross-file script-scope conflicts where a
//! local non-block-scoped declaration (class/function) collides with a remote
//! block-scoped variable (let/const).
//!
//! When two script files share global scope and one declares `const`/`let`
//! while another declares `class`/`function` with the same name, tsc reports
//! TS2451 ("Cannot redeclare block-scoped variable") on every conflicting
//! declaration — not just on the block-scoped ones.
//!
//! Regression: `duplicateIdentifierRelatedSpans1.ts` was emitting TS2300 for
//! the `class Bar {}` declaration in file2.ts when file1.ts had `const Bar`
//! at script scope. The diagnostic chooser inspected only the conflicts set
//! (which holds local declarations), so the remote `const Bar`'s
//! `BLOCK_SCOPED_VARIABLE` flag was invisible to the cross-file branch.

use std::sync::Arc;
use tsz_binder::BinderState;
use tsz_checker::context::CheckerOptions;
use tsz_checker::module_resolution::build_module_resolution_maps;
use tsz_checker::state::CheckerState;
use tsz_common::common::ModuleKind;
use tsz_parser::parser::ParserState;
use tsz_solver::TypeInterner;

fn compile_script_files(files: &[(&str, &str)], entry_idx: usize) -> Vec<(u32, String, u32)> {
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
        .map(|d| (d.code, d.message_text.clone(), d.start))
        .collect()
}

/// When a local `class Bar {}` in script file collides with a remote `const Bar`
/// in another script file, tsc emits TS2451 ("Cannot redeclare block-scoped
/// variable") on the class declaration — the redeclaration error subsumes the
/// generic "duplicate identifier" diagnostic when ANY conflicting declaration
/// (local or remote) is block-scoped.
#[test]
fn cross_file_class_vs_remote_const_uses_ts2451() {
    let file1 = "class Foo { }\nconst Bar = 3;\n";
    let file2 = "type Foo = number;\nclass Bar {}\n";
    let file3 = "type Foo = 54;\nlet Bar = 42\n";

    // Entry = file2.ts (where `class Bar {}` is local). The remote `const Bar`
    // in file1 and `let Bar` in file3 are both block-scoped, so the local
    // class redeclaration must surface as TS2451 — not TS2300.
    let diags = compile_script_files(
        &[
            ("file1.ts", file1),
            ("file2.ts", file2),
            ("file3.ts", file3),
        ],
        1,
    );
    let bar_diags: Vec<_> = diags
        .iter()
        .filter(|(code, msg, _)| matches!(*code, 2300 | 2451) && msg.contains("'Bar'"))
        .collect();

    assert!(
        !bar_diags.is_empty(),
        "expected duplicate-identifier diagnostic for 'Bar' in file2.ts, got: {diags:?}"
    );
    assert!(
        bar_diags.iter().all(|(code, _, _)| *code == 2451),
        "class-vs-remote-const conflict at script scope must emit TS2451 only; got: {bar_diags:?}"
    );
}

/// Same scenario but entry = file3.ts where `let Bar` is local. The local
/// `let` is itself block-scoped, so this branch was already correct, but we
/// lock it in alongside the new file2 case to catch any regression that
/// accidentally narrows the `BLOCK_SCOPED_VARIABLE` detection.
#[test]
fn cross_file_let_vs_remote_const_uses_ts2451() {
    let file1 = "class Foo { }\nconst Bar = 3;\n";
    let file2 = "type Foo = number;\nclass Bar {}\n";
    let file3 = "type Foo = 54;\nlet Bar = 42\n";

    let diags = compile_script_files(
        &[
            ("file1.ts", file1),
            ("file2.ts", file2),
            ("file3.ts", file3),
        ],
        2,
    );
    let bar_diags: Vec<_> = diags
        .iter()
        .filter(|(code, msg, _)| matches!(*code, 2300 | 2451) && msg.contains("'Bar'"))
        .collect();

    assert!(
        !bar_diags.is_empty(),
        "expected duplicate-identifier diagnostic for 'Bar' in file3.ts, got: {diags:?}"
    );
    assert!(
        bar_diags.iter().all(|(code, _, _)| *code == 2451),
        "let-vs-remote-const conflict at script scope must emit TS2451; got: {bar_diags:?}"
    );
}
