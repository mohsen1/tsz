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

