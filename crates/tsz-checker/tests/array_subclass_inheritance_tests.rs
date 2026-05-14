use tsz_checker::context::CheckerOptions;

fn strict_diagnostics(source: &str) -> Vec<(u32, String)> {
    let libs = tsz_checker::test_utils::load_default_lib_files();
    tsz_checker::test_utils::check_source_with_libs(
        source,
        "test.ts",
        CheckerOptions {
            strict: true,
            ..CheckerOptions::default()
        },
        &libs,
    )
    .into_iter()
    .map(|d| (d.code, d.message_text))
    .collect()
}

#[test]
fn array_subclass_inherits_array_methods() {
    let diagnostics = strict_diagnostics(
        r#"
class ExtendableArray<T> extends Array<T> {
  static get [Symbol.species]() {
    return Array;
  }
}

declare const ea: ExtendableArray<number>;
const mapped = ea.map(x => x * 2);
const filtered = ea.filter(x => x > 1);
const first: number = ea[0];
const lengthCheck: number = ea.length;
const mappedCheck: number[] = mapped;
const filteredCheck: number[] = filtered;
"#,
    );

    let unexpected: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| matches!(*code, 2322 | 2339 | 7006))
        .collect();
    assert!(
        unexpected.is_empty(),
        "array subclass should inherit Array<T> members without TS2339/TS7006/TS2322, got: {diagnostics:#?}"
    );
}
