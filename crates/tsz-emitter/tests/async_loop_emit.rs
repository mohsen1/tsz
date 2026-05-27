use tsz_common::common::ScriptTarget;
use tsz_emitter::output::printer::{PrintOptions, lower_and_print};
use tsz_parser::parser::ParserState;

fn emit_es5(source: &str) -> String {
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let opts = PrintOptions {
        target: ScriptTarget::ES5,
        remove_comments: true,
        ..PrintOptions::default()
    };
    lower_and_print(&parser.arena, root, opts).code
}

#[test]
fn async_while_branch_continue_stays_in_current_case() {
    let output = emit_es5(
        "declare var x, y: any;
        async function f() {
            while (x) {
                if (1) continue;
                await y;
            }
        }
        async function g() {
            D: while (x) {
                if (1) continue D;
                await y;
            }
        }",
    );

    assert!(
        output.contains("if (1)\n                        return [3 /*break*/, 0];\n                    return [4 /*yield*/, y];"),
        "Branch-local async while continues should emit one same-case generator backedge before the following await.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("return [3 /*break*/, 0];\n                    return [3 /*break*/, 0];"),
        "The async while loop must not append a duplicate backedge after a terminating continue branch.\nOutput:\n{output}"
    );
}

#[test]
fn async_while_branch_break_targets_loop_exit() {
    let output = emit_es5(
        "declare var x, y: any;
        async function f() {
            while (x) {
                if (1) break;
                await y;
            }
        }
        async function g() {
            H: while (x) {
                if (1) break H;
                await y;
            }
        }",
    );

    assert!(
        output.contains("if (1)\n                        return [3 /*break*/, 2];\n                    return [4 /*yield*/, y];"),
        "Branch-local async while breaks should target the loop exit in the current case, including labeled breaks.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("4294967295"),
        "Loop-exit placeholders inside branch-local labeled breaks must be patched before printing.\nOutput:\n{output}"
    );
}
