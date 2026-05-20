use tsz_checker::test_utils::check_source_diagnostics;

fn diagnostic_codes(source: &str) -> Vec<u32> {
    check_source_diagnostics(source)
        .into_iter()
        .map(|diag| diag.code)
        .collect()
}

#[test]
fn abstract_class_argument_rejected_for_concrete_constructor_parameter() {
    let codes = diagnostic_codes(
        r#"
abstract class Animal {
  abstract makeSound(): string;
  constructor(public name: string) {}
}

class Dog extends Animal {
  makeSound() { return "woof"; }
}

function createInstance<T extends Animal>(
  Ctor: new (name: string) => T,
  name: string
): T {
  return new Ctor(name);
}

createInstance(Animal, "Abstract");
"#,
    );

    assert!(
        codes.contains(&2345),
        "expected TS2345 when abstract class is passed to concrete constructor parameter; got {codes:?}"
    );
}

#[test]
fn concrete_class_argument_still_assigns_to_concrete_constructor_parameter() {
    let codes = diagnostic_codes(
        r#"
abstract class Base {
  abstract id(): string;
  constructor(public name: string) {}
}

class Concrete extends Base {
  id() { return this.name; }
}

function createInstance<T extends Base>(
  Ctor: new (name: string) => T,
  name: string
): T {
  return new Ctor(name);
}

createInstance(Concrete, "ok");
"#,
    );

    assert!(
        codes.is_empty(),
        "expected concrete class to satisfy concrete constructor parameter; got {codes:?}"
    );
}

#[test]
fn abstract_class_argument_assigns_to_abstract_constructor_parameter() {
    let codes = diagnostic_codes(
        r#"
abstract class Shape {
  abstract area(): number;
  constructor(public name: string) {}
}

function receiveAbstract<T extends Shape>(
  Ctor: abstract new (name: string) => T,
  name: string
): void {
  void Ctor;
  void name;
}

receiveAbstract(Shape, "shape");
"#,
    );

    assert!(
        codes.is_empty(),
        "expected abstract class to satisfy abstract constructor parameter; got {codes:?}"
    );
}
