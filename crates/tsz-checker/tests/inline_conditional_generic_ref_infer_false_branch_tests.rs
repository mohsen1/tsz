//! Regression tests for #9696.
//!
//! Structural rule: when an inline conditional return type checks an inferred
//! type parameter against a generic type reference containing `infer` and the
//! relation is false, the false branch must substitute the inferred parameter.

use tsz_checker::test_utils::check_source_strict;

fn assert_ts2322(source: &str, label: &str) {
    let diagnostics = check_source_strict(source);
    let ts2322: Vec<_> = diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.code == 2322)
        .collect();
    assert!(
        !ts2322.is_empty(),
        "[{label}] expected TS2322 from the false branch result, got diagnostics: {diagnostics:#?}"
    );
}

fn assert_ts2322_with_default_libs(source: &str, label: &str) {
    let libs = tsz_checker::test_utils::load_default_lib_files();
    let diagnostics = tsz_checker::test_utils::check_source_with_libs(
        source,
        "test.ts",
        tsz_checker::context::CheckerOptions {
            strict: true,
            ..Default::default()
        },
        &libs,
    );
    let ts2322: Vec<_> = diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.code == 2322)
        .collect();
    assert!(
        !ts2322.is_empty(),
        "[{label}] expected TS2322 from the false branch result, got diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn inline_promise_infer_false_branch_preserves_inferred_argument() {
    let source = r#"
declare function unwrap<Input>(value: Input): Input extends Promise<infer Value> ? Value : Input;

const result = unwrap(42);
const bad: string = result;
"#;

    assert_ts2322_with_default_libs(source, "Promise<infer Value> false branch");
}

#[test]
fn inline_generic_ref_infer_false_branch_preserves_inferred_argument() {
    let source = r#"
interface Holder<Item> { value: Item; }
declare function unwrap<Input>(value: Input): Input extends Holder<infer Value> ? Value : Input;

const result = unwrap("hello");
const bad: number = result;
"#;

    assert_ts2322(source, "Holder<infer Value> false branch");
}

#[test]
fn inline_multi_arg_generic_ref_infer_false_branch_preserves_inferred_argument() {
    let source = r#"
interface Pair<Left, Right> { left: Left; right: Right; }
declare function choose<Candidate>(value: Candidate): Candidate extends Pair<string, infer Picked> ? Picked : Candidate;

const result = choose(99);
const bad: string = result;
"#;

    assert_ts2322(source, "Pair<string, infer Picked> false branch");
}

#[test]
fn named_alias_form_still_preserves_false_branch() {
    let source = r#"
interface Wrapper<Element> { element: Element; }
type Unwrap<Subject> = Subject extends Wrapper<infer Inner> ? Inner : Subject;
declare function convert<Given>(value: Given): Unwrap<Given>;

const result = convert(true);
const bad: string = result;
"#;

    assert_ts2322(source, "named alias false branch");
}
