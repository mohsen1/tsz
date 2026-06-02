use tsz_checker::test_utils::check_source_code_messages as diagnostics;

fn ts2322_messages(source: &str) -> Vec<String> {
    diagnostics(source)
        .into_iter()
        .filter_map(|(code, msg)| (code == 2322).then_some(msg))
        .collect()
}

#[test]
fn polymorphic_this_intersection_source_displays_simple_head() {
    let source = r#"
class Box {
    self!: this;
    set(value: string & { brand: true }) {
        this.self = value;
    }
}
"#;

    let msgs = ts2322_messages(source);
    assert_eq!(msgs.len(), 1, "expected one TS2322, got: {msgs:#?}");
    assert!(
        msgs[0].contains("Type 'string' is not assignable to type 'this'."),
        "polymorphic this assignment should display the simple intersection head, got: {msgs:#?}"
    );
    assert!(
        !msgs[0].contains("string & { brand: true; }"),
        "polymorphic this assignment should not render the whole intersection when the head is simple, got: {msgs:#?}"
    );
}

#[test]
fn polymorphic_this_application_intersection_source_keeps_whole_display() {
    let source = r#"
interface Holder<T> {
    value: T;
}

class Box {
    self!: this;
    set(value: Holder<string> & { brand: true }) {
        this.self = value;
    }
}
"#;

    let msgs = ts2322_messages(source);
    assert_eq!(msgs.len(), 1, "expected one TS2322, got: {msgs:#?}");
    assert!(
        msgs[0]
            .contains("Type 'Holder<string> & { brand: true; }' is not assignable to type 'this'."),
        "application intersection source should keep the whole source display, got: {msgs:#?}"
    );
}

#[test]
fn derived_polymorphic_this_assignment_displays_derived_class_source() {
    let source = r#"
class C {
    self = this;
}

class D extends C {
    d = new D();
    bar() {
        this.self = this.d;
    }
}
"#;

    let msgs = ts2322_messages(source);
    assert_eq!(msgs.len(), 1, "expected one TS2322, got: {msgs:#?}");
    assert!(
        msgs[0].contains("Type 'D' is not assignable to type 'this'."),
        "derived polymorphic this assignment should display the derived class, got: {msgs:#?}"
    );
    assert!(
        !msgs[0].contains("D & C"),
        "derived polymorphic this source should not keep the evaluated class intersection, got: {msgs:#?}"
    );
}

#[test]
fn renamed_derived_polymorphic_this_assignment_displays_derived_class_source() {
    let source = r#"
class Parent {
    self = this;
}

class Child extends Parent {
    child = new Child();
    set() {
        this.self = this.child;
    }
}
"#;

    let msgs = ts2322_messages(source);
    assert_eq!(msgs.len(), 1, "expected one TS2322, got: {msgs:#?}");
    assert!(
        msgs[0].contains("Type 'Child' is not assignable to type 'this'."),
        "renamed derived polymorphic this assignment should display the derived class, got: {msgs:#?}"
    );
    assert!(
        !msgs[0].contains("Child & Parent"),
        "renamed case should not depend on specific class names or keep the evaluated intersection, got: {msgs:#?}"
    );
}
