#[test]
fn compile_nullish_coalescing() {
    // Test nullish coalescing (??) compilation
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist"
          },
          "include": ["src/**/*.ts"]
        }"#,
    );

    write_file(
        &base.join("src/nullish.ts"),
        r#"
export function getValueOrDefault(value: string | null | undefined): string {
    return value ?? "default";
}

export function getNumberOrZero(num: number | null): number {
    return num ?? 0;
}
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(
        result.diagnostics.is_empty(),
        "Should compile without errors: {:?}",
        result.diagnostics
    );

    let js = std::fs::read_to_string(base.join("dist/src/nullish.js")).expect("read js");
    assert!(!js.is_empty(), "JS output should not be empty");
}

#[test]
fn compile_optional_chaining_with_call() {
    // Test optional chaining with method calls
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist"
          },
          "include": ["src/**/*.ts"]
        }"#,
    );

    write_file(
        &base.join("src/optcall.ts"),
        r#"
interface Logger {
    log?: (msg: string) => void;
}

export function maybeLog(logger: Logger, msg: string): void {
    logger.log?.(msg);
}
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(
        result.diagnostics.is_empty(),
        "Should compile without errors: {:?}",
        result.diagnostics
    );

    let js = std::fs::read_to_string(base.join("dist/src/optcall.js")).expect("read js");
    assert!(!js.is_empty(), "JS output should not be empty");
}

#[test]
fn compile_class_inheritance() {
    // Test class inheritance compilation
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist"
          },
          "include": ["src/**/*.ts"]
        }"#,
    );

    write_file(
        &base.join("src/classes.ts"),
        r#"
export class Animal {
    constructor(public name: string) {}
    speak(): string {
        return this.name;
    }
}

export class Dog extends Animal {
    constructor(name: string) {
        super(name);
    }
    speak(): string {
        return "Woof: " + super.speak();
    }
}
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(
        result.diagnostics.is_empty(),
        "Should compile without errors: {:?}",
        result.diagnostics
    );

    let js = std::fs::read_to_string(base.join("dist/src/classes.js")).expect("read js");
    assert!(!js.is_empty(), "JS output should not be empty");
}

#[test]
fn compile_class_static_members() {
    // Test class static members compilation
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist"
          },
          "include": ["src/**/*.ts"]
        }"#,
    );

    write_file(
        &base.join("src/staticclass.ts"),
        r#"
export class Counter {
    static count: number = 0;

    static increment(): number {
        Counter.count += 1;
        return Counter.count;
    }

    static reset(): void {
        Counter.count = 0;
    }
}
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(
        result.diagnostics.is_empty(),
        "Should compile without errors: {:?}",
        result.diagnostics
    );

    let js = std::fs::read_to_string(base.join("dist/src/staticclass.js")).expect("read js");
    assert!(!js.is_empty(), "JS output should not be empty");
}

#[test]
fn compile_class_accessors() {
    // Test class getter/setter compilation
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist"
          },
          "include": ["src/**/*.ts"]
        }"#,
    );

    write_file(
        &base.join("src/accessors.ts"),
        r#"
export class Rectangle {
    private _width: number = 0;
    private _height: number = 0;

    get width(): number {
        return this._width;
    }

    set width(value: number) {
        this._width = value;
    }

    get area(): number {
        return this._width * this._height;
    }
}
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(
        result.diagnostics.is_empty(),
        "Should compile without errors: {:?}",
        result.diagnostics
    );

    let js = std::fs::read_to_string(base.join("dist/src/accessors.js")).expect("read js");
    assert!(!js.is_empty(), "JS output should not be empty");
}

#[test]
fn compile_computed_property_names() {
    // Test computed property names compilation
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist"
          },
          "include": ["src/**/*.ts"]
        }"#,
    );

    write_file(
        &base.join("src/computed.ts"),
        r#"
const KEY = "dynamicKey";

export const obj = {
    [KEY]: "value",
    ["literal" + "Key"]: 42
};

export function getProp(key: string): { [k: string]: number } {
    return { [key]: 100 };
}
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(
        result.diagnostics.is_empty(),
        "Should compile without errors: {:?}",
        result.diagnostics
    );

    let js = std::fs::read_to_string(base.join("dist/src/computed.js")).expect("read js");
    assert!(!js.is_empty(), "JS output should not be empty");
}
