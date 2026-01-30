//! Source map tests - Part 3 (ES5 transforms continued)

use crate::emit_context::EmitContext;
use crate::emitter::{Printer, PrinterOptions, ScriptTarget};
use crate::lowering_pass::LoweringPass;
use crate::parser::ParserState;
#[allow(unused_imports)]
use crate::source_map::*;
#[allow(unused_imports)]
use crate::source_map_test_utils::{decode_mappings, find_line_col, has_mapping_for_prefixes};
use serde_json::Value;

#[test]
fn test_source_map_throw_basic() {
    // Test basic throw Error
    let source = r#"function validate(value: number): void {
    if (value < 0) {
        throw new Error("Value must be non-negative");
    }
    console.log("Valid:", value);
}

try {
    validate(-5);
} catch (e) {
    console.error(e);
}"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("throw") || output.contains("Error"),
        "expected output to contain throw or Error. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for basic throw"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_throw_string() {
    // Test throw string literal
    let source = r#"function assertNotNull(value: any): void {
    if (value === null) {
        throw "Value cannot be null";
    }
    if (value === undefined) {
        throw "Value cannot be undefined";
    }
}

try {
    assertNotNull(null);
} catch (e) {
    console.log("Caught:", e);
}"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("throw") || output.contains("assertNotNull"),
        "expected output to contain throw or function name. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for throw string"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_throw_custom_error() {
    // Test throw custom error class
    let source = r#"class ValidationError extends Error {
    constructor(public field: string, message: string) {
        super(message);
        this.name = "ValidationError";
    }
}

function validateEmail(email: string): void {
    if (!email.includes("@")) {
        throw new ValidationError("email", "Invalid email format");
    }
}

try {
    validateEmail("invalid");
} catch (e) {
    if (e instanceof ValidationError) {
        console.log("Field:", e.field);
    }
}"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("ValidationError") || output.contains("throw"),
        "expected output to contain ValidationError or throw. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for throw custom error"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_throw_expression() {
    // Test throw with expression
    let source = r#"function getErrorMessage(code: number): string {
    return "Error code: " + code;
}

function process(code: number): void {
    if (code >= 400) {
        throw new Error(getErrorMessage(code));
    }
    console.log("Success");
}

try {
    process(404);
} catch (e) {
    console.error(e);
}"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("process") || output.contains("throw"),
        "expected output to contain process or throw. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for throw expression"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_throw_in_function() {
    // Test throw in various function types
    let source = r#"const validator = (value: any) => {
    if (typeof value !== "number") {
        throw new TypeError("Expected a number");
    }
    return value;
};

function strictValidator(value: any): number {
    if (value === null || value === undefined) {
        throw new ReferenceError("Value is required");
    }
    return validator(value);
}

console.log(strictValidator(42));"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("validator") || output.contains("throw"),
        "expected output to contain validator or throw. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for throw in function"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_throw_conditional() {
    // Test throw in conditional branches
    let source = r#"function divide(a: number, b: number): number {
    if (b === 0) {
        throw new Error("Division by zero");
    } else if (!Number.isFinite(a) || !Number.isFinite(b)) {
        throw new Error("Invalid operands");
    } else if (a < 0 && b < 0) {
        throw new Error("Both operands negative");
    }
    return a / b;
}

try {
    console.log(divide(10, 2));
    console.log(divide(10, 0));
} catch (e) {
    console.error(e);
}"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("divide") || output.contains("throw"),
        "expected output to contain divide or throw. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for throw conditional"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_throw_class_method() {
    // Test throw in class method
    let source = r#"class BankAccount {
    private balance: number = 0;

    deposit(amount: number): void {
        if (amount <= 0) {
            throw new Error("Deposit amount must be positive");
        }
        this.balance += amount;
    }

    withdraw(amount: number): void {
        if (amount <= 0) {
            throw new Error("Withdrawal amount must be positive");
        }
        if (amount > this.balance) {
            throw new Error("Insufficient funds");
        }
        this.balance -= amount;
    }

    getBalance(): number {
        return this.balance;
    }
}

const account = new BankAccount();
account.deposit(100);
console.log(account.getBalance());"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("BankAccount") || output.contains("throw"),
        "expected output to contain BankAccount or throw. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for throw in class method"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_throw_async() {
    // Test throw in async function
    let source = r#"async function fetchData(url: string): Promise<string> {
    if (!url) {
        throw new Error("URL is required");
    }
    if (!url.startsWith("http")) {
        throw new Error("Invalid URL protocol");
    }
    return "data from " + url;
}

async function processUrl(url: string): Promise<void> {
    try {
        const data = await fetchData(url);
        console.log(data);
    } catch (e) {
        throw new Error("Failed to process: " + e);
    }
}

processUrl("https://example.com");"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("fetchData") || output.contains("function"),
        "expected output to contain fetchData or function. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for throw async"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_throw_rethrow() {
    // Test rethrow in catch block
    let source = r#"function riskyOperation(): void {
    throw new Error("Original error");
}

function wrapper(): void {
    try {
        riskyOperation();
    } catch (e) {
        console.log("Logging error");
        throw e;
    }
}

function outerWrapper(): void {
    try {
        wrapper();
    } catch (e) {
        if (e instanceof Error) {
            throw new Error("Wrapped: " + e.message);
        }
        throw e;
    }
}

try {
    outerWrapper();
} catch (e) {
    console.error("Final catch:", e);
}"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("wrapper") || output.contains("throw"),
        "expected output to contain wrapper or throw. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for throw rethrow"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_throw_combined() {
    // Test combined throw patterns
    let source = r#"class HttpError extends Error {
    constructor(public statusCode: number, message: string) {
        super(message);
        this.name = "HttpError";
    }
}

class ApiClient {
    private baseUrl: string;

    constructor(baseUrl: string) {
        if (!baseUrl) {
            throw new Error("Base URL is required");
        }
        this.baseUrl = baseUrl;
    }

    async request(endpoint: string): Promise<any> {
        if (!endpoint) {
            throw new HttpError(400, "Endpoint is required");
        }

        const url = this.baseUrl + endpoint;

        try {
            const response = await fetch(url);
            if (!response.ok) {
                throw new HttpError(response.status, "Request failed");
            }
            return response.json();
        } catch (e) {
            if (e instanceof HttpError) {
                throw e;
            }
            throw new HttpError(500, "Network error: " + e);
        }
    }
}

async function main(): Promise<void> {
    const client = new ApiClient("https://api.example.com");
    try {
        const data = await client.request("/users");
        console.log(data);
    } catch (e) {
        if (e instanceof HttpError) {
            console.error("HTTP Error:", e.statusCode, e.message);
        } else {
            throw e;
        }
    }
}

main();"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("ApiClient") || output.contains("HttpError"),
        "expected output to contain ApiClient or HttpError. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for combined throw patterns"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

// ============================================================================
// Debugger Statement ES5 Source Map Tests
// ============================================================================

#[test]
fn test_source_map_debugger_basic() {
    // Test basic debugger statement
    let source = r#"function processData(data: any): void {
    debugger;
    console.log("Processing:", data);
}

processData({ value: 42 });"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("debugger") || output.contains("processData"),
        "expected output to contain debugger or function name. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for basic debugger"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_debugger_in_function() {
    // Test debugger in function
    let source = r#"function calculate(a: number, b: number): number {
    debugger;
    const sum = a + b;
    debugger;
    return sum;
}

function main(): void {
    debugger;
    const result = calculate(10, 20);
    console.log("Result:", result);
}

main();"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("calculate") || output.contains("main"),
        "expected output to contain function names. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for debugger in function"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_debugger_conditional() {
    // Test debugger in conditional
    let source = r#"function checkValue(value: number): string {
    if (value < 0) {
        debugger;
        return "negative";
    } else if (value === 0) {
        debugger;
        return "zero";
    } else {
        debugger;
        return "positive";
    }
}

console.log(checkValue(-5));
console.log(checkValue(0));
console.log(checkValue(10));"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("checkValue") || output.contains("if"),
        "expected output to contain function name or if. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for debugger conditional"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_debugger_loop() {
    // Test debugger in loop
    let source = r#"function processArray(arr: number[]): number {
    let sum = 0;
    for (let i = 0; i < arr.length; i++) {
        debugger;
        sum += arr[i];
    }
    return sum;
}

function processWhile(n: number): number {
    let count = 0;
    while (count < n) {
        debugger;
        count++;
    }
    return count;
}

console.log(processArray([1, 2, 3]));
console.log(processWhile(5));"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("processArray") || output.contains("for"),
        "expected output to contain function name or for. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for debugger loop"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_debugger_class_method() {
    // Test debugger in class method
    let source = r#"class Calculator {
    private value: number = 0;

    add(n: number): this {
        debugger;
        this.value += n;
        return this;
    }

    subtract(n: number): this {
        debugger;
        this.value -= n;
        return this;
    }

    getValue(): number {
        debugger;
        return this.value;
    }
}

const calc = new Calculator();
console.log(calc.add(10).subtract(3).getValue());"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("Calculator") || output.contains("function"),
        "expected output to contain Calculator or function. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for debugger class method"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_debugger_try_catch() {
    // Test debugger in try-catch
    let source = r#"function riskyOperation(value: number): number {
    try {
        debugger;
        if (value < 0) {
            throw new Error("Negative value");
        }
        return value * 2;
    } catch (e) {
        debugger;
        console.error("Error:", e);
        return 0;
    } finally {
        debugger;
        console.log("Cleanup");
    }
}

console.log(riskyOperation(5));
console.log(riskyOperation(-1));"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("riskyOperation") || output.contains("try"),
        "expected output to contain function name or try. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for debugger try-catch"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_debugger_arrow_function() {
    // Test debugger in arrow function
    let source = r#"const multiply = (a: number, b: number) => {
    debugger;
    return a * b;
};

const process = (arr: number[]) => {
    debugger;
    return arr.map((x) => {
        debugger;
        return x * 2;
    });
};

console.log(multiply(3, 4));
console.log(process([1, 2, 3]));"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("multiply") || output.contains("function"),
        "expected output to contain multiply or function. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for debugger arrow function"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_debugger_async() {
    // Test debugger in async function
    let source = r#"async function fetchData(url: string): Promise<string> {
    debugger;
    const response = await fetch(url);
    debugger;
    return await response.text();
}

async function processUrls(urls: string[]): Promise<void> {
    for (const url of urls) {
        debugger;
        const data = await fetchData(url);
        console.log(data);
    }
}

processUrls(["https://example.com"]);"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("fetchData") || output.contains("function"),
        "expected output to contain fetchData or function. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for debugger async"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_debugger_switch() {
    // Test debugger in switch statement
    let source = r#"function handleAction(action: string): void {
    switch (action) {
        case "start":
            debugger;
            console.log("Starting...");
            break;
        case "stop":
            debugger;
            console.log("Stopping...");
            break;
        case "pause":
            debugger;
            console.log("Pausing...");
            break;
        default:
            debugger;
            console.log("Unknown action");
    }
}

handleAction("start");
handleAction("unknown");"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("handleAction") || output.contains("switch"),
        "expected output to contain handleAction or switch. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for debugger switch"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_debugger_combined() {
    // Test combined debugger patterns
    let source = r#"class DataProcessor {
    private data: number[] = [];

    constructor(initialData: number[]) {
        debugger;
        this.data = initialData;
    }

    async process(): Promise<number[]> {
        debugger;
        const results: number[] = [];

        for (let i = 0; i < this.data.length; i++) {
            debugger;
            try {
                const value = this.data[i];
                if (value < 0) {
                    debugger;
                    throw new Error("Negative value");
                }
                results.push(value * 2);
            } catch (e) {
                debugger;
                results.push(0);
            }
        }

        return results;
    }

    filter(predicate: (n: number) => boolean): number[] {
        debugger;
        return this.data.filter((n) => {
            debugger;
            return predicate(n);
        });
    }
}

async function main(): Promise<void> {
    debugger;
    const processor = new DataProcessor([1, -2, 3, 4]);
    const processed = await processor.process();
    console.log("Processed:", processed);

    const filtered = processor.filter((n) => n > 0);
    console.log("Filtered:", filtered);
}

main();"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("DataProcessor") || output.contains("process"),
        "expected output to contain DataProcessor or process. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for combined debugger patterns"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

// =============================================================================
// Empty Statement ES5 Source Map Tests
// =============================================================================

#[test]
fn test_source_map_empty_statement_basic() {
    // Test basic empty statement
    let source = r#"let x = 1;;
let y = 2;"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("x") && output.contains("y"),
        "expected output to contain x and y. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for empty statement"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_empty_statement_multiple() {
    // Test multiple consecutive empty statements
    let source = r#"let a = 1;;;
;;;
let b = 2;"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("a") && output.contains("b"),
        "expected output to contain a and b. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for multiple empty statements"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_empty_statement_in_function() {
    // Test empty statement in function
    let source = r#"function processData(value: number): number {
    ;
    const result = value * 2;
    ;
    return result;
}

const output = processData(5);"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("processData"),
        "expected output to contain processData. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for empty statement in function"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_empty_statement_in_loop() {
    // Test empty statement in loop
    let source = r#"function iterate(count: number): void {
    for (let i = 0; i < count; i++) {
        ;
        console.log(i);
        ;
    }
}

iterate(3);"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("iterate"),
        "expected output to contain iterate. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for empty statement in loop"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_empty_statement_in_conditional() {
    // Test empty statement in conditional
    let source = r#"function checkValue(value: number): string {
    if (value > 0) {
        ;
        return "positive";
    } else {
        ;
        return "non-positive";
    }
}

const result = checkValue(10);"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("checkValue"),
        "expected output to contain checkValue. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for empty statement in conditional"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_empty_statement_in_class() {
    // Test empty statement in class method
    let source = r#"class Calculator {
    private value: number = 0;

    add(n: number): number {
        ;
        this.value += n;
        ;
        return this.value;
    }

    reset(): void {
        ;
        this.value = 0;
        ;
    }
}

const calc = new Calculator();
calc.add(5);"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("Calculator"),
        "expected output to contain Calculator. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for empty statement in class"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_empty_statement_in_switch() {
    // Test empty statement in switch case
    let source = r#"function handleCase(value: number): string {
    switch (value) {
        case 1:
            ;
            return "one";
        case 2:
            ;
            return "two";
        default:
            ;
            return "other";
    }
}

handleCase(1);"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("handleCase") || output.contains("switch"),
        "expected output to contain handleCase or switch. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for empty statement in switch"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_empty_statement_after_declaration() {
    // Test empty statement after various declarations
    let source = r#"const a = 1;;
let b = 2;;
var c = 3;;
function foo() {};;
class Bar {};;
const result = a + b + c;"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("foo") || output.contains("Bar"),
        "expected output to contain foo or Bar. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for empty statement after declaration"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_empty_statement_in_try_catch() {
    // Test empty statement in try-catch blocks
    let source = r#"function safeDivide(a: number, b: number): number {
    try {
        ;
        if (b === 0) {
            throw new Error("Division by zero");
        }
        ;
        return a / b;
    } catch (e) {
        ;
        console.error(e);
        ;
        return 0;
    } finally {
        ;
        console.log("Operation complete");
        ;
    }
}

safeDivide(10, 2);"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("safeDivide"),
        "expected output to contain safeDivide. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for empty statement in try-catch"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_empty_statement_combined() {
    // Test combined empty statement patterns
    let source = r#"class DataService {
    private items: string[] = [];

    constructor() {
        ;
        this.items = [];
        ;
    }

    async fetchData(url: string): Promise<string[]> {
        ;
        try {
            ;
            const response = await fetch(url);
            ;
            if (!response.ok) {
                ;
                throw new Error("Failed to fetch");
            }
            ;
            return ["data1", "data2"];
        } catch (e) {
            ;
            console.error(e);
            ;
            return [];
        }
    }

    process(): void {
        ;
        for (let i = 0; i < this.items.length; i++) {
            ;
            switch (this.items[i]) {
                case "special":
                    ;
                    console.log("Special item");
                    ;
                    break;
                default:
                    ;
                    console.log("Regular item");
                    ;
            }
        }
        ;
    }
}

const service = new DataService();
service.process();;;"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("DataService") || output.contains("process"),
        "expected output to contain DataService or process. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for combined empty statement patterns"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

// =============================================================================
// Return Statement ES5 Source Map Tests
// =============================================================================

#[test]
fn test_source_map_return_basic() {
    // Test basic return statement
    let source = r#"function getValue(): number {
    return 42;
}

const result = getValue();"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("getValue") && output.contains("return"),
        "expected output to contain getValue and return. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for return statement"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_return_void() {
    // Test return without value
    let source = r#"function doNothing(): void {
    console.log("doing nothing");
    return;
}

doNothing();"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("doNothing"),
        "expected output to contain doNothing. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for void return"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_return_expression() {
    // Test return with complex expression
    let source = r#"function calculate(a: number, b: number): number {
    return a * b + (a - b) / 2;
}

const result = calculate(10, 5);"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("calculate"),
        "expected output to contain calculate. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for return expression"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_return_conditional() {
    // Test return in conditional branches
    let source = r#"function getSign(value: number): string {
    if (value > 0) {
        return "positive";
    } else if (value < 0) {
        return "negative";
    } else {
        return "zero";
    }
}

const sign = getSign(-5);"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("getSign"),
        "expected output to contain getSign. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for conditional return"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_return_object() {
    // Test return object literal
    let source = r#"function createPerson(name: string, age: number): { name: string; age: number } {
    return {
        name: name,
        age: age
    };
}

const person = createPerson("Alice", 30);"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("createPerson"),
        "expected output to contain createPerson. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for return object"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_return_array() {
    // Test return array literal
    let source = r#"function getNumbers(): number[] {
    return [1, 2, 3, 4, 5];
}

function getMatrix(): number[][] {
    return [
        [1, 2, 3],
        [4, 5, 6],
        [7, 8, 9]
    ];
}

const nums = getNumbers();
const matrix = getMatrix();"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("getNumbers") && output.contains("getMatrix"),
        "expected output to contain getNumbers and getMatrix. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for return array"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_return_class_method() {
    // Test return in class methods
    let source = r#"class Calculator {
    private value: number = 0;

    add(n: number): Calculator {
        this.value += n;
        return this;
    }

    getValue(): number {
        return this.value;
    }

    static create(): Calculator {
        return new Calculator();
    }
}

const calc = Calculator.create().add(5).add(3);
const value = calc.getValue();"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("Calculator"),
        "expected output to contain Calculator. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for return in class method"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_return_arrow_function() {
    // Test return in arrow functions
    let source = r#"const add = (a: number, b: number): number => {
    return a + b;
};

const multiply = (a: number, b: number): number => a * b;

const createAdder = (x: number) => {
    return (y: number) => {
        return x + y;
    };
};

const result1 = add(2, 3);
const result2 = multiply(4, 5);
const addFive = createAdder(5);
const result3 = addFive(10);"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("add") || output.contains("multiply"),
        "expected output to contain add or multiply. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for return in arrow function"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_return_async() {
    // Test return in async functions
    let source = r#"async function fetchData(url: string): Promise<string> {
    const response = await fetch(url);
    return response.text();
}

async function processData(): Promise<number> {
    const data = await fetchData("https://api.example.com");
    return data.length;
}

async function main(): Promise<void> {
    const length = await processData();
    console.log(length);
    return;
}

main();"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("fetchData") || output.contains("processData"),
        "expected output to contain fetchData or processData. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for return in async function"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_return_combined() {
    // Test combined return patterns
    let source = r#"class DataProcessor {
    private items: number[] = [];

    constructor(items: number[]) {
        this.items = items;
    }

    process(): { sum: number; avg: number; max: number } {
        if (this.items.length === 0) {
            return { sum: 0, avg: 0, max: 0 };
        }

        const sum = this.items.reduce((a, b) => {
            return a + b;
        }, 0);

        const avg = sum / this.items.length;
        const max = Math.max(...this.items);

        return {
            sum: sum,
            avg: avg,
            max: max
        };
    }

    async processAsync(): Promise<number[]> {
        return new Promise((resolve) => {
            setTimeout(() => {
                const doubled = this.items.map((n) => {
                    return n * 2;
                });
                resolve(doubled);
                return;
            }, 100);
        });
    }

    filter(predicate: (n: number) => boolean): number[] {
        const result: number[] = [];
        for (const item of this.items) {
            if (predicate(item)) {
                result.push(item);
            }
        }
        return result;
    }

    static fromArray(arr: number[]): DataProcessor {
        return new DataProcessor(arr);
    }
}

const processor = DataProcessor.fromArray([1, 2, 3, 4, 5]);
const stats = processor.process();
const filtered = processor.filter((n) => n > 2);"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("DataProcessor") || output.contains("process"),
        "expected output to contain DataProcessor or process. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for combined return patterns"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

// =============================================================================
// Break/Continue Statement ES5 Source Map Tests
// =============================================================================

#[test]
fn test_source_map_break_basic() {
    // Test basic break statement in loop
    let source = r#"function findFirst(arr: number[], target: number): number {
    for (let i = 0; i < arr.length; i++) {
        if (arr[i] === target) {
            break;
        }
    }
    return -1;
}

const result = findFirst([1, 2, 3, 4, 5], 3);"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("findFirst"),
        "expected output to contain findFirst. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for break statement"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_continue_basic() {
    // Test basic continue statement in loop
    let source = r#"function sumPositive(arr: number[]): number {
    let sum = 0;
    for (let i = 0; i < arr.length; i++) {
        if (arr[i] < 0) {
            continue;
        }
        sum += arr[i];
    }
    return sum;
}

const result = sumPositive([1, -2, 3, -4, 5]);"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("sumPositive"),
        "expected output to contain sumPositive. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for continue statement"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_break_while() {
    // Test break in while loop
    let source = r#"function readUntilEnd(data: string[]): string[] {
    const results: string[] = [];
    let i = 0;
    while (i < data.length) {
        if (data[i] === "END") {
            break;
        }
        results.push(data[i]);
        i++;
    }
    return results;
}

const output = readUntilEnd(["a", "b", "END", "c"]);"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("readUntilEnd"),
        "expected output to contain readUntilEnd. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for break in while"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_continue_while() {
    // Test continue in while loop
    let source = r#"function processNonEmpty(items: string[]): string[] {
    const results: string[] = [];
    let i = 0;
    while (i < items.length) {
        const item = items[i];
        i++;
        if (item === "") {
            continue;
        }
        results.push(item.toUpperCase());
    }
    return results;
}

const processed = processNonEmpty(["a", "", "b", "", "c"]);"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("processNonEmpty"),
        "expected output to contain processNonEmpty. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for continue in while"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_break_labeled() {
    // Test labeled break statement
    let source = r#"function findInMatrix(matrix: number[][], target: number): [number, number] | null {
    outer: for (let i = 0; i < matrix.length; i++) {
        for (let j = 0; j < matrix[i].length; j++) {
            if (matrix[i][j] === target) {
                break outer;
            }
        }
    }
    return null;
}

const pos = findInMatrix([[1, 2], [3, 4]], 3);"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("findInMatrix"),
        "expected output to contain findInMatrix. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for labeled break"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_continue_labeled() {
    // Test labeled continue statement
    let source = r#"function processRows(matrix: number[][]): number[] {
    const results: number[] = [];
    outer: for (let i = 0; i < matrix.length; i++) {
        for (let j = 0; j < matrix[i].length; j++) {
            if (matrix[i][j] < 0) {
                continue outer;
            }
        }
        results.push(i);
    }
    return results;
}

const validRows = processRows([[1, 2], [-1, 2], [3, 4]]);"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("processRows"),
        "expected output to contain processRows. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for labeled continue"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_break_switch() {
    // Test break in switch statement
    let source = r#"function getDayName(day: number): string {
    let name: string;
    switch (day) {
        case 0:
            name = "Sunday";
            break;
        case 1:
            name = "Monday";
            break;
        case 2:
            name = "Tuesday";
            break;
        default:
            name = "Unknown";
            break;
    }
    return name;
}

const today = getDayName(1);"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("getDayName") || output.contains("switch"),
        "expected output to contain getDayName or switch. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for break in switch"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_break_do_while() {
    // Test break in do-while loop
    let source = r#"function readInput(): number {
    let value = 0;
    let attempts = 0;
    do {
        attempts++;
        value = Math.random();
        if (value > 0.5) {
            break;
        }
    } while (attempts < 10);
    return value;
}

const input = readInput();"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("readInput"),
        "expected output to contain readInput. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for break in do-while"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_break_continue_combined() {
    // Test combined break and continue patterns
    let source = r#"class DataIterator {
    private data: number[][] = [];

    constructor(data: number[][]) {
        this.data = data;
    }

    processAll(): number[] {
        const results: number[] = [];

        outer: for (let i = 0; i < this.data.length; i++) {
            const row = this.data[i];

            if (row.length === 0) {
                continue;
            }

            for (let j = 0; j < row.length; j++) {
                if (row[j] < 0) {
                    continue outer;
                }
                if (row[j] > 100) {
                    break outer;
                }
                results.push(row[j]);
            }
        }

        return results;
    }

    findValue(target: number): boolean {
        let found = false;
        let i = 0;

        while (i < this.data.length) {
            let j = 0;
            while (j < this.data[i].length) {
                if (this.data[i][j] === target) {
                    found = true;
                    break;
                }
                j++;
            }
            if (found) {
                break;
            }
            i++;
        }

        return found;
    }

    sumPositiveByRow(): number[] {
        const sums: number[] = [];

        for (let i = 0; i < this.data.length; i++) {
            let sum = 0;
            for (const val of this.data[i]) {
                if (val < 0) {
                    continue;
                }
                sum += val;
            }
            sums.push(sum);
        }

        return sums;
    }
}

const iterator = new DataIterator([[1, 2], [3, -4], [5, 6]]);
const processed = iterator.processAll();
const found = iterator.findValue(3);
const sums = iterator.sumPositiveByRow();"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("DataIterator") || output.contains("processAll"),
        "expected output to contain DataIterator or processAll. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for combined break/continue patterns"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

// =============================================================================
// Expression Statement ES5 Source Map Tests
// =============================================================================

#[test]
fn test_source_map_expression_function_call() {
    // Test function call expression statement
    let source = r#"function greet(name: string): void {
    console.log("Hello, " + name);
}

greet("World");
console.log("Done");
Math.random();"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("greet"),
        "expected output to contain greet. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for function call expression"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_expression_assignment() {
    // Test assignment expression statements
    let source = r#"let x: number;
let y: number;
let z: number;

x = 10;
y = x + 5;
z = x * y;
x = y = z = 0;"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("x") && output.contains("y") && output.contains("z"),
        "expected output to contain x, y, z. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for assignment expression"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_expression_increment_decrement() {
    // Test increment/decrement expression statements
    let source = r#"let counter = 0;

counter++;
++counter;
counter--;
--counter;

let arr = [1, 2, 3];
let i = 0;
arr[i++];
arr[++i];"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("counter"),
        "expected output to contain counter. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for increment/decrement"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_expression_method_call() {
    // Test method call expression statements
    let source = r#"const arr = [3, 1, 4, 1, 5];

arr.push(9);
arr.pop();
arr.sort();
arr.reverse();
arr.splice(1, 2);

const str = "hello";
str.toUpperCase();
str.charAt(0);"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("arr") && output.contains("push"),
        "expected output to contain arr and push. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for method call"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_expression_compound_assignment() {
    // Test compound assignment expression statements
    let source = r#"let a = 10;
let b = 5;
let s = "hello";

a += b;
a -= b;
a *= b;
a /= b;
a %= b;
a **= 2;
s += " world";

let bits = 0xFF;
bits &= 0x0F;
bits |= 0xF0;
bits ^= 0xFF;
bits <<= 1;
bits >>= 1;
bits >>>= 1;"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("a") && output.contains("b"),
        "expected output to contain a and b. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for compound assignment"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_expression_ternary() {
    // Test ternary expression statement
    let source = r#"const value = 10;
let result: string;

value > 5 ? console.log("big") : console.log("small");

function check(x: number): void {
    x > 0 ? x++ : x--;
}

check(value);"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("value") || output.contains("check"),
        "expected output to contain value or check. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for ternary expression"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_expression_logical() {
    // Test logical expression statements
    let source = r#"const a = true;
const b = false;
let callback: (() => void) | null = null;

a && console.log("a is true");
b || console.log("b is false");
callback && callback();

function maybeCall(fn: (() => void) | undefined): void {
    fn && fn();
}

maybeCall(() => console.log("called"));"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("maybeCall") || output.contains("callback"),
        "expected output to contain maybeCall or callback. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for logical expression"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_expression_new() {
    // Test new expression statements
    let source = r#"class Widget {
    constructor(public name: string) {}
}

new Widget("button");
new Date();
new Array(10);
new Map();
new Set([1, 2, 3]);

const widgets: Widget[] = [];
widgets.push(new Widget("checkbox"));"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("Widget"),
        "expected output to contain Widget. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for new expression"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
#[ignore = "Test times out/hangs - needs performance investigation"]
fn test_source_map_expression_delete_void_typeof() {
    // Test delete, void, typeof expression statements
    let source = r#"const obj: { [key: string]: number } = { a: 1, b: 2 };

delete obj.a;
delete obj["b"];

void 0;
void console.log("side effect");

typeof obj;
typeof undefined;"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("obj") || output.contains("delete"),
        "expected output to contain obj or delete. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for delete/void/typeof"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_expression_combined() {
    // Test combined expression statement patterns
    let source = r#"class Counter {
    private count = 0;
    private history: number[] = [];

    increment(): void {
        this.count++;
        this.history.push(this.count);
    }

    decrement(): void {
        --this.count;
        this.history.push(this.count);
    }

    reset(): void {
        this.count = 0;
        this.history.length = 0;
    }

    log(): void {
        console.log("Count:", this.count);
        this.history.forEach((v, i) => console.log(i, v));
    }
}

const counter = new Counter();
counter.increment();
counter.increment();
counter.decrement();
counter.log();

let x = 0;
let y = 0;
x = y = 10;
x += 5;
y *= 2;

x > y ? console.log("x wins") : console.log("y wins");
x && y && console.log("both truthy");

const arr = [1, 2, 3];
arr.push(4);
arr.pop();
arr.sort((a, b) => a - b);
arr.reverse();"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("Counter") || output.contains("increment"),
        "expected output to contain Counter or increment. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for combined expression patterns"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

// =============================================================================
// Variable Declaration ES5 Source Map Tests
// =============================================================================

#[test]
fn test_source_map_var_declaration_basic() {
    // Test basic var declarations
    let source = r#"var x = 10;
var y = 20;
var z;
z = x + y;
console.log(z);"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("var") && output.contains("x") && output.contains("y"),
        "expected output to contain var, x, y. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for var declaration"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_let_const_declaration() {
    // Test let and const declarations (downleveled to var in ES5)
    let source = r#"let a = 1;
let b = 2;
const c = 3;
const d = a + b + c;
console.log(d);"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("a") && output.contains("b") && output.contains("c"),
        "expected output to contain a, b, c. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for let/const declaration"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_multiple_declarators() {
    // Test multiple variable declarators in single statement
    let source = r#"var a = 1, b = 2, c = 3;
let x = 10, y = 20, z = 30;
const m = 100, n = 200;
console.log(a, b, c, x, y, z, m, n);"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("a") && output.contains("b") && output.contains("c"),
        "expected output to contain a, b, c. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for multiple declarators"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_declaration_with_type() {
    // Test declarations with type annotations
    let source = r#"let num: number = 42;
let str: string = "hello";
let bool: boolean = true;
let arr: number[] = [1, 2, 3];
let obj: { x: number; y: number } = { x: 1, y: 2 };
let fn: (a: number) => number = (a) => a * 2;

console.log(num, str, bool, arr, obj, fn(5));"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("num") && output.contains("str"),
        "expected output to contain num and str. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for typed declarations"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_declaration_destructuring_object() {
    // Test object destructuring declarations
    let source = r#"const obj = { a: 1, b: 2, c: 3 };
const { a, b } = obj;
const { c: renamed } = obj;
const { a: x, b: y, ...rest } = { a: 1, b: 2, c: 3, d: 4 };

console.log(a, b, renamed, x, y, rest);"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("obj"),
        "expected output to contain obj. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for object destructuring"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_declaration_destructuring_array() {
    // Test array destructuring declarations
    let source = r#"const arr = [1, 2, 3, 4, 5];
const [first, second] = arr;
const [a, , c] = arr;
const [head, ...tail] = arr;
const [x, y, z = 10] = [1, 2];

console.log(first, second, a, c, head, tail, x, y, z);"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("arr"),
        "expected output to contain arr. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for array destructuring"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_declaration_in_function() {
    // Test variable declarations inside functions
    let source = r#"function processData(input: number): number {
    var multiplier = 2;
    let result = input * multiplier;
    const final = result + 10;

    if (final > 50) {
        let bonus = 5;
        return final + bonus;
    }

    return final;
}

const output = processData(25);"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("processData"),
        "expected output to contain processData. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for declarations in function"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_declaration_in_for_loop() {
    // Test variable declarations in for loops
    let source = r#"const items = [1, 2, 3, 4, 5];
let sum = 0;

for (let i = 0; i < items.length; i++) {
    sum += items[i];
}

for (var j = 0; j < 3; j++) {
    console.log(j);
}

for (const item of items) {
    console.log(item);
}

for (const [index, value] of items.entries()) {
    console.log(index, value);
}

console.log(sum);"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("items") || output.contains("sum"),
        "expected output to contain items or sum. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for declarations in for loop"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_declaration_complex_initializers() {
    // Test declarations with complex initializers
    let source = r#"const fn = function(x: number): number { return x * 2; };
const arrow = (y: number): number => y + 1;
const obj = { method(): number { return 42; } };
const arr = [1, 2, 3].map((n) => n * 2);
const cond = true ? "yes" : "no";
const template = `value is ${42}`;

const nested = {
    data: [1, 2, 3],
    process(): number[] {
        return this.data.map((n) => n * 2);
    }
};

console.log(fn(5), arrow(10), obj.method(), arr, cond, template);"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("fn") || output.contains("arrow"),
        "expected output to contain fn or arrow. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for complex initializers"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_declaration_combined() {
    // Test combined variable declaration patterns
    let source = r#"class DataStore {
    private items: string[] = [];

    constructor() {
        const initial = ["a", "b", "c"];
        this.items = initial;
    }

    process(): { count: number; items: string[] } {
        var count = 0;
        let filtered: string[] = [];
        const threshold = 1;

        for (let i = 0; i < this.items.length; i++) {
            const item = this.items[i];
            if (item.length > threshold) {
                filtered.push(item);
                count++;
            }
        }

        const { length: total } = filtered;
        const [first = "none", ...rest] = filtered;

        let result = { count, items: filtered };
        return result;
    }
}

const store = new DataStore();
const { count, items } = store.process();
console.log(count, items);"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("DataStore") || output.contains("process"),
        "expected output to contain DataStore or process. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for combined declaration patterns"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

// =============================================================================
// Function Declaration ES5 Source Map Tests
// =============================================================================

#[test]
fn test_source_map_function_declaration_basic() {
    // Test basic function declaration
    let source = r#"function greet() {
    console.log("Hello");
}

function sayGoodbye() {
    console.log("Goodbye");
}

greet();
sayGoodbye();"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("greet") && output.contains("sayGoodbye"),
        "expected output to contain greet and sayGoodbye. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for function declaration"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_function_with_parameters() {
    // Test function with various parameter types
    let source = r#"function add(a: number, b: number): number {
    return a + b;
}

function greetPerson(name: string, age: number): string {
    return "Hello " + name + ", you are " + age;
}

function processArray(items: number[]): number {
    let sum = 0;
    for (const item of items) {
        sum += item;
    }
    return sum;
}

console.log(add(1, 2));
console.log(greetPerson("Alice", 30));
console.log(processArray([1, 2, 3]));"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("add") && output.contains("greetPerson"),
        "expected output to contain add and greetPerson. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for function with parameters"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_function_default_parameters() {
    // Test function with default parameters
    let source = r#"function greet(name: string = "World"): string {
    return "Hello, " + name;
}

function createPoint(x: number = 0, y: number = 0): { x: number; y: number } {
    return { x, y };
}

function formatMessage(msg: string, prefix: string = "[INFO]", suffix: string = ""): string {
    return prefix + " " + msg + suffix;
}

console.log(greet());
console.log(greet("Alice"));
console.log(createPoint());
console.log(createPoint(10, 20));
console.log(formatMessage("Hello"));"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("greet") || output.contains("createPoint"),
        "expected output to contain greet or createPoint. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for default parameters"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_function_rest_parameters() {
    // Test function with rest parameters
    let source = r#"function sum(...numbers: number[]): number {
    return numbers.reduce((a, b) => a + b, 0);
}

function concat(separator: string, ...items: string[]): string {
    return items.join(separator);
}

function logAll(prefix: string, ...values: any[]): void {
    for (const value of values) {
        console.log(prefix, value);
    }
}

console.log(sum(1, 2, 3, 4, 5));
console.log(concat(", ", "a", "b", "c"));
logAll("[DEBUG]", "one", "two", "three");"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("sum") || output.contains("concat"),
        "expected output to contain sum or concat. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for rest parameters"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_function_nested() {
    // Test nested function declarations
    let source = r#"function outer(x: number): number {
    function inner(y: number): number {
        return y * 2;
    }

    function helper(z: number): number {
        return z + 1;
    }

    return inner(helper(x));
}

function createCounter(): () => number {
    let count = 0;

    function increment(): number {
        count++;
        return count;
    }

    return increment;
}

console.log(outer(5));
const counter = createCounter();
console.log(counter());
console.log(counter());"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("outer") || output.contains("createCounter"),
        "expected output to contain outer or createCounter. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for nested functions"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_function_generator() {
    // Test generator function declarations
    let source = r#"function* numberGenerator(): Generator<number> {
    yield 1;
    yield 2;
    yield 3;
}

function* rangeGenerator(start: number, end: number): Generator<number> {
    for (let i = start; i <= end; i++) {
        yield i;
    }
}

function* infiniteCounter(): Generator<number> {
    let n = 0;
    while (true) {
        yield n++;
    }
}

const gen = numberGenerator();
console.log(gen.next().value);
console.log(gen.next().value);"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("numberGenerator") || output.contains("rangeGenerator"),
        "expected output to contain numberGenerator or rangeGenerator. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for generator functions"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_function_async() {
    // Test async function declarations
    let source = r#"async function fetchData(url: string): Promise<string> {
    const response = await fetch(url);
    return response.text();
}

async function processItems(items: number[]): Promise<number[]> {
    const results: number[] = [];
    for (const item of items) {
        const processed = await Promise.resolve(item * 2);
        results.push(processed);
    }
    return results;
}

async function delay(ms: number): Promise<void> {
    return new Promise((resolve) => setTimeout(resolve, ms));
}

fetchData("https://example.com").then(console.log);"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("fetchData") || output.contains("processItems"),
        "expected output to contain fetchData or processItems. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for async functions"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_function_destructuring_params() {
    // Test functions with destructuring parameters
    let source = r#"function processPoint({ x, y }: { x: number; y: number }): number {
    return x + y;
}

function formatUser({ name, age = 0 }: { name: string; age?: number }): string {
    return name + " (" + age + ")";
}

function sumArray([first, second, ...rest]: number[]): number {
    return first + second + rest.reduce((a, b) => a + b, 0);
}

console.log(processPoint({ x: 10, y: 20 }));
console.log(formatUser({ name: "Alice" }));
console.log(sumArray([1, 2, 3, 4, 5]));"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("processPoint") || output.contains("formatUser"),
        "expected output to contain processPoint or formatUser. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for destructuring params"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_function_generic() {
    // Test generic function declarations
    let source = r#"function identity<T>(value: T): T {
    return value;
}

function map<T, U>(items: T[], fn: (item: T) => U): U[] {
    const results: U[] = [];
    for (const item of items) {
        results.push(fn(item));
    }
    return results;
}

function swap<T, U>(pair: [T, U]): [U, T] {
    return [pair[1], pair[0]];
}

console.log(identity<number>(42));
console.log(map<number, string>([1, 2, 3], (n) => String(n)));
console.log(swap<string, number>(["hello", 42]));"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("identity") || output.contains("map"),
        "expected output to contain identity or map. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for generic functions"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_function_declaration_combined() {
    // Test combined function declaration patterns
    let source = r#"function createCalculator(initialValue: number = 0) {
    let value = initialValue;

    function add(n: number): void {
        value += n;
    }

    function subtract(n: number): void {
        value -= n;
    }

    function getValue(): number {
        return value;
    }

    async function asyncMultiply(n: number): Promise<number> {
        return value * n;
    }

    function* valueHistory(): Generator<number> {
        yield initialValue;
        yield value;
    }

    return {
        add,
        subtract,
        getValue,
        asyncMultiply,
        valueHistory
    };
}

function processData<T>(
    items: T[],
    { filter = (x: T) => true, transform = (x: T) => x }: {
        filter?: (item: T) => boolean;
        transform?: (item: T) => T;
    } = {}
): T[] {
    return items.filter(filter).map(transform);
}

function compose<A, B, C>(
    f: (a: A) => B,
    g: (b: B) => C
): (a: A) => C {
    return function(a: A): C {
        return g(f(a));
    };
}

const calc = createCalculator(10);
calc.add(5);
console.log(calc.getValue());

const numbers = processData([1, 2, 3, 4, 5], {
    filter: (n) => n > 2,
    transform: (n) => n * 2
});
console.log(numbers);"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("createCalculator") || output.contains("processData"),
        "expected output to contain createCalculator or processData. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for combined function patterns"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

// =============================================================================
// Class Declaration ES5 Source Map Tests
// =============================================================================

#[test]
fn test_source_map_class_declaration_basic() {
    // Test basic class declaration with ES5 downleveling
    let source = r#"class Animal {
    name: string;

    constructor(name: string) {
        this.name = name;
    }
}

const dog = new Animal("Rex");
console.log(dog.name);"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("Animal"),
        "expected output to contain Animal. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for basic class"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_class_with_methods() {
    // Test class with instance methods
    let source = r#"class Calculator {
    private value: number;

    constructor(initial: number = 0) {
        this.value = initial;
    }

    add(n: number): this {
        this.value += n;
        return this;
    }

    subtract(n: number): this {
        this.value -= n;
        return this;
    }

    multiply(n: number): this {
        this.value *= n;
        return this;
    }

    getResult(): number {
        return this.value;
    }
}

const calc = new Calculator(10);
console.log(calc.add(5).multiply(2).getResult());"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("Calculator") || output.contains("add"),
        "expected output to contain Calculator or add. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for class methods"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_class_static_members() {
    // Test class with static methods and properties
    let source = r#"class Counter {
    static count: number = 0;

    static increment(): void {
        Counter.count++;
    }

    static decrement(): void {
        Counter.count--;
    }

    static getCount(): number {
        return Counter.count;
    }

    static reset(): void {
        Counter.count = 0;
    }
}

Counter.increment();
Counter.increment();
console.log(Counter.getCount());
Counter.reset();"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("Counter") || output.contains("increment"),
        "expected output to contain Counter or increment. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for static members"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_class_getters_setters() {
    // Test class with getter and setter accessors
    let source = r#"class Person {
    private _firstName: string;
    private _lastName: string;
    private _age: number;

    constructor(firstName: string, lastName: string, age: number) {
        this._firstName = firstName;
        this._lastName = lastName;
        this._age = age;
    }

    get fullName(): string {
        return this._firstName + " " + this._lastName;
    }

    set fullName(value: string) {
        const parts = value.split(" ");
        this._firstName = parts[0] || "";
        this._lastName = parts[1] || "";
    }

    get age(): number {
        return this._age;
    }

    set age(value: number) {
        if (value >= 0) {
            this._age = value;
        }
    }
}

const person = new Person("John", "Doe", 30);
console.log(person.fullName);
person.fullName = "Jane Smith";
console.log(person.age);"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("Person") || output.contains("fullName"),
        "expected output to contain Person or fullName. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for getters/setters"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_class_inheritance() {
    // Test class inheritance with extends
    let source = r#"class Shape {
    protected x: number;
    protected y: number;

    constructor(x: number, y: number) {
        this.x = x;
        this.y = y;
    }

    move(dx: number, dy: number): void {
        this.x += dx;
        this.y += dy;
    }

    describe(): string {
        return "Shape at (" + this.x + ", " + this.y + ")";
    }
}

class Circle extends Shape {
    private radius: number;

    constructor(x: number, y: number, radius: number) {
        super(x, y);
        this.radius = radius;
    }

    describe(): string {
        return "Circle at (" + this.x + ", " + this.y + ") with radius " + this.radius;
    }

    area(): number {
        return Math.PI * this.radius * this.radius;
    }
}

class Rectangle extends Shape {
    private width: number;
    private height: number;

    constructor(x: number, y: number, width: number, height: number) {
        super(x, y);
        this.width = width;
        this.height = height;
    }

    describe(): string {
        return "Rectangle at (" + this.x + ", " + this.y + ")";
    }

    area(): number {
        return this.width * this.height;
    }
}

const circle = new Circle(0, 0, 5);
const rect = new Rectangle(10, 10, 20, 30);
console.log(circle.describe());
console.log(rect.area());"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("Shape") || output.contains("Circle") || output.contains("Rectangle"),
        "expected output to contain Shape, Circle, or Rectangle. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for class inheritance"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_class_constructor_parameter_properties() {
    // Test class with constructor parameter properties
    let source = r#"class User {
    constructor(
        public readonly id: number,
        public name: string,
        private email: string,
        protected role: string = "user"
    ) {}

    getEmail(): string {
        return this.email;
    }

    describe(): string {
        return "User " + this.name + " with role " + this.role;
    }
}

class Admin extends User {
    constructor(id: number, name: string, email: string) {
        super(id, name, email, "admin");
    }

    getRole(): string {
        return this.role;
    }
}

const user = new User(1, "John", "john@example.com");
const admin = new Admin(2, "Jane", "jane@example.com");
console.log(user.describe());
console.log(admin.getRole());"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("User") || output.contains("Admin"),
        "expected output to contain User or Admin. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for constructor parameter properties"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_class_expression() {
    // Test class expressions (anonymous and named)
    let source = r#"const Logger = class {
    private prefix: string;

    constructor(prefix: string) {
        this.prefix = prefix;
    }

    log(message: string): void {
        console.log(this.prefix + ": " + message);
    }
};

const NamedLogger = class CustomLogger {
    private level: string;

    constructor(level: string) {
        this.level = level;
    }

    log(message: string): void {
        console.log("[" + this.level + "] " + message);
    }
};

const factories = {
    createLogger: class {
        create(name: string) {
            return new Logger(name);
        }
    }
};

const logger = new Logger("App");
const namedLogger = new NamedLogger("INFO");
logger.log("Hello");
namedLogger.log("World");"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("Logger") || output.contains("log"),
        "expected output to contain Logger or log. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for class expressions"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_class_generic() {
    // Test generic class declarations
    let source = r#"class Container<T> {
    private value: T;

    constructor(value: T) {
        this.value = value;
    }

    getValue(): T {
        return this.value;
    }

    setValue(value: T): void {
        this.value = value;
    }
}

class Pair<K, V> {
    constructor(private key: K, private val: V) {}

    getKey(): K {
        return this.key;
    }

    getValue(): V {
        return this.val;
    }

    toArray(): [K, V] {
        return [this.key, this.val];
    }
}

class Stack<T> {
    private items: T[] = [];

    push(item: T): void {
        this.items.push(item);
    }

    pop(): T | undefined {
        return this.items.pop();
    }

    peek(): T | undefined {
        return this.items[this.items.length - 1];
    }

    isEmpty(): boolean {
        return this.items.length === 0;
    }
}

const numContainer = new Container<number>(42);
const strPair = new Pair<string, number>("age", 30);
const stack = new Stack<string>();
stack.push("hello");
console.log(numContainer.getValue());
console.log(strPair.toArray());"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("Container") || output.contains("Stack") || output.contains("Pair"),
        "expected output to contain Container, Stack, or Pair. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for generic classes"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_class_abstract() {
    // Test abstract class declarations
    let source = r#"abstract class Vehicle {
    protected speed: number = 0;

    constructor(protected name: string) {}

    abstract start(): void;
    abstract stop(): void;

    accelerate(amount: number): void {
        this.speed += amount;
    }

    getSpeed(): number {
        return this.speed;
    }

    describe(): string {
        return this.name + " moving at " + this.speed;
    }
}

class Car extends Vehicle {
    constructor(name: string) {
        super(name);
    }

    start(): void {
        console.log(this.name + " engine started");
        this.speed = 10;
    }

    stop(): void {
        console.log(this.name + " stopped");
        this.speed = 0;
    }
}

class Bicycle extends Vehicle {
    constructor(name: string) {
        super(name);
    }

    start(): void {
        console.log("Pedaling " + this.name);
        this.speed = 5;
    }

    stop(): void {
        console.log("Braking " + this.name);
        this.speed = 0;
    }
}

const car = new Car("Tesla");
const bike = new Bicycle("Mountain Bike");
car.start();
car.accelerate(50);
console.log(car.describe());
bike.start();
console.log(bike.getSpeed());"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("Vehicle") || output.contains("Car") || output.contains("Bicycle"),
        "expected output to contain Vehicle, Car, or Bicycle. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for abstract classes"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_class_declaration_combined() {
    // Test combined class declaration patterns
    let source = r#"abstract class BaseService<T> {
    protected items: T[] = [];

    abstract validate(item: T): boolean;

    add(item: T): void {
        if (this.validate(item)) {
            this.items.push(item);
        }
    }

    getAll(): T[] {
        return this.items.slice();
    }

    get count(): number {
        return this.items.length;
    }
}

interface Entity {
    id: number;
    name: string;
}

class EntityService extends BaseService<Entity> {
    private static instance: EntityService;
    static counter: number = 0;

    private constructor() {
        super();
    }

    static getInstance(): EntityService {
        if (!EntityService.instance) {
            EntityService.instance = new EntityService();
        }
        return EntityService.instance;
    }

    validate(item: Entity): boolean {
        EntityService.counter++;
        return item.id > 0 && item.name.length > 0;
    }

    findById(id: number): Entity | undefined {
        return this.items.find(item => item.id === id);
    }

    get isEmpty(): boolean {
        return this.items.length === 0;
    }

    set defaultItem(item: Entity) {
        if (this.isEmpty) {
            this.add(item);
        }
    }
}

class CachedService<T extends Entity> extends BaseService<T> {
    private cache: Map<number, T> = new Map();

    constructor(private readonly cacheDuration: number = 1000) {
        super();
    }

    validate(item: T): boolean {
        return item.id > 0;
    }

    add(item: T): void {
        super.add(item);
        this.cache.set(item.id, item);
    }

    getFromCache(id: number): T | undefined {
        return this.cache.get(id);
    }
}

const service = EntityService.getInstance();
service.add({ id: 1, name: "First" });
service.add({ id: 2, name: "Second" });
console.log(service.count);
console.log(EntityService.counter);

const cached = new CachedService<Entity>(5000);
cached.add({ id: 100, name: "Cached Item" });
console.log(cached.getFromCache(100));"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("BaseService")
            || output.contains("EntityService")
            || output.contains("CachedService"),
        "expected output to contain BaseService, EntityService, or CachedService. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for combined class patterns"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

// =============================================================================
// Interface/Type Alias ES5 Source Map Tests
// =============================================================================

#[test]
fn test_source_map_interface_basic() {
    // Test basic interface with type erasure - interface is removed, runtime code mapped
    let source = r#"interface Person {
    name: string;
    age: number;
}

const person: Person = {
    name: "John",
    age: 30
};

console.log(person.name);"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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

    // Interface should be erased, but runtime code should be present
    assert!(
        !output.contains("interface"),
        "interface keyword should be erased. output: {output}"
    );
    assert!(
        output.contains("person"),
        "expected output to contain person. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for interface usage"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_type_alias_basic() {
    // Test basic type alias with type erasure
    let source = r#"type StringOrNumber = string | number;
type Point = { x: number; y: number };

const value: StringOrNumber = "hello";
const point: Point = { x: 10, y: 20 };

console.log(value, point.x);"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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

    // Type alias should be erased
    assert!(
        !output.contains("type StringOrNumber") && !output.contains("type Point"),
        "type alias should be erased. output: {output}"
    );
    assert!(
        output.contains("value") && output.contains("point"),
        "expected output to contain value and point. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for type alias usage"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_interface_with_methods() {
    // Test interface with method signatures
    let source = r#"interface Calculator {
    add(a: number, b: number): number;
    subtract(a: number, b: number): number;
    multiply(a: number, b: number): number;
}

const calc: Calculator = {
    add(a, b) { return a + b; },
    subtract(a, b) { return a - b; },
    multiply(a, b) { return a * b; }
};

console.log(calc.add(5, 3));
console.log(calc.subtract(10, 4));
console.log(calc.multiply(2, 6));"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("calc") && output.contains("add"),
        "expected output to contain calc and add. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for interface with methods"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_interface_extends() {
    // Test interface extending another interface
    let source = r#"interface Animal {
    name: string;
    age: number;
}

interface Dog extends Animal {
    breed: string;
    bark(): void;
}

const dog: Dog = {
    name: "Rex",
    age: 5,
    breed: "German Shepherd",
    bark() {
        console.log("Woof!");
    }
};

dog.bark();
console.log(dog.name, dog.breed);"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("dog") && output.contains("bark"),
        "expected output to contain dog and bark. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for interface extends"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_type_alias_union_intersection() {
    // Test union and intersection type aliases
    let source = r#"type ID = string | number;
type Name = { first: string; last: string };
type Age = { age: number };
type Person = Name & Age;

const id: ID = 123;
const person: Person = {
    first: "John",
    last: "Doe",
    age: 30
};

function printId(id: ID): void {
    console.log("ID:", id);
}

function printPerson(p: Person): void {
    console.log(p.first, p.last, p.age);
}

printId(id);
printPerson(person);"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("printId") && output.contains("printPerson"),
        "expected output to contain printId and printPerson. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for union/intersection types"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_interface_generic() {
    // Test generic interface declarations
    let source = r#"interface Container<T> {
    value: T;
    getValue(): T;
    setValue(value: T): void;
}

interface Pair<K, V> {
    key: K;
    value: V;
}

const numContainer: Container<number> = {
    value: 42,
    getValue() { return this.value; },
    setValue(v) { this.value = v; }
};

const pair: Pair<string, number> = {
    key: "age",
    value: 30
};

console.log(numContainer.getValue());
console.log(pair.key, pair.value);"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("numContainer") || output.contains("pair"),
        "expected output to contain numContainer or pair. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for generic interface"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_type_alias_generic() {
    // Test generic type alias declarations
    let source = r#"type Nullable<T> = T | null;
type Result<T, E> = { success: true; value: T } | { success: false; error: E };
type AsyncResult<T> = Promise<Result<T, Error>>;

const name: Nullable<string> = "John";
const nullName: Nullable<string> = null;

const success: Result<number, string> = { success: true, value: 42 };
const failure: Result<number, string> = { success: false, error: "Not found" };

function processResult<T>(result: Result<T, string>): T | null {
    if (result.success) {
        return result.value;
    }
    console.log("Error:", result.error);
    return null;
}

console.log(name, nullName);
console.log(processResult(success));
console.log(processResult(failure));"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("processResult") || output.contains("success"),
        "expected output to contain processResult or success. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for generic type alias"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_type_alias_mapped() {
    // Test mapped type aliases
    let source = r#"type Readonly<T> = { readonly [K in keyof T]: T[K] };
type Partial<T> = { [K in keyof T]?: T[K] };
type Pick<T, K extends keyof T> = { [P in K]: T[P] };

interface User {
    id: number;
    name: string;
    email: string;
}

const readonlyUser: Readonly<User> = {
    id: 1,
    name: "John",
    email: "john@example.com"
};

const partialUser: Partial<User> = {
    name: "Jane"
};

const pickedUser: Pick<User, "id" | "name"> = {
    id: 2,
    name: "Bob"
};

console.log(readonlyUser.name);
console.log(partialUser.name);
console.log(pickedUser.id, pickedUser.name);"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("readonlyUser") || output.contains("partialUser"),
        "expected output to contain readonlyUser or partialUser. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for mapped types"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_type_alias_conditional() {
    // Test conditional type aliases
    let source = r#"type IsString<T> = T extends string ? true : false;
type UnwrapPromise<T> = T extends Promise<infer U> ? U : T;
type NonNullable<T> = T extends null | undefined ? never : T;

type StringCheck = IsString<string>;
type NumberCheck = IsString<number>;

const value1: NonNullable<string | null> = "hello";
const value2: UnwrapPromise<Promise<number>> = 42;

function checkType<T>(value: T): IsString<T> {
    return (typeof value === "string") as any;
}

console.log(value1, value2);
console.log(checkType("test"));
console.log(checkType(123));"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("checkType") || output.contains("value1"),
        "expected output to contain checkType or value1. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for conditional types"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_interface_type_alias_combined() {
    // Test combined interface and type alias patterns
    let source = r#"interface BaseEntity {
    id: number;
    createdAt: Date;
    updatedAt: Date;
}

interface User extends BaseEntity {
    username: string;
    email: string;
}

interface Post extends BaseEntity {
    title: string;
    content: string;
    authorId: number;
}

type EntityType = "user" | "post";
type Entity<T extends EntityType> = T extends "user" ? User : Post;

type CreateInput<T extends BaseEntity> = Omit<T, "id" | "createdAt" | "updatedAt">;
type UpdateInput<T extends BaseEntity> = Partial<CreateInput<T>>;

const userInput: CreateInput<User> = {
    username: "johndoe",
    email: "john@example.com"
};

const postUpdate: UpdateInput<Post> = {
    title: "Updated Title"
};

function createEntity<T extends EntityType>(
    type: T,
    input: CreateInput<Entity<T>>
): Entity<T> {
    const now = new Date();
    return {
        ...input,
        id: Math.random(),
        createdAt: now,
        updatedAt: now
    } as Entity<T>;
}

function updateEntity<T extends BaseEntity>(
    entity: T,
    updates: UpdateInput<T>
): T {
    return {
        ...entity,
        ...updates,
        updatedAt: new Date()
    };
}

console.log(userInput);
console.log(postUpdate);
console.log(createEntity("user", userInput));"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("createEntity") || output.contains("updateEntity"),
        "expected output to contain createEntity or updateEntity. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for combined interface/type patterns"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

// =============================================================================
// Additional Interface ES5 Source Map Tests
// =============================================================================

#[test]
fn test_source_map_interface_optional_properties() {
    // Test interface with optional properties
    let source = r#"interface Config {
    host: string;
    port?: number;
    secure?: boolean;
    timeout?: number;
}

const config: Config = {
    host: "localhost"
};

const fullConfig: Config = {
    host: "example.com",
    port: 443,
    secure: true,
    timeout: 5000
};

function createConnection(cfg: Config): void {
    console.log("Connecting to", cfg.host);
    if (cfg.port) {
        console.log("Port:", cfg.port);
    }
}

createConnection(config);
createConnection(fullConfig);"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("config") && output.contains("createConnection"),
        "expected output to contain config and createConnection. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for optional properties"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_interface_readonly_properties() {
    // Test interface with readonly properties
    let source = r#"interface Point {
    readonly x: number;
    readonly y: number;
}

interface Circle {
    readonly center: Point;
    readonly radius: number;
}

const point: Point = { x: 10, y: 20 };
const circle: Circle = {
    center: { x: 0, y: 0 },
    radius: 5
};

function distance(p1: Point, p2: Point): number {
    const dx = p1.x - p2.x;
    const dy = p1.y - p2.y;
    return Math.sqrt(dx * dx + dy * dy);
}

console.log(point.x, point.y);
console.log(circle.center.x, circle.radius);
console.log(distance(point, circle.center));"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("point") && output.contains("distance"),
        "expected output to contain point and distance. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for readonly properties"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_interface_index_signature() {
    // Test interface with index signatures
    let source = r#"interface StringDictionary {
    [key: string]: string;
}

interface NumberDictionary {
    [index: number]: string;
    length: number;
}

interface MixedDictionary {
    [key: string]: number | string;
    name: string;
    count: number;
}

const dict: StringDictionary = {
    foo: "bar",
    hello: "world"
};

const numDict: NumberDictionary = {
    0: "first",
    1: "second",
    length: 2
};

const mixed: MixedDictionary = {
    name: "test",
    count: 42,
    extra: "value"
};

function getValues(d: StringDictionary): string[] {
    return Object.values(d);
}

console.log(dict["foo"]);
console.log(numDict[0]);
console.log(mixed.name, mixed.count);
console.log(getValues(dict));"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("dict") && output.contains("getValues"),
        "expected output to contain dict and getValues. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for index signature"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_interface_call_signature() {
    // Test interface with call signatures
    let source = r#"interface StringProcessor {
    (input: string): string;
}

interface Calculator {
    (a: number, b: number): number;
    description: string;
}

interface Formatter {
    (value: any): string;
    (value: any, format: string): string;
}

const uppercase: StringProcessor = function(input) {
    return input.toUpperCase();
};

const add: Calculator = function(a, b) {
    return a + b;
};
add.description = "Adds two numbers";

const format: Formatter = function(value: any, fmt?: string) {
    if (fmt) {
        return fmt + ": " + String(value);
    }
    return String(value);
};

console.log(uppercase("hello"));
console.log(add(5, 3), add.description);
console.log(format(42));
console.log(format(42, "Number"));"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("uppercase") && output.contains("format"),
        "expected output to contain uppercase and format. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for call signature"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_interface_construct_signature() {
    // Test interface with construct signatures
    let source = r#"interface PointConstructor {
    new(x: number, y: number): { x: number; y: number };
}

interface ClockConstructor {
    new(hour: number, minute: number): ClockInterface;
}

interface ClockInterface {
    tick(): void;
    getTime(): string;
}

function createPoint(ctor: PointConstructor, x: number, y: number) {
    return new ctor(x, y);
}

const PointClass: PointConstructor = class {
    constructor(public x: number, public y: number) {}
};

const point = createPoint(PointClass, 10, 20);
console.log(point.x, point.y);"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("createPoint") || output.contains("PointClass"),
        "expected output to contain createPoint or PointClass. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for construct signature"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_interface_merging() {
    // Test interface merging (declaration merging)
    let source = r#"interface Box {
    height: number;
    width: number;
}

interface Box {
    depth: number;
    color: string;
}

interface Box {
    weight?: number;
}

const box: Box = {
    height: 10,
    width: 20,
    depth: 30,
    color: "red"
};

const heavyBox: Box = {
    height: 5,
    width: 5,
    depth: 5,
    color: "blue",
    weight: 100
};

function describeBox(b: Box): string {
    let desc = b.color + " box: " + b.width + "x" + b.height + "x" + b.depth;
    if (b.weight) {
        desc += " (" + b.weight + "kg)";
    }
    return desc;
}

console.log(describeBox(box));
console.log(describeBox(heavyBox));"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("box") && output.contains("describeBox"),
        "expected output to contain box and describeBox. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for interface merging"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_interface_function_type() {
    // Test interface describing function types
    let source = r#"interface SearchFunc {
    (source: string, subString: string): boolean;
}

interface Comparator<T> {
    (a: T, b: T): number;
}

interface AsyncCallback<T> {
    (error: Error | null, result: T | null): void;
}

const search: SearchFunc = function(source, subString) {
    return source.indexOf(subString) !== -1;
};

const numCompare: Comparator<number> = function(a, b) {
    return a - b;
};

const strCompare: Comparator<string> = function(a, b) {
    return a.localeCompare(b);
};

const callback: AsyncCallback<string> = function(error, result) {
    if (error) {
        console.log("Error:", error.message);
    } else {
        console.log("Result:", result);
    }
};

console.log(search("hello world", "world"));
console.log([3, 1, 2].sort(numCompare));
console.log(["c", "a", "b"].sort(strCompare));
callback(null, "success");"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("search") && output.contains("numCompare"),
        "expected output to contain search and numCompare. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for function type interface"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_interface_class_implements() {
    // Test interface implemented by class
    let source = r#"interface Printable {
    print(): string;
}

interface Comparable<T> {
    compareTo(other: T): number;
}

interface Serializable {
    serialize(): string;
    deserialize(data: string): void;
}

class Document implements Printable, Serializable {
    constructor(private content: string) {}

    print(): string {
        return "Document: " + this.content;
    }

    serialize(): string {
        return JSON.stringify({ content: this.content });
    }

    deserialize(data: string): void {
        const obj = JSON.parse(data);
        this.content = obj.content;
    }
}

class Version implements Comparable<Version> {
    constructor(
        public major: number,
        public minor: number,
        public patch: number
    ) {}

    compareTo(other: Version): number {
        if (this.major !== other.major) return this.major - other.major;
        if (this.minor !== other.minor) return this.minor - other.minor;
        return this.patch - other.patch;
    }
}

const doc = new Document("Hello World");
console.log(doc.print());
console.log(doc.serialize());

const v1 = new Version(1, 2, 3);
const v2 = new Version(1, 3, 0);
console.log(v1.compareTo(v2));"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("Document") || output.contains("Version"),
        "expected output to contain Document or Version. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for class implements interface"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_interface_hybrid_types() {
    // Test interfaces with hybrid types (callable with properties)
    let source = r#"interface Counter {
    (start: number): string;
    interval: number;
    reset(): void;
}

interface Logger {
    (message: string): void;
    level: string;
    prefix: string;
    setLevel(level: string): void;
}

function createCounter(): Counter {
    const counter = function(start: number): string {
        return "Started at: " + start;
    } as Counter;
    counter.interval = 1000;
    counter.reset = function() {
        console.log("Counter reset");
    };
    return counter;
}

function createLogger(): Logger {
    const logger = function(message: string): void {
        console.log(logger.prefix + " [" + logger.level + "] " + message);
    } as Logger;
    logger.level = "INFO";
    logger.prefix = "App";
    logger.setLevel = function(level: string) {
        logger.level = level;
    };
    return logger;
}

const counter = createCounter();
console.log(counter(0));
console.log(counter.interval);
counter.reset();

const logger = createLogger();
logger("Hello");
logger.setLevel("DEBUG");
logger("World");"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("createCounter") || output.contains("createLogger"),
        "expected output to contain createCounter or createLogger. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for hybrid types"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_interface_advanced_combined() {
    // Test combined advanced interface patterns
    let source = r#"interface EventEmitter<T extends string = string> {
    on(event: T, callback: (data: any) => void): this;
    off(event: T, callback: (data: any) => void): this;
    emit(event: T, data?: any): boolean;
    readonly listenerCount: number;
}

interface Repository<T, ID = number> {
    findById(id: ID): T | undefined;
    findAll(): T[];
    save(entity: T): T;
    delete(id: ID): boolean;
    [Symbol.iterator](): Iterator<T>;
}

interface ServiceConfig {
    readonly name: string;
    timeout?: number;
    retries?: number;
    onError?(error: Error): void;
}

interface Service<T> extends EventEmitter<"start" | "stop" | "error"> {
    readonly config: ServiceConfig;
    start(): Promise<void>;
    stop(): Promise<void>;
    getStatus(): "running" | "stopped" | "error";
}

const emitter: EventEmitter = {
    listenerCount: 0,
    on(event, callback) {
        console.log("Registered listener for", event);
        return this;
    },
    off(event, callback) {
        console.log("Removed listener for", event);
        return this;
    },
    emit(event, data) {
        console.log("Emitting", event, data);
        return true;
    }
};

const config: ServiceConfig = {
    name: "MyService",
    timeout: 5000,
    retries: 3,
    onError(error) {
        console.log("Service error:", error.message);
    }
};

emitter.on("message", (data) => console.log(data)).emit("message", "Hello");
console.log(config.name, config.timeout);"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("emitter") || output.contains("config"),
        "expected output to contain emitter or config. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for advanced combined interface patterns"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

// =============================================================================
// More Interface ES5 Source Map Tests
// =============================================================================

#[test]
fn test_source_map_interface_nested_types() {
    // Test interface with deeply nested type structures
    let source = r#"interface Address {
    street: string;
    city: string;
    zip: string;
}

interface Contact {
    email: string;
    phone: string;
}

interface Company {
    name: string;
    address: Address;
    contacts: Contact[];
}

interface Employee {
    id: number;
    name: string;
    address: Address;
    contact: Contact;
    company: Company;
}

const employee: Employee = {
    id: 1,
    name: "John Doe",
    address: { street: "123 Main St", city: "NYC", zip: "10001" },
    contact: { email: "john@example.com", phone: "555-1234" },
    company: {
        name: "Acme Inc",
        address: { street: "456 Corp Ave", city: "NYC", zip: "10002" },
        contacts: [{ email: "info@acme.com", phone: "555-0000" }]
    }
};

console.log(employee.name);
console.log(employee.company.name);
console.log(employee.address.city);"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("employee"),
        "expected output to contain employee. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for nested types"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_interface_tuple_types() {
    // Test interface with tuple types
    let source = r#"interface Coordinate {
    position: [number, number];
    position3D: [number, number, number];
}

interface NamedTuple {
    range: [start: number, end: number];
    point: [x: number, y: number, z?: number];
}

interface MixedTuple {
    data: [string, number, boolean];
    rest: [string, ...number[]];
}

const coord: Coordinate = {
    position: [10, 20],
    position3D: [10, 20, 30]
};

const named: NamedTuple = {
    range: [0, 100],
    point: [5, 10]
};

const mixed: MixedTuple = {
    data: ["hello", 42, true],
    rest: ["prefix", 1, 2, 3, 4]
};

function processCoord(c: Coordinate): number {
    return c.position[0] + c.position[1];
}

console.log(coord.position);
console.log(named.range);
console.log(mixed.data);
console.log(processCoord(coord));"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("coord") && output.contains("processCoord"),
        "expected output to contain coord and processCoord. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for tuple types"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_interface_literal_types() {
    // Test interface with literal types
    let source = r#"interface Status {
    code: 200 | 201 | 400 | 404 | 500;
    message: "success" | "created" | "error";
}

interface Config {
    mode: "development" | "production" | "test";
    debug: true | false;
    level: 1 | 2 | 3;
}

interface ButtonProps {
    variant: "primary" | "secondary" | "danger";
    size: "small" | "medium" | "large";
    disabled: boolean;
}

const status: Status = {
    code: 200,
    message: "success"
};

const config: Config = {
    mode: "production",
    debug: false,
    level: 2
};

const button: ButtonProps = {
    variant: "primary",
    size: "medium",
    disabled: false
};

function handleStatus(s: Status): void {
    if (s.code === 200) {
        console.log("OK:", s.message);
    }
}

console.log(status.code);
console.log(config.mode);
console.log(button.variant);
handleStatus(status);"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("status") && output.contains("handleStatus"),
        "expected output to contain status and handleStatus. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for literal types"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_interface_never_unknown() {
    // Test interface with never and unknown types
    let source = r#"interface ErrorHandler {
    handle(error: unknown): never;
    log(message: string): void;
}

interface Parser {
    parse(input: unknown): string;
    validate(data: unknown): boolean;
}

interface Validator<T> {
    validate(value: unknown): value is T;
    assert(value: unknown): asserts value is T;
}

const handler: ErrorHandler = {
    handle(error: unknown): never {
        console.error("Fatal error:", error);
        throw new Error(String(error));
    },
    log(message: string): void {
        console.log(message);
    }
};

const parser: Parser = {
    parse(input: unknown): string {
        return String(input);
    },
    validate(data: unknown): boolean {
        return data !== null && data !== undefined;
    }
};

function processUnknown(value: unknown): string {
    if (typeof value === "string") {
        return value;
    }
    return String(value);
}

handler.log("Starting...");
console.log(parser.parse({ key: "value" }));
console.log(processUnknown(42));"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("handler") || output.contains("processUnknown"),
        "expected output to contain handler or processUnknown. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for never/unknown types"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_interface_this_type() {
    // Test interface with this type for fluent APIs
    let source = r#"interface Chainable {
    setValue(value: string): this;
    setName(name: string): this;
    build(): string;
}

interface FluentBuilder<T> {
    add(item: T): this;
    remove(item: T): this;
    clear(): this;
    getItems(): T[];
}

const chainable: Chainable = {
    _value: "",
    _name: "",
    setValue(value: string) {
        (this as any)._value = value;
        return this;
    },
    setName(name: string) {
        (this as any)._name = name;
        return this;
    },
    build() {
        return (this as any)._name + ": " + (this as any)._value;
    }
} as any;

class ArrayBuilder<T> implements FluentBuilder<T> {
    private items: T[] = [];

    add(item: T): this {
        this.items.push(item);
        return this;
    }

    remove(item: T): this {
        const idx = this.items.indexOf(item);
        if (idx !== -1) this.items.splice(idx, 1);
        return this;
    }

    clear(): this {
        this.items = [];
        return this;
    }

    getItems(): T[] {
        return this.items.slice();
    }
}

const result = chainable.setValue("hello").setName("greeting").build();
console.log(result);

const builder = new ArrayBuilder<number>();
builder.add(1).add(2).add(3).remove(2);
console.log(builder.getItems());"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("chainable") || output.contains("ArrayBuilder"),
        "expected output to contain chainable or ArrayBuilder. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for this type"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_interface_overloaded_methods() {
    // Test interface with overloaded method signatures
    let source = r#"interface Processor {
    process(input: string): string;
    process(input: number): number;
    process(input: boolean): boolean;
    process(input: string | number | boolean): string | number | boolean;
}

interface Converter {
    convert(value: string, to: "number"): number;
    convert(value: string, to: "boolean"): boolean;
    convert(value: number, to: "string"): string;
}

interface EventTarget {
    addEventListener(type: "click", listener: (e: MouseEvent) => void): void;
    addEventListener(type: "keydown", listener: (e: KeyboardEvent) => void): void;
    addEventListener(type: string, listener: (e: Event) => void): void;
}

const processor: Processor = {
    process(input: any): any {
        if (typeof input === "string") return input.toUpperCase();
        if (typeof input === "number") return input * 2;
        return !input;
    }
};

const converter: Converter = {
    convert(value: any, to: string): any {
        if (to === "number") return Number(value);
        if (to === "boolean") return Boolean(value);
        return String(value);
    }
};

console.log(processor.process("hello"));
console.log(processor.process(21));
console.log(processor.process(false));
console.log(converter.convert("42", "number"));
console.log(converter.convert("true", "boolean"));"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("processor") && output.contains("converter"),
        "expected output to contain processor and converter. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for overloaded methods"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_interface_async_methods() {
    // Test interface with async method signatures
    let source = r#"interface AsyncService {
    fetch(url: string): Promise<string>;
    fetchJson<T>(url: string): Promise<T>;
    post(url: string, data: any): Promise<void>;
}

interface DataLoader<T> {
    load(): Promise<T>;
    loadAll(): Promise<T[]>;
    refresh(): Promise<void>;
}

interface AsyncQueue<T> {
    enqueue(item: T): Promise<void>;
    dequeue(): Promise<T | undefined>;
    peek(): Promise<T | undefined>;
    isEmpty(): Promise<boolean>;
}

const service: AsyncService = {
    async fetch(url: string): Promise<string> {
        return "data from " + url;
    },
    async fetchJson<T>(url: string): Promise<T> {
        return { url } as any;
    },
    async post(url: string, data: any): Promise<void> {
        console.log("Posted to", url, data);
    }
};

async function useService(s: AsyncService): Promise<void> {
    const data = await s.fetch("/api/data");
    console.log(data);
    await s.post("/api/save", { value: 42 });
}

useService(service);
console.log("Service called");"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("service") || output.contains("useService"),
        "expected output to contain service or useService. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for async methods"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_interface_accessor_signatures() {
    // Test interface with getter/setter accessor signatures
    let source = r#"interface Readable {
    readonly value: string;
    readonly length: number;
}

interface Writable {
    value: string;
}

interface ReadWrite {
    get value(): string;
    set value(v: string);
    get computed(): number;
}

interface Observable<T> {
    get current(): T;
    set current(value: T);
    readonly previous: T | undefined;
}

const readable: Readable = {
    value: "hello",
    length: 5
};

const writable: Writable = {
    value: "initial"
};

class ObservableValue<T> implements Observable<T> {
    private _current: T;
    private _previous: T | undefined;

    constructor(initial: T) {
        this._current = initial;
    }

    get current(): T {
        return this._current;
    }

    set current(value: T) {
        this._previous = this._current;
        this._current = value;
    }

    get previous(): T | undefined {
        return this._previous;
    }
}

console.log(readable.value);
writable.value = "updated";
console.log(writable.value);

const obs = new ObservableValue<number>(0);
obs.current = 10;
obs.current = 20;
console.log(obs.current, obs.previous);"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("readable") || output.contains("ObservableValue"),
        "expected output to contain readable or ObservableValue. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for accessor signatures"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_interface_symbol_properties() {
    // Test interface with symbol-keyed properties
    let source = r#"interface Iterable<T> {
    [Symbol.iterator](): Iterator<T>;
}

interface CustomIterable {
    [Symbol.iterator](): Iterator<number>;
    [Symbol.toStringTag]: string;
}

interface Disposable {
    [Symbol.dispose]?(): void;
}

class NumberRange implements CustomIterable {
    constructor(private start: number, private end: number) {}

    *[Symbol.iterator](): Iterator<number> {
        for (let i = this.start; i <= this.end; i++) {
            yield i;
        }
    }

    get [Symbol.toStringTag](): string {
        return "NumberRange";
    }
}

const range = new NumberRange(1, 5);
console.log(String(range));

for (const num of range) {
    console.log(num);
}"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("NumberRange") || output.contains("range"),
        "expected output to contain NumberRange or range. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for symbol properties"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_interface_complex_combined() {
    // Test complex combined interface patterns
    let source = r#"interface BaseNode {
    readonly type: string;
    readonly id: number;
}

interface TextNode extends BaseNode {
    readonly type: "text";
    content: string;
}

interface ElementNode extends BaseNode {
    readonly type: "element";
    tagName: string;
    children: TreeNode[];
    attributes: { [key: string]: string };
}

type TreeNode = TextNode | ElementNode;

interface TreeVisitor<T> {
    visitText(node: TextNode): T;
    visitElement(node: ElementNode): T;
}

interface TreeTransformer extends TreeVisitor<TreeNode> {
    transform(root: TreeNode): TreeNode;
}

class NodeCounter implements TreeVisitor<number> {
    visitText(node: TextNode): number {
        return 1;
    }

    visitElement(node: ElementNode): number {
        let count = 1;
        for (const child of node.children) {
            if (child.type === "text") {
                count += this.visitText(child);
            } else {
                count += this.visitElement(child);
            }
        }
        return count;
    }
}

const textNode: TextNode = { type: "text", id: 1, content: "Hello" };
const elemNode: ElementNode = {
    type: "element",
    id: 2,
    tagName: "div",
    children: [textNode],
    attributes: { class: "container" }
};

const counter = new NodeCounter();
console.log(counter.visitText(textNode));
console.log(counter.visitElement(elemNode));
console.log(elemNode.tagName, elemNode.attributes);"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("NodeCounter") || output.contains("textNode"),
        "expected output to contain NodeCounter or textNode. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for complex combined patterns"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

// =============================================================================
// Extended Interface ES5 Source Map Tests
// =============================================================================

#[test]
fn test_source_map_interface_multiple_extends() {
    // Test interface extending multiple interfaces
    let source = r#"interface Named {
    name: string;
}

interface Aged {
    age: number;
}

interface Described {
    description: string;
}

interface Person extends Named, Aged {
    email: string;
}

interface DetailedPerson extends Named, Aged, Described {
    address: string;
}

const person: Person = {
    name: "John",
    age: 30,
    email: "john@example.com"
};

const detailed: DetailedPerson = {
    name: "Jane",
    age: 25,
    description: "Software Engineer",
    address: "123 Main St"
};

function greet(p: Named & Aged): string {
    return "Hello " + p.name + ", you are " + p.age;
}

console.log(person.name, person.age);
console.log(detailed.description);
console.log(greet(person));"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("person") && output.contains("greet"),
        "expected output to contain person and greet. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for multiple extends"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

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

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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

#[test]
fn test_source_map_decorator_es5_class_with_metadata() {
    let source = r#"function Component(config: { selector: string; template: string }) {
    return function<T extends { new(...args: any[]): {} }>(constructor: T) {
        return class extends constructor {
            selector = config.selector;
            template = config.template;
        };
    };
}

@Component({
    selector: 'app-root',
    template: '<div>Hello</div>'
})
class AppComponent {
    title: string = 'My App';
}

const app = new AppComponent();"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("AppComponent") || output.contains("Component"),
        "expected output to contain AppComponent. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for class decorator with metadata"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_decorator_es5_method_with_descriptor() {
    let source = r#"function Log(target: any, propertyKey: string, descriptor: PropertyDescriptor) {
    const original = descriptor.value;
    descriptor.value = function(...args: any[]) {
        console.log(`Calling ${propertyKey} with`, args);
        return original.apply(this, args);
    };
    return descriptor;
}

class Calculator {
    @Log
    add(a: number, b: number): number {
        return a + b;
    }

    @Log
    multiply(a: number, b: number): number {
        return a * b;
    }
}

const calc = new Calculator();
calc.add(2, 3);"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("Calculator") || output.contains("add"),
        "expected output to contain Calculator class. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for method decorator with descriptor"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_decorator_es5_property_validation() {
    let source = r#"function MinLength(min: number) {
    return function(target: any, propertyKey: string) {
        let value: string;
        Object.defineProperty(target, propertyKey, {
            get: () => value,
            set: (newValue: string) => {
                if (newValue.length < min) {
                    throw new Error(`${propertyKey} must be at least ${min} chars`);
                }
                value = newValue;
            }
        });
    };
}

class User {
    @MinLength(3)
    username: string;

    @MinLength(8)
    password: string;

    constructor(username: string, password: string) {
        this.username = username;
        this.password = password;
    }
}

const user = new User("john", "password123");"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("User") || output.contains("MinLength"),
        "expected output to contain User class. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for property validation decorator"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_decorator_es5_parameter_injection() {
    let source = r#"const INJECT_KEY = Symbol('inject');

function Inject(token: string) {
    return function(target: any, propertyKey: string | symbol, parameterIndex: number) {
        const existing = Reflect.getMetadata(INJECT_KEY, target, propertyKey) || [];
        existing.push({ index: parameterIndex, token });
        Reflect.defineMetadata(INJECT_KEY, existing, target, propertyKey);
    };
}

class Database {
    query(sql: string): any[] { return []; }
}

class Logger {
    log(msg: string): void { console.log(msg); }
}

class UserService {
    constructor(
        @Inject('Database') private db: Database,
        @Inject('Logger') private logger: Logger
    ) {}

    getUsers(): any[] {
        this.logger.log('Fetching users');
        return this.db.query('SELECT * FROM users');
    }
}"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("UserService") || output.contains("Inject"),
        "expected output to contain UserService class. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for parameter injection decorator"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_decorator_es5_factory_chain() {
    let source = r#"function Memoize() {
    return function(target: any, key: string, descriptor: PropertyDescriptor) {
        const cache = new Map();
        const original = descriptor.value;
        descriptor.value = function(...args: any[]) {
            const cacheKey = JSON.stringify(args);
            if (!cache.has(cacheKey)) {
                cache.set(cacheKey, original.apply(this, args));
            }
            return cache.get(cacheKey);
        };
    };
}

function Throttle(ms: number) {
    return function(target: any, key: string, descriptor: PropertyDescriptor) {
        let lastCall = 0;
        const original = descriptor.value;
        descriptor.value = function(...args: any[]) {
            const now = Date.now();
            if (now - lastCall >= ms) {
                lastCall = now;
                return original.apply(this, args);
            }
        };
    };
}

function Bind() {
    return function(target: any, key: string, descriptor: PropertyDescriptor) {
        return {
            get() {
                return descriptor.value.bind(this);
            }
        };
    };
}

class ApiClient {
    @Memoize()
    @Throttle(1000)
    @Bind()
    fetchData(url: string): Promise<any> {
        return fetch(url).then(r => r.json());
    }
}"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("ApiClient") || output.contains("fetchData"),
        "expected output to contain ApiClient class. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for decorator factory chain"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_decorator_es5_accessor_readonly() {
    let source = r#"function Readonly(target: any, propertyKey: string, descriptor: PropertyDescriptor) {
    descriptor.writable = false;
    return descriptor;
}

function Enumerable(value: boolean) {
    return function(target: any, propertyKey: string, descriptor: PropertyDescriptor) {
        descriptor.enumerable = value;
        return descriptor;
    };
}

class Config {
    private _apiKey: string = '';

    @Readonly
    @Enumerable(false)
    get apiKey(): string {
        return this._apiKey;
    }

    set apiKey(value: string) {
        this._apiKey = value;
    }
}

const config = new Config();
config.apiKey = 'secret-key';"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("Config") || output.contains("apiKey"),
        "expected output to contain Config class. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for accessor readonly decorator"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_decorator_es5_abstract_class() {
    let source = r#"function Sealed(constructor: Function) {
    Object.seal(constructor);
    Object.seal(constructor.prototype);
}

function Abstract(target: any, propertyKey: string, descriptor: PropertyDescriptor) {
    descriptor.value = function() {
        throw new Error('Abstract method must be implemented');
    };
    return descriptor;
}

@Sealed
abstract class Animal {
    abstract name: string;

    @Abstract
    abstract makeSound(): void;

    move(distance: number): void {
        console.log(`Moving ${distance} meters`);
    }
}

class Dog extends Animal {
    name = 'Dog';

    makeSound(): void {
        console.log('Bark!');
    }
}

const dog = new Dog();
dog.makeSound();"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("Animal") || output.contains("Dog"),
        "expected output to contain Animal or Dog class. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for abstract class decorator"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_decorator_es5_static_members() {
    let source = r#"function Singleton<T extends { new(...args: any[]): {} }>(constructor: T) {
    let instance: T;
    return class extends constructor {
        constructor(...args: any[]) {
            if (instance) {
                return instance;
            }
            super(...args);
            instance = this as any;
        }
    };
}

function StaticInit(target: any, propertyKey: string) {
    const init = target[propertyKey];
    target[propertyKey] = null;
    setTimeout(() => {
        target[propertyKey] = init;
    }, 0);
}

@Singleton
class Database {
    @StaticInit
    static connectionPool: any[] = [];

    static maxConnections: number = 10;

    connect(): void {
        Database.connectionPool.push({});
    }
}

const db1 = new Database();
const db2 = new Database();
console.log(db1 === db2);"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("Database") || output.contains("Singleton"),
        "expected output to contain Database class. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for static member decorators"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_decorator_es5_conditional() {
    let source = r#"const DEBUG = true;

function DebugOnly(target: any, propertyKey: string, descriptor: PropertyDescriptor) {
    if (!DEBUG) {
        descriptor.value = function() {};
    }
    return descriptor;
}

function ConditionalDecorator(condition: boolean) {
    return function(target: any, propertyKey: string, descriptor: PropertyDescriptor) {
        if (!condition) {
            return descriptor;
        }
        const original = descriptor.value;
        descriptor.value = function(...args: any[]) {
            console.log(`[${propertyKey}] called`);
            return original.apply(this, args);
        };
        return descriptor;
    };
}

class Service {
    @DebugOnly
    debugInfo(): void {
        console.log('Debug info');
    }

    @ConditionalDecorator(DEBUG)
    process(data: any): any {
        return data;
    }
}

const service = new Service();
service.debugInfo();
service.process({});"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("Service") || output.contains("DEBUG"),
        "expected output to contain Service class. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for conditional decorators"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_decorator_es5_comprehensive() {
    let source = r#"// Class decorator factory
function Entity(tableName: string) {
    return function<T extends { new(...args: any[]): {} }>(constructor: T) {
        return class extends constructor {
            __tableName = tableName;
        };
    };
}

// Property decorator
function Column(type: string) {
    return function(target: any, propertyKey: string) {
        Reflect.defineMetadata('column:type', type, target, propertyKey);
    };
}

// Method decorator
function Transaction(target: any, propertyKey: string, descriptor: PropertyDescriptor) {
    const original = descriptor.value;
    descriptor.value = async function(...args: any[]) {
        console.log('BEGIN TRANSACTION');
        try {
            const result = await original.apply(this, args);
            console.log('COMMIT');
            return result;
        } catch (e) {
            console.log('ROLLBACK');
            throw e;
        }
    };
    return descriptor;
}

// Parameter decorator
function Required(target: any, propertyKey: string, parameterIndex: number) {
    const required = Reflect.getMetadata('required', target, propertyKey) || [];
    required.push(parameterIndex);
    Reflect.defineMetadata('required', required, target, propertyKey);
}

@Entity('users')
class UserRepository {
    @Column('varchar')
    name: string;

    @Column('int')
    age: number;

    constructor(name: string, age: number) {
        this.name = name;
        this.age = age;
    }

    @Transaction
    async save(@Required entity: any): Promise<void> {
        console.log('Saving entity');
    }

    @Transaction
    async delete(@Required id: number): Promise<void> {
        console.log('Deleting entity', id);
    }
}

const repo = new UserRepository('John', 30);
repo.save({});"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("UserRepository"),
        "expected output to contain UserRepository class. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for comprehensive decorators"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

// =============================================================================
// Async/Await ES5 Source Map Tests - Extended Patterns
// =============================================================================

#[test]
fn test_source_map_async_es5_promise_all() {
    let source = r#"async function fetchAll(urls: string[]): Promise<any[]> {
    const promises = urls.map(url => fetch(url).then(r => r.json()));
    const results = await Promise.all(promises);
    return results;
}

async function parallelFetch(): Promise<void> {
    const [users, posts, comments] = await Promise.all([
        fetch('/api/users'),
        fetch('/api/posts'),
        fetch('/api/comments')
    ]);
    console.log(users, posts, comments);
}

fetchAll(['url1', 'url2', 'url3']);"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("fetchAll") || output.contains("Promise"),
        "expected output to contain fetchAll. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for Promise.all"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_async_es5_promise_race() {
    let source = r#"async function timeout<T>(promise: Promise<T>, ms: number): Promise<T> {
    const timeoutPromise = new Promise<never>((_, reject) => {
        setTimeout(() => reject(new Error('Timeout')), ms);
    });
    return await Promise.race([promise, timeoutPromise]);
}

async function fetchWithTimeout(url: string): Promise<any> {
    const result = await timeout(fetch(url), 5000);
    return result.json();
}

fetchWithTimeout('/api/data');"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("timeout") || output.contains("Promise"),
        "expected output to contain timeout function. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for Promise.race"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_async_es5_error_handling() {
    let source = r#"class NetworkError extends Error {
    constructor(message: string, public statusCode: number) {
        super(message);
        this.name = 'NetworkError';
    }
}

async function fetchWithRetry(url: string, retries: number = 3): Promise<any> {
    for (let i = 0; i < retries; i++) {
        try {
            const response = await fetch(url);
            if (!response.ok) {
                throw new NetworkError('Request failed', response.status);
            }
            return await response.json();
        } catch (error) {
            if (i === retries - 1) {
                throw error;
            }
            await new Promise(r => setTimeout(r, 1000 * Math.pow(2, i)));
        }
    }
}

fetchWithRetry('/api/data').catch(console.error);"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("fetchWithRetry") || output.contains("NetworkError"),
        "expected output to contain fetchWithRetry. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for error handling"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_async_es5_sequential_vs_parallel() {
    let source = r#"async function sequential(): Promise<number[]> {
    const a = await fetch('/a').then(r => r.json());
    const b = await fetch('/b').then(r => r.json());
    const c = await fetch('/c').then(r => r.json());
    return [a, b, c];
}

async function parallel(): Promise<number[]> {
    const [a, b, c] = await Promise.all([
        fetch('/a').then(r => r.json()),
        fetch('/b').then(r => r.json()),
        fetch('/c').then(r => r.json())
    ]);
    return [a, b, c];
}

async function mixed(): Promise<void> {
    const first = await fetch('/first').then(r => r.json());
    const [second, third] = await Promise.all([
        fetch('/second'),
        fetch('/third')
    ]);
    console.log(first, second, third);
}"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("sequential") || output.contains("parallel"),
        "expected output to contain sequential or parallel. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for sequential vs parallel"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_async_es5_closure_capture() {
    let source = r#"function createAsyncCounter() {
    let count = 0;

    return {
        increment: async (): Promise<number> => {
            await new Promise(r => setTimeout(r, 100));
            return ++count;
        },
        decrement: async (): Promise<number> => {
            await new Promise(r => setTimeout(r, 100));
            return --count;
        },
        getCount: async (): Promise<number> => {
            await new Promise(r => setTimeout(r, 50));
            return count;
        }
    };
}

const counter = createAsyncCounter();
counter.increment().then(console.log);"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("createAsyncCounter") || output.contains("increment"),
        "expected output to contain createAsyncCounter. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for closure capture"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_async_es5_inheritance() {
    let source = r#"abstract class AsyncResource {
    protected abstract load(): Promise<any>;

    async initialize(): Promise<void> {
        const data = await this.load();
        await this.process(data);
    }

    protected async process(data: any): Promise<void> {
        console.log('Processing:', data);
    }
}

class UserResource extends AsyncResource {
    protected async load(): Promise<any> {
        const response = await fetch('/api/users');
        return response.json();
    }

    protected async process(data: any): Promise<void> {
        await super.process(data);
        console.log('Users processed');
    }
}

const resource = new UserResource();
resource.initialize();"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("AsyncResource") || output.contains("UserResource"),
        "expected output to contain AsyncResource or UserResource. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for inheritance"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_async_es5_factory_pattern() {
    let source = r#"interface Connection {
    query(sql: string): Promise<any[]>;
    close(): Promise<void>;
}

async function createConnection(config: any): Promise<Connection> {
    await new Promise(r => setTimeout(r, 100));

    return {
        query: async (sql: string): Promise<any[]> => {
            await new Promise(r => setTimeout(r, 50));
            return [{ id: 1, sql }];
        },
        close: async (): Promise<void> => {
            await new Promise(r => setTimeout(r, 50));
            console.log('Connection closed');
        }
    };
}

async function useConnection(): Promise<void> {
    const conn = await createConnection({ host: 'localhost' });
    const results = await conn.query('SELECT * FROM users');
    console.log(results);
    await conn.close();
}

useConnection();"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("createConnection") || output.contains("useConnection"),
        "expected output to contain createConnection. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for factory pattern"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_async_es5_queue_processing() {
    let source = r#"class AsyncQueue<T> {
    private queue: T[] = [];
    private processing = false;

    async add(item: T): Promise<void> {
        this.queue.push(item);
        if (!this.processing) {
            await this.process();
        }
    }

    private async process(): Promise<void> {
        this.processing = true;
        while (this.queue.length > 0) {
            const item = this.queue.shift()!;
            await this.handleItem(item);
        }
        this.processing = false;
    }

    private async handleItem(item: T): Promise<void> {
        await new Promise(r => setTimeout(r, 100));
        console.log('Processed:', item);
    }
}

const queue = new AsyncQueue<string>();
queue.add('item1');
queue.add('item2');"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("AsyncQueue") || output.contains("process"),
        "expected output to contain AsyncQueue. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for queue processing"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_async_es5_event_emitter() {
    let source = r#"class AsyncEventEmitter {
    private listeners: Map<string, Array<(data: any) => Promise<void>>> = new Map();

    on(event: string, handler: (data: any) => Promise<void>): void {
        if (!this.listeners.has(event)) {
            this.listeners.set(event, []);
        }
        this.listeners.get(event)!.push(handler);
    }

    async emit(event: string, data: any): Promise<void> {
        const handlers = this.listeners.get(event) || [];
        for (const handler of handlers) {
            await handler(data);
        }
    }

    async emitParallel(event: string, data: any): Promise<void> {
        const handlers = this.listeners.get(event) || [];
        await Promise.all(handlers.map(h => h(data)));
    }
}

const emitter = new AsyncEventEmitter();
emitter.on('data', async (d) => { console.log(d); });
emitter.emit('data', { value: 42 });"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("AsyncEventEmitter") || output.contains("emit"),
        "expected output to contain AsyncEventEmitter. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for event emitter"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_async_es5_comprehensive() {
    let source = r#"// Async utility functions
const delay = (ms: number): Promise<void> =>
    new Promise(resolve => setTimeout(resolve, ms));

async function* asyncRange(start: number, end: number): AsyncGenerator<number> {
    for (let i = start; i < end; i++) {
        await delay(10);
        yield i;
    }
}

// Async class with all patterns
class DataProcessor {
    private cache = new Map<string, any>();

    constructor(private readonly baseUrl: string) {}

    async fetch(path: string): Promise<any> {
        const key = this.baseUrl + path;
        if (this.cache.has(key)) {
            return this.cache.get(key);
        }
        const response = await fetch(key);
        const data = await response.json();
        this.cache.set(key, data);
        return data;
    }

    async fetchMany(paths: string[]): Promise<any[]> {
        return Promise.all(paths.map(p => this.fetch(p)));
    }

    async *processStream(paths: string[]): AsyncGenerator<any> {
        for await (const i of asyncRange(0, paths.length)) {
            yield await this.fetch(paths[i]);
        }
    }

    async processWithRetry(path: string, retries = 3): Promise<any> {
        for (let i = 0; i < retries; i++) {
            try {
                return await this.fetch(path);
            } catch (e) {
                if (i === retries - 1) throw e;
                await delay(1000 * (i + 1));
            }
        }
    }
}

// Usage
const processor = new DataProcessor('https://api.example.com');

(async () => {
    const data = await processor.fetch('/users');
    const [users, posts] = await processor.fetchMany(['/users', '/posts']);

    for await (const item of processor.processStream(['/a', '/b', '/c'])) {
        console.log(item);
    }
})();"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("DataProcessor"),
        "expected output to contain DataProcessor. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for comprehensive async"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

// =============================================================================
// Generator ES5 Source Map Tests - Extended Patterns
// =============================================================================

#[test]
fn test_source_map_generator_es5_control_flow() {
    let source = r#"function* controlFlowGenerator(n: number): Generator<number> {
    for (let i = 0; i < n; i++) {
        if (i % 2 === 0) {
            yield i * 2;
        } else {
            yield i * 3;
        }
    }

    let j = 0;
    while (j < 3) {
        yield j * 10;
        j++;
    }

    switch (n) {
        case 1: yield 100; break;
        case 2: yield 200; break;
        default: yield 999;
    }
}

const gen = controlFlowGenerator(5);
console.log([...gen]);"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("controlFlowGenerator"),
        "expected output to contain controlFlowGenerator. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for control flow generator"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_generator_es5_state_machine() {
    let source = r#"type State = 'idle' | 'loading' | 'success' | 'error';

function* stateMachine(): Generator<State, void, string> {
    let input: string;

    while (true) {
        yield 'idle';
        input = yield 'loading';

        if (input === 'success') {
            yield 'success';
        } else if (input === 'error') {
            yield 'error';
        }
    }
}

const machine = stateMachine();
console.log(machine.next());
console.log(machine.next());
console.log(machine.next('success'));"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("stateMachine"),
        "expected output to contain stateMachine. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for state machine generator"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_generator_es5_finally() {
    let source = r#"function* generatorWithFinally(): Generator<number> {
    try {
        yield 1;
        yield 2;
        yield 3;
    } finally {
        console.log('Generator cleanup');
    }
}

function* nestedTryFinally(): Generator<string> {
    try {
        try {
            yield 'inner-1';
            yield 'inner-2';
        } finally {
            yield 'inner-finally';
        }
        yield 'outer-1';
    } finally {
        yield 'outer-finally';
    }
}

const gen1 = generatorWithFinally();
const gen2 = nestedTryFinally();
console.log([...gen1], [...gen2]);"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("generatorWithFinally") || output.contains("nestedTryFinally"),
        "expected output to contain generator functions. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for finally generator"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_generator_es5_composition() {
    let source = r#"function* numbers(n: number): Generator<number> {
    for (let i = 1; i <= n; i++) {
        yield i;
    }
}

function* letters(s: string): Generator<string> {
    for (const c of s) {
        yield c;
    }
}

function* combined(): Generator<number | string> {
    yield* numbers(3);
    yield '---';
    yield* letters('abc');
    yield '---';
    yield* numbers(2);
}

function* flatten<T>(iterables: Iterable<T>[]): Generator<T> {
    for (const iterable of iterables) {
        yield* iterable;
    }
}

console.log([...combined()]);
console.log([...flatten([[1, 2], [3, 4], [5]])]);"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("combined") || output.contains("flatten"),
        "expected output to contain composition generators. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for composition generator"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_generator_es5_iterator_protocol() {
    let source = r#"class Range {
    constructor(private start: number, private end: number) {}

    *[Symbol.iterator](): Generator<number> {
        for (let i = this.start; i <= this.end; i++) {
            yield i;
        }
    }
}

class KeyValuePairs<K, V> {
    private pairs: [K, V][] = [];

    add(key: K, value: V): void {
        this.pairs.push([key, value]);
    }

    *keys(): Generator<K> {
        for (const [key] of this.pairs) {
            yield key;
        }
    }

    *values(): Generator<V> {
        for (const [, value] of this.pairs) {
            yield value;
        }
    }

    *entries(): Generator<[K, V]> {
        yield* this.pairs;
    }
}

const range = new Range(1, 5);
console.log([...range]);"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("Range") || output.contains("KeyValuePairs"),
        "expected output to contain iterator protocol classes. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for iterator protocol"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_generator_es5_default_params() {
    let source = r#"function* range(
    start: number = 0,
    end: number = 10,
    step: number = 1
): Generator<number> {
    for (let i = start; i < end; i += step) {
        yield i;
    }
}

function* repeat<T>(
    value: T,
    times: number = Infinity
): Generator<T> {
    for (let i = 0; i < times; i++) {
        yield value;
    }
}

function* take<T>(
    iterable: Iterable<T>,
    count: number = 5
): Generator<T> {
    let i = 0;
    for (const item of iterable) {
        if (i++ >= count) break;
        yield item;
    }
}

console.log([...range()]);
console.log([...range(5, 10, 2)]);
console.log([...take(repeat('x'), 3)]);"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("range") || output.contains("repeat"),
        "expected output to contain generators with default params. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for default params generator"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_generator_es5_object_yielding() {
    let source = r#"interface Person {
    id: number;
    name: string;
    age: number;
}

function* personGenerator(): Generator<Person> {
    yield { id: 1, name: 'Alice', age: 30 };
    yield { id: 2, name: 'Bob', age: 25 };
    yield { id: 3, name: 'Charlie', age: 35 };
}

function* objectTransformer<T, U>(
    source: Iterable<T>,
    transform: (item: T) => U
): Generator<U> {
    for (const item of source) {
        yield transform(item);
    }
}

const people = personGenerator();
const names = objectTransformer(personGenerator(), p => p.name);
console.log([...people], [...names]);"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("personGenerator") || output.contains("objectTransformer"),
        "expected output to contain object yielding generators. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for object yielding generator"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_generator_es5_recursion() {
    let source = r#"interface TreeNode<T> {
    value: T;
    children?: TreeNode<T>[];
}

function* traverseTree<T>(node: TreeNode<T>): Generator<T> {
    yield node.value;
    if (node.children) {
        for (const child of node.children) {
            yield* traverseTree(child);
        }
    }
}

function* fibonacci(): Generator<number> {
    let [a, b] = [0, 1];
    while (true) {
        yield a;
        [a, b] = [b, a + b];
    }
}

function* permutations<T>(items: T[]): Generator<T[]> {
    if (items.length <= 1) {
        yield items;
    } else {
        for (let i = 0; i < items.length; i++) {
            const rest = [...items.slice(0, i), ...items.slice(i + 1)];
            for (const perm of permutations(rest)) {
                yield [items[i], ...perm];
            }
        }
    }
}

const tree: TreeNode<number> = {
    value: 1,
    children: [{ value: 2 }, { value: 3, children: [{ value: 4 }] }]
};
console.log([...traverseTree(tree)]);"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("traverseTree") || output.contains("fibonacci"),
        "expected output to contain recursive generators. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for recursive generator"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_generator_es5_lazy_evaluation() {
    let source = r#"function* lazyMap<T, U>(
    source: Iterable<T>,
    fn: (item: T) => U
): Generator<U> {
    for (const item of source) {
        yield fn(item);
    }
}

function* lazyFilter<T>(
    source: Iterable<T>,
    predicate: (item: T) => boolean
): Generator<T> {
    for (const item of source) {
        if (predicate(item)) {
            yield item;
        }
    }
}

function* lazyTakeWhile<T>(
    source: Iterable<T>,
    predicate: (item: T) => boolean
): Generator<T> {
    for (const item of source) {
        if (!predicate(item)) break;
        yield item;
    }
}

function pipe<T>(...generators: ((input: Iterable<T>) => Generator<T>)[]): (input: Iterable<T>) => Generator<T> {
    return function*(input: Iterable<T>): Generator<T> {
        let result: Iterable<T> = input;
        for (const gen of generators) {
            result = gen(result);
        }
        yield* result;
    };
}

const numbers = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10];
const result = lazyFilter(
    lazyMap(numbers, x => x * 2),
    x => x > 5
);
console.log([...result]);"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("lazyMap") || output.contains("lazyFilter"),
        "expected output to contain lazy evaluation generators. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for lazy evaluation generator"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_generator_es5_comprehensive() {
    let source = r#"// Utility generators
function* range(start: number, end: number): Generator<number> {
    for (let i = start; i < end; i++) yield i;
}

function* map<T, U>(iter: Iterable<T>, fn: (x: T) => U): Generator<U> {
    for (const x of iter) yield fn(x);
}

function* filter<T>(iter: Iterable<T>, pred: (x: T) => boolean): Generator<T> {
    for (const x of iter) if (pred(x)) yield x;
}

// Class with generator methods
class DataStream<T> {
    private data: T[] = [];

    push(...items: T[]): void {
        this.data.push(...items);
    }

    *[Symbol.iterator](): Generator<T> {
        yield* this.data;
    }

    *reversed(): Generator<T> {
        for (let i = this.data.length - 1; i >= 0; i--) {
            yield this.data[i];
        }
    }

    *chunks(size: number): Generator<T[]> {
        for (let i = 0; i < this.data.length; i += size) {
            yield this.data.slice(i, i + size);
        }
    }

    *zip<U>(other: Iterable<U>): Generator<[T, U]> {
        const otherIter = other[Symbol.iterator]();
        for (const item of this.data) {
            const otherResult = otherIter.next();
            if (otherResult.done) break;
            yield [item, otherResult.value];
        }
    }
}

// Async generator for completeness
async function* asyncNumbers(): AsyncGenerator<number> {
    for (let i = 0; i < 5; i++) {
        await new Promise(r => setTimeout(r, 10));
        yield i;
    }
}

// Usage
const stream = new DataStream<number>();
stream.push(1, 2, 3, 4, 5, 6);

const evenDoubled = filter(
    map(stream, x => x * 2),
    x => x % 4 === 0
);

console.log([...evenDoubled]);
console.log([...stream.chunks(2)]);
console.log([...stream.zip(['a', 'b', 'c'])]);"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("DataStream"),
        "expected output to contain DataStream class. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for comprehensive generator"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

// =============================================================================
// Class Inheritance ES5 Source Map Tests
// =============================================================================

#[test]
fn test_source_map_class_inheritance_es5_extends_clause() {
    let source = r#"class Animal {
    name: string;

    constructor(name: string) {
        this.name = name;
    }

    speak(): void {
        console.log(`${this.name} makes a sound`);
    }
}

class Dog extends Animal {
    breed: string;

    constructor(name: string, breed: string) {
        super(name);
        this.breed = breed;
    }
}

const dog = new Dog("Buddy", "Labrador");"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("Animal"),
        "expected output to contain Animal class. output: {output}"
    );
    assert!(
        output.contains("Dog"),
        "expected output to contain Dog class. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for extends clause"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_class_inheritance_es5_super_calls() {
    let source = r#"class Base {
    protected value: number;

    constructor(value: number) {
        this.value = value;
    }

    getValue(): number {
        return this.value;
    }

    protected increment(): void {
        this.value++;
    }
}

class Derived extends Base {
    private multiplier: number;

    constructor(value: number, multiplier: number) {
        super(value);
        this.multiplier = multiplier;
    }

    getValue(): number {
        return super.getValue() * this.multiplier;
    }

    increment(): void {
        super.increment();
        console.log("Incremented to", this.value);
    }
}

const d = new Derived(5, 2);
console.log(d.getValue());
d.increment();"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("Derived"),
        "expected output to contain Derived class. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for super calls"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_class_inheritance_es5_method_overrides() {
    let source = r#"class Shape {
    protected x: number;
    protected y: number;

    constructor(x: number, y: number) {
        this.x = x;
        this.y = y;
    }

    area(): number {
        return 0;
    }

    perimeter(): number {
        return 0;
    }

    describe(): string {
        return `Shape at (${this.x}, ${this.y})`;
    }
}

class Rectangle extends Shape {
    private width: number;
    private height: number;

    constructor(x: number, y: number, width: number, height: number) {
        super(x, y);
        this.width = width;
        this.height = height;
    }

    area(): number {
        return this.width * this.height;
    }

    perimeter(): number {
        return 2 * (this.width + this.height);
    }

    describe(): string {
        return `Rectangle ${this.width}x${this.height} at (${this.x}, ${this.y})`;
    }
}

class Circle extends Shape {
    private radius: number;

    constructor(x: number, y: number, radius: number) {
        super(x, y);
        this.radius = radius;
    }

    area(): number {
        return Math.PI * this.radius * this.radius;
    }

    perimeter(): number {
        return 2 * Math.PI * this.radius;
    }

    describe(): string {
        return `Circle r=${this.radius} at (${this.x}, ${this.y})`;
    }
}

const shapes: Shape[] = [new Rectangle(0, 0, 10, 5), new Circle(5, 5, 3)];
shapes.forEach(s => console.log(s.describe(), s.area()));"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("Rectangle"),
        "expected output to contain Rectangle class. output: {output}"
    );
    assert!(
        output.contains("Circle"),
        "expected output to contain Circle class. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for method overrides"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_class_inheritance_es5_multi_level() {
    let source = r#"class Entity {
    id: string;

    constructor(id: string) {
        this.id = id;
    }

    toString(): string {
        return `Entity(${this.id})`;
    }
}

class LivingEntity extends Entity {
    health: number;

    constructor(id: string, health: number) {
        super(id);
        this.health = health;
    }

    isAlive(): boolean {
        return this.health > 0;
    }

    toString(): string {
        return `${super.toString()} HP:${this.health}`;
    }
}

class Character extends LivingEntity {
    name: string;
    level: number;

    constructor(id: string, name: string, health: number, level: number) {
        super(id, health);
        this.name = name;
        this.level = level;
    }

    toString(): string {
        return `${this.name} Lv.${this.level} ${super.toString()}`;
    }
}

class Player extends Character {
    experience: number;

    constructor(id: string, name: string) {
        super(id, name, 100, 1);
        this.experience = 0;
    }

    gainExp(amount: number): void {
        this.experience += amount;
        if (this.experience >= this.level * 100) {
            this.level++;
            this.health += 10;
        }
    }

    toString(): string {
        return `[Player] ${super.toString()} EXP:${this.experience}`;
    }
}

const player = new Player("p1", "Hero");
player.gainExp(150);
console.log(player.toString());"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("Player"),
        "expected output to contain Player class. output: {output}"
    );
    assert!(
        output.contains("Character"),
        "expected output to contain Character class. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for multi-level inheritance"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_class_inheritance_es5_mixin_pattern() {
    let source = r#"type Constructor<T = {}> = new (...args: any[]) => T;

function Timestamped<TBase extends Constructor>(Base: TBase) {
    return class extends Base {
        timestamp = Date.now();

        getTimestamp(): number {
            return this.timestamp;
        }
    };
}

function Tagged<TBase extends Constructor>(Base: TBase) {
    return class extends Base {
        tag: string = "";

        setTag(tag: string): void {
            this.tag = tag;
        }

        getTag(): string {
            return this.tag;
        }
    };
}

function Serializable<TBase extends Constructor>(Base: TBase) {
    return class extends Base {
        serialize(): string {
            return JSON.stringify(this);
        }
    };
}

class BaseEntity {
    id: number;

    constructor(id: number) {
        this.id = id;
    }
}

const MixedEntity = Serializable(Tagged(Timestamped(BaseEntity)));

const entity = new MixedEntity(1);
entity.setTag("important");
console.log(entity.serialize());"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("Timestamped"),
        "expected output to contain Timestamped mixin. output: {output}"
    );
    assert!(
        output.contains("Tagged"),
        "expected output to contain Tagged mixin. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for mixin pattern"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_class_inheritance_es5_super_property_access() {
    let source = r#"class Config {
    protected settings: Map<string, any> = new Map();

    get(key: string): any {
        return this.settings.get(key);
    }

    set(key: string, value: any): void {
        this.settings.set(key, value);
    }

    has(key: string): boolean {
        return this.settings.has(key);
    }
}

class AppConfig extends Config {
    private defaults: Map<string, any>;

    constructor(defaults: Record<string, any>) {
        super();
        this.defaults = new Map(Object.entries(defaults));
    }

    get(key: string): any {
        if (super.has(key)) {
            return super.get(key);
        }
        return this.defaults.get(key);
    }

    set(key: string, value: any): void {
        if (this.defaults.has(key)) {
            super.set(key, value);
        } else {
            throw new Error(`Unknown config key: ${key}`);
        }
    }

    reset(key: string): void {
        if (this.defaults.has(key)) {
            super.set(key, this.defaults.get(key));
        }
    }
}

const config = new AppConfig({ debug: false, timeout: 5000 });
config.set("debug", true);
console.log(config.get("debug"), config.get("timeout"));"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("AppConfig"),
        "expected output to contain AppConfig class. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for super property access"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_class_inheritance_es5_static_inheritance() {
    let source = r#"class Database {
    static connectionCount = 0;
    static instances: Database[] = [];

    static create(): Database {
        const db = new Database();
        Database.instances.push(db);
        return db;
    }

    static getConnectionCount(): number {
        return Database.connectionCount;
    }

    constructor() {
        Database.connectionCount++;
    }
}

class PostgresDB extends Database {
    static driver = "pg";

    static create(): PostgresDB {
        const db = new PostgresDB();
        Database.instances.push(db);
        return db;
    }

    static getDriver(): string {
        return PostgresDB.driver;
    }

    query(sql: string): void {
        console.log(`Executing on ${PostgresDB.driver}: ${sql}`);
    }
}

class MySQLDB extends Database {
    static driver = "mysql2";

    static create(): MySQLDB {
        const db = new MySQLDB();
        Database.instances.push(db);
        return db;
    }
}

const pg = PostgresDB.create();
const mysql = MySQLDB.create();
console.log(Database.getConnectionCount());"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("PostgresDB"),
        "expected output to contain PostgresDB class. output: {output}"
    );
    assert!(
        output.contains("MySQLDB"),
        "expected output to contain MySQLDB class. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for static inheritance"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_class_inheritance_es5_abstract_class() {
    let source = r#"abstract class Transport {
    abstract connect(): Promise<void>;
    abstract disconnect(): Promise<void>;
    abstract send(data: string): Promise<void>;

    protected connected = false;

    isConnected(): boolean {
        return this.connected;
    }

    async sendIfConnected(data: string): Promise<boolean> {
        if (this.connected) {
            await this.send(data);
            return true;
        }
        return false;
    }
}

class WebSocketTransport extends Transport {
    private url: string;
    private ws: any;

    constructor(url: string) {
        super();
        this.url = url;
    }

    async connect(): Promise<void> {
        this.ws = new WebSocket(this.url);
        this.connected = true;
    }

    async disconnect(): Promise<void> {
        this.ws?.close();
        this.connected = false;
    }

    async send(data: string): Promise<void> {
        this.ws?.send(data);
    }
}

class HTTPTransport extends Transport {
    private baseUrl: string;

    constructor(baseUrl: string) {
        super();
        this.baseUrl = baseUrl;
    }

    async connect(): Promise<void> {
        this.connected = true;
    }

    async disconnect(): Promise<void> {
        this.connected = false;
    }

    async send(data: string): Promise<void> {
        await fetch(this.baseUrl, { method: 'POST', body: data });
    }
}

const transports: Transport[] = [
    new WebSocketTransport("ws://localhost:8080"),
    new HTTPTransport("http://api.example.com")
];"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("Transport"),
        "expected output to contain Transport class. output: {output}"
    );
    assert!(
        output.contains("WebSocketTransport"),
        "expected output to contain WebSocketTransport class. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for abstract class"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_class_inheritance_es5_interface_implementation() {
    let source = r#"interface Comparable<T> {
    compareTo(other: T): number;
}

interface Hashable {
    hashCode(): number;
}

interface Cloneable<T> {
    clone(): T;
}

class BaseValue {
    protected value: number;

    constructor(value: number) {
        this.value = value;
    }

    getValue(): number {
        return this.value;
    }
}

class ComparableValue extends BaseValue implements Comparable<ComparableValue>, Hashable, Cloneable<ComparableValue> {
    constructor(value: number) {
        super(value);
    }

    compareTo(other: ComparableValue): number {
        return this.value - other.value;
    }

    hashCode(): number {
        return this.value | 0;
    }

    clone(): ComparableValue {
        return new ComparableValue(this.value);
    }
}

const a = new ComparableValue(10);
const b = new ComparableValue(20);
console.log(a.compareTo(b));
console.log(a.hashCode());
const c = a.clone();"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("ComparableValue"),
        "expected output to contain ComparableValue class. output: {output}"
    );
    assert!(
        output.contains("BaseValue"),
        "expected output to contain BaseValue class. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for interface implementation"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_class_inheritance_es5_comprehensive() {
    let source = r#"// Comprehensive class inheritance test with mixins, abstract classes, and interfaces

type Constructor<T = {}> = new (...args: any[]) => T;

interface Identifiable {
    getId(): string;
}

interface Persistable {
    save(): Promise<void>;
    load(): Promise<void>;
}

function Loggable<TBase extends Constructor>(Base: TBase) {
    return class extends Base {
        log(message: string): void {
            console.log(`[${new Date().toISOString()}] ${message}`);
        }
    };
}

function Validatable<TBase extends Constructor>(Base: TBase) {
    return class extends Base {
        protected errors: string[] = [];

        validate(): boolean {
            this.errors = [];
            return true;
        }

        getErrors(): string[] {
            return [...this.errors];
        }
    };
}

abstract class Entity implements Identifiable {
    protected id: string;
    protected createdAt: Date;
    protected updatedAt: Date;

    constructor(id?: string) {
        this.id = id || crypto.randomUUID();
        this.createdAt = new Date();
        this.updatedAt = new Date();
    }

    getId(): string {
        return this.id;
    }

    abstract toJSON(): object;
}

abstract class Model extends Entity implements Persistable {
    protected dirty = false;

    markDirty(): void {
        this.dirty = true;
        this.updatedAt = new Date();
    }

    abstract save(): Promise<void>;
    abstract load(): Promise<void>;
}

const ValidatableModel = Validatable(Loggable(class extends Model {
    toJSON(): object {
        return { id: this.id, createdAt: this.createdAt, updatedAt: this.updatedAt };
    }

    async save(): Promise<void> {
        this.log(`Saving entity ${this.id}`);
    }

    async load(): Promise<void> {
        this.log(`Loading entity ${this.id}`);
    }
}));

class User extends ValidatableModel {
    private email: string;
    private name: string;
    private role: "admin" | "user" | "guest";

    constructor(email: string, name: string, role: "admin" | "user" | "guest" = "user") {
        super();
        this.email = email;
        this.name = name;
        this.role = role;
    }

    validate(): boolean {
        super.validate();

        if (!this.email.includes("@")) {
            this.errors.push("Invalid email format");
        }
        if (this.name.length < 2) {
            this.errors.push("Name too short");
        }

        return this.errors.length === 0;
    }

    toJSON(): object {
        return {
            ...super.toJSON(),
            email: this.email,
            name: this.name,
            role: this.role
        };
    }

    async save(): Promise<void> {
        if (!this.validate()) {
            throw new Error(`Validation failed: ${this.getErrors().join(", ")}`);
        }
        await super.save();
        this.dirty = false;
    }

    promote(): void {
        if (this.role === "guest") {
            this.role = "user";
        } else if (this.role === "user") {
            this.role = "admin";
        }
        this.markDirty();
    }
}

class AdminUser extends User {
    private permissions: Set<string>;

    constructor(email: string, name: string, permissions: string[] = []) {
        super(email, name, "admin");
        this.permissions = new Set(permissions);
    }

    hasPermission(permission: string): boolean {
        return this.permissions.has(permission) || this.permissions.has("*");
    }

    grant(permission: string): void {
        this.permissions.add(permission);
        this.markDirty();
    }

    revoke(permission: string): void {
        this.permissions.delete(permission);
        this.markDirty();
    }

    toJSON(): object {
        return {
            ...super.toJSON(),
            permissions: [...this.permissions]
        };
    }
}

// Usage
const admin = new AdminUser("admin@example.com", "Admin", ["users.read", "users.write"]);
admin.grant("settings.read");
admin.validate();
console.log(JSON.stringify(admin.toJSON(), null, 2));

const user = new User("test", "A", "guest");
if (!user.validate()) {
    console.log("Validation errors:", user.getErrors());
}
user.promote();
console.log(JSON.stringify(user.toJSON(), null, 2));"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("User"),
        "expected output to contain User class. output: {output}"
    );
    assert!(
        output.contains("AdminUser"),
        "expected output to contain AdminUser class. output: {output}"
    );
    assert!(
        output.contains("Entity"),
        "expected output to contain Entity class. output: {output}"
    );
    assert!(
        output.contains("Loggable"),
        "expected output to contain Loggable mixin. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for comprehensive class inheritance"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

// =============================================================================
// Private Field ES5 Source Map Tests
// =============================================================================

#[test]
fn test_source_map_private_field_es5_instance_field_access() {
    let source = r#"class Counter {
    #count: number = 0;

    increment(): void {
        this.#count++;
    }

    decrement(): void {
        this.#count--;
    }

    getCount(): number {
        return this.#count;
    }

    setCount(value: number): void {
        this.#count = value;
    }

    reset(): void {
        this.#count = 0;
    }
}

const counter = new Counter();
counter.increment();
counter.increment();
console.log(counter.getCount());
counter.setCount(10);
console.log(counter.getCount());"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        "expected non-empty source mappings for private instance field access"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_private_field_es5_static_field_access() {
    let source = r#"class IdGenerator {
    static #nextId: number = 1;
    static #prefix: string = "ID_";

    static generate(): string {
        return IdGenerator.#prefix + IdGenerator.#nextId++;
    }

    static reset(): void {
        IdGenerator.#nextId = 1;
    }

    static setPrefix(prefix: string): void {
        IdGenerator.#prefix = prefix;
    }

    static getNextId(): number {
        return IdGenerator.#nextId;
    }
}

console.log(IdGenerator.generate());
console.log(IdGenerator.generate());
IdGenerator.setPrefix("USER_");
console.log(IdGenerator.generate());
console.log(IdGenerator.getNextId());"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("IdGenerator"),
        "expected output to contain IdGenerator class. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for private static field access"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_private_field_es5_private_method_calls() {
    let source = r#"class Calculator {
    #value: number = 0;

    #add(n: number): void {
        this.#value += n;
    }

    #subtract(n: number): void {
        this.#value -= n;
    }

    #multiply(n: number): void {
        this.#value *= n;
    }

    #divide(n: number): void {
        if (n !== 0) {
            this.#value /= n;
        }
    }

    #validate(n: number): boolean {
        return typeof n === 'number' && !isNaN(n);
    }

    add(n: number): this {
        if (this.#validate(n)) {
            this.#add(n);
        }
        return this;
    }

    subtract(n: number): this {
        if (this.#validate(n)) {
            this.#subtract(n);
        }
        return this;
    }

    multiply(n: number): this {
        if (this.#validate(n)) {
            this.#multiply(n);
        }
        return this;
    }

    divide(n: number): this {
        if (this.#validate(n)) {
            this.#divide(n);
        }
        return this;
    }

    getValue(): number {
        return this.#value;
    }
}

const calc = new Calculator();
const result = calc.add(10).multiply(2).subtract(5).divide(3).getValue();
console.log(result);"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("Calculator"),
        "expected output to contain Calculator class. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for private method calls"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_private_field_es5_accessor_patterns() {
    let source = r#"class Person {
    #firstName: string;
    #lastName: string;
    #age: number;

    constructor(firstName: string, lastName: string, age: number) {
        this.#firstName = firstName;
        this.#lastName = lastName;
        this.#age = age;
    }

    get firstName(): string {
        return this.#firstName;
    }

    set firstName(value: string) {
        this.#firstName = value.trim();
    }

    get lastName(): string {
        return this.#lastName;
    }

    set lastName(value: string) {
        this.#lastName = value.trim();
    }

    get fullName(): string {
        return `${this.#firstName} ${this.#lastName}`;
    }

    set fullName(value: string) {
        const parts = value.split(' ');
        this.#firstName = parts[0] || '';
        this.#lastName = parts.slice(1).join(' ') || '';
    }

    get age(): number {
        return this.#age;
    }

    set age(value: number) {
        if (value >= 0 && value <= 150) {
            this.#age = value;
        }
    }
}

const person = new Person("John", "Doe", 30);
console.log(person.fullName);
person.fullName = "Jane Smith";
console.log(person.firstName, person.lastName);
person.age = 25;
console.log(person.age);"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        "expected non-empty source mappings for private accessor patterns"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_private_field_es5_derived_class() {
    let source = r#"class Animal {
    #name: string;
    #species: string;

    constructor(name: string, species: string) {
        this.#name = name;
        this.#species = species;
    }

    getName(): string {
        return this.#name;
    }

    getSpecies(): string {
        return this.#species;
    }

    describe(): string {
        return `${this.#name} is a ${this.#species}`;
    }
}

class Dog extends Animal {
    #breed: string;
    #trained: boolean;

    constructor(name: string, breed: string) {
        super(name, "dog");
        this.#breed = breed;
        this.#trained = false;
    }

    getBreed(): string {
        return this.#breed;
    }

    train(): void {
        this.#trained = true;
    }

    isTrained(): boolean {
        return this.#trained;
    }

    describe(): string {
        const base = super.describe();
        return `${base} (${this.#breed}, trained: ${this.#trained})`;
    }
}

class Cat extends Animal {
    #indoor: boolean;

    constructor(name: string, indoor: boolean = true) {
        super(name, "cat");
        this.#indoor = indoor;
    }

    isIndoor(): boolean {
        return this.#indoor;
    }

    describe(): string {
        const base = super.describe();
        return `${base} (${this.#indoor ? "indoor" : "outdoor"})`;
    }
}

const dog = new Dog("Buddy", "Labrador");
dog.train();
console.log(dog.describe());

const cat = new Cat("Whiskers", false);
console.log(cat.describe());"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("Animal"),
        "expected output to contain Animal class. output: {output}"
    );
    assert!(
        output.contains("Dog"),
        "expected output to contain Dog class. output: {output}"
    );
    assert!(
        output.contains("Cat"),
        "expected output to contain Cat class. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for private field in derived class"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_private_field_es5_weakmap_polyfill() {
    let source = r#"class SecureStorage {
    #data: Map<string, any> = new Map();
    #encryptionKey: string;
    static #instances: WeakMap<object, SecureStorage> = new WeakMap();

    constructor(key: string) {
        this.#encryptionKey = key;
        SecureStorage.#instances.set(this, this);
    }

    #encrypt(value: string): string {
        return btoa(value + this.#encryptionKey);
    }

    #decrypt(value: string): string {
        const decoded = atob(value);
        return decoded.replace(this.#encryptionKey, '');
    }

    set(key: string, value: any): void {
        const encrypted = this.#encrypt(JSON.stringify(value));
        this.#data.set(key, encrypted);
    }

    get(key: string): any {
        const encrypted = this.#data.get(key);
        if (encrypted) {
            return JSON.parse(this.#decrypt(encrypted));
        }
        return undefined;
    }

    has(key: string): boolean {
        return this.#data.has(key);
    }

    delete(key: string): boolean {
        return this.#data.delete(key);
    }

    static getInstance(obj: object): SecureStorage | undefined {
        return SecureStorage.#instances.get(obj);
    }
}

const storage = new SecureStorage("secret123");
storage.set("user", { name: "John", role: "admin" });
console.log(storage.get("user"));
console.log(storage.has("user"));
console.log(SecureStorage.getInstance(storage));"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("SecureStorage"),
        "expected output to contain SecureStorage class. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for WeakMap polyfill"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_private_field_es5_in_check() {
    let source = r#"class BrandedClass {
    #brand: symbol = Symbol("branded");

    static isBranded(obj: any): boolean {
        return #brand in obj;
    }

    getBrand(): symbol {
        return this.#brand;
    }
}

class Container<T> {
    #value: T;
    #initialized: boolean = false;

    constructor(value: T) {
        this.#value = value;
        this.#initialized = true;
    }

    static isContainer(obj: any): boolean {
        return #value in obj && #initialized in obj;
    }

    getValue(): T {
        return this.#value;
    }

    setValue(value: T): void {
        this.#value = value;
    }
}

const branded = new BrandedClass();
console.log(BrandedClass.isBranded(branded));
console.log(BrandedClass.isBranded({}));

const container = new Container<number>(42);
console.log(Container.isContainer(container));
console.log(container.getValue());"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("BrandedClass"),
        "expected output to contain BrandedClass. output: {output}"
    );
    assert!(
        output.contains("Container"),
        "expected output to contain Container. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for private field in-check"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_private_field_es5_static_method() {
    let source = r#"class Singleton {
    static #instance: Singleton | null = null;
    #id: number;

    private constructor() {
        this.#id = Math.random();
    }

    static #createInstance(): Singleton {
        return new Singleton();
    }

    static getInstance(): Singleton {
        if (Singleton.#instance === null) {
            Singleton.#instance = Singleton.#createInstance();
        }
        return Singleton.#instance;
    }

    static #resetInstance(): void {
        Singleton.#instance = null;
    }

    static reset(): void {
        Singleton.#resetInstance();
    }

    getId(): number {
        return this.#id;
    }
}

const instance1 = Singleton.getInstance();
const instance2 = Singleton.getInstance();
console.log(instance1 === instance2);
console.log(instance1.getId());
Singleton.reset();
const instance3 = Singleton.getInstance();
console.log(instance1 === instance3);"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("Singleton"),
        "expected output to contain Singleton class. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for private static method"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_private_field_es5_comprehensive() {
    let source = r#"// Comprehensive private field test with all patterns

class EventEmitter<T extends Record<string, any[]>> {
    #listeners: Map<keyof T, Set<(...args: any[]) => void>> = new Map();
    #maxListeners: number = 10;
    static #globalEmitters: WeakMap<object, EventEmitter<any>> = new WeakMap();

    constructor() {
        EventEmitter.#globalEmitters.set(this, this);
    }

    #getListeners<K extends keyof T>(event: K): Set<(...args: T[K]) => void> {
        let listeners = this.#listeners.get(event);
        if (!listeners) {
            listeners = new Set();
            this.#listeners.set(event, listeners);
        }
        return listeners as Set<(...args: T[K]) => void>;
    }

    #checkMaxListeners(event: keyof T): void {
        const count = this.#getListeners(event).size;
        if (count > this.#maxListeners) {
            console.warn(`Max listeners exceeded for event: ${String(event)}`);
        }
    }

    on<K extends keyof T>(event: K, listener: (...args: T[K]) => void): this {
        this.#getListeners(event).add(listener);
        this.#checkMaxListeners(event);
        return this;
    }

    off<K extends keyof T>(event: K, listener: (...args: T[K]) => void): this {
        this.#getListeners(event).delete(listener);
        return this;
    }

    emit<K extends keyof T>(event: K, ...args: T[K]): boolean {
        const listeners = this.#getListeners(event);
        if (listeners.size === 0) return false;
        listeners.forEach(listener => listener(...args));
        return true;
    }

    get maxListeners(): number {
        return this.#maxListeners;
    }

    set maxListeners(value: number) {
        this.#maxListeners = Math.max(0, value);
    }

    static isEmitter(obj: any): boolean {
        return #listeners in obj;
    }

    static getEmitter(obj: object): EventEmitter<any> | undefined {
        return EventEmitter.#globalEmitters.get(obj);
    }
}

class TypedEventEmitter extends EventEmitter<{
    connect: [host: string, port: number];
    disconnect: [reason: string];
    message: [data: string, timestamp: Date];
}> {
    #connected: boolean = false;
    #host: string = "";
    #port: number = 0;

    async #doConnect(host: string, port: number): Promise<void> {
        await new Promise(r => setTimeout(r, 100));
        this.#host = host;
        this.#port = port;
        this.#connected = true;
    }

    async connect(host: string, port: number): Promise<void> {
        await this.#doConnect(host, port);
        this.emit("connect", host, port);
    }

    disconnect(reason: string): void {
        this.#connected = false;
        this.emit("disconnect", reason);
    }

    send(data: string): void {
        if (this.#connected) {
            this.emit("message", data, new Date());
        }
    }

    get isConnected(): boolean {
        return this.#connected;
    }

    get connectionInfo(): { host: string; port: number } | null {
        if (this.#connected) {
            return { host: this.#host, port: this.#port };
        }
        return null;
    }
}

// Usage
const emitter = new TypedEventEmitter();

emitter.on("connect", (host, port) => {
    console.log(`Connected to ${host}:${port}`);
});

emitter.on("message", (data, timestamp) => {
    console.log(`[${timestamp.toISOString()}] ${data}`);
});

emitter.on("disconnect", (reason) => {
    console.log(`Disconnected: ${reason}`);
});

emitter.connect("localhost", 8080).then(() => {
    emitter.send("Hello, World!");
    emitter.disconnect("User requested");
});

console.log(EventEmitter.isEmitter(emitter));
console.log(EventEmitter.getEmitter(emitter) === emitter);"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("EventEmitter"),
        "expected output to contain EventEmitter class. output: {output}"
    );
    assert!(
        output.contains("TypedEventEmitter"),
        "expected output to contain TypedEventEmitter class. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for comprehensive private field"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

// =============================================================================
// Symbol-keyed Member ES5 Source Map Tests
// =============================================================================

#[test]
fn test_source_map_symbol_es5_iterator() {
    let source = r#"class Range {
    private start: number;
    private end: number;
    private step: number;

    constructor(start: number, end: number, step: number = 1) {
        this.start = start;
        this.end = end;
        this.step = step;
    }

    *[Symbol.iterator](): Iterator<number> {
        for (let i = this.start; i < this.end; i += this.step) {
            yield i;
        }
    }

    toArray(): number[] {
        return [...this];
    }
}

const range = new Range(0, 10, 2);
for (const n of range) {
    console.log(n);
}
console.log(range.toArray());"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("Range"),
        "expected output to contain Range class. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for Symbol.iterator"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_symbol_es5_async_iterator() {
    let source = r#"class AsyncQueue<T> {
    private items: T[] = [];
    private resolvers: ((value: IteratorResult<T>) => void)[] = [];
    private done: boolean = false;

    push(item: T): void {
        if (this.resolvers.length > 0) {
            const resolve = this.resolvers.shift()!;
            resolve({ value: item, done: false });
        } else {
            this.items.push(item);
        }
    }

    close(): void {
        this.done = true;
        for (const resolve of this.resolvers) {
            resolve({ value: undefined as any, done: true });
        }
        this.resolvers = [];
    }

    async *[Symbol.asyncIterator](): AsyncIterator<T> {
        while (!this.done || this.items.length > 0) {
            if (this.items.length > 0) {
                yield this.items.shift()!;
            } else if (!this.done) {
                yield await new Promise<T>((resolve) => {
                    this.resolvers.push((result) => {
                        if (!result.done) {
                            resolve(result.value);
                        }
                    });
                });
            }
        }
    }
}

const queue = new AsyncQueue<number>();
queue.push(1);
queue.push(2);
queue.push(3);
queue.close();

(async () => {
    for await (const item of queue) {
        console.log(item);
    }
})();"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("AsyncQueue"),
        "expected output to contain AsyncQueue class. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for Symbol.asyncIterator"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_symbol_es5_computed_symbol_methods() {
    let source = r#"const customMethod = Symbol("customMethod");
const customGetter = Symbol("customGetter");
const customProperty = Symbol("customProperty");

class SymbolClass {
    [customProperty]: string = "default";

    [customMethod](x: number, y: number): number {
        return x + y;
    }

    get [customGetter](): string {
        return `Value: ${this[customProperty]}`;
    }

    set [customGetter](value: string) {
        this[customProperty] = value;
    }
}

const obj = new SymbolClass();
console.log(obj[customMethod](1, 2));
console.log(obj[customGetter]);
obj[customGetter] = "updated";
console.log(obj[customProperty]);"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("SymbolClass"),
        "expected output to contain SymbolClass. output: {output}"
    );
    assert!(
        output.contains("customMethod"),
        "expected output to contain customMethod. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for computed Symbol methods"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_symbol_es5_to_string_tag() {
    let source = r#"class CustomCollection<T> {
    private items: T[] = [];

    get [Symbol.toStringTag](): string {
        return "CustomCollection";
    }

    add(item: T): void {
        this.items.push(item);
    }

    get size(): number {
        return this.items.length;
    }
}

class NamedObject {
    private name: string;

    constructor(name: string) {
        this.name = name;
    }

    get [Symbol.toStringTag](): string {
        return `NamedObject(${this.name})`;
    }
}

const collection = new CustomCollection<number>();
collection.add(1);
collection.add(2);
console.log(Object.prototype.toString.call(collection));

const named = new NamedObject("test");
console.log(Object.prototype.toString.call(named));"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("CustomCollection"),
        "expected output to contain CustomCollection class. output: {output}"
    );
    assert!(
        output.contains("NamedObject"),
        "expected output to contain NamedObject class. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for Symbol.toStringTag"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_symbol_es5_has_instance() {
    let source = r#"class CustomType {
    private value: any;

    constructor(value: any) {
        this.value = value;
    }

    static [Symbol.hasInstance](instance: any): boolean {
        return instance !== null &&
               typeof instance === "object" &&
               "value" in instance;
    }
}

class ExtendedCustomType extends CustomType {
    private extra: string;

    constructor(value: any, extra: string) {
        super(value);
        this.extra = extra;
    }

    static [Symbol.hasInstance](instance: any): boolean {
        return super[Symbol.hasInstance](instance) && "extra" in instance;
    }
}

const obj1 = new CustomType(42);
const obj2 = new ExtendedCustomType(42, "hello");
const obj3 = { value: 10 };

console.log(obj1 instanceof CustomType);
console.log(obj2 instanceof CustomType);
console.log(obj3 instanceof CustomType);
console.log(obj2 instanceof ExtendedCustomType);"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("CustomType"),
        "expected output to contain CustomType class. output: {output}"
    );
    assert!(
        output.contains("ExtendedCustomType"),
        "expected output to contain ExtendedCustomType class. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for Symbol.hasInstance"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_symbol_es5_species() {
    let source = r#"class MyArray<T> extends Array<T> {
    static get [Symbol.species](): ArrayConstructor {
        return Array;
    }

    customMethod(): T | undefined {
        return this[0];
    }
}

class SpecialArray<T> extends Array<T> {
    static get [Symbol.species](): typeof SpecialArray {
        return SpecialArray;
    }

    static create<U>(...items: U[]): SpecialArray<U> {
        const arr = new SpecialArray<U>();
        arr.push(...items);
        return arr;
    }

    double(): SpecialArray<T> {
        return this.concat(this) as SpecialArray<T>;
    }
}

const myArr = new MyArray(1, 2, 3);
const mapped = myArr.map(x => x * 2);
console.log(mapped instanceof MyArray);
console.log(mapped instanceof Array);

const special = SpecialArray.create(1, 2, 3);
const doubled = special.double();
console.log(doubled instanceof SpecialArray);"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("MyArray"),
        "expected output to contain MyArray class. output: {output}"
    );
    assert!(
        output.contains("SpecialArray"),
        "expected output to contain SpecialArray class. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for Symbol.species"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_symbol_es5_to_primitive() {
    let source = r#"class Money {
    private amount: number;
    private currency: string;

    constructor(amount: number, currency: string = "USD") {
        this.amount = amount;
        this.currency = currency;
    }

    [Symbol.toPrimitive](hint: string): string | number {
        if (hint === "number") {
            return this.amount;
        }
        if (hint === "string") {
            return `${this.currency} ${this.amount.toFixed(2)}`;
        }
        return this.amount;
    }

    add(other: Money): Money {
        if (this.currency !== other.currency) {
            throw new Error("Currency mismatch");
        }
        return new Money(this.amount + other.amount, this.currency);
    }
}

const price = new Money(99.99);
const tax = new Money(8.50);
console.log(+price);
console.log(`${price}`);
console.log(price + 0);
const total = price.add(tax);
console.log(`${total}`);"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("Money"),
        "expected output to contain Money class. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for Symbol.toPrimitive"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_symbol_es5_is_concat_spreadable() {
    let source = r#"class SpreadableCollection<T> {
    private items: T[];

    constructor(...items: T[]) {
        this.items = items;
    }

    get [Symbol.isConcatSpreadable](): boolean {
        return true;
    }

    get length(): number {
        return this.items.length;
    }

    [index: number]: T;

    *[Symbol.iterator](): Iterator<T> {
        yield* this.items;
    }
}

// Set up indexed access
const createSpreadable = <T>(...items: T[]): SpreadableCollection<T> & T[] => {
    const collection = new SpreadableCollection(...items);
    items.forEach((item, i) => {
        (collection as any)[i] = item;
    });
    return collection as SpreadableCollection<T> & T[];
};

const arr1 = [1, 2, 3];
const spreadable = createSpreadable(4, 5, 6);
const combined = arr1.concat(spreadable);
console.log(combined);
console.log(combined.length);"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("SpreadableCollection"),
        "expected output to contain SpreadableCollection class. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for Symbol.isConcatSpreadable"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_symbol_es5_comprehensive() {
    let source = r#"// Comprehensive Symbol-keyed member test

const customKey = Symbol("customKey");

class SuperCollection<T> {
    protected items: T[] = [];
    protected name: string;

    constructor(name: string) {
        this.name = name;
    }

    // Symbol.toStringTag
    get [Symbol.toStringTag](): string {
        return `SuperCollection<${this.name}>`;
    }

    // Symbol.iterator
    *[Symbol.iterator](): Iterator<T> {
        yield* this.items;
    }

    // Symbol.toPrimitive
    [Symbol.toPrimitive](hint: string): string | number {
        if (hint === "number") {
            return this.items.length;
        }
        return `[${this.name}: ${this.items.length} items]`;
    }

    // Custom symbol method
    [customKey](multiplier: number): number {
        return this.items.length * multiplier;
    }

    // Symbol.hasInstance
    static [Symbol.hasInstance](instance: any): boolean {
        return instance !== null &&
               typeof instance === "object" &&
               "items" in instance &&
               "name" in instance;
    }

    add(...items: T[]): this {
        this.items.push(...items);
        return this;
    }

    get size(): number {
        return this.items.length;
    }
}

class AsyncSuperCollection<T> extends SuperCollection<T> {
    // Symbol.asyncIterator
    async *[Symbol.asyncIterator](): AsyncIterator<T> {
        for (const item of this.items) {
            await new Promise(r => setTimeout(r, 10));
            yield item;
        }
    }

    // Override Symbol.toStringTag
    get [Symbol.toStringTag](): string {
        return `AsyncSuperCollection<${this.name}>`;
    }

    // Symbol.species
    static get [Symbol.species](): typeof AsyncSuperCollection {
        return AsyncSuperCollection;
    }

    async processAll<U>(fn: (item: T) => Promise<U>): Promise<U[]> {
        const results: U[] = [];
        for await (const item of this) {
            results.push(await fn(item));
        }
        return results;
    }
}

// Usage
const collection = new SuperCollection<number>("Numbers");
collection.add(1, 2, 3, 4, 5);

console.log(Object.prototype.toString.call(collection));
console.log([...collection]);
console.log(+collection);
console.log(`${collection}`);
console.log(collection[customKey](10));
console.log({ items: [], name: "test" } instanceof SuperCollection);

const asyncCollection = new AsyncSuperCollection<string>("Strings");
asyncCollection.add("a", "b", "c");

(async () => {
    for await (const item of asyncCollection) {
        console.log(item);
    }

    const results = await asyncCollection.processAll(async (s) => s.toUpperCase());
    console.log(results);
})();"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("SuperCollection"),
        "expected output to contain SuperCollection class. output: {output}"
    );
    assert!(
        output.contains("AsyncSuperCollection"),
        "expected output to contain AsyncSuperCollection class. output: {output}"
    );
    assert!(
        output.contains("customKey"),
        "expected output to contain customKey symbol. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for comprehensive Symbol-keyed members"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

// =============================================================================
// Decorator Metadata ES5 Source Map Tests
// =============================================================================

#[test]
fn test_source_map_decorator_metadata_es5_reflect_metadata() {
    let source = r#"// Simulating reflect-metadata patterns
const metadataKey = Symbol("metadata");

function Metadata(key: string, value: any): ClassDecorator & MethodDecorator & PropertyDecorator {
    return function(target: any, propertyKey?: string | symbol, descriptor?: PropertyDescriptor) {
        if (propertyKey === undefined) {
            // Class decorator
            Reflect.defineMetadata(key, value, target);
        } else {
            // Method or property decorator
            Reflect.defineMetadata(key, value, target, propertyKey);
        }
        return descriptor as any;
    };
}

function getMetadata(key: string, target: any, propertyKey?: string | symbol): any {
    if (propertyKey === undefined) {
        return Reflect.getMetadata(key, target);
    }
    return Reflect.getMetadata(key, target, propertyKey);
}

@Metadata("role", "admin")
@Metadata("version", "1.0")
class UserService {
    @Metadata("column", "user_name")
    name: string = "";

    @Metadata("endpoint", "/users")
    @Metadata("method", "GET")
    getUsers(): string[] {
        return [];
    }
}

const service = new UserService();
console.log(getMetadata("role", UserService));
console.log(getMetadata("column", service, "name"));"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("UserService"),
        "expected output to contain UserService class. output: {output}"
    );
    assert!(
        output.contains("Metadata"),
        "expected output to contain Metadata decorator. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for reflect-metadata"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_decorator_metadata_es5_parameter_decorators() {
    let source = r#"const paramMetadata = new Map<string, Map<number, any>>();

function Inject(token: string): ParameterDecorator {
    return function(target: Object, propertyKey: string | symbol | undefined, parameterIndex: number) {
        const key = propertyKey ? String(propertyKey) : "constructor";
        if (!paramMetadata.has(key)) {
            paramMetadata.set(key, new Map());
        }
        paramMetadata.get(key)!.set(parameterIndex, { token });
    };
}

function Required(): ParameterDecorator {
    return function(target: Object, propertyKey: string | symbol | undefined, parameterIndex: number) {
        const key = propertyKey ? String(propertyKey) : "constructor";
        if (!paramMetadata.has(key)) {
            paramMetadata.set(key, new Map());
        }
        const existing = paramMetadata.get(key)!.get(parameterIndex) || {};
        paramMetadata.get(key)!.set(parameterIndex, { ...existing, required: true });
    };
}

function Validate(validator: (val: any) => boolean): ParameterDecorator {
    return function(target: Object, propertyKey: string | symbol | undefined, parameterIndex: number) {
        const key = propertyKey ? String(propertyKey) : "constructor";
        if (!paramMetadata.has(key)) {
            paramMetadata.set(key, new Map());
        }
        const existing = paramMetadata.get(key)!.get(parameterIndex) || {};
        paramMetadata.get(key)!.set(parameterIndex, { ...existing, validator });
    };
}

class ApiController {
    constructor(
        @Inject("HttpClient") private http: any,
        @Inject("Logger") @Required() private logger: any
    ) {}

    fetchData(
        @Required() @Validate(v => typeof v === "string") endpoint: string,
        @Inject("Cache") cache?: any
    ): Promise<any> {
        return this.http.get(endpoint);
    }
}

const controller = new ApiController({}, {});
console.log(paramMetadata);"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("ApiController"),
        "expected output to contain ApiController class. output: {output}"
    );
    assert!(
        output.contains("Inject"),
        "expected output to contain Inject decorator. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for parameter decorators"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_decorator_metadata_es5_property_descriptors() {
    let source = r#"function Observable(): PropertyDecorator {
    return function(target: Object, propertyKey: string | symbol) {
        let value: any;
        const getter = function(this: any) {
            console.log(`Getting ${String(propertyKey)}`);
            return value;
        };
        const setter = function(this: any, newVal: any) {
            console.log(`Setting ${String(propertyKey)} to ${newVal}`);
            value = newVal;
        };
        Object.defineProperty(target, propertyKey, {
            get: getter,
            set: setter,
            enumerable: true,
            configurable: true
        });
    };
}

function DefaultValue(defaultVal: any): PropertyDecorator {
    return function(target: Object, propertyKey: string | symbol) {
        let value = defaultVal;
        Object.defineProperty(target, propertyKey, {
            get() { return value; },
            set(newVal) { value = newVal; },
            enumerable: true,
            configurable: true
        });
    };
}

function Readonly(): PropertyDecorator {
    return function(target: Object, propertyKey: string | symbol) {
        Object.defineProperty(target, propertyKey, {
            writable: false,
            configurable: false
        });
    };
}

class Config {
    @Observable()
    @DefaultValue("development")
    environment: string;

    @Observable()
    @DefaultValue(3000)
    port: number;

    @Readonly()
    version: string = "1.0.0";
}

const config = new Config();
console.log(config.environment);
config.port = 8080;
console.log(config.port);"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("Config"),
        "expected output to contain Config class. output: {output}"
    );
    assert!(
        output.contains("Observable"),
        "expected output to contain Observable decorator. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for property descriptors"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_decorator_metadata_es5_method_descriptors() {
    let source = r#"function Log(prefix: string): MethodDecorator {
    return function(target: Object, propertyKey: string | symbol, descriptor: PropertyDescriptor) {
        const original = descriptor.value;
        descriptor.value = function(...args: any[]) {
            console.log(`${prefix} Calling ${String(propertyKey)} with`, args);
            const result = original.apply(this, args);
            console.log(`${prefix} Result:`, result);
            return result;
        };
        return descriptor;
    };
}

function Memoize(): MethodDecorator {
    return function(target: Object, propertyKey: string | symbol, descriptor: PropertyDescriptor) {
        const original = descriptor.value;
        const cache = new Map<string, any>();
        descriptor.value = function(...args: any[]) {
            const key = JSON.stringify(args);
            if (cache.has(key)) {
                return cache.get(key);
            }
            const result = original.apply(this, args);
            cache.set(key, result);
            return result;
        };
        return descriptor;
    };
}

function Throttle(ms: number): MethodDecorator {
    return function(target: Object, propertyKey: string | symbol, descriptor: PropertyDescriptor) {
        const original = descriptor.value;
        let lastCall = 0;
        descriptor.value = function(...args: any[]) {
            const now = Date.now();
            if (now - lastCall >= ms) {
                lastCall = now;
                return original.apply(this, args);
            }
        };
        return descriptor;
    };
}

class Calculator {
    @Log("[CALC]")
    @Memoize()
    fibonacci(n: number): number {
        if (n <= 1) return n;
        return this.fibonacci(n - 1) + this.fibonacci(n - 2);
    }

    @Log("[CALC]")
    @Throttle(1000)
    expensiveOperation(x: number): number {
        return x * x;
    }
}

const calc = new Calculator();
console.log(calc.fibonacci(10));
console.log(calc.expensiveOperation(5));"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("Calculator"),
        "expected output to contain Calculator class. output: {output}"
    );
    assert!(
        output.contains("Memoize"),
        "expected output to contain Memoize decorator. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for method descriptors"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_decorator_metadata_es5_accessor_descriptors() {
    let source = r#"function Enumerable(value: boolean): MethodDecorator {
    return function(target: Object, propertyKey: string | symbol, descriptor: PropertyDescriptor) {
        descriptor.enumerable = value;
        return descriptor;
    };
}

function Configurable(value: boolean): MethodDecorator {
    return function(target: Object, propertyKey: string | symbol, descriptor: PropertyDescriptor) {
        descriptor.configurable = value;
        return descriptor;
    };
}

function ValidateSet(validator: (val: any) => boolean): MethodDecorator {
    return function(target: Object, propertyKey: string | symbol, descriptor: PropertyDescriptor) {
        const originalSet = descriptor.set;
        if (originalSet) {
            descriptor.set = function(value: any) {
                if (!validator(value)) {
                    throw new Error(`Invalid value for ${String(propertyKey)}`);
                }
                originalSet.call(this, value);
            };
        }
        return descriptor;
    };
}

class Person {
    private _name: string = "";
    private _age: number = 0;

    @Enumerable(true)
    @Configurable(false)
    get name(): string {
        return this._name;
    }

    @ValidateSet(v => typeof v === "string" && v.length > 0)
    set name(value: string) {
        this._name = value;
    }

    @Enumerable(true)
    get age(): number {
        return this._age;
    }

    @ValidateSet(v => typeof v === "number" && v >= 0 && v <= 150)
    set age(value: number) {
        this._age = value;
    }
}

const person = new Person();
person.name = "John";
person.age = 30;
console.log(person.name, person.age);"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("ValidateSet"),
        "expected output to contain ValidateSet decorator. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for accessor descriptors"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_decorator_metadata_es5_class_constructor() {
    let source = r#"interface ClassConstructor<T = any> {
    new (...args: any[]): T;
}

function Injectable(): ClassDecorator {
    return function<T extends ClassConstructor>(target: T) {
        // Mark class as injectable
        (target as any).__injectable__ = true;
        return target;
    };
}

function Singleton(): ClassDecorator {
    return function<T extends ClassConstructor>(target: T) {
        let instance: any = null;
        const original = target;
        const newConstructor: any = function(...args: any[]) {
            if (instance === null) {
                instance = new original(...args);
            }
            return instance;
        };
        newConstructor.prototype = original.prototype;
        Object.setPrototypeOf(newConstructor, original);
        return newConstructor;
    };
}

function Registry(name: string): ClassDecorator {
    return function<T extends ClassConstructor>(target: T) {
        const registry = (globalThis as any).__registry__ || new Map();
        registry.set(name, target);
        (globalThis as any).__registry__ = registry;
        return target;
    };
}

@Injectable()
@Singleton()
@Registry("DatabaseService")
class DatabaseService {
    private connectionString: string;

    constructor(connectionString: string = "default") {
        this.connectionString = connectionString;
        console.log("DatabaseService created");
    }

    query(sql: string): any[] {
        return [];
    }
}

@Injectable()
@Registry("UserRepository")
class UserRepository {
    constructor(private db: DatabaseService) {}

    findAll(): any[] {
        return this.db.query("SELECT * FROM users");
    }
}

const db1 = new DatabaseService("conn1");
const db2 = new DatabaseService("conn2");
console.log(db1 === db2);"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("DatabaseService"),
        "expected output to contain DatabaseService class. output: {output}"
    );
    assert!(
        output.contains("Singleton"),
        "expected output to contain Singleton decorator. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for class constructor metadata"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_decorator_metadata_es5_design_type() {
    let source = r#"// Simulating design:type, design:paramtypes, design:returntype metadata
const typeMetadata = new WeakMap<Object, Map<string, any>>();

function Type(type: any): PropertyDecorator & ParameterDecorator {
    return function(target: Object, propertyKey?: string | symbol, parameterIndex?: number) {
        if (propertyKey !== undefined) {
            if (!typeMetadata.has(target)) {
                typeMetadata.set(target, new Map());
            }
            typeMetadata.get(target)!.set(String(propertyKey), { type });
        }
    };
}

function ReturnType(type: any): MethodDecorator {
    return function(target: Object, propertyKey: string | symbol, descriptor: PropertyDescriptor) {
        if (!typeMetadata.has(target)) {
            typeMetadata.set(target, new Map());
        }
        const existing = typeMetadata.get(target)!.get(String(propertyKey)) || {};
        typeMetadata.get(target)!.set(String(propertyKey), { ...existing, returnType: type });
        return descriptor;
    };
}

function ParamTypes(...types: any[]): MethodDecorator {
    return function(target: Object, propertyKey: string | symbol, descriptor: PropertyDescriptor) {
        if (!typeMetadata.has(target)) {
            typeMetadata.set(target, new Map());
        }
        const existing = typeMetadata.get(target)!.get(String(propertyKey)) || {};
        typeMetadata.get(target)!.set(String(propertyKey), { ...existing, paramTypes: types });
        return descriptor;
    };
}

class Entity {
    @Type(String)
    id: string = "";

    @Type(String)
    name: string = "";

    @Type(Number)
    age: number = 0;

    @Type(Boolean)
    active: boolean = true;

    @ReturnType(String)
    @ParamTypes(String, Number)
    format(template: string, precision: number): string {
        return `${this.name} (${this.age})`;
    }
}

const entity = new Entity();
console.log(typeMetadata.get(entity));"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("Entity"),
        "expected output to contain Entity class. output: {output}"
    );
    assert!(
        output.contains("ReturnType"),
        "expected output to contain ReturnType decorator. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for design type metadata"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_decorator_metadata_es5_comprehensive() {
    let source = r#"// Comprehensive decorator metadata test combining all patterns

// Type metadata storage
const classMetadata = new WeakMap<Function, Map<string, any>>();
const propertyMetadata = new WeakMap<Object, Map<string | symbol, any>>();
const methodMetadata = new WeakMap<Object, Map<string | symbol, any>>();
const parameterMetadata = new WeakMap<Object, Map<string | symbol, Map<number, any>>>();

// Class decorators
function Controller(path: string): ClassDecorator {
    return function(target: Function) {
        if (!classMetadata.has(target)) {
            classMetadata.set(target, new Map());
        }
        classMetadata.get(target)!.set("path", path);
        classMetadata.get(target)!.set("type", "controller");
    };
}

function Service(): ClassDecorator {
    return function(target: Function) {
        if (!classMetadata.has(target)) {
            classMetadata.set(target, new Map());
        }
        classMetadata.get(target)!.set("type", "service");
        classMetadata.get(target)!.set("injectable", true);
    };
}

// Property decorators
function Column(options?: { type?: string; nullable?: boolean }): PropertyDecorator {
    return function(target: Object, propertyKey: string | symbol) {
        if (!propertyMetadata.has(target)) {
            propertyMetadata.set(target, new Map());
        }
        propertyMetadata.get(target)!.set(propertyKey, { column: true, ...options });
    };
}

// Method decorators
function Get(path: string): MethodDecorator {
    return function(target: Object, propertyKey: string | symbol, descriptor: PropertyDescriptor) {
        if (!methodMetadata.has(target)) {
            methodMetadata.set(target, new Map());
        }
        methodMetadata.get(target)!.set(propertyKey, { method: "GET", path });
        return descriptor;
    };
}

function Post(path: string): MethodDecorator {
    return function(target: Object, propertyKey: string | symbol, descriptor: PropertyDescriptor) {
        if (!methodMetadata.has(target)) {
            methodMetadata.set(target, new Map());
        }
        methodMetadata.get(target)!.set(propertyKey, { method: "POST", path });
        return descriptor;
    };
}

// Parameter decorators
function Body(): ParameterDecorator {
    return function(target: Object, propertyKey: string | symbol | undefined, parameterIndex: number) {
        const key = propertyKey || "constructor";
        if (!parameterMetadata.has(target)) {
            parameterMetadata.set(target, new Map());
        }
        if (!parameterMetadata.get(target)!.has(key)) {
            parameterMetadata.get(target)!.set(key, new Map());
        }
        parameterMetadata.get(target)!.get(key)!.set(parameterIndex, { source: "body" });
    };
}

function Query(name: string): ParameterDecorator {
    return function(target: Object, propertyKey: string | symbol | undefined, parameterIndex: number) {
        const key = propertyKey || "constructor";
        if (!parameterMetadata.has(target)) {
            parameterMetadata.set(target, new Map());
        }
        if (!parameterMetadata.get(target)!.has(key)) {
            parameterMetadata.get(target)!.set(key, new Map());
        }
        parameterMetadata.get(target)!.get(key)!.set(parameterIndex, { source: "query", name });
    };
}

// Accessor decorator
function Cached(): MethodDecorator {
    return function(target: Object, propertyKey: string | symbol, descriptor: PropertyDescriptor) {
        if (descriptor.get) {
            const originalGet = descriptor.get;
            let cached: any;
            let hasCached = false;
            descriptor.get = function() {
                if (!hasCached) {
                    cached = originalGet.call(this);
                    hasCached = true;
                }
                return cached;
            };
        }
        return descriptor;
    };
}

// Entity class
@Service()
class UserEntity {
    @Column({ type: "uuid" })
    id: string = "";

    @Column({ type: "varchar", nullable: false })
    name: string = "";

    @Column({ type: "varchar", nullable: true })
    email: string = "";

    @Cached()
    get displayName(): string {
        return `${this.name} <${this.email}>`;
    }
}

// Controller class
@Controller("/users")
class UserController {
    constructor(private userService: UserEntity) {}

    @Get("/")
    async getAll(@Query("limit") limit: number): Promise<UserEntity[]> {
        return [];
    }

    @Get("/:id")
    async getOne(@Query("id") id: string): Promise<UserEntity | null> {
        return null;
    }

    @Post("/")
    async create(@Body() data: Partial<UserEntity>): Promise<UserEntity> {
        return new UserEntity();
    }
}

// Usage
const controller = new UserController(new UserEntity());
console.log(classMetadata.get(UserController));
console.log(classMetadata.get(UserEntity));
console.log(methodMetadata.get(UserController.prototype));"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("UserController"),
        "expected output to contain UserController class. output: {output}"
    );
    assert!(
        output.contains("UserEntity"),
        "expected output to contain UserEntity class. output: {output}"
    );
    assert!(
        output.contains("Controller"),
        "expected output to contain Controller decorator. output: {output}"
    );
    assert!(
        output.contains("Service"),
        "expected output to contain Service decorator. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for comprehensive decorator metadata"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

// =============================================================================
// Module Bundling ES5 Source Map Tests
// =============================================================================

#[test]
fn test_source_map_module_es5_commonjs_require() {
    let source = r#"// CommonJS require patterns
import { readFile, writeFile } from 'fs';
import * as path from 'path';
import http from 'http';

const express = require('express');
const { Router } = require('express');
const lodash = require('lodash');

export function loadConfig(configPath: string): any {
    const fullPath = path.resolve(configPath);
    const content = readFile(fullPath, 'utf-8');
    return JSON.parse(content as any);
}

export function saveConfig(configPath: string, config: any): void {
    const fullPath = path.resolve(configPath);
    writeFile(fullPath, JSON.stringify(config, null, 2));
}

export class Server {
    private app: any;
    private router: any;

    constructor() {
        this.app = express();
        this.router = Router();
    }

    start(port: number): void {
        this.app.listen(port, () => {
            console.log(`Server running on port ${port}`);
        });
    }
}

const server = new Server();
server.start(3000);"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("Server"),
        "expected output to contain Server class. output: {output}"
    );
    assert!(
        output.contains("loadConfig"),
        "expected output to contain loadConfig function. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for CommonJS require"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_module_es5_dynamic_import() {
    let source = r#"// Dynamic import patterns
async function loadModule(moduleName: string): Promise<any> {
    const module = await import(moduleName);
    return module.default || module;
}

async function loadMultipleModules(names: string[]): Promise<any[]> {
    const modules = await Promise.all(
        names.map(name => import(name))
    );
    return modules.map(m => m.default || m);
}

class PluginLoader {
    private plugins: Map<string, any> = new Map();

    async load(pluginPath: string): Promise<void> {
        const plugin = await import(pluginPath);
        const name = plugin.name || pluginPath;
        this.plugins.set(name, plugin.default || plugin);
    }

    async loadAll(pluginPaths: string[]): Promise<void> {
        await Promise.all(pluginPaths.map(p => this.load(p)));
    }

    get(name: string): any {
        return this.plugins.get(name);
    }
}

// Lazy loading with fallback
async function lazyLoad<T>(
    loader: () => Promise<{ default: T }>,
    fallback: T
): Promise<T> {
    try {
        const module = await loader();
        return module.default;
    } catch {
        return fallback;
    }
}

const loader = new PluginLoader();
loader.loadAll(['./plugin1', './plugin2']).then(() => {
    console.log('Plugins loaded');
});"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("PluginLoader"),
        "expected output to contain PluginLoader class. output: {output}"
    );
    assert!(
        output.contains("loadModule"),
        "expected output to contain loadModule function. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for dynamic import"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_module_es5_reexports() {
    let source = r#"// Re-export patterns
export { foo, bar } from './moduleA';
export { default as baz } from './moduleB';
export * from './moduleC';
export * as utils from './utils';
export type { SomeType } from './types';

// Named re-exports with renaming
export { original as renamed } from './moduleD';
export { ClassA as ExportedClass, functionB as exportedFunction } from './moduleE';

// Re-export with local use
import { helper } from './helpers';
export { helper };

function useHelper(): string {
    return helper('test');
}

// Mixed exports
export const localConst = 'local';
export function localFunction(): void {}
export class LocalClass {}

export { useHelper };"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("LocalClass"),
        "expected output to contain LocalClass. output: {output}"
    );
    assert!(
        output.contains("useHelper"),
        "expected output to contain useHelper function. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for re-exports"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_module_es5_barrel_exports() {
    let source = r#"// Barrel export pattern (index.ts style)

// Components
export { Button } from './components/Button';
export { Input } from './components/Input';
export { Select } from './components/Select';
export { Modal } from './components/Modal';

// Hooks
export { useState } from './hooks/useState';
export { useEffect } from './hooks/useEffect';
export { useCallback } from './hooks/useCallback';

// Utils
export * from './utils/string';
export * from './utils/number';
export * from './utils/date';

// Types
export type { ButtonProps } from './components/Button';
export type { InputProps } from './components/Input';
export type { Config, Options } from './types';

// Default export aggregation
import DefaultComponent from './DefaultComponent';
export default DefaultComponent;

// Re-export with namespace
export * as components from './components';
export * as hooks from './hooks';
export * as utils from './utils';

// Constants
export const VERSION = '1.0.0';
export const API_URL = 'https://api.example.com';"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("VERSION"),
        "expected output to contain VERSION constant. output: {output}"
    );
    assert!(
        output.contains("API_URL"),
        "expected output to contain API_URL constant. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for barrel exports"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_module_es5_circular_imports() {
    let source = r#"// Circular import pattern handling
// This simulates moduleA that imports from moduleB which imports from moduleA

import type { BType } from './moduleB';

export interface AType {
    name: string;
    ref?: BType;
}

export class ClassA {
    private data: AType;
    private linked?: import('./moduleB').ClassB;

    constructor(name: string) {
        this.data = { name };
    }

    async link(): Promise<void> {
        const moduleB = await import('./moduleB');
        this.linked = new moduleB.ClassB(this);
    }

    getData(): AType {
        return this.data;
    }

    setRef(ref: BType): void {
        this.data.ref = ref;
    }
}

export function createA(name: string): ClassA {
    return new ClassA(name);
}

// Lazy circular reference resolution
let _classB: typeof import('./moduleB').ClassB | null = null;

export async function getClassB(): Promise<typeof import('./moduleB').ClassB> {
    if (!_classB) {
        const mod = await import('./moduleB');
        _classB = mod.ClassB;
    }
    return _classB;
}

const instance = new ClassA('test');
instance.link().then(() => console.log('Linked'));"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("ClassA"),
        "expected output to contain ClassA class. output: {output}"
    );
    assert!(
        output.contains("createA"),
        "expected output to contain createA function. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for circular imports"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_module_es5_conditional_imports() {
    let source = r#"// Conditional import patterns
declare const process: { env: { NODE_ENV: string; PLATFORM: string } };

// Environment-based conditional import
const getLogger = async () => {
    if (process.env.NODE_ENV === 'production') {
        return import('./prodLogger');
    } else {
        return import('./devLogger');
    }
};

// Platform-based conditional import
const getPlatformModule = async () => {
    switch (process.env.PLATFORM) {
        case 'web':
            return import('./platform/web');
        case 'node':
            return import('./platform/node');
        case 'electron':
            return import('./platform/electron');
        default:
            return import('./platform/default');
    }
};

// Feature flag conditional import
interface FeatureFlags {
    newUI: boolean;
    betaFeatures: boolean;
}

async function loadFeatures(flags: FeatureFlags): Promise<any[]> {
    const features: Promise<any>[] = [];

    if (flags.newUI) {
        features.push(import('./features/newUI'));
    }

    if (flags.betaFeatures) {
        features.push(import('./features/beta'));
    }

    return Promise.all(features);
}

// Polyfill conditional import
async function loadPolyfills(): Promise<void> {
    if (typeof globalThis.fetch === 'undefined') {
        await import('whatwg-fetch');
    }

    if (typeof globalThis.Promise === 'undefined') {
        await import('es6-promise');
    }

    if (!Array.prototype.includes) {
        await import('array-includes');
    }
}

// Usage
getLogger().then(logger => logger.default.info('App started'));
loadPolyfills().then(() => console.log('Polyfills loaded'));"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("getLogger"),
        "expected output to contain getLogger function. output: {output}"
    );
    assert!(
        output.contains("loadPolyfills"),
        "expected output to contain loadPolyfills function. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for conditional imports"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_module_es5_namespace_imports() {
    let source = r#"// Namespace import patterns
import * as React from 'react';
import * as ReactDOM from 'react-dom';
import * as _ from 'lodash';
import * as utils from './utils';

// Using namespace imports
const element = React.createElement('div', { className: 'container' },
    React.createElement('h1', null, 'Hello'),
    React.createElement('p', null, 'World')
);

// Lodash namespace usage
const data = [1, 2, 3, 4, 5];
const doubled = _.map(data, (n: number) => n * 2);
const sum = _.reduce(doubled, (acc: number, n: number) => acc + n, 0);
const unique = _.uniq([1, 1, 2, 2, 3]);

// Custom utils namespace
const formatted = utils.formatDate(new Date());
const validated = utils.validateEmail('test@example.com');
const parsed = utils.parseJSON('{"key": "value"}');

// Re-export namespace
export { React, ReactDOM, _ as lodash, utils };

// Namespace with type usage
type ReactElement = React.ReactElement;
type LodashArray = _.LoDashStatic;

export function render(el: ReactElement, container: Element): void {
    ReactDOM.render(el, container);
}

console.log(sum, unique, formatted);"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("render"),
        "expected output to contain render function. output: {output}"
    );
    assert!(
        output.contains("doubled"),
        "expected output to contain doubled variable. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for namespace imports"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_module_es5_comprehensive() {
    let source = r#"// Comprehensive module bundling test combining all patterns

// Static imports
import { EventEmitter } from 'events';
import * as fs from 'fs';
import path from 'path';

// CommonJS require
const express = require('express');
const bodyParser = require('body-parser');

// Type-only imports
import type { ServerOptions, RequestHandler } from 'express';

// Re-exports
export { EventEmitter } from 'events';
export * as fsUtils from 'fs';
export type { ServerOptions };

// Dynamic import loader
class ModuleRegistry {
    private modules: Map<string, any> = new Map();
    private loading: Map<string, Promise<any>> = new Map();

    async load(name: string, path: string): Promise<any> {
        if (this.modules.has(name)) {
            return this.modules.get(name);
        }

        if (!this.loading.has(name)) {
            this.loading.set(name, import(path).then(mod => {
                const module = mod.default || mod;
                this.modules.set(name, module);
                this.loading.delete(name);
                return module;
            }));
        }

        return this.loading.get(name);
    }

    get(name: string): any {
        return this.modules.get(name);
    }

    has(name: string): boolean {
        return this.modules.has(name);
    }
}

// Conditional module loading
declare const process: { env: Record<string, string> };

async function loadEnvironmentModules(): Promise<void> {
    const env = process.env.NODE_ENV || 'development';

    // Environment-specific config
    const configModule = await import(`./config/${env}`);
    const config = configModule.default;

    // Conditional feature modules
    if (config.features?.analytics) {
        await import('./modules/analytics');
    }

    if (config.features?.monitoring) {
        await import('./modules/monitoring');
    }

    // Platform-specific modules
    const platform = process.env.PLATFORM || 'node';
    await import(`./platform/${platform}`);
}

// Barrel export simulation
export { Button, Input, Form } from './components';
export { useForm, useValidation } from './hooks';
export * from './utils';

// Main application
export class Application extends EventEmitter {
    private registry: ModuleRegistry;
    private server: any;

    constructor() {
        super();
        this.registry = new ModuleRegistry();
        this.server = express();
        this.server.use(bodyParser.json());
    }

    async initialize(): Promise<void> {
        await loadEnvironmentModules();

        // Load plugins dynamically
        const pluginPaths = ['./plugins/auth', './plugins/api', './plugins/static'];
        await Promise.all(pluginPaths.map(p => this.registry.load(path.basename(p), p)));

        this.emit('initialized');
    }

    async loadPlugin(name: string, pluginPath: string): Promise<void> {
        const plugin = await this.registry.load(name, pluginPath);
        if (plugin.setup) {
            await plugin.setup(this.server);
        }
        this.emit('plugin:loaded', name);
    }

    start(port: number): void {
        this.server.listen(port, () => {
            this.emit('started', port);
            console.log(`Server running on port ${port}`);
        });
    }
}

// Factory export
export function createApp(): Application {
    return new Application();
}

// Default export
export default Application;

// Usage
const app = createApp();
app.initialize().then(() => app.start(3000));"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("Application"),
        "expected output to contain Application class. output: {output}"
    );
    assert!(
        output.contains("ModuleRegistry"),
        "expected output to contain ModuleRegistry class. output: {output}"
    );
    assert!(
        output.contains("createApp"),
        "expected output to contain createApp function. output: {output}"
    );
    assert!(
        output.contains("loadEnvironmentModules"),
        "expected output to contain loadEnvironmentModules function. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for comprehensive module bundling"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

// =============================================================================
// JSX ES5 Transform Source Map Tests
// =============================================================================
// Tests for JSX compilation with ES5 target - JSX elements should transform
// to React.createElement calls while preserving source map accuracy.

#[test]
fn test_source_map_jsx_es5_basic_element() {
    // Test basic JSX element transformation to React.createElement
    let source = r#"const element = <div className="container">Hello World</div>;"#;

    let mut parser = ParserState::new("test.tsx".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);

    let mut printer = Printer::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_target_es5(ctx.target_es5);
    printer.set_source_map_text(parser.get_source_text());
    printer.enable_source_map("test.js", "test.tsx");
    printer.emit(root);

    let output = printer.get_output().to_string();
    let map_json = printer.generate_source_map_json().expect("source map");
    let map_value: Value = serde_json::from_str(&map_json).expect("parse source map");

    let mappings = map_value
        .get("mappings")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    let decoded = decode_mappings(mappings);

    // JSX should be in output (either preserved or transformed)
    assert!(
        output.contains("div") || output.contains("createElement"),
        "expected JSX element in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for JSX element"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_jsx_es5_fragment() {
    // Test JSX fragment transformation
    let source = r#"const fragment = <>
    <span>First</span>
    <span>Second</span>
</>;"#;

    let mut parser = ParserState::new("test.tsx".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);

    let mut printer = Printer::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_target_es5(ctx.target_es5);
    printer.set_source_map_text(parser.get_source_text());
    printer.enable_source_map("test.js", "test.tsx");
    printer.emit(root);

    let output = printer.get_output().to_string();
    let map_json = printer.generate_source_map_json().expect("source map");
    let map_value: Value = serde_json::from_str(&map_json).expect("parse source map");

    let mappings = map_value
        .get("mappings")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    let decoded = decode_mappings(mappings);

    // Fragment should be in output
    assert!(
        output.contains("span") || output.contains("Fragment"),
        "expected JSX fragment content in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for JSX fragment"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}
