use crate::context::{CheckerOptions, ScriptTarget};
use crate::test_utils::{check_source_with_libs, load_compiled_lib_files};

#[test]
fn promise_all_readonly_tuple_prefers_tuple_overload_from_merged_libs() {
    let libs = load_compiled_lib_files(&[
        "lib.es5.d.ts",
        "lib.es2015.core.d.ts",
        "lib.es2015.collection.d.ts",
        "lib.es2015.iterable.d.ts",
        "lib.es2015.generator.d.ts",
        "lib.es2015.promise.d.ts",
        "lib.es2015.symbol.d.ts",
        "lib.es2015.symbol.wellknown.d.ts",
    ]);
    assert!(
        libs.iter()
            .any(|lib| lib.file_name == "lib.es2015.promise.d.ts"),
        "expected Promise lib to load"
    );
    let diagnostics = check_source_with_libs(
        r#"
const promises = [Promise.resolve(0)] as const;
Promise.all(promises).then((results) => {
    const tuple: [number] = results;
    const second = results[1];
});
"#,
        "test.ts",
        CheckerOptions {
            strict_null_checks: false,
            target: ScriptTarget::ESNext,
            ..CheckerOptions::default()
        },
        &libs,
    );
    let codes: Vec<_> = diagnostics.iter().map(|diag| diag.code).collect();
    assert!(
        codes.contains(
            &crate::diagnostics::diagnostic_codes::TUPLE_TYPE_OF_LENGTH_HAS_NO_ELEMENT_AT_INDEX,
        ),
        "expected Promise.all tuple overload to expose TS2493, got: {diagnostics:?}"
    );
    assert!(
        !diagnostics.iter().any(|diag| {
            diag.code == crate::diagnostics::diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE
                && diag.message_text.contains("number[]")
                && diag.message_text.contains("[number]")
        }),
        "Promise.all readonly tuple argument must not fall through to the iterable overload: {diagnostics:?}"
    );
}
