#[test]
fn compile_multi_file_project_with_imports() {
    // End-to-end test for a multi-file project.
    // Note: Uses namespace imports to avoid known named import type resolution issues.
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    // Create tsconfig.json with CommonJS module for testable require() output
    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist",
            "rootDir": "src",
            "module": "commonjs",
            "declaration": true,
            "sourceMap": true
          },
          "include": ["src/**/*.ts"]
        }"#,
    );

    // src/models/user.ts - basic model with interface and class
    write_file(
        &base.join("src/models/user.ts"),
        r#"
export interface User {
    id: number;
    name: string;
    email: string;
}

export class UserImpl implements User {
    id: number;
    name: string;
    email: string;

    constructor(id: number, name: string, email: string) {
        this.id = id;
        this.name = name;
        this.email = email;
    }

    getDisplayName(): string {
        return this.name + " <" + this.email + ">";
    }
}

export type UserId = number;
"#,
    );

    // src/utils/helpers.ts - utility functions
    // Note: Avoid String.prototype.indexOf due to lib.d.ts resolution issues
    write_file(
        &base.join("src/utils/helpers.ts"),
        r#"
export function formatName(first: string, last: string): string {
    return first + " " + last;
}

export function validateEmail(email: string): boolean {
    return email.length > 0;
}

export const DEFAULT_PAGE_SIZE = 20;
"#,
    );

    // src/services/user-service.ts - service using namespace imports
    write_file(
        &base.join("src/services/user-service.ts"),
        r#"
import * as models from '../models/user';
import * as helpers from '../utils/helpers';

export class UserService {
    private users: models.User[] = [];

    createUser(id: number, firstName: string, lastName: string, email: string): models.User | null {
        if (!helpers.validateEmail(email)) {
            return null;
        }
        const name = helpers.formatName(firstName, lastName);
        const user = new models.UserImpl(id, name, email);
        this.users.push(user);
        return user;
    }

    getUserCount(): number {
        return this.users.length;
    }
}
"#,
    );

    // src/index.ts - main entry point using namespace re-exports
    write_file(
        &base.join("src/index.ts"),
        r#"
// Re-export all from each module
export * from './models/user';
export * from './utils/helpers';
export * from './services/user-service';
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(
        result.diagnostics.is_empty(),
        "Expected no diagnostics, got: {:?}",
        result.diagnostics
    );

    // Verify all output files exist
    assert!(
        base.join("dist/models/user.js").is_file(),
        "models/user.js should exist"
    );
    assert!(
        base.join("dist/models/user.d.ts").is_file(),
        "models/user.d.ts should exist"
    );
    assert!(
        base.join("dist/models/user.js.map").is_file(),
        "models/user.js.map should exist"
    );

    assert!(
        base.join("dist/utils/helpers.js").is_file(),
        "utils/helpers.js should exist"
    );
    assert!(
        base.join("dist/utils/helpers.d.ts").is_file(),
        "utils/helpers.d.ts should exist"
    );

    assert!(
        base.join("dist/services/user-service.js").is_file(),
        "services/user-service.js should exist"
    );
    assert!(
        base.join("dist/services/user-service.d.ts").is_file(),
        "services/user-service.d.ts should exist"
    );

    assert!(
        base.join("dist/index.js").is_file(),
        "index.js should exist"
    );
    assert!(
        base.join("dist/index.d.ts").is_file(),
        "index.d.ts should exist"
    );

    // Verify user-service.js has correct CommonJS require statements
    let service_js = std::fs::read_to_string(base.join("dist/services/user-service.js"))
        .expect("read service js");
    assert!(
        service_js.contains("require(") || service_js.contains("import"),
        "Service JS should have require or import statements: {service_js}"
    );
    assert!(
        service_js.contains("../models/user") || service_js.contains("./models/user"),
        "Service JS should reference models/user: {service_js}"
    );
    assert!(
        service_js.contains("../utils/helpers") || service_js.contains("./utils/helpers"),
        "Service JS should reference utils/helpers: {service_js}"
    );

    // Verify index.js has re-exports (CommonJS uses Object.defineProperty pattern)
    let index_js = std::fs::read_to_string(base.join("dist/index.js")).expect("read index js");
    assert!(
        index_js.contains("exports")
            && (index_js.contains("require(") || index_js.contains("Object.defineProperty")),
        "Index JS should have CommonJS exports: {index_js}"
    );

    // Verify declaration file for index has re-export statements
    let index_dts = std::fs::read_to_string(base.join("dist/index.d.ts")).expect("read index d.ts");
    assert!(
        index_dts.contains("export *") && index_dts.contains("./models/user"),
        "Index d.ts should have re-export statements: {index_dts}"
    );

    // Verify source map for user-service has correct sources
    let service_map_contents =
        std::fs::read_to_string(base.join("dist/services/user-service.js.map"))
            .expect("read service map");
    let service_map: Value =
        serde_json::from_str(&service_map_contents).expect("parse service map json");
    let sources = service_map
        .get("sources")
        .and_then(|v| v.as_array())
        .expect("sources array");
    assert!(!sources.is_empty(), "Source map should have sources");
    let sources_content = service_map.get("sourcesContent").and_then(|v| v.as_array());
    assert!(
        sources_content.is_some(),
        "Source map should have sourcesContent"
    );
}

#[test]
fn compile_multi_file_project_with_default_and_named_imports() {
    // Test default and named import styles
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist",
            "rootDir": "src",
            "module": "commonjs",
            "esModuleInterop": true,
            "declaration": true
          },
          "include": ["src/**/*.ts"]
        }"#,
    );

    // src/constants.ts - default export
    write_file(
        &base.join("src/constants.ts"),
        r#"
const CONFIG = {
    apiUrl: "https://api.example.com",
    timeout: 5000
};

export default CONFIG;
export const VERSION = "1.0.0";
"#,
    );

    // src/math.ts - multiple named exports
    write_file(
        &base.join("src/math.ts"),
        r#"
export function add(a: number, b: number): number {
    return a + b;
}

export function multiply(a: number, b: number): number {
    return a * b;
}

export const PI = 3.14159;
"#,
    );

    // src/app.ts - uses default and named imports
    write_file(
        &base.join("src/app.ts"),
        r#"
// Default import
import CONFIG from './constants';
// Named import alongside default
import { VERSION } from './constants';
// Named imports with alias
import { add as addNumbers, multiply, PI } from './math';

export function runApp(): string {
    const sum = addNumbers(1, 2);
    const product = multiply(3, 4);
    const circumference = 2 * PI * 10;
    const url = CONFIG.apiUrl;

    return url + " v" + VERSION + " sum=" + sum + " product=" + product + " circ=" + circumference;
}
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(
        result.diagnostics.is_empty(),
        "Expected no diagnostics, got: {:?}",
        result.diagnostics
    );

    // Verify all files compiled
    assert!(base.join("dist/constants.js").is_file());
    assert!(base.join("dist/math.js").is_file());
    assert!(base.join("dist/app.js").is_file());
    assert!(base.join("dist/app.d.ts").is_file());

    // Verify app.js has the necessary require statements
    let app_js = std::fs::read_to_string(base.join("dist/app.js")).expect("read app js");
    assert!(
        app_js.contains("./constants") || app_js.contains("constants"),
        "App JS should reference constants: {app_js}"
    );
    assert!(
        app_js.contains("./math") || app_js.contains("math"),
        "App JS should reference math: {app_js}"
    );

    // Verify declaration file has correct exports
    let app_dts = std::fs::read_to_string(base.join("dist/app.d.ts")).expect("read app d.ts");
    assert!(
        app_dts.contains("runApp"),
        "App d.ts should export runApp: {app_dts}"
    );
}

#[test]
fn compile_default_interface_and_default_value_export_merge_without_default_conflicts() {
    let temp = TempDir::new().expect("temp dir");
    let base = temp.path.as_path();

    write_file(
        &base.join("tsconfig.json"),
        r#"{
  "compilerOptions": {
    "target": "es2015",
    "module": "commonjs"
  },
  "files": ["a.ts", "b.ts", "index.ts"]
}"#,
    );

    write_file(
        &base.join("b.ts"),
        r#"export const zzz = 123;
export default zzz;
"#,
    );

    write_file(
        &base.join("a.ts"),
        r#"export default interface zzz {
    x: string;
}

import zzz from "./b";

const x: zzz = { x: "" };
zzz;

export { zzz as default };
"#,
    );

    write_file(
        &base.join("index.ts"),
        r#"import zzz from "./a";

const x: zzz = { x: "" };
zzz;

import originalZZZ from "./b";
originalZZZ;

const y: originalZZZ = x;
"#,
    );

    let mut args = default_args();
    args.project = Some(base.join("tsconfig.json"));

    let result = compile(&args, base).expect("compile should succeed");
    let codes: Vec<u32> = result.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        result
            .diagnostics
            .iter()
            .all(|d| d.code != 2323 && d.code != 2528),
        "Expected interface/value default export merge without TS2323/TS2528, got codes: {codes:?}\nDiagnostics: {:?}",
        result.diagnostics
    );
}

#[test]
fn compile_module_augmentation_default_interface_alias_merges_without_ts2300() {
    let temp = TempDir::new().expect("temp dir");
    let base = temp.path.as_path();

    write_file(
        &base.join("tsconfig.json"),
        r#"{
  "compilerOptions": {
    "target": "es2015",
    "module": "commonjs"
  },
  "files": ["a.ts", "b.ts", "c.ts"]
}"#,
    );

    write_file(
        &base.join("a.ts"),
        r#"interface I {}
export default I;
"#,
    );

    write_file(
        &base.join("b.ts"),
        r#"export {};
declare module "./a" {
    export default interface I { x: number; }
}
"#,
    );

    write_file(
        &base.join("c.ts"),
        r#"import I from "./a";
function f(i: I) {
    i.x;
}
"#,
    );

    let mut args = default_args();
    args.project = Some(base.join("tsconfig.json"));

    let result = compile(&args, base).expect("compile should succeed");
    let codes: Vec<u32> = result.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        result
            .diagnostics
            .iter()
            .all(|d| d.code != diagnostic_codes::DUPLICATE_IDENTIFIER),
        "Expected module augmentation default interface alias merge to avoid TS2300, got codes: {codes:?}\nDiagnostics: {:?}",
        result.diagnostics
    );
    assert!(
        result.diagnostics.iter().all(|d| d.code != 2339),
        "Expected merged default interface alias to expose x in imports, got codes: {codes:?}\nDiagnostics: {:?}",
        result.diagnostics
    );
}

#[test]
fn compile_multi_file_project_with_type_imports() {
    // Test type-only imports compile correctly
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist",
            "rootDir": ".",
            "module": "commonjs",
            "declaration": true
          },
          "include": ["src/**/*.ts"]
        }"#,
    );

    // src/types.ts - shared types
    write_file(
        &base.join("src/types.ts"),
        r#"
export interface Logger {
    log(msg: string): void;
}

export type LogLevel = "debug" | "info" | "error";
"#,
    );

    // src/logger.ts - uses types (type-only import)
    write_file(
        &base.join("src/logger.ts"),
        r#"
import type { Logger, LogLevel } from './types';

export class ConsoleLogger implements Logger {
    private level: LogLevel;

    constructor(level: LogLevel) {
        this.level = level;
    }

    log(msg: string): void {
        // log implementation
    }

    getLevel(): LogLevel {
        return this.level;
    }
}

export function createLogger(level: LogLevel): Logger {
    return new ConsoleLogger(level);
}
"#,
    );

    // src/index.ts - re-exports everything
    write_file(
        &base.join("src/index.ts"),
        r#"
export type { Logger, LogLevel } from './types';
export { ConsoleLogger, createLogger } from './logger';
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(
        result.diagnostics.is_empty(),
        "Type imports should compile without errors: {:?}",
        result.diagnostics
    );

    assert!(base.join("dist/src/types.js").is_file());
    assert!(base.join("dist/src/logger.js").is_file());
    assert!(base.join("dist/src/index.js").is_file());
    assert!(base.join("dist/src/index.d.ts").is_file());

    // Verify declaration file has type exports
    let index_dts =
        std::fs::read_to_string(base.join("dist/src/index.d.ts")).expect("read index d.ts");
    assert!(
        index_dts.contains("Logger") && index_dts.contains("LogLevel"),
        "Index d.ts should have type exports for Logger and LogLevel: {index_dts}"
    );

    // Verify logger.js has the class implementation
    let logger_js =
        std::fs::read_to_string(base.join("dist/src/logger.js")).expect("read logger js");
    assert!(
        logger_js.contains("ConsoleLogger") && logger_js.contains("createLogger"),
        "Logger JS should have class and function exports: {logger_js}"
    );
}

#[test]
fn compile_type_only_export_equals_chain_reports_ts1361_without_ts2339() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "target": "es2015",
            "module": "commonjs",
            "esModuleInterop": true,
            "noEmit": true
          },
          "files": ["a.ts", "b.ts", "c.ts", "d.ts", "e.ts", "f.ts", "g.ts"]
        }"#,
    );

    write_file(&base.join("a.ts"), "export class A {}\n");
    write_file(
        &base.join("b.ts"),
        "import type * as types from './a';\nexport = types;\n",
    );
    write_file(
        &base.join("c.ts"),
        "import * as types from './a';\nexport = types;\n",
    );
    write_file(
        &base.join("d.ts"),
        "import types from './b';\nnew types.A();\n",
    );
    write_file(
        &base.join("e.ts"),
        "import types = require('./b');\nnew types.A();\n",
    );
    write_file(
        &base.join("f.ts"),
        "import * as types from './b';\nnew types.A();\n",
    );
    write_file(
        &base.join("g.ts"),
        "import type types from './c';\nnew types.A();\n",
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");
    let ts1361 = result.diagnostics.iter().filter(|d| d.code == 1361).count();
    let ts2339 = result.diagnostics.iter().filter(|d| d.code == 2339).count();

    assert_eq!(
        ts1361, 4,
        "Expected one TS1361 per consumer file in the export= type-only chain. Diagnostics: {:?}",
        result.diagnostics
    );
    assert_eq!(
        ts2339, 0,
        "Did not expect TS2339 after the type-only value error. Diagnostics: {:?}",
        result.diagnostics
    );
}

/// Reproduces `conformance/externalModules/typeOnly/chained.ts`.
///
/// Chain: `/a.ts: export type {{ A as B }}` →
///        `/b.ts: export {{ B as C }} from './a'` →
///        `/c.ts: import type {{ C }} from './b'; export {{ C as D }}` →
///        `/d.ts: import {{ D }} from './c'; new D()`.
///
/// The closest direct type-only marker to `D`'s use site is the
/// `import type { C } from './b'` clause in `/c.ts`. `tsc` therefore emits
/// **TS1361** (was imported using `import type`), not TS1362
/// (which would attribute to the upstream `export type { A as B }`).
///
/// Regression: prior to fix, the chain walk visited the alias
/// `C in /b.ts` first (whose import chain lands in `/a.ts`'s `export type`),
/// inferred TS1362 from that cross-file resolution, and returned before
/// reaching the more authoritative `import type` marker on `C in /c.ts`.
/// The fix walks every alias on the chain — including ones in non-current
/// binders — and prefers any direct `import type`/`export type` syntactic
/// marker over inferred cross-file kinds.
#[test]
fn compile_chained_type_only_alias_attributes_to_import_type_marker() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "target": "es2015",
            "module": "commonjs",
            "noEmit": true
          },
          "files": ["a.ts", "b.ts", "c.ts", "d.ts"]
        }"#,
    );
    write_file(
        &base.join("a.ts"),
        "class A { a!: string }\nexport type { A as B };\nexport type Z = A;\n",
    );
    write_file(
        &base.join("b.ts"),
        "import { Z as Y } from './a';\nexport { B as C } from './a';\n",
    );
    write_file(
        &base.join("c.ts"),
        "import type { C } from './b';\nexport { C as D };\n",
    );
    write_file(
        &base.join("d.ts"),
        "import { D } from './c';\nnew D();\nconst d: D = {};\n",
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    let ts1361: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| d.code == 1361)
        .collect();
    let ts1362: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| d.code == 1362)
        .collect();

    assert_eq!(
        ts1361.len(),
        1,
        "Expected exactly one TS1361 attributing to the `import type` marker. \
         Got TS1361={ts1361:?}, TS1362={ts1362:?}, all={:?}",
        result.diagnostics
    );
    assert!(
        ts1362.is_empty(),
        "Did not expect TS1362 — the upstream `export type` is shadowed \
         by the closer `import type` marker. \
         Got TS1362={ts1362:?}, all={:?}",
        result.diagnostics
    );
}

#[test]
fn compile_declaration_true_emits_dts_files() {
    // Test that declaration: true produces .d.ts files
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist",
            "rootDir": ".",
            "declaration": true
          },
          "include": ["src/**/*.ts"]
        }"#,
    );
    write_file(
        &base.join("src/index.ts"),
        r#"
export const VERSION = "1.0.0";
export function greet(name: string): string {
    return "Hello, " + name;
}
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(
        result.diagnostics.is_empty(),
        "Expected no diagnostics, got: {:?}",
        result.diagnostics
    );

    // JS file should exist
    assert!(
        base.join("dist/src/index.js").is_file(),
        "JS output should exist"
    );

    // Declaration file should exist
    assert!(
        base.join("dist/src/index.d.ts").is_file(),
        "Declaration file should exist when declaration: true"
    );

    // Verify declaration file content
    let dts = std::fs::read_to_string(base.join("dist/src/index.d.ts")).expect("read d.ts");
    assert!(
        dts.contains("VERSION") && dts.contains("string"),
        "Declaration should contain VERSION: {dts}"
    );
    assert!(
        dts.contains("greet") && dts.contains("name"),
        "Declaration should contain greet function: {dts}"
    );
}

#[test]
fn compile_declaration_mapped_type_as_literals_are_preserved() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "declaration": true,
            "emitDeclarationOnly": true,
            "outDir": "dist"
          },
          "files": ["index.ts"]
        }"#,
    );
    write_file(
        &base.join("index.ts"),
        r#"
export type ConstraintStringAs<T> = {
  [K in Extract<keyof T, "as"> as K]: T[K];
};

export type ConstraintTemplateAs<T> = {
  [K in Extract<keyof T, `as`> as K]: T[K];
};

export type NameStringAs<T> = {
  [K in keyof T as K extends 'as' ? K : never]: T[K];
};

export type NameIdentifierHasAs<T> = {
  [K in keyof T as K extends "has" ? K : never]: T[K];
};

export type NamePropertyAs<T> = {
  [K in keyof T as K extends { as: unknown } ? K : never]: T[K];
};
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(
        result.diagnostics.is_empty(),
        "Expected no diagnostics, got: {:?}",
        result.diagnostics
    );

    let dts = std::fs::read_to_string(base.join("dist/index.d.ts")).expect("read d.ts");
    for expected in [
        r#"[K in Extract<keyof T, "as"> as K]: T[K];"#,
        r#"[K in Extract<keyof T, `as`> as K]: T[K];"#,
        "[K in keyof T as K extends 'as' ? K : never]: T[K];",
        r#"[K in keyof T as K extends "has" ? K : never]: T[K];"#,
    ] {
        assert!(
            dts.contains(expected),
            "Expected declaration to contain `{expected}`: {dts}"
        );
    }
    assert!(
        dts.contains("K extends {") && dts.contains("as: unknown") && dts.contains("} ? K : never"),
        "Expected object type with `as` property to remain intact: {dts}"
    );
    for corrupted in [
        r#"Extract<keyof T, " as K"#,
        "Extract<keyof T, ` as K",
        "as ' ?",
        "as : unknown",
    ] {
        assert!(
            !dts.contains(corrupted),
            "Declaration should not contain corrupted mapped type text `{corrupted}`: {dts}"
        );
    }
}

#[test]
fn compile_strip_internal_omits_exported_declarations() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "declaration": true,
            "emitDeclarationOnly": true,
            "stripInternal": true,
            "outDir": "dist"
          },
          "files": ["index.ts"]
        }"#,
    );
    write_file(
        &base.join("index.ts"),
        r#"/** @internal */
export const stripped = 2;

/** @internal */
export function hiddenFunction() {}

/** @internal */
export interface HiddenInterface {
  value: string;
}

export const visible = 3;
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(
        result.diagnostics.is_empty(),
        "Expected no diagnostics, got: {:?}",
        result.diagnostics
    );
    let dts = std::fs::read_to_string(base.join("dist/index.d.ts")).expect("read d.ts");
    assert!(
        dts.contains("visible"),
        "Expected visible declaration to remain: {dts}"
    );
    for stripped_name in ["stripped", "hiddenFunction", "HiddenInterface", "@internal"] {
        assert!(
            !dts.contains(stripped_name),
            "Expected {stripped_name} to be stripped from declaration output: {dts}"
        );
    }
}

#[test]
fn compile_config_no_emit_helpers_reaches_printer() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "target": "es5",
            "module": "commonjs",
            "noEmitHelpers": true,
            "ignoreDeprecations": "6.0",
            "outDir": "dist"
          },
          "files": ["main.ts"]
        }"#,
    );
    write_file(
        &base.join("main.ts"),
        "class Base {}\nclass Derived extends Base {}\nexport const value = new Derived();\n",
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");
    assert!(
        result.diagnostics.is_empty(),
        "Expected no diagnostics, got: {:?}",
        result.diagnostics
    );

    let js = std::fs::read_to_string(base.join("dist/main.js")).expect("read JS output");
    assert!(
        js.contains("__extends(Derived, _super);"),
        "Expected helper call to remain: {js}"
    );
    assert!(
        !js.contains("var __extends ="),
        "Expected noEmitHelpers from config to suppress helper declaration: {js}"
    );
}

#[test]
fn compile_config_remove_comments_reaches_printer() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "target": "es2020",
            "module": "esnext",
            "removeComments": true,
            "outDir": "dist"
          },
          "files": ["main.ts"]
        }"#,
    );
    write_file(
        &base.join("main.ts"),
        "/* leading block comment */\nexport const value = 1; // trailing comment\n",
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");
    assert!(
        result.diagnostics.is_empty(),
        "Expected no diagnostics, got: {:?}",
        result.diagnostics
    );

    let js = std::fs::read_to_string(base.join("dist/main.js")).expect("read JS output");
    assert!(
        !js.contains("leading block comment") && !js.contains("trailing comment"),
        "Expected removeComments from config to strip comments: {js}"
    );
    assert!(
        js.contains("export const value = 1;"),
        "Expected emitted statement to remain: {js}"
    );
}

#[test]
fn compile_config_use_define_for_class_fields_false_reaches_printer() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "target": "es2022",
            "module": "commonjs",
            "useDefineForClassFields": false,
            "skipLibCheck": true,
            "outDir": "dist"
          },
          "files": ["main.ts"]
        }"#,
    );
    write_file(
        &base.join("main.ts"),
        r#"class Base {
  set x(value: number) {
    console.log("setter", value);
  }
}

class Derived extends Base {
  // @ts-ignore Deliberately comparing emit semantics for accessor/property override.
  x = 1;
}

new Derived();
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");
    assert!(
        result.diagnostics.is_empty(),
        "Expected no diagnostics, got: {:?}",
        result.diagnostics
    );

    let js = std::fs::read_to_string(base.join("dist/main.js")).expect("read JS output");
    assert!(
        js.contains("constructor()"),
        "Expected useDefineForClassFields=false from config to lower the field into the constructor: {js}"
    );
    assert!(
        js.contains("this.x = 1;"),
        "Expected legacy assignment semantics for class field: {js}"
    );
    assert!(
        !js.contains("\n    x = 1;"),
        "Expected native class field syntax to be suppressed: {js}"
    );
}

#[test]
fn compile_config_and_cli_new_line_reach_printer() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "target": "es2020",
            "module": "esnext",
            "newLine": "crlf",
            "outDir": "dist-config"
          },
          "files": ["main.ts"]
        }"#,
    );
    write_file(
        &base.join("main.ts"),
        "export const x = 1;\nexport const y = 2;\n",
    );

    let args = default_args();
    let result = compile(&args, base).expect("config compile should succeed");
    assert!(
        result.diagnostics.is_empty(),
        "Expected no diagnostics, got: {:?}",
        result.diagnostics
    );
    let js = std::fs::read(base.join("dist-config/main.js")).expect("read config JS output");
    assert!(
        js.windows(2).any(|pair| pair == b"\r\n"),
        "Expected config newLine=crlf to emit CRLF: {js:?}"
    );

    let mut args = default_args();
    args.target = Some(crate::args::Target::Es2020);
    args.module = Some(crate::args::Module::EsNext);
    args.new_line = Some(crate::args::NewLine::Crlf);
    args.out_dir = Some(PathBuf::from("dist-cli"));
    args.files = vec![PathBuf::from("main.ts")];
    let result = compile(&args, base).expect("CLI compile should succeed");
    assert!(
        result.diagnostics.is_empty(),
        "Expected no diagnostics, got: {:?}",
        result.diagnostics
    );
    let js = std::fs::read(base.join("dist-cli/main.js")).expect("read CLI JS output");
    assert!(
        js.windows(2).any(|pair| pair == b"\r\n"),
        "Expected CLI --newLine crlf to emit CRLF: {js:?}"
    );
}

#[test]
fn compile_declaration_no_check_no_lib_keeps_default_type_import() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "noCheck": true,
            "noLib": true,
            "declaration": true,
            "emitDeclarationOnly": true,
            "module": "esnext",
            "target": "es2022",
            "outDir": "dist"
          },
          "files": ["main.ts", "dep.ts"]
        }"#,
    );
    write_file(&base.join("dep.ts"), "export default class Foo {}\n");
    write_file(
        &base.join("main.ts"),
        "import Foo from \"./dep\";\nexport let x: Foo;\n",
    );

    let args = default_args();
    let _ = compile(&args, base).expect("compile should succeed");

    let dts = std::fs::read_to_string(base.join("dist/main.d.ts")).expect("read main.d.ts");
    assert!(
        dts.contains("import Foo from \"./dep\";"),
        "Expected declaration emit to keep default import used as a type: {dts}"
    );
    assert!(
        dts.contains("export declare let x: Foo;"),
        "Expected declaration emit to reference imported Foo: {dts}"
    );
}

#[test]
fn compile_declaration_false_no_dts_files() {
    // Test that declaration: false (or absent) does NOT produce .d.ts files
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist",
            "declaration": false
          },
          "include": ["src/**/*.ts"]
        }"#,
    );
    write_file(&base.join("src/index.ts"), "export const value = 42;");

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(result.diagnostics.is_empty());

    // JS file should exist
    assert!(
        base.join("dist/src/index.js").is_file(),
        "JS output should exist"
    );

    // Declaration file should NOT exist
    assert!(
        !base.join("dist/src/index.d.ts").is_file(),
        "Declaration file should NOT exist when declaration: false"
    );
}

#[test]
fn compile_declaration_absent_no_dts_files() {
    // Test that missing declaration option does NOT produce .d.ts files
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
    write_file(&base.join("src/index.ts"), "export const value = 42;");

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(result.diagnostics.is_empty());

    // JS file should exist
    assert!(
        base.join("dist/src/index.js").is_file(),
        "JS output should exist"
    );

    // Declaration file should NOT exist (declaration defaults to false)
    assert!(
        !base.join("dist/src/index.d.ts").is_file(),
        "Declaration file should NOT exist when declaration is not specified"
    );
}

#[test]
fn compile_declaration_interface_and_type() {
    // Test declaration output for interfaces and type aliases
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist",
            "rootDir": ".",
            "declaration": true
          },
          "include": ["src/**/*.ts"]
        }"#,
    );
    write_file(
        &base.join("src/types.ts"),
        r#"
export interface User {
    id: number;
    name: string;
    email: string;
}

export type UserId = number;

export type UserRole = "admin" | "user" | "guest";
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(result.diagnostics.is_empty());

    // Declaration file should exist
    let dts_path = base.join("dist/src/types.d.ts");
    assert!(dts_path.is_file(), "Declaration file should exist");

    let dts = std::fs::read_to_string(&dts_path).expect("read d.ts");

    // Interface should be in declaration
    assert!(
        dts.contains("interface User"),
        "Declaration should contain User interface: {dts}"
    );
    assert!(
        dts.contains("id") && dts.contains("number"),
        "Declaration should contain id property: {dts}"
    );
    assert!(
        dts.contains("name") && dts.contains("string"),
        "Declaration should contain name property: {dts}"
    );

    // Type aliases should be in declaration
    assert!(
        dts.contains("UserId"),
        "Declaration should contain UserId type: {dts}"
    );
    assert!(
        dts.contains("UserRole"),
        "Declaration should contain UserRole type: {dts}"
    );
}

#[test]
fn compile_declaration_class_with_methods() {
    // Test declaration output for classes with methods
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist",
            "rootDir": ".",
            "declaration": true
          },
          "include": ["src/**/*.ts"]
        }"#,
    );
    // Note: Avoid `return this` pattern which triggers a known false positive
    // with Lazy type resolution (class type not resolved for `this` assignability)
    write_file(
        &base.join("src/calculator.ts"),
        r#"
export class Calculator {
    private value: number;

    constructor(initial: number) {
        this.value = initial;
    }

    add(n: number): void {
        this.value = this.value + n;
    }

    subtract(n: number): void {
        this.value = this.value - n;
    }

    getResult(): number {
        return this.value;
    }
}
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(
        result.diagnostics.is_empty(),
        "Expected no diagnostics, got: {:?}",
        result.diagnostics
    );

    // Declaration file should exist
    let dts_path = base.join("dist/src/calculator.d.ts");
    assert!(dts_path.is_file(), "Declaration file should exist");

    let dts = std::fs::read_to_string(&dts_path).expect("read d.ts");

    // Class should be in declaration
    assert!(
        dts.contains("class Calculator"),
        "Declaration should contain Calculator class: {dts}"
    );

    // Methods should be in declaration
    assert!(
        dts.contains("add") && dts.contains("void"),
        "Declaration should contain add method with void return: {dts}"
    );
    assert!(
        dts.contains("subtract") && dts.contains("void"),
        "Declaration should contain subtract method with void return: {dts}"
    );
    assert!(
        dts.contains("getResult") && dts.contains("number"),
        "Declaration should contain getResult method: {dts}"
    );

    // Private members should be marked private in declaration
    assert!(
        dts.contains("private") && dts.contains("value"),
        "Declaration should contain private value: {dts}"
    );
}

#[test]
fn compile_declaration_with_declaration_dir() {
    // Test that declarationDir puts .d.ts files in separate directory
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist",
            "rootDir": "src",
            "declaration": true,
            "declarationDir": "types"
          },
          "include": ["src/**/*.ts"]
        }"#,
    );
    write_file(&base.join("src/index.ts"), "export const value = 42;");

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(result.diagnostics.is_empty());

    // JS file should be in outDir
    assert!(
        base.join("dist/index.js").is_file(),
        "JS output should be in dist/"
    );

    // Declaration file should be in declarationDir, NOT in outDir
    assert!(
        base.join("types/index.d.ts").is_file(),
        "Declaration file should be in types/"
    );
    assert!(
        !base.join("dist/index.d.ts").is_file(),
        "Declaration file should NOT be in dist/ when declarationDir is set"
    );
}

#[test]
fn compile_emit_declaration_only_from_tsconfig_suppresses_js_output() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "declaration": true,
            "emitDeclarationOnly": true,
            "outDir": "dist",
            "target": "es2017",
            "lib": ["es2017"],
            "skipLibCheck": true,
            "typeRoots": ["./empty-types"]
          },
          "files": ["main.ts"]
        }"#,
    );
    std::fs::create_dir_all(base.join("empty-types")).expect("empty typeRoots");
    write_file(
        &base.join("main.ts"),
        "export const value: string = \"ok\";\n",
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(result.diagnostics.is_empty());
    assert!(
        base.join("dist/main.d.ts").is_file(),
        "Declaration output should be emitted"
    );
    assert!(
        !base.join("dist/main.js").exists(),
        "JavaScript output should be suppressed by emitDeclarationOnly"
    );
    assert!(
        result
            .emitted_files
            .iter()
            .any(|path| path.ends_with("dist/main.d.ts")),
        "emitted_files should include declaration output: {:?}",
        result.emitted_files
    );
    assert!(
        !result
            .emitted_files
            .iter()
            .any(|path| path.ends_with("dist/main.js")),
        "emitted_files should not include JavaScript output: {:?}",
        result.emitted_files
    );
}

#[test]
fn declaration_emit_expands_foreign_import_mapped_keys_from_nested_package() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "declaration": true,
            "emitDeclarationOnly": true,
            "outDir": "dist",
            "rootDir": "r",
            "target": "es2017",
            "module": "commonjs",
            "moduleResolution": "node",
            "ignoreDeprecations": "6.0",
            "skipLibCheck": true,
            "strict": true,
            "typeRoots": ["./empty-types"]
          },
          "files": ["r/entry.ts"]
        }"#,
    );
    std::fs::create_dir_all(base.join("empty-types")).expect("empty typeRoots");
    write_file(
        &base.join("r/entry.ts"),
        r#"import { foo } from "foo";

export const x = foo();
"#,
    );
    write_file(
        &base.join("r/node_modules/foo/index.d.ts"),
        r#"export function foo(): { [K in import("keys").Key]?: string };
"#,
    );
    write_file(
        &base.join("r/node_modules/foo/node_modules/keys/index.d.ts"),
        r#"export type Key = "a" | "b";
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(
        result.diagnostics.is_empty(),
        "did not expect diagnostics: {:?}",
        result.diagnostics
    );

    let dts = std::fs::read_to_string(base.join("dist/entry.d.ts"))
        .expect("Declaration output should be emitted");
    assert!(
        dts.contains("a?: string | undefined;"),
        "expected expanded mapped key 'a': {dts}",
    );
    assert!(
        dts.contains("b?: string | undefined;"),
        "expected expanded mapped key 'b': {dts}",
    );
    assert!(
        !dts.contains("[K in"),
        "foreign mapped type should not leak into declaration output: {dts}",
    );
}

#[test]
fn declaration_emit_skips_file_with_ts4023_but_writes_unaffected_files() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("type.ts"),
        r#"export namespace Foo {
    export const sym = Symbol();
}

export type Type = { x?: { [Foo.sym]: 0 } };
"#,
    );
    write_file(
        &base.join("index.ts"),
        r#"import { type Type } from "./type";

export const foo = { ...({} as Type) };
"#,
    );

    let args = CliArgs::try_parse_from([
        "tsz",
        "--ignoreConfig",
        "--target",
        "es2015",
        "--strict",
        "--lib",
        "esnext",
        "--declaration",
        "--emitDeclarationOnly",
        "--listEmittedFiles",
        "--outDir",
        "dist",
        "--pretty",
        "false",
        "index.ts",
        "type.ts",
    ])
    .expect("CLI args should parse");
    let result = compile(&args, base).expect("compile should succeed");

    assert!(
        result.diagnostics.iter().any(|diag| diag.code
            == diagnostic_codes::EXPORTED_VARIABLE_HAS_OR_IS_USING_NAME_FROM_EXTERNAL_MODULE_BUT_CANNOT_BE_NAMED),
        "expected TS4023 diagnostic, got: {:#?}",
        result.diagnostics
    );
    assert!(
        !base.join("dist/index.d.ts").exists(),
        "Declaration output for file with TS4023 should not be written"
    );
    assert!(
        base.join("dist/type.d.ts").is_file(),
        "Unaffected declaration output should still be written"
    );
    assert!(
        !result
            .emitted_files
            .iter()
            .any(|path| path.ends_with("dist/index.d.ts")),
        "emitted files should not include blocked declaration: {:?}",
        result.emitted_files
    );
    assert!(
        result
            .emitted_files
            .iter()
            .any(|path| path.ends_with("dist/type.d.ts")),
        "emitted files should include unaffected declaration: {:?}",
        result.emitted_files
    );
}

#[test]
fn compile_emit_declaration_only_from_cli_suppresses_js_output() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    std::fs::create_dir_all(base.join("empty-types")).expect("empty typeRoots");
    write_file(
        &base.join("main.ts"),
        "export const value: string = \"ok\";\n",
    );

    let args = CliArgs::try_parse_from([
        "tsz",
        "--declaration",
        "--emitDeclarationOnly",
        "--pretty",
        "false",
        "--typeRoots",
        "./empty-types",
        "--skipLibCheck",
        "--target",
        "es2017",
        "--lib",
        "es2017",
        "--outDir",
        "dist",
        "main.ts",
    ])
    .expect("CLI args should parse");
    let result = compile(&args, base).expect("compile should succeed");

    assert!(result.diagnostics.is_empty());
    assert!(
        base.join("dist/main.d.ts").is_file(),
        "Declaration output should be emitted"
    );
    assert!(
        !base.join("dist/main.js").exists(),
        "JavaScript output should be suppressed by CLI emitDeclarationOnly"
    );
}

#[test]
fn compile_config_allow_importing_ts_extensions_requires_emit_guard() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "allowImportingTsExtensions": true
          },
          "files": ["main.ts"]
        }"#,
    );
    write_file(&base.join("main.ts"), "export const value = 1;\n");

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");
    let codes: Vec<u32> = result.diagnostics.iter().map(|d| d.code).collect();

    assert!(
        codes.contains(
            &diagnostic_codes::OPTION_ALLOWIMPORTINGTSEXTENSIONS_CAN_ONLY_BE_USED_WHEN_ONE_OF_NOEMIT_EMITDECLAR
        ),
        "allowImportingTsExtensions without an emit guard should report TS5096, got: {codes:?}"
    );
}

#[test]
fn compile_config_allow_importing_ts_extensions_accepts_no_emit_guard() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "allowImportingTsExtensions": true,
            "noEmit": true
          },
          "files": ["main.ts"]
        }"#,
    );
    write_file(&base.join("main.ts"), "export const value = 1;\n");

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");
    let codes: Vec<u32> = result.diagnostics.iter().map(|d| d.code).collect();

    assert!(
        !codes.contains(
            &diagnostic_codes::OPTION_ALLOWIMPORTINGTSEXTENSIONS_CAN_ONLY_BE_USED_WHEN_ONE_OF_NOEMIT_EMITDECLAR
        ),
        "allowImportingTsExtensions with noEmit should not report TS5096, got: {codes:?}"
    );
}

#[test]
fn compile_cli_allow_importing_ts_extensions_requires_emit_guard() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(&base.join("main.ts"), "export const value = 1;\n");

    let args = CliArgs::try_parse_from([
        "tsz",
        "--allowImportingTsExtensions",
        "--ignoreConfig",
        "main.ts",
    ])
    .expect("CLI args should parse");
    let result = compile(&args, base).expect("compile should succeed");
    let codes: Vec<u32> = result.diagnostics.iter().map(|d| d.code).collect();

    assert!(
        codes.contains(
            &diagnostic_codes::OPTION_ALLOWIMPORTINGTSEXTENSIONS_CAN_ONLY_BE_USED_WHEN_ONE_OF_NOEMIT_EMITDECLAR
        ),
        "CLI allowImportingTsExtensions without an emit guard should report TS5096, got: {codes:?}"
    );
}

#[test]
fn compile_cli_allow_importing_ts_extensions_accepts_no_emit_guard() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(&base.join("main.ts"), "export const value = 1;\n");

    let args = CliArgs::try_parse_from([
        "tsz",
        "--allowImportingTsExtensions",
        "--noEmit",
        "--ignoreConfig",
        "main.ts",
    ])
    .expect("CLI args should parse");
    let result = compile(&args, base).expect("compile should succeed");
    let codes: Vec<u32> = result.diagnostics.iter().map(|d| d.code).collect();

    assert!(
        !codes.contains(
            &diagnostic_codes::OPTION_ALLOWIMPORTINGTSEXTENSIONS_CAN_ONLY_BE_USED_WHEN_ONE_OF_NOEMIT_EMITDECLAR
        ),
        "CLI allowImportingTsExtensions with noEmit should not report TS5096, got: {codes:?}"
    );
}

#[test]
fn compile_bundler_dts_value_import_reports_ts2846_not_ts2307() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "target": "es2015",
            "module": "esnext",
            "moduleResolution": "bundler",
            "noEmit": true
          },
          "files": ["a.ts", "types.d.ts"]
        }"#,
    );
    write_file(&base.join("a.ts"), "export {};\n");
    write_file(&base.join("types.d.ts"), "import {} from \"./a.d.ts\";\n");

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");
    let codes: Vec<u32> = result.diagnostics.iter().map(|d| d.code).collect();

    assert!(
        codes.contains(
            &diagnostic_codes::A_DECLARATION_FILE_CANNOT_BE_IMPORTED_WITHOUT_IMPORT_TYPE_DID_YOU_MEAN_TO_IMPORT
        ),
        "expected TS2846 for value import of ./a.d.ts, got: {:#?}",
        result.diagnostics
    );
    assert!(
        !codes
            .contains(&diagnostic_codes::CANNOT_FIND_MODULE_OR_ITS_CORRESPONDING_TYPE_DECLARATIONS),
        "TS2846 should suppress TS2307 for ./a.d.ts when ./a.ts exists, got: {:#?}",
        result.diagnostics
    );
}

