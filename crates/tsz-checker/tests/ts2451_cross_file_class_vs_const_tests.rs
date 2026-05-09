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

use tsz_checker::context::CheckerOptions;
use tsz_common::common::ModuleKind;

fn compile_script_files(files: &[(&str, &str)], entry_idx: usize) -> Vec<(u32, String, u32)> {
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

#[test]
fn namespace_variable_members_do_not_conflict_with_cross_file_globals() {
    let globals = "const exportedName = 1;\nconst localName = 2;\n";
    let members = "namespace Box {\n  export const exportedName = \"x\";\n  const localName = \"y\";\n  const exportedUse: string = exportedName;\n  const localUse: string = localName;\n}\n\nconst globalUse: number = 1;\n";

    let diags = compile_script_files(&[("globals.ts", globals), ("members.ts", members)], 1);
    let false_redecls: Vec<_> = diags
        .iter()
        .filter(|(code, msg, _)| {
            matches!(*code, 2300 | 2451)
                && (msg.contains("'exportedName'") || msg.contains("'localName'"))
        })
        .collect();

    assert!(
        false_redecls.is_empty(),
        "namespace members must not be compared against remote script globals; got: {false_redecls:?}. All diagnostics: {diags:?}"
    );
}
