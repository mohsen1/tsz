use crate::diagnostics::Diagnostic;
use crate::test_utils::check_source_diagnostics;

fn diagnostic_messages<'a>(diagnostics: &[&'a Diagnostic]) -> Vec<&'a str> {
    diagnostics
        .iter()
        .map(|diagnostic| diagnostic.message_text.as_str())
        .collect()
}

#[test]
fn constructor_parameters_rest_spread_is_iterable() {
    let diags = check_source_diagnostics(
        r#"
function create<T extends new (...args: any[]) => any>(
  ctor: T,
  ...args: ConstructorParameters<T>
): InstanceType<T> {
  return new ctor(...args);
}

class MyClass2 {
  constructor(public x: number) {}
}

const inst = create(MyClass2, 42);
"#,
    );

    let errors: Vec<_> = diags
        .iter()
        .filter(|d| d.code == 2488 || d.code == 2345 || d.code == 2322)
        .collect();
    assert_eq!(
        errors.len(),
        0,
        "Expected ConstructorParameters<T> rest spread to be accepted, got: {:?}",
        diagnostic_messages(&errors)
    );
}
