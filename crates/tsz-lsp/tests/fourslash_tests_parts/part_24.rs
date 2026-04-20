#[test]
fn definition_class_extends_target() {
    let mut t = FourslashTest::new(
        "
        class /*def*/Animal {
            name: string = '';
        }
        class Dog extends /*ref*/Animal {
            breed: string = '';
        }
    ",
    );
    t.go_to_definition("ref").expect_at_marker("def");
}

#[test]
fn definition_interface_extends_target() {
    let mut t = FourslashTest::new(
        "
        interface /*def*/Serializable {
            serialize(): string;
        }
        interface JsonSerializable extends /*ref*/Serializable {
            toJSON(): object;
        }
    ",
    );
    t.go_to_definition("ref").expect_at_marker("def");
}

#[test]
fn definition_class_implements_target() {
    let mut t = FourslashTest::new(
        "
        interface /*def*/Disposable {
            dispose(): void;
        }
        class Resource implements /*ref*/Disposable {
            dispose() {}
        }
    ",
    );
    t.go_to_definition("ref").expect_at_marker("def");
}

// =============================================================================
// Rename: Advanced Patterns (NEW)
// =============================================================================

#[test]
fn rename_interface() {
    let mut t = FourslashTest::new(
        "
        interface /*r*/Config { host: string; }
        const c: Config = { host: '' };
        function setup(c: Config) {}
    ",
    );
    t.rename("r", "Options")
        .expect_success()
        .expect_total_edits(3);
}

#[test]
fn rename_enum() {
    let mut t = FourslashTest::new(
        "
        enum /*r*/Status { Active, Inactive }
        const s: Status = Status.Active;
    ",
    );
    t.rename("r", "State")
        .expect_success()
        .expect_total_edits(3);
}

#[test]
fn rename_type_alias() {
    let mut t = FourslashTest::new(
        "
        type /*r*/ID = string;
        const id: ID = '123';
    ",
    );
    t.rename("r", "Identifier")
        .expect_success()
        .expect_total_edits(2);
}

#[test]
fn rename_function_with_calls() {
    let mut t = FourslashTest::new(
        "
        function /*r*/greet(name: string) { return 'Hello ' + name; }
        greet('Alice');
        greet('Bob');
    ",
    );
    t.rename("r", "sayHello")
        .expect_success()
        .expect_total_edits(3);
}

#[test]
fn rename_class_across_type_and_value() {
    let mut t = FourslashTest::new(
        "
        class /*r*/Foo { x = 1; }
        const f: Foo = new Foo();
    ",
    );
    t.rename("r", "Bar").expect_success().expect_total_edits(3);
}

#[test]
#[ignore = "requires destructuring pattern rename support"]
fn rename_destructured() {
    let mut t = FourslashTest::new(
        "
        const { /*r*/name, age } = { name: 'Alice', age: 30 };
        console.log(name);
    ",
    );
    t.rename("r", "fullName")
        .expect_success()
        .expect_total_edits(2);
}

#[test]
fn rename_across_scopes() {
    let mut t = FourslashTest::new(
        "
        function outer() {
            const /*r*/val = 1;
            function inner() {
                return val + 1;
            }
            return val + inner();
        }
    ",
    );
    t.rename("r", "result")
        .expect_success()
        .expect_total_edits(3);
}

// =============================================================================
// Completions: Advanced Patterns (NEW)
// =============================================================================

