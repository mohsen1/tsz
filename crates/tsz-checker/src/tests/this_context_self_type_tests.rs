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
