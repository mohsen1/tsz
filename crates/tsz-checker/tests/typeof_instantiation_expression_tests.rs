use tsz_checker::context::CheckerOptions;
use tsz_checker::diagnostics::Diagnostic;
use tsz_checker::test_utils::{check_source, check_source_with_libs, load_default_lib_files};

fn diagnostics(source: &str) -> Vec<Diagnostic> {
    check_source(source, "test.ts", CheckerOptions::default())
}

fn diagnostics_with_default_libs(source: &str) -> Vec<Diagnostic> {
    let libs = load_default_lib_files();
    assert!(!libs.is_empty(), "expected default libs to load");
    check_source_with_libs(source, "test.ts", CheckerOptions::default(), &libs)
}

fn assert_no_false_typeof_instantiation_diagnostics(diags: &[Diagnostic]) {
    let unexpected: Vec<_> = diags
        .iter()
        .filter(|diag| matches!(diag.code, 2304 | 2344 | 2503 | 2833))
        .collect();
    assert!(
        unexpected.is_empty(),
        "expected no namespace/constraint diagnostics for typeof instantiation expression, got {diags:#?}"
    );
}

#[test]
fn typeof_property_instantiation_resolves_as_value_property_chain() {
    let diags = diagnostics(
        r#"
type ReturnOf<T extends (...args: any[]) => any> =
    T extends (...args: any[]) => infer R ? R : never;

declare const ops: {
    convert<T>(value: unknown): T;
};

type Converted = ReturnOf<typeof ops.convert<string>>;
declare const converted: Converted;

const ok: string = converted;
const bad: number = converted;
"#,
    );

    assert_no_false_typeof_instantiation_diagnostics(&diags);
}

#[test]
fn nested_typeof_property_instantiation_resolves_with_renamed_bindings() {
    let diags = diagnostics(
        r#"
type ReturnOf<T extends (...args: any[]) => any> =
    T extends (...args: any[]) => infer R ? R : never;

declare const services: {
    mapper: {
        pick<U>(value: unknown): U;
    };
};

type Picked = ReturnOf<typeof services.mapper.pick<boolean>>;
declare const picked: Picked;

const ok: boolean = picked;
const bad: string = picked;
"#,
    );

    assert_no_false_typeof_instantiation_diagnostics(&diags);
}

#[test]
fn reported_array_map_typeof_instantiation_does_not_resolve_arr_as_namespace() {
    let diags = diagnostics_with_default_libs(
        r#"
const arr = [1, 2, 3];

type Mapper = typeof arr.map<string>;
"#,
    );

    assert_no_false_typeof_instantiation_diagnostics(&diags);
}

#[test]
fn return_type_of_array_map_instantiation_does_not_resolve_numbers_as_namespace() {
    let diags = diagnostics_with_default_libs(
        r#"
const numbers = [1, 2, 3];

type MapResult = ReturnType<typeof numbers.map<string>>;
declare const mapped: MapResult;

const ok: string[] = mapped;
"#,
    );

    assert_no_false_typeof_instantiation_diagnostics(&diags);
}
