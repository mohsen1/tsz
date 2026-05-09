use tsz_checker::context::CheckerOptions;
use tsz_checker::test_utils::{check_source, check_source_with_libs, load_lib_files};

const LIB_NAMES: &[&str] = &[
    "es5.d.ts",
    "dom.d.ts",
    "dom.iterable.d.ts",
    "es2015.d.ts",
    "es2015.core.d.ts",
    "es2015.collection.d.ts",
    "es2015.iterable.d.ts",
    "es2015.generator.d.ts",
    "es2015.promise.d.ts",
    "es2015.proxy.d.ts",
    "es2015.reflect.d.ts",
    "es2015.symbol.d.ts",
    "es2015.symbol.wellknown.d.ts",
    "es2024.object.d.ts",
];

fn diagnostics(source: &str) -> Vec<tsz_checker::diagnostics::Diagnostic> {
    check_source(source, "test.ts", CheckerOptions::default())
        .into_iter()
        .filter(|diagnostic| diagnostic.code != 2318)
        .collect()
}

fn diagnostics_with_libs(source: &str) -> Vec<tsz_checker::diagnostics::Diagnostic> {
    let options = CheckerOptions {
        strict: true,
        ..Default::default()
    };
    let libs = load_lib_files(LIB_NAMES);
    check_source_with_libs(source, "test.ts", options, &libs)
}

#[test]
fn invalid_explicit_type_arg_constraints_suppress_call_argument_cascades() {
    let source = r#"
function someGenerics1<T, U extends T>(n: T, m: number) { }
someGenerics1<string, number>(3, 4);

function someGenerics5<U extends number, T>(n: T, f: (x: U) => void) { }
someGenerics5<string, number>(null, null);
"#;

    let diagnostics = diagnostics(source);
    let ts2344 = diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.code == 2344)
        .count();
    let ts2345: Vec<_> = diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.code == 2345)
        .collect();

    assert_eq!(ts2344, 2, "expected one TS2344 for each bad type argument");
    assert!(
        ts2345.is_empty(),
        "invalid explicit type arguments should suppress same-call TS2345 cascades, got: {ts2345:#?}"
    );
}

#[test]
fn unresolved_sensitive_callback_context_uses_type_parameter_constraint() {
    let source = r#"
interface WindowLike {
    closed: boolean;
}

function someGenerics3<T extends WindowLike>(producer: () => T) { }
someGenerics3(() => '');
"#;

    let diagnostics = diagnostics(source);
    let ts2322: Vec<_> = diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.code == 2322)
        .collect();

    assert_eq!(
        ts2322.len(),
        1,
        "expected callback return to be checked against the generic constraint, got: {diagnostics:#?}"
    );
    assert!(
        ts2322[0].message_text.contains("WindowLike"),
        "expected diagnostic to mention the constraint type, got: {:?}",
        ts2322[0]
    );
}

#[test]
fn lib_backed_window_constraint_contextualizes_sensitive_callback_return() {
    let source = r#"
function someGenerics3<T extends Window>(producer: () => T) { }
someGenerics3(() => '');
someGenerics3<number>(() => 3);
"#;

    let diagnostics = diagnostics_with_libs(source);
    let ts2322: Vec<_> = diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.code == 2322)
        .collect();
    let ts2344: Vec<_> = diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.code == 2344)
        .collect();

    assert!(
        ts2322
            .iter()
            .any(|diagnostic| diagnostic.message_text.contains("Window")),
        "expected callback return to be checked against Window, got: {diagnostics:#?}"
    );
    assert!(
        ts2344
            .iter()
            .any(|diagnostic| diagnostic.message_text.contains("Window")),
        "expected explicit type argument to be checked against Window, got: {diagnostics:#?}"
    );
}

#[test]
fn object_group_by_key_constraint_uses_property_key_in_diagnostic() {
    let source = r#"
interface Employee {
    name: string;
}

const employees: Employee[] = [];
Object.groupBy(employees, employee => employee);
"#;

    let diagnostics = diagnostics_with_libs(source);
    let ts2322: Vec<_> = diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.code == 2322)
        .collect();

    assert!(
        ts2322.iter().any(|diagnostic| diagnostic
            .message_text
            .contains("Type 'Employee' is not assignable to type 'PropertyKey'.")),
        "expected Object.groupBy key constraint to display PropertyKey, got: {diagnostics:#?}"
    );
}

#[test]
fn ts2344_constraint_message_expands_keyof_any_to_primitive_key_union() {
    // tsc strips `aliasSymbol` from the constraint when emitting TS2344
    // ("does not satisfy the constraint 'X'"), so a `Record<K extends
    // keyof any, T>` (or any other constraint that evaluates to the
    // primitive-key union) renders as `string | number | symbol` rather
    // than as the lib alias `PropertyKey`. This mirrors tsc's behaviour for
    // tests like `compiler/jsxIntrinsicElementsTypeArgumentErrors.tsx` and
    // `conformance/types/mapped/mappedTypeErrors.ts`, both of which expect
    // the structural union in the TS2344 fingerprint.
    let source = r#"
type Record<K extends keyof any, T> = { [P in K]: T };
type T = Record<object, number>;
"#;

    let diagnostics = diagnostics(source);
    let ts2344: Vec<_> = diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.code == 2344)
        .collect();

    assert!(
        ts2344.iter().any(|diagnostic| diagnostic
            .message_text
            .contains("does not satisfy the constraint 'string | number | symbol'.")),
        "expected TS2344 to print the structural primitive-key union, got: {diagnostics:#?}"
    );
    assert!(
        ts2344
            .iter()
            .all(|diagnostic| !diagnostic.message_text.contains("constraint 'PropertyKey'")),
        "TS2344 must not collapse `keyof any` to the `PropertyKey` alias, got: {diagnostics:#?}"
    );
}
