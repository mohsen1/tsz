use tsz_checker::test_utils::check_js_source_code_messages;
use tsz_checker::test_utils::check_source_code_messages;

/// Regression test for: checker emits TS1262 for only the first top-level `await`
/// declaration in an external module. Each illegal `await` binding must get its
/// own TS1262 (plus any TS2451 redeclaration diagnostics).
///
/// See: <https://github.com/mohsen1/tsz/issues/2816>
#[test]
fn ts1262_emitted_for_each_await_declaration_in_module() {
    let diagnostics = check_source_code_messages(
        r#"
export {};

const await = 1;
let await = 2;
var await = 3;
"#,
    );

    let ts1262_count = diagnostics.iter().filter(|(code, _)| *code == 1262).count();
    assert_eq!(
        ts1262_count, 3,
        "expected TS1262 for each of const/let/var `await` declarations, got {ts1262_count}; full diagnostics: {diagnostics:#?}"
    );
}

/// A single `await` declaration in a module must still produce exactly one TS1262.
#[test]
fn ts1262_single_await_declaration_still_reported() {
    let diagnostics = check_source_code_messages(
        r#"
export {};

const await = 1;
"#,
    );

    let ts1262_count = diagnostics.iter().filter(|(code, _)| *code == 1262).count();
    assert_eq!(
        ts1262_count, 1,
        "expected exactly one TS1262 for a single `await` declaration; got {ts1262_count}; full diagnostics: {diagnostics:#?}"
    );
}

/// In a non-module (script) file, `await` is not reserved at the top level
/// and must not produce TS1262.
#[test]
fn ts1262_not_emitted_in_script_file() {
    let diagnostics = check_source_code_messages(
        r#"
const await = 1;
"#,
    );

    let ts1262_count = diagnostics.iter().filter(|(code, _)| *code == 1262).count();
    assert_eq!(
        ts1262_count, 0,
        "expected no TS1262 in a non-module script file; got {ts1262_count}; full diagnostics: {diagnostics:#?}"
    );
}

/// Destructured top-level `await` bindings in modules must report TS1262 even
/// when spacing or declaration kind differs from the narrow raw-text fallback.
#[test]
fn ts1262_emitted_for_destructured_await_bindings_in_module() {
    let diagnostics = check_source_code_messages(
        r#"
export {};

var { await } = { await: 1 };
let {await} = { await: 2 };
const {await} = { await: 3 };
var [ await ] = [4];
"#,
    );

    let ts1262_count = diagnostics.iter().filter(|(code, _)| *code == 1262).count();
    assert_eq!(
        ts1262_count, 4,
        "expected TS1262 for each destructured `await` binding; got {ts1262_count}; full diagnostics: {diagnostics:#?}"
    );
}

/// Checked JavaScript files with module syntax use the same top-level reserved
/// `await` gate as TypeScript modules.
#[test]
fn ts1262_emitted_for_checked_js_await_declaration_in_module() {
    let diagnostics = check_js_source_code_messages(
        r#"
const await = 1;
export {};
"#,
    );

    let ts1262_count = diagnostics.iter().filter(|(code, _)| *code == 1262).count();
    assert_eq!(
        ts1262_count, 1,
        "expected TS1262 for checked-JS module `await` declaration; got {ts1262_count}; full diagnostics: {diagnostics:#?}"
    );
}

/// Import-equals declarations store their binding name directly on the import
/// declaration, not in an import clause. The AST path must still report TS1262
/// without falling back to source-text scanning.
#[test]
fn ts1262_emitted_for_import_equals_await_name() {
    let diagnostics = check_source_code_messages(
        r#"
import await = require("pkg");
"#,
    );

    let ts1262_count = diagnostics.iter().filter(|(code, _)| *code == 1262).count();
    assert_eq!(
        ts1262_count, 1,
        "expected TS1262 for import-equals `await` binding; got {ts1262_count}; full diagnostics: {diagnostics:#?}"
    );
}
