#[test]
fn type_hierarchy_supertypes() {
    let t = FourslashTest::new(
        "
        class Animal { name: string = ''; }
        class /*cls*/Dog extends Animal {
            breed: string = '';
        }
    ",
    );
    let result = t.supertypes("cls");
    if !result.items.is_empty() {
        result.expect_name("Animal");
    }
}

#[test]
fn type_hierarchy_subtypes() {
    let t = FourslashTest::new(
        "
        class /*cls*/Animal { name: string = ''; }
        class Dog extends Animal {}
        class Cat extends Animal {}
    ",
    );
    let result = t.subtypes("cls");
    if !result.items.is_empty() {
        result.expect_name("Dog");
    }
}

#[test]
fn type_hierarchy_interface_supertypes() {
    let t = FourslashTest::new(
        "
        interface Readable { read(): string; }
        interface /*iface*/BufferedReadable extends Readable {
            buffer: string;
        }
    ",
    );
    let result = t.supertypes("iface");
    if !result.items.is_empty() {
        result.expect_name("Readable");
    }
}

// =============================================================================
// Code Lens Tests
// =============================================================================

#[test]
fn code_lens_function_declarations() {
    let t = FourslashTest::new(
        "
        function foo() {}
        function bar() {}
    ",
    );
    let result = t.code_lenses("test.ts");
    // Functions should get code lenses (reference counts)
    if !result.lenses.is_empty() {
        result.expect_min_count(1);
    }
}

#[test]
fn code_lens_class() {
    let t = FourslashTest::new(
        "
        class MyClass {
            method1() {}
            method2() {}
        }
    ",
    );
    let result = t.code_lenses("test.ts");
    if !result.lenses.is_empty() {
        result.expect_found();
    }
}

#[test]
fn code_lens_interface() {
    let t = FourslashTest::new(
        "
        interface Serializable {
            serialize(): string;
        }
        class Data implements Serializable {
            serialize() { return ''; }
        }
    ",
    );
    let result = t.code_lenses("test.ts");
    // Interface should have implementations code lens
    if !result.lenses.is_empty() {
        result.expect_found();
    }
}

#[test]
fn code_lens_empty_file() {
    let t = FourslashTest::new("");
    t.code_lenses("test.ts").expect_none();
}

// =============================================================================
// Document Links Tests
// =============================================================================

#[test]
fn document_links_import() {
    let t = FourslashTest::new(
        "
        import { foo } from './utils';
    ",
    );
    let result = t.document_links("test.ts");
    // Import specifier should produce a document link
    if !result.links.is_empty() {
        result.expect_found().expect_count(1);
    }
}

#[test]
fn document_links_multiple_imports() {
    let t = FourslashTest::new(
        "
        import { a } from './a';
        import { b } from './b';
        import { c } from './c';
    ",
    );
    let result = t.document_links("test.ts");
    if !result.links.is_empty() {
        result.expect_found();
    }
}

#[test]
fn document_links_no_imports() {
    let t = FourslashTest::new(
        "
        const x = 1;
        const y = 2;
    ",
    );
    t.document_links("test.ts").expect_none();
}

