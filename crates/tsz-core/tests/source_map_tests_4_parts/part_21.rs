/// Test source map generation for re-exports with aliases in ES5 output.
/// Validates that `export { foo as bar } from "module"` generates proper source mappings.
#[test]
fn test_source_map_reexport_alias_es5() {
    let source = r#"// Re-export with aliases from other modules
export { useState as useStateHook } from "react";
export { Component as ReactComponent } from "react";
export { map as lodashMap, filter as lodashFilter } from "lodash";

// Re-export default as named
export { default as axios } from "axios";
export { default as express } from "express";

// Mixed re-exports with and without aliases
export { readFile as readFileAsync, writeFile as writeFileAsync } from "fs/promises";

// Re-export everything with namespace alias handled separately
// export * as utils from "./utils";

// Local function that uses re-exports conceptually
function useLibraries(): void {
    console.log("Libraries configured");
}

export { useLibraries };"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let options = PrinterOptions {
        target: ScriptTarget::ES5,
        ..Default::default()
    };
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);

    let mut printer = Printer::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_target_es5(ctx.target_es5);
    printer.set_source_map_text(parser.get_source_text());
    printer.enable_source_map("test.js", "test.ts");
    printer.emit(root);

    let output = printer.get_output().to_string();
    let map_json = printer.generate_source_map_json().expect("source map");
    let map_value: Value = serde_json::from_str(&map_json).expect("parse source map");

    let mappings = map_value
        .get("mappings")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    let decoded = decode_mappings(mappings);

    assert!(
        output.contains("useLibraries"),
        "expected useLibraries function in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for re-export aliases"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

/// Test source map generation for default import aliases in ES5 output.
/// Validates that `import MyAlias from "module"` generates proper source mappings.
#[test]
fn test_source_map_import_default_alias_es5() {
    let source = r#"// Default imports (which are essentially aliases for the default export)
import React from "react";
import Express from "express";
import Lodash from "lodash";

// Using default imports
const app = Express();
const element = React.createElement("div", null, "Hello");
const sorted = Lodash.sortBy([3, 1, 2]);

// Default import with named imports
import Axios, { AxiosResponse, AxiosError } from "axios";

async function fetchData(): Promise<AxiosResponse> {
    try {
        return await Axios.get("/api/data");
    } catch (error) {
        throw error as AxiosError;
    }
}

// Re-assigning default imports
const MyReact = React;
const MyExpress = Express;

console.log(app, element, sorted);"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let options = PrinterOptions {
        target: ScriptTarget::ES5,
        ..Default::default()
    };
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);

    let mut printer = Printer::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_target_es5(ctx.target_es5);
    printer.set_source_map_text(parser.get_source_text());
    printer.enable_source_map("test.js", "test.ts");
    printer.emit(root);

    let output = printer.get_output().to_string();
    let map_json = printer.generate_source_map_json().expect("source map");
    let map_value: Value = serde_json::from_str(&map_json).expect("parse source map");

    let mappings = map_value
        .get("mappings")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    let decoded = decode_mappings(mappings);

    assert!(
        output.contains("fetchData"),
        "expected fetchData function in output. output: {output}"
    );
    assert!(
        output.contains("MyReact"),
        "expected MyReact variable in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for default import aliases"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

/// Test source map generation for namespace import aliases in ES5 output.
/// Validates that `import * as ns from "module"` generates proper source mappings.
#[test]
fn test_source_map_import_namespace_alias_es5() {
    let source = r#"// Namespace imports
import * as React from "react";
import * as ReactDOM from "react-dom";
import * as Lodash from "lodash";
import * as Utils from "./utils";

// Using namespace imports
const element = React.createElement("div", { className: "container" }, "Hello");
const root = ReactDOM.createRoot(document.getElementById("root")!);

// Destructuring from namespace
const { map, filter, reduce } = Lodash;
const { formatDate, parseDate } = Utils;

// Using destructured values
const doubled = map([1, 2, 3], (n: number) => n * 2);
const evens = filter([1, 2, 3, 4], (n: number) => n % 2 === 0);

// Aliasing namespace members
const lodashMap = Lodash.map;
const lodashFilter = Lodash.filter;

function renderApp(): void {
    root.render(element);
}

console.log(doubled, evens);
renderApp();"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let options = PrinterOptions {
        target: ScriptTarget::ES5,
        ..Default::default()
    };
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);

    let mut printer = Printer::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_target_es5(ctx.target_es5);
    printer.set_source_map_text(parser.get_source_text());
    printer.enable_source_map("test.js", "test.ts");
    printer.emit(root);

    let output = printer.get_output().to_string();
    let map_json = printer.generate_source_map_json().expect("source map");
    let map_value: Value = serde_json::from_str(&map_json).expect("parse source map");

    let mappings = map_value
        .get("mappings")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    let decoded = decode_mappings(mappings);

    assert!(
        output.contains("renderApp"),
        "expected renderApp function in output. output: {output}"
    );
    assert!(
        output.contains("doubled"),
        "expected doubled variable in output. output: {output}"
    );
    assert!(
        output.contains("lodashMap"),
        "expected lodashMap variable in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for namespace import aliases"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

/// Comprehensive test combining multiple import/export alias patterns.
/// Tests named, default, namespace imports and exports with various alias combinations.
#[test]
fn test_source_map_import_export_alias_es5_comprehensive() {
    let source = r#"// Comprehensive import/export alias patterns

// Namespace imports
import * as path from "path";
import * as fs from "fs";

// Default imports
import express from "express";
import cors from "cors";

// Named imports with aliases
import { readFile as readFileAsync, writeFile as writeFileAsync } from "fs/promises";
import { join as joinPath, resolve as resolvePath, dirname as getDirname } from "path";

// Mixed default and named with aliases
import axios, { AxiosInstance as HttpClient, AxiosResponse as HttpResponse } from "axios";

// Internal implementations
class ApiClient {
    private client: HttpClient;
    private basePath: string;

    constructor(baseUrl: string) {
        this.client = axios.create({ baseURL: baseUrl });
        this.basePath = resolvePath(getDirname(""), "api");
    }

    async get<T>(endpoint: string): Promise<HttpResponse<T>> {
        const fullPath = joinPath(this.basePath, endpoint);
        console.log(`Fetching from: ${fullPath}`);
        return this.client.get(endpoint);
    }

    async loadConfig(configPath: string): Promise<string> {
        const absolutePath = path.resolve(configPath);
        const content = await readFileAsync(absolutePath, "utf-8");
        return content;
    }

    async saveConfig(configPath: string, data: string): Promise<void> {
        const absolutePath = path.resolve(configPath);
        await writeFileAsync(absolutePath, data, "utf-8");
    }
}

// Create app with middleware
const app = express();
app.use(cors());

// Export with aliases
export { ApiClient as Client };
export { app as application };

// Re-export with aliases
export { readFileAsync as readFile, writeFileAsync as writeFile };
export { joinPath, resolvePath, getDirname };

// Export default with alias pattern
const defaultClient = new ApiClient("https://api.example.com");
export { defaultClient as default };"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let options = PrinterOptions {
        target: ScriptTarget::ES5,
        ..Default::default()
    };
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);

    let mut printer = Printer::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_target_es5(ctx.target_es5);
    printer.set_source_map_text(parser.get_source_text());
    printer.enable_source_map("test.js", "test.ts");
    printer.emit(root);

    let output = printer.get_output().to_string();
    let map_json = printer.generate_source_map_json().expect("source map");
    let map_value: Value = serde_json::from_str(&map_json).expect("parse source map");

    let mappings = map_value
        .get("mappings")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    let decoded = decode_mappings(mappings);

    assert!(
        output.contains("ApiClient"),
        "expected ApiClient class in output. output: {output}"
    );
    assert!(
        output.contains("loadConfig"),
        "expected loadConfig method in output. output: {output}"
    );
    assert!(
        output.contains("saveConfig"),
        "expected saveConfig method in output. output: {output}"
    );
    assert!(
        output.contains("defaultClient"),
        "expected defaultClient variable in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for comprehensive import/export aliases"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

// =============================================================================
// ES5 SOURCE MAP TESTS: MAPPED TYPE EXPRESSIONS
// =============================================================================

/// Test source map generation for Partial<T> mapped type in ES5 output.
/// Validates that Partial utility type generates proper source mappings.
#[test]
fn test_source_map_mapped_type_partial_es5() {
    let source = r#"// Custom Partial implementation
type MyPartial<T> = {
    [P in keyof T]?: T[P];
};

interface User {
    id: number;
    name: string;
    email: string;
    age: number;
}

// Function using Partial
function updateUser(user: User, updates: Partial<User>): User {
    return { ...user, ...updates };
}

function patchUser(user: User, patch: MyPartial<User>): User {
    return { ...user, ...patch };
}

// Creating partial objects
const fullUser: User = { id: 1, name: "Alice", email: "alice@example.com", age: 30 };
const partialUpdate: Partial<User> = { name: "Alicia" };
const updatedUser = updateUser(fullUser, partialUpdate);

// Nested partial
type DeepPartial<T> = {
    [P in keyof T]?: T[P] extends object ? DeepPartial<T[P]> : T[P];
};

interface Config {
    database: { host: string; port: number };
    cache: { enabled: boolean; ttl: number };
}

function mergeConfig(base: Config, override: DeepPartial<Config>): Config {
    return { ...base, ...override } as Config;
}

console.log(updatedUser);"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let options = PrinterOptions {
        target: ScriptTarget::ES5,
        ..Default::default()
    };
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);

    let mut printer = Printer::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_target_es5(ctx.target_es5);
    printer.set_source_map_text(parser.get_source_text());
    printer.enable_source_map("test.js", "test.ts");
    printer.emit(root);

    let output = printer.get_output().to_string();
    let map_json = printer.generate_source_map_json().expect("source map");
    let map_value: Value = serde_json::from_str(&map_json).expect("parse source map");

    let mappings = map_value
        .get("mappings")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    let decoded = decode_mappings(mappings);

    assert!(
        output.contains("updateUser"),
        "expected updateUser function in output. output: {output}"
    );
    assert!(
        output.contains("patchUser"),
        "expected patchUser function in output. output: {output}"
    );
    assert!(
        output.contains("mergeConfig"),
        "expected mergeConfig function in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for Partial mapped type"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

/// Test source map generation for Required<T> mapped type in ES5 output.
/// Validates that Required utility type generates proper source mappings.
#[test]
fn test_source_map_mapped_type_required_es5() {
    let source = r#"// Custom Required implementation
type MyRequired<T> = {
    [P in keyof T]-?: T[P];
};

interface PartialUser {
    id?: number;
    name?: string;
    email?: string;
}

// Function requiring all properties
function createUser(data: Required<PartialUser>): PartialUser {
    return {
        id: data.id,
        name: data.name,
        email: data.email
    };
}

function validateUser(data: MyRequired<PartialUser>): boolean {
    return data.id > 0 && data.name.length > 0 && data.email.includes("@");
}

// Builder pattern with Required
class UserBuilder {
    private data: Partial<PartialUser> = {};

    setId(id: number): this {
        this.data.id = id;
        return this;
    }

    setName(name: string): this {
        this.data.name = name;
        return this;
    }

    setEmail(email: string): this {
        this.data.email = email;
        return this;
    }

    build(): Required<PartialUser> {
        if (!this.data.id || !this.data.name || !this.data.email) {
            throw new Error("All fields required");
        }
        return this.data as Required<PartialUser>;
    }
}

const builder = new UserBuilder();
const user = builder.setId(1).setName("Bob").setEmail("bob@example.com").build();
console.log(validateUser(user));"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let options = PrinterOptions {
        target: ScriptTarget::ES5,
        ..Default::default()
    };
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);

    let mut printer = Printer::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_target_es5(ctx.target_es5);
    printer.set_source_map_text(parser.get_source_text());
    printer.enable_source_map("test.js", "test.ts");
    printer.emit(root);

    let output = printer.get_output().to_string();
    let map_json = printer.generate_source_map_json().expect("source map");
    let map_value: Value = serde_json::from_str(&map_json).expect("parse source map");

    let mappings = map_value
        .get("mappings")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    let decoded = decode_mappings(mappings);

    assert!(
        output.contains("createUser"),
        "expected createUser function in output. output: {output}"
    );
    assert!(
        output.contains("validateUser"),
        "expected validateUser function in output. output: {output}"
    );
    assert!(
        output.contains("UserBuilder"),
        "expected UserBuilder class in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for Required mapped type"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

/// Test source map generation for Readonly<T> mapped type in ES5 output.
/// Validates that Readonly utility type generates proper source mappings.
#[test]
fn test_source_map_mapped_type_readonly_es5() {
    let source = r#"// Custom Readonly implementation
type MyReadonly<T> = {
    readonly [P in keyof T]: T[P];
};

interface MutableState {
    count: number;
    items: string[];
    lastUpdated: Date;
}

// Frozen state pattern
function freezeState<T extends object>(state: T): Readonly<T> {
    return Object.freeze({ ...state });
}

function getImmutableState(state: MutableState): MyReadonly<MutableState> {
    return state;
}

// Deep readonly
type DeepReadonly<T> = {
    readonly [P in keyof T]: T[P] extends object ? DeepReadonly<T[P]> : T[P];
};

interface AppState {
    user: { name: string; settings: { theme: string } };
    data: { items: number[] };
}

function getAppState(): DeepReadonly<AppState> {
    return {
        user: { name: "Alice", settings: { theme: "dark" } },
        data: { items: [1, 2, 3] }
    };
}

// Working with readonly
class StateManager {
    private state: MutableState = { count: 0, items: [], lastUpdated: new Date() };

    getState(): Readonly<MutableState> {
        return this.state;
    }

    increment(): void {
        this.state.count++;
        this.state.lastUpdated = new Date();
    }
}

const manager = new StateManager();
const readonlyState = manager.getState();
console.log(readonlyState.count);"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let options = PrinterOptions {
        target: ScriptTarget::ES5,
        ..Default::default()
    };
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);

    let mut printer = Printer::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_target_es5(ctx.target_es5);
    printer.set_source_map_text(parser.get_source_text());
    printer.enable_source_map("test.js", "test.ts");
    printer.emit(root);

    let output = printer.get_output().to_string();
    let map_json = printer.generate_source_map_json().expect("source map");
    let map_value: Value = serde_json::from_str(&map_json).expect("parse source map");

    let mappings = map_value
        .get("mappings")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    let decoded = decode_mappings(mappings);

    assert!(
        output.contains("freezeState"),
        "expected freezeState function in output. output: {output}"
    );
    assert!(
        output.contains("getAppState"),
        "expected getAppState function in output. output: {output}"
    );
    assert!(
        output.contains("StateManager"),
        "expected StateManager class in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for Readonly mapped type"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

/// Test source map generation for Pick<T, K> mapped type in ES5 output.
/// Validates that Pick utility type generates proper source mappings.
#[test]
fn test_source_map_mapped_type_pick_es5() {
    let source = r#"// Custom Pick implementation
type MyPick<T, K extends keyof T> = {
    [P in K]: T[P];
};

interface FullUser {
    id: number;
    name: string;
    email: string;
    password: string;
    createdAt: Date;
    updatedAt: Date;
}

// Pick specific properties
type PublicUser = Pick<FullUser, "id" | "name" | "email">;
type UserCredentials = MyPick<FullUser, "email" | "password">;

function getPublicProfile(user: FullUser): PublicUser {
    return {
        id: user.id,
        name: user.name,
        email: user.email
    };
}

function extractCredentials(user: FullUser): UserCredentials {
    return {
        email: user.email,
        password: user.password
    };
}

// Generic pick function
function pick<T extends object, K extends keyof T>(
    obj: T,
    keys: K[]
): Pick<T, K> {
    const result = {} as Pick<T, K>;
    for (const key of keys) {
        result[key] = obj[key];
    }
    return result;
}

const fullUser: FullUser = {
    id: 1,
    name: "Alice",
    email: "alice@example.com",
    password: "secret",
    createdAt: new Date(),
    updatedAt: new Date()
};

const publicUser = getPublicProfile(fullUser);
const picked = pick(fullUser, ["id", "name"]);
console.log(publicUser, picked);"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let options = PrinterOptions {
        target: ScriptTarget::ES5,
        ..Default::default()
    };
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);

    let mut printer = Printer::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_target_es5(ctx.target_es5);
    printer.set_source_map_text(parser.get_source_text());
    printer.enable_source_map("test.js", "test.ts");
    printer.emit(root);

    let output = printer.get_output().to_string();
    let map_json = printer.generate_source_map_json().expect("source map");
    let map_value: Value = serde_json::from_str(&map_json).expect("parse source map");

    let mappings = map_value
        .get("mappings")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    let decoded = decode_mappings(mappings);

    assert!(
        output.contains("getPublicProfile"),
        "expected getPublicProfile function in output. output: {output}"
    );
    assert!(
        output.contains("extractCredentials"),
        "expected extractCredentials function in output. output: {output}"
    );
    assert!(
        output.contains("pick"),
        "expected pick function in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for Pick mapped type"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

/// Test source map generation for Record<K, T> mapped type in ES5 output.
/// Validates that Record utility type generates proper source mappings.
#[test]
fn test_source_map_mapped_type_record_es5() {
    let source = r#"// Custom Record implementation
type MyRecord<K extends keyof any, T> = {
    [P in K]: T;
};

// Record with string keys
type UserRoles = Record<string, boolean>;
type CountryCode = "US" | "UK" | "CA" | "AU";
type CountryNames = Record<CountryCode, string>;

function createUserRoles(): UserRoles {
    return {
        admin: true,
        editor: false,
        viewer: true
    };
}

function getCountryNames(): CountryNames {
    return {
        US: "United States",
        UK: "United Kingdom",
        CA: "Canada",
        AU: "Australia"
    };
}

// Record with number keys
type IndexedData = Record<number, string>;

function createIndexedData(items: string[]): IndexedData {
    const result: IndexedData = {};
    items.forEach((item, index) => {
        result[index] = item;
    });
    return result;
}

// Nested Record
type NestedRecord = Record<string, Record<string, number>>;

function createNestedRecord(): NestedRecord {
    return {
        users: { count: 100, active: 50 },
        posts: { count: 500, published: 450 }
    };
}

// Generic record creator
function createRecord<K extends string, T>(
    keys: K[],
    value: T
): MyRecord<K, T> {
    const result = {} as MyRecord<K, T>;
    for (const key of keys) {
        result[key] = value;
    }
    return result;
}

const roles = createUserRoles();
const countries = getCountryNames();
const indexed = createIndexedData(["a", "b", "c"]);
console.log(roles, countries, indexed);"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let options = PrinterOptions {
        target: ScriptTarget::ES5,
        ..Default::default()
    };
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);

    let mut printer = Printer::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_target_es5(ctx.target_es5);
    printer.set_source_map_text(parser.get_source_text());
    printer.enable_source_map("test.js", "test.ts");
    printer.emit(root);

    let output = printer.get_output().to_string();
    let map_json = printer.generate_source_map_json().expect("source map");
    let map_value: Value = serde_json::from_str(&map_json).expect("parse source map");

    let mappings = map_value
        .get("mappings")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    let decoded = decode_mappings(mappings);

    assert!(
        output.contains("createUserRoles"),
        "expected createUserRoles function in output. output: {output}"
    );
    assert!(
        output.contains("getCountryNames"),
        "expected getCountryNames function in output. output: {output}"
    );
    assert!(
        output.contains("createNestedRecord"),
        "expected createNestedRecord function in output. output: {output}"
    );
    assert!(
        output.contains("createRecord"),
        "expected createRecord function in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for Record mapped type"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

/// Comprehensive test combining multiple mapped type patterns.
/// Tests Partial, Required, Readonly, Pick, Record, and custom mapped types together.
#[test]
fn test_source_map_mapped_type_es5_comprehensive() {
    let source = r#"// Comprehensive mapped type utility library

// Standard mapped types
type Partial<T> = { [P in keyof T]?: T[P] };
type Required<T> = { [P in keyof T]-?: T[P] };
type Readonly<T> = { readonly [P in keyof T]: T[P] };
type Pick<T, K extends keyof T> = { [P in K]: T[P] };
type Record<K extends keyof any, T> = { [P in K]: T };
type Omit<T, K extends keyof any> = Pick<T, Exclude<keyof T, K>>;

// Custom mapped types
type Mutable<T> = { -readonly [P in keyof T]: T[P] };
type Nullable<T> = { [P in keyof T]: T[P] | null };
type NonNullableProps<T> = { [P in keyof T]: NonNullable<T[P]> };

// Key remapping
type Getters<T> = {
    [K in keyof T as `get${Capitalize<string & K>}`]: () => T[K];
};

type Setters<T> = {
    [K in keyof T as `set${Capitalize<string & K>}`]: (value: T[K]) => void;
};

// Entity interface
interface Entity {
    id: number;
    name: string;
    createdAt: Date;
    updatedAt: Date | null;
}

// Repository using mapped types
class Repository<T extends Entity> {
    private items: Map<number, T> = new Map();

    create(data: Omit<T, "id" | "createdAt" | "updatedAt">): T {
        const now = new Date();
        const id = this.items.size + 1;
        const entity = {
            ...data,
            id,
            createdAt: now,
            updatedAt: null
        } as T;
        this.items.set(id, entity);
        return entity;
    }

    update(id: number, data: Partial<Omit<T, "id" | "createdAt">>): T | undefined {
        const entity = this.items.get(id);
        if (entity) {
            const updated = { ...entity, ...data, updatedAt: new Date() };
            this.items.set(id, updated);
            return updated;
        }
        return undefined;
    }

    findById(id: number): Readonly<T> | undefined {
        return this.items.get(id);
    }

    findAll(): ReadonlyArray<Readonly<T>> {
        return Array.from(this.items.values());
    }

    getFields<K extends keyof T>(id: number, fields: K[]): Pick<T, K> | undefined {
        const entity = this.items.get(id);
        if (entity) {
            const result = {} as Pick<T, K>;
            for (const field of fields) {
                result[field] = entity[field];
            }
            return result;
        }
        return undefined;
    }
}

// Form state using mapped types
type FormState<T> = {
    values: T;
    errors: Partial<Record<keyof T, string>>;
    touched: Partial<Record<keyof T, boolean>>;
    dirty: boolean;
};

function createFormState<T>(initial: T): FormState<T> {
    return {
        values: initial,
        errors: {},
        touched: {},
        dirty: false
    };
}

interface User extends Entity {
    email: string;
    role: "admin" | "user";
}

const userRepo = new Repository<User>();
const newUser = userRepo.create({ name: "Alice", email: "alice@example.com", role: "user" });
const formState = createFormState({ name: "", email: "" });
console.log(newUser, formState);"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let options = PrinterOptions {
        target: ScriptTarget::ES5,
        ..Default::default()
    };
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);

    let mut printer = Printer::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_target_es5(ctx.target_es5);
    printer.set_source_map_text(parser.get_source_text());
    printer.enable_source_map("test.js", "test.ts");
    printer.emit(root);

    let output = printer.get_output().to_string();
    let map_json = printer.generate_source_map_json().expect("source map");
    let map_value: Value = serde_json::from_str(&map_json).expect("parse source map");

    let mappings = map_value
        .get("mappings")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    let decoded = decode_mappings(mappings);

    assert!(
        output.contains("Repository"),
        "expected Repository class in output. output: {output}"
    );
    assert!(
        output.contains("create"),
        "expected create method in output. output: {output}"
    );
    assert!(
        output.contains("update"),
        "expected update method in output. output: {output}"
    );
    assert!(
        output.contains("findById"),
        "expected findById method in output. output: {output}"
    );
    assert!(
        output.contains("getFields"),
        "expected getFields method in output. output: {output}"
    );
    assert!(
        output.contains("createFormState"),
        "expected createFormState function in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for comprehensive mapped types"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

// =============================================================================
// ES5 SOURCE MAP TESTS: UTILITY TYPES
// =============================================================================

