#[test]
fn test_source_map_interface_recursive_types() {
    // Test interface with recursive/self-referencing types
    let source = r#"interface TreeNode<T> {
    value: T;
    children: TreeNode<T>[];
    parent?: TreeNode<T>;
}

interface LinkedListNode<T> {
    value: T;
    next: LinkedListNode<T> | null;
    prev: LinkedListNode<T> | null;
}

interface JSONValue {
    [key: string]: JSONValue | string | number | boolean | null | JSONValue[];
}

const tree: TreeNode<string> = {
    value: "root",
    children: [
        { value: "child1", children: [] },
        { value: "child2", children: [
            { value: "grandchild", children: [] }
        ]}
    ]
};

const listNode: LinkedListNode<number> = {
    value: 1,
    next: { value: 2, next: null, prev: null },
    prev: null
};

function traverseTree<T>(node: TreeNode<T>, callback: (val: T) => void): void {
    callback(node.value);
    for (const child of node.children) {
        traverseTree(child, callback);
    }
}

traverseTree(tree, (v) => console.log(v));
console.log(listNode.value);"#;

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
        output.contains("tree") && output.contains("traverseTree"),
        "expected output to contain tree and traverseTree. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for recursive types"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_interface_discriminated_unions() {
    // Test interface with discriminated union patterns
    let source = r#"interface SuccessResult {
    kind: "success";
    data: string;
    timestamp: number;
}

interface ErrorResult {
    kind: "error";
    error: string;
    code: number;
}

interface LoadingResult {
    kind: "loading";
    progress: number;
}

type Result = SuccessResult | ErrorResult | LoadingResult;

interface Action {
    type: string;
}

interface AddAction extends Action {
    type: "add";
    payload: number;
}

interface RemoveAction extends Action {
    type: "remove";
    id: string;
}

type AppAction = AddAction | RemoveAction;

function handleResult(result: Result): string {
    switch (result.kind) {
        case "success":
            return "Data: " + result.data;
        case "error":
            return "Error " + result.code + ": " + result.error;
        case "loading":
            return "Loading: " + result.progress + "%";
    }
}

const success: SuccessResult = { kind: "success", data: "hello", timestamp: Date.now() };
const error: ErrorResult = { kind: "error", error: "Not found", code: 404 };

console.log(handleResult(success));
console.log(handleResult(error));"#;

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
        output.contains("handleResult") || output.contains("success"),
        "expected output to contain handleResult or success. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for discriminated unions"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_interface_type_guards() {
    // Test interface with type guard patterns
    let source = r#"interface Fish {
    swim(): void;
    name: string;
}

interface Bird {
    fly(): void;
    name: string;
}

interface Cat {
    meow(): void;
    name: string;
}

type Animal = Fish | Bird | Cat;

function isFish(animal: Animal): animal is Fish {
    return (animal as Fish).swim !== undefined;
}

function isBird(animal: Animal): animal is Bird {
    return (animal as Bird).fly !== undefined;
}

const fish: Fish = {
    name: "Nemo",
    swim() { console.log("Swimming..."); }
};

const bird: Bird = {
    name: "Tweety",
    fly() { console.log("Flying..."); }
};

function handleAnimal(animal: Animal): void {
    if (isFish(animal)) {
        animal.swim();
    } else if (isBird(animal)) {
        animal.fly();
    } else {
        animal.meow();
    }
}

handleAnimal(fish);
handleAnimal(bird);
console.log(fish.name, bird.name);"#;

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
        output.contains("isFish") || output.contains("handleAnimal"),
        "expected output to contain isFish or handleAnimal. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for type guards"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_interface_rest_elements() {
    // Test interface with rest elements in types
    let source = r#"interface FunctionWithRest {
    (...args: number[]): number;
}

interface ArrayWithRest {
    items: [string, ...number[]];
    mixed: [boolean, string, ...any[]];
}

interface SpreadParams {
    call(...args: string[]): void;
    apply(first: number, ...rest: string[]): string;
}

const sum: FunctionWithRest = function(...args: number[]): number {
    return args.reduce((a, b) => a + b, 0);
};

const arr: ArrayWithRest = {
    items: ["header", 1, 2, 3, 4],
    mixed: [true, "text", 1, "a", null]
};

const params: SpreadParams = {
    call(...args: string[]): void {
        console.log(args.join(", "));
    },
    apply(first: number, ...rest: string[]): string {
        return first + ": " + rest.join(" ");
    }
};

console.log(sum(1, 2, 3, 4, 5));
console.log(arr.items);
params.call("a", "b", "c");
console.log(params.apply(42, "hello", "world"));"#;

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
        output.contains("sum") || output.contains("params"),
        "expected output to contain sum or params. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for rest elements"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

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

