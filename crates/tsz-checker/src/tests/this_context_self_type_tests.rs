use crate::test_utils::check_source_diagnostics;

fn diagnostic_codes(source: &str) -> Vec<u32> {
    check_source_diagnostics(source)
        .into_iter()
        .map(|diag| diag.code)
        .collect()
}

#[test]
fn method_this_parameter_accepts_same_class_instance_receiver() {
    let codes = diagnostic_codes(
        r#"
class Handler {
  value = 10;

  handle(this: Handler, x: number): number {
    return this.value + x;
  }
}

const h = new Handler();
h.handle(5);
"#,
    );

    assert!(
        codes.is_empty(),
        "expected no diagnostics for same class instance receiver; got {codes:?}"
    );
}

#[test]
fn method_this_parameter_accepts_renamed_same_class_instance_receiver() {
    let codes = diagnostic_codes(
        r#"
class Runner {
  count = 1;

  run(this: Runner, step: number): number {
    return this.count + step;
  }
}

const r = new Runner();
r.run(2);
"#,
    );

    assert!(
        codes.is_empty(),
        "expected no diagnostics for renamed same class receiver; got {codes:?}"
    );
}

#[test]
fn method_this_parameter_rejects_unbound_call_without_receiver() {
    let codes = diagnostic_codes(
        r#"
class Owner {
  value = 1;

  handle(this: Owner, x: number): number {
    return this.value + x;
  }
}

const borrowed = new Owner().handle;
borrowed(1);
"#,
    );

    assert!(
        codes.contains(&2684),
        "expected TS2684 for unbound call without receiver; got {codes:?}"
    );
}

#[test]
fn explicit_this_annotation_different_class_default_param_no_error() {
    // When a method has `this: OtherClass`, default parameter values that call
    // `this.method()` should use OtherClass as the this type, not the enclosing class.
    let codes = diagnostic_codes(
        r#"
class Example {
    getNumber(): number { return 1; }
}
class Weird {
    doSomething(this: Example, a = this.getNumber()) {
        return a;
    }
}
"#,
    );
    assert!(
        codes.is_empty(),
        "expected no diagnostics for explicit this annotation in default param; got {codes:?}"
    );
}

#[test]
fn explicit_this_annotation_free_function_default_param_no_error() {
    // Same rule applies to free functions with explicit `this:` annotations.
    let codes = diagnostic_codes(
        r#"
class Example {
    getNumber(): number { return 1; }
}
function weird(this: Example, a = this.getNumber()) {
    return a;
}
"#,
    );
    assert!(
        codes.is_empty(),
        "expected no diagnostics for explicit this annotation in free function default param; got {codes:?}"
    );
}

#[test]
fn explicit_this_annotation_different_name_no_error() {
    // Renamed type parameter variable shouldn't matter — the rule is structural.
    let codes = diagnostic_codes(
        r#"
class Provider {
    getValue(): string { return ""; }
}
class Consumer {
    process(this: Provider, x = this.getValue()) {
        return x;
    }
}
"#,
    );
    assert!(
        codes.is_empty(),
        "expected no diagnostics when this annotation refers to a differently-named class; got {codes:?}"
    );
}
