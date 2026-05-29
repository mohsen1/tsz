#[test]
fn test_source_map_interface_callback_patterns() {
    // Test interface with various callback patterns
    let source = r#"interface EventCallback<T> {
    (event: T): void;
}

interface AsyncCallback<T, E = Error> {
    (error: E | null, result: T | null): void;
}

interface Middleware<T> {
    (context: T, next: () => void): void;
}

interface Reducer<S, A> {
    (state: S, action: A): S;
}

interface EventEmitter<Events extends Record<string, any>> {
    on<K extends keyof Events>(event: K, callback: EventCallback<Events[K]>): void;
    emit<K extends keyof Events>(event: K, data: Events[K]): void;
}

const onClick: EventCallback<{ x: number; y: number }> = (event) => {
    console.log("Clicked at", event.x, event.y);
};

const fetchCallback: AsyncCallback<string> = (error, result) => {
    if (error) console.log("Error:", error.message);
    else console.log("Result:", result);
};

const logger: Middleware<{ path: string }> = (ctx, next) => {
    console.log("Request:", ctx.path);
    next();
};

const counterReducer: Reducer<number, { type: string }> = (state, action) => {
    if (action.type === "increment") return state + 1;
    if (action.type === "decrement") return state - 1;
    return state;
};

onClick({ x: 100, y: 200 });
fetchCallback(null, "data");
logger({ path: "/api" }, () => console.log("Done"));
console.log(counterReducer(0, { type: "increment" }));"#;

    let (parser, root) = parse_test_source(source);

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
        output.contains("onClick") || output.contains("counterReducer"),
        "expected output to contain onClick or counterReducer. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for callback patterns"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_interface_utility_patterns() {
    // Test interface with utility type patterns
    let source = r#"interface User {
    id: number;
    name: string;
    email: string;
    age: number;
    role: "admin" | "user";
}

interface PartialUser {
    id?: number;
    name?: string;
    email?: string;
}

interface RequiredUser {
    id: number;
    name: string;
    email: string;
}

interface UserKeys {
    keys: keyof User;
}

interface UserUpdate {
    data: Partial<User>;
    updatedAt: Date;
}

interface UserCreation {
    data: Omit<User, "id">;
    createdAt: Date;
}

const partialUser: PartialUser = { name: "John" };
const requiredUser: RequiredUser = { id: 1, name: "Jane", email: "jane@example.com" };

const update: UserUpdate = {
    data: { name: "Updated Name", age: 31 },
    updatedAt: new Date()
};

const creation: UserCreation = {
    data: { name: "New User", email: "new@example.com", age: 25, role: "user" },
    createdAt: new Date()
};

function updateUser(id: number, updates: Partial<User>): void {
    console.log("Updating user", id, "with", updates);
}

console.log(partialUser.name);
console.log(requiredUser.email);
updateUser(1, update.data);"#;

    let (parser, root) = parse_test_source(source);

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
        output.contains("partialUser") || output.contains("updateUser"),
        "expected output to contain partialUser or updateUser. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for utility patterns"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_interface_module_patterns() {
    // Test interface with module-like patterns
    let source = r#"interface ModuleExports {
    default: () => void;
    named: string;
    Config: { version: string };
}

interface PluginInterface {
    name: string;
    version: string;
    init(): void;
    destroy(): void;
}

interface ModuleLoader {
    load(name: string): Promise<ModuleExports>;
    unload(name: string): void;
    getLoaded(): string[];
}

interface PluginRegistry {
    register(plugin: PluginInterface): void;
    unregister(name: string): void;
    get(name: string): PluginInterface | undefined;
    list(): PluginInterface[];
}

const myPlugin: PluginInterface = {
    name: "MyPlugin",
    version: "1.0.0",
    init() { console.log("Plugin initialized"); },
    destroy() { console.log("Plugin destroyed"); }
};

const registry: PluginRegistry = {
    plugins: [] as PluginInterface[],
    register(plugin) {
        (this as any).plugins.push(plugin);
    },
    unregister(name) {
        const idx = (this as any).plugins.findIndex((p: PluginInterface) => p.name === name);
        if (idx !== -1) (this as any).plugins.splice(idx, 1);
    },
    get(name) {
        return (this as any).plugins.find((p: PluginInterface) => p.name === name);
    },
    list() {
        return (this as any).plugins;
    }
} as any;

registry.register(myPlugin);
console.log(registry.list());
myPlugin.init();"#;

    let (parser, root) = parse_test_source(source);

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
        output.contains("myPlugin") || output.contains("registry"),
        "expected output to contain myPlugin or registry. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for module patterns"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_interface_builder_patterns() {
    // Test interface with builder patterns
    let source = r#"interface QueryBuilder<T> {
    select(...fields: (keyof T)[]): this;
    where(condition: Partial<T>): this;
    orderBy(field: keyof T, direction: "asc" | "desc"): this;
    limit(count: number): this;
    execute(): T[];
}

interface FormBuilder<T> {
    field<K extends keyof T>(name: K, value: T[K]): this;
    validate(): boolean;
    build(): T;
    reset(): this;
}

interface HttpRequestBuilder {
    url(url: string): this;
    method(method: "GET" | "POST" | "PUT" | "DELETE"): this;
    header(name: string, value: string): this;
    body(data: any): this;
    send(): Promise<Response>;
}

class SimpleQueryBuilder<T> implements QueryBuilder<T> {
    private query: any = {};

    select(...fields: (keyof T)[]): this {
        this.query.fields = fields;
        return this;
    }

    where(condition: Partial<T>): this {
        this.query.where = condition;
        return this;
    }

    orderBy(field: keyof T, direction: "asc" | "desc"): this {
        this.query.orderBy = { field, direction };
        return this;
    }

    limit(count: number): this {
        this.query.limit = count;
        return this;
    }

    execute(): T[] {
        console.log("Executing query:", this.query);
        return [];
    }
}

interface User { id: number; name: string; age: number }

const query = new SimpleQueryBuilder<User>()
    .select("name", "age")
    .where({ age: 25 })
    .orderBy("name", "asc")
    .limit(10);

console.log(query.execute());"#;

    let (parser, root) = parse_test_source(source);

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
        output.contains("SimpleQueryBuilder") || output.contains("query"),
        "expected output to contain SimpleQueryBuilder or query. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for builder patterns"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_interface_state_machine() {
    // Test interface with state machine patterns
    let source = r#"interface State<T extends string> {
    name: T;
    onEnter?(): void;
    onExit?(): void;
}

interface Transition<From extends string, To extends string> {
    from: From;
    to: To;
    condition?(): boolean;
    action?(): void;
}

interface StateMachine<States extends string> {
    currentState: States;
    states: State<States>[];
    transitions: Transition<States, States>[];
    transition(to: States): boolean;
    canTransition(to: States): boolean;
}

type TrafficLightState = "red" | "yellow" | "green";

const trafficLight: StateMachine<TrafficLightState> = {
    currentState: "red",
    states: [
        { name: "red", onEnter: () => console.log("Stop!") },
        { name: "yellow", onEnter: () => console.log("Caution!") },
        { name: "green", onEnter: () => console.log("Go!") }
    ],
    transitions: [
        { from: "red", to: "green" },
        { from: "green", to: "yellow" },
        { from: "yellow", to: "red" }
    ],
    canTransition(to: TrafficLightState): boolean {
        return this.transitions.some(t => t.from === this.currentState && t.to === to);
    },
    transition(to: TrafficLightState): boolean {
        if (this.canTransition(to)) {
            this.currentState = to;
            const state = this.states.find(s => s.name === to);
            if (state && state.onEnter) state.onEnter();
            return true;
        }
        return false;
    }
};

console.log("Current:", trafficLight.currentState);
trafficLight.transition("green");
trafficLight.transition("yellow");
trafficLight.transition("red");"#;

    let (parser, root) = parse_test_source(source);

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
        output.contains("trafficLight") || output.contains("currentState"),
        "expected output to contain trafficLight or currentState. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for state machine patterns"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

// =============================================================================
// Enum ES5 Source Map Tests - Extended Patterns
// =============================================================================

#[test]
fn test_source_map_enum_es5_bitwise_flags() {
    let source = r#"enum Permission {
    None = 0,
    Read = 1 << 0,
    Write = 1 << 1,
    Execute = 1 << 2,
    ReadWrite = Read | Write,
    All = Read | Write | Execute
}

const userPerms: Permission = Permission.ReadWrite;
const hasRead = (userPerms & Permission.Read) !== 0;"#;

    let (parser, root) = parse_test_source(source);

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
        output.contains("Permission") || output.contains("ReadWrite"),
        "expected output to contain Permission enum. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for bitwise flag enum"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_enum_es5_explicit_numeric() {
    let source = r#"enum HttpStatus {
    OK = 200,
    Created = 201,
    Accepted = 202,
    BadRequest = 400,
    Unauthorized = 401,
    Forbidden = 403,
    NotFound = 404,
    InternalServerError = 500
}

function handleResponse(status: HttpStatus): string {
    if (status >= 400) {
        return "Error";
    }
    return "Success";
}"#;

    let (parser, root) = parse_test_source(source);

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
        output.contains("HttpStatus") || output.contains("200"),
        "expected output to contain HttpStatus enum. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for explicit numeric enum"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_enum_es5_expression_initializers() {
    let source = r#"const BASE = 100;

enum Computed {
    First = BASE,
    Second = BASE + 1,
    Third = BASE * 2,
    Fourth = Math.floor(BASE / 3),
    Fifth = "prefix".length
}

const val = Computed.Third;"#;

    let (parser, root) = parse_test_source(source);

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
        output.contains("Computed") || output.contains("BASE"),
        "expected output to contain Computed enum. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for expression initializer enum"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_enum_es5_ambient_declare() {
    let source = r#"declare enum ExternalStatus {
    Active,
    Inactive,
    Pending
}

enum LocalStatus {
    Active = 0,
    Inactive = 1,
    Pending = 2
}

const status: LocalStatus = LocalStatus.Active;"#;

    let (parser, root) = parse_test_source(source);

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

    // Declare enums should be erased, only LocalStatus should remain
    assert!(
        output.contains("LocalStatus"),
        "expected output to contain LocalStatus enum. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for ambient declare enum"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_enum_es5_member_as_type() {
    let source = r#"enum Direction {
    Up = "UP",
    Down = "DOWN",
    Left = "LEFT",
    Right = "RIGHT"
}

type VerticalDirection = Direction.Up | Direction.Down;
type HorizontalDirection = Direction.Left | Direction.Right;

function move(dir: VerticalDirection): void {
    console.log(dir);
}

move(Direction.Up);"#;

    let (parser, root) = parse_test_source(source);

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
        output.contains("Direction") || output.contains("UP"),
        "expected output to contain Direction enum. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for enum member as type"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_enum_es5_keyof_typeof() {
    let source = r#"enum Color {
    Red = "red",
    Green = "green",
    Blue = "blue"
}

type ColorKey = keyof typeof Color;
type ColorValue = typeof Color[ColorKey];

function getColorName(key: ColorKey): string {
    return Color[key];
}

const result = getColorName("Red");"#;

    let (parser, root) = parse_test_source(source);

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
        output.contains("Color") || output.contains("getColorName"),
        "expected output to contain Color enum. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for keyof typeof enum"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_enum_es5_nested_in_module() {
    let source = r#"module App {
    export enum Status {
        Loading,
        Ready,
        Error
    }

    export module Sub {
        export enum Priority {
            Low = 1,
            Medium = 2,
            High = 3
        }
    }
}

const status = App.Status.Ready;
const priority = App.Sub.Priority.High;"#;

    let (parser, root) = parse_test_source(source);

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
        output.contains("App") || output.contains("Status"),
        "expected output to contain App module with enums. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for nested enum in module"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_enum_es5_with_interface() {
    let source = r#"enum TaskStatus {
    Todo = "TODO",
    InProgress = "IN_PROGRESS",
    Done = "DONE"
}

interface Task {
    id: number;
    title: string;
    status: TaskStatus;
}

function createTask(title: string): Task {
    return {
        id: Date.now(),
        title: title,
        status: TaskStatus.Todo
    };
}

const task = createTask("Test task");"#;

    let (parser, root) = parse_test_source(source);

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
        output.contains("TaskStatus") || output.contains("createTask"),
        "expected output to contain TaskStatus enum. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for enum with interface"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_enum_es5_function_parameter() {
    let source = r#"enum LogLevel {
    Debug = 0,
    Info = 1,
    Warn = 2,
    Error = 3
}

function log(level: LogLevel, message: string): void {
    if (level >= LogLevel.Warn) {
        console.error(`[${LogLevel[level]}] ${message}`);
    } else {
        console.log(`[${LogLevel[level]}] ${message}`);
    }
}

log(LogLevel.Info, "Application started");
log(LogLevel.Error, "Something went wrong");"#;

    let (parser, root) = parse_test_source(source);

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
        output.contains("LogLevel") || output.contains("log"),
        "expected output to contain LogLevel enum. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for enum as function parameter"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_enum_es5_advanced_combined() {
    let source = r#"// Numeric enum with explicit values
enum Priority {
    Critical = 100,
    High = 75,
    Medium = 50,
    Low = 25
}

// String enum
enum Category {
    Bug = "BUG",
    Feature = "FEATURE",
    Task = "TASK"
}

// Const enum (should be inlined)
const enum Visibility {
    Public,
    Private,
    Internal
}

// Enum in class
class Issue {
    priority: Priority;
    category: Category;
    visibility: number;

    constructor(priority: Priority, category: Category) {
        this.priority = priority;
        this.category = category;
        this.visibility = Visibility.Public;
    }

    isPriority(level: Priority): boolean {
        return this.priority >= level;
    }
}

// Generic with enum constraint
function filterByCategory<T extends { category: Category }>(
    items: T[],
    category: Category
): T[] {
    return items.filter(item => item.category === category);
}

const issue = new Issue(Priority.High, Category.Bug);
console.log(issue.isPriority(Priority.Medium));"#;

    let (parser, root) = parse_test_source(source);

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
        output.contains("Priority") || output.contains("Category"),
        "expected output to contain enum declarations. output: {output}"
    );
    assert!(
        output.contains("Issue"),
        "expected output to contain Issue class. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for advanced enum patterns"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

// =============================================================================
// Class Field ES5 Source Map Tests
// =============================================================================

#[test]
fn test_source_map_class_field_public_basic() {
    let source = r#"class Person {
    name: string;
    age: number;
    active: boolean;
}

const person = new Person();
person.name = "John";
person.age = 30;"#;

    let (parser, root) = parse_test_source(source);

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
        output.contains("Person"),
        "expected output to contain Person class. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for public fields"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_class_field_public_initializers() {
    let source = r#"class Config {
    host: string = "localhost";
    port: number = 8080;
    debug: boolean = false;
    tags: string[] = [];
    metadata: object = {};
}

const config = new Config();
console.log(config.host, config.port);"#;

    let (parser, root) = parse_test_source(source);

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
        output.contains("Config") || output.contains("localhost"),
        "expected output to contain Config class or initializers. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for field initializers"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_class_field_static_basic() {
    let source = r#"class Counter {
    static count: number = 0;
    static name: string = "Counter";

    static increment(): void {
        Counter.count++;
    }

    static reset(): void {
        Counter.count = 0;
    }
}

Counter.increment();
console.log(Counter.count);"#;

    let (parser, root) = parse_test_source(source);

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
        output.contains("Counter"),
        "expected output to contain Counter class. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for static fields"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_class_field_static_initializers() {
    let source = r#"class App {
    static version: string = "1.0.0";
    static buildDate: Date = new Date();
    static features: string[] = ["auth", "logging"];
    static config = {
        debug: true,
        timeout: 5000
    };
}

console.log(App.version, App.features);"#;

    let (parser, root) = parse_test_source(source);

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
        output.contains("App") || output.contains("version"),
        "expected output to contain App class. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for static field initializers"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_class_field_computed() {
    let source = r#"const nameKey = "name";
const ageKey = "age";

class Person {
    [nameKey]: string = "Unknown";
    [ageKey]: number = 0;
    ["status"]: string = "active";
    [Symbol.toStringTag]: string = "Person";
}

const p = new Person();
console.log(p[nameKey]);"#;

    let (parser, root) = parse_test_source(source);

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
        output.contains("Person") || output.contains("nameKey"),
        "expected output to contain Person class or computed keys. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for computed fields"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_class_field_private_es5() {
    let source = r#"class BankAccount {
    #balance: number = 0;
    #owner: string;

    constructor(owner: string, initialBalance: number) {
        this.#owner = owner;
        this.#balance = initialBalance;
    }

    deposit(amount: number): void {
        this.#balance += amount;
    }

    getBalance(): number {
        return this.#balance;
    }
}

const account = new BankAccount("John", 100);
account.deposit(50);"#;

    let (parser, root) = parse_test_source(source);

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
        output.contains("BankAccount") || output.contains("deposit"),
        "expected output to contain BankAccount class. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for private fields ES5"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_class_field_static_private() {
    let source = r#"class Logger {
    static #instance: Logger | null = null;
    static #logLevel: number = 1;

    private constructor() {}

    static getInstance(): Logger {
        if (!Logger.#instance) {
            Logger.#instance = new Logger();
        }
        return Logger.#instance;
    }

    static setLogLevel(level: number): void {
        Logger.#logLevel = level;
    }
}

const logger = Logger.getInstance();"#;

    let (parser, root) = parse_test_source(source);

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
        output.contains("Logger") || output.contains("getInstance"),
        "expected output to contain Logger class. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for static private fields"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_class_field_readonly() {
    let source = r#"class Constants {
    readonly PI: number = 3.14159;
    readonly E: number = 2.71828;
    static readonly MAX_SIZE: number = 1000;
    static readonly APP_NAME: string = "MyApp";
}

const c = new Constants();
console.log(c.PI, Constants.MAX_SIZE);"#;

    let (parser, root) = parse_test_source(source);

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
        output.contains("Constants") || output.contains("3.14159"),
        "expected output to contain Constants class. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for readonly fields"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_class_field_with_accessors() {
    let source = r#"class Rectangle {
    #width: number = 0;
    #height: number = 0;

    get width(): number {
        return this.#width;
    }

    set width(value: number) {
        this.#width = Math.max(0, value);
    }

    get height(): number {
        return this.#height;
    }

    set height(value: number) {
        this.#height = Math.max(0, value);
    }

    get area(): number {
        return this.#width * this.#height;
    }
}

const rect = new Rectangle();
rect.width = 10;
rect.height = 5;"#;

    let (parser, root) = parse_test_source(source);

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
        output.contains("Rectangle") || output.contains("width"),
        "expected output to contain Rectangle class. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for fields with accessors"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_class_field_combined() {
    let source = r#"const dynamicKey = "dynamicProp";

class CompleteEntity {
    // Public fields
    name: string = "";
    count: number = 0;

    // Static fields
    static instances: number = 0;
    static readonly VERSION: string = "1.0";

    // Private fields
    #id: number;
    #secret: string = "hidden";

    // Static private
    static #totalCreated: number = 0;

    // Computed field
    [dynamicKey]: boolean = true;

    // Readonly
    readonly createdAt: Date = new Date();

    constructor(name: string) {
        this.name = name;
        this.#id = ++CompleteEntity.#totalCreated;
        CompleteEntity.instances++;
    }

    get id(): number {
        return this.#id;
    }

    static getTotal(): number {
        return CompleteEntity.#totalCreated;
    }
}

const entity = new CompleteEntity("Test");
console.log(entity.name, entity.id, CompleteEntity.instances);"#;

    let (parser, root) = parse_test_source(source);

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
        output.contains("CompleteEntity"),
        "expected output to contain CompleteEntity class. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for combined class fields"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

// =============================================================================
// Decorator ES5 Source Map Tests - Extended Patterns
// =============================================================================

