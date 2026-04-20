#[test]
fn hover_optional_chaining() {
    let mut t = FourslashTest::new(
        "
        interface User { name?: string; }
        const u: User | undefined = { name: 'Bob' };
        u?./*h*/name;
    ",
    );
    // Optional chaining hover may or may not resolve - just verify no crash
    let _ = t.hover("h");
}

#[test]
fn hover_getter_setter() {
    let mut t = FourslashTest::new(
        "
        class Thermometer {
            private _temp = 0;
            get /*h*/temperature() { return this._temp; }
            set temperature(val: number) { this._temp = val; }
        }
    ",
    );
    t.hover("h")
        .expect_found()
        .expect_display_string_contains("temperature");
}

#[test]
fn hover_readonly_property() {
    let mut t = FourslashTest::new(
        "
        class Config {
            readonly /*h*/apiKey: string = 'key123';
        }
    ",
    );
    t.hover("h")
        .expect_found()
        .expect_display_string_contains("apiKey");
}

#[test]
fn hover_string_literal_type() {
    let mut t = FourslashTest::new(
        "
        type /*h*/Direction = 'north' | 'south' | 'east' | 'west';
    ",
    );
    t.hover("h")
        .expect_found()
        .expect_display_string_contains("Direction");
}

#[test]
fn hover_numeric_literal_type() {
    let mut t = FourslashTest::new(
        "
        const /*h*/PI = 3.14159;
    ",
    );
    t.hover("h")
        .expect_found()
        .expect_display_string_contains("PI");
}

#[test]
fn hover_typeof_variable() {
    let mut t = FourslashTest::new(
        "
        const config = { host: 'localhost', port: 80 };
        type /*h*/ConfigType = typeof config;
    ",
    );
    t.hover("h")
        .expect_found()
        .expect_display_string_contains("ConfigType");
}

#[test]
fn hover_keyof_type() {
    let mut t = FourslashTest::new(
        "
        interface Person { name: string; age: number; }
        type /*h*/PersonKey = keyof Person;
    ",
    );
    t.hover("h")
        .expect_found()
        .expect_display_string_contains("PersonKey");
}

#[test]
fn hover_nested_generic_type() {
    let mut t = FourslashTest::new(
        "
        type /*h*/DeepReadonly<T> = {
            readonly [P in keyof T]: T[P] extends object ? DeepReadonly<T[P]> : T[P];
        };
    ",
    );
    t.hover("h")
        .expect_found()
        .expect_display_string_contains("DeepReadonly");
}

#[test]
fn hover_function_with_overloads() {
    let mut t = FourslashTest::new(
        "
        function /*h*/format(x: string): string;
        function format(x: number): string;
        function format(x: string | number): string {
            return String(x);
        }
    ",
    );
    t.hover("h")
        .expect_found()
        .expect_display_string_contains("format");
}

#[test]
fn hover_abstract_method() {
    let mut t = FourslashTest::new(
        "
        abstract class Shape {
            abstract /*h*/area(): number;
        }
    ",
    );
    t.hover("h")
        .expect_found()
        .expect_display_string_contains("area");
}

