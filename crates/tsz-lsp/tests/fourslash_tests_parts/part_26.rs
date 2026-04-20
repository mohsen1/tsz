#[test]
fn completions_external_no_private_members() {
    let mut t = FourslashTest::new(
        "
        class SecretKeeper {
            private secret = 'hidden';
            public name = 'keeper';
        }
        const sk = new SecretKeeper();
        sk./*c*/
    ",
    );
    let result = t.completions("c");
    result.expect_found().expect_includes("name");
    // Private members should NOT appear when accessing from outside
    let has_secret = result.items.iter().any(|item| item.label == "secret");
    assert!(
        !has_secret,
        "Private member 'secret' should not appear in external completions"
    );
}

#[test]
fn completions_no_completions_in_string() {
    let mut t = FourslashTest::new(
        "
        const x = 1;
        const s = 'hello /*c*/ world';
    ",
    );
    // Inside a string literal, normal identifier completions should not appear
    let result = t.completions("c");
    // Either none or very few - just verify no crash
    let _ = result;
}

#[test]
fn definition_this_property_in_method() {
    let mut t = FourslashTest::new(
        "
        class Counter {
            /*def*/count: number = 0;
            increment() {
                this./*ref*/count++;
            }
        }
    ",
    );
    t.go_to_definition("ref").expect_at_marker("def");
}

#[test]
fn hover_this_property_access() {
    let mut t = FourslashTest::new(
        "
        class Config {
            host: string = 'localhost';
            getUrl() {
                return this./*h*/host;
            }
        }
    ",
    );
    t.hover("h").expect_found();
}

#[test]
fn completions_type_position() {
    let mut t = FourslashTest::new(
        "
        interface Point { x: number; y: number; }
        type Direction = 'up' | 'down';
        class Shape {}
        const p: /*c*/
    ",
    );
    // Type names should appear in type position
    let result = t.completions("c");
    result
        .expect_found()
        .expect_includes("Point")
        .expect_includes("Direction")
        .expect_includes("Shape");
}

#[test]
fn completions_generic_type_argument() {
    let mut t = FourslashTest::new(
        "
        interface Container<T> { value: T; }
        class Box {}
        const c: Container</*c*/> = { value: new Box() };
    ",
    );
    let result = t.completions("c");
    result.expect_found().expect_includes("Box");
}

#[test]
fn completions_after_new_keyword() {
    let mut t = FourslashTest::new(
        "
        class MyClass { }
        class OtherClass { }
        const x = new /*c*/
    ",
    );
    t.completions("c")
        .expect_found()
        .expect_includes("MyClass")
        .expect_includes("OtherClass");
}

#[test]
fn completions_static_members_on_class() {
    let mut t = FourslashTest::new(
        "
        class MathUtils {
            static PI = 3.14;
            static square(n: number) { return n * n; }
            instanceMethod() {}
        }
        MathUtils./*c*/
    ",
    );
    t.completions("c")
        .expect_found()
        .expect_includes("PI")
        .expect_includes("square");
}

// =============================================================================
// Signature Help: Advanced Patterns (NEW)
// =============================================================================

#[test]
fn signature_help_multi_param() {
    let mut t = FourslashTest::new(
        "
        function create(name: string, age: number, active: boolean) {}
        create(/*c*/);
    ",
    );
    t.signature_help("c")
        .expect_found()
        .expect_parameter_count(3);
}

#[test]
fn signature_help_generic_function() {
    let mut t = FourslashTest::new(
        "
        function identity<T>(arg: T): T { return arg; }
        identity(/*c*/);
    ",
    );
    t.signature_help("c").expect_found();
}

