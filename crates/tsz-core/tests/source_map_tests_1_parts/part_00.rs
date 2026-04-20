#[test]
fn test_vlq_encode_positive() {
    // Simple positive numbers
    assert_eq!(vlq::encode(0), "A");
    assert_eq!(vlq::encode(1), "C");
    assert_eq!(vlq::encode(15), "e");
    assert_eq!(vlq::encode(16), "gB");
}

#[test]
fn test_vlq_encode_negative() {
    // Negative numbers (sign in LSB)
    assert_eq!(vlq::encode(-1), "D");
    assert_eq!(vlq::encode(-15), "f");
}

#[test]
fn test_vlq_decode() {
    // Decode what we encode
    for value in [-100, -1, 0, 1, 100, 1000] {
        let encoded = vlq::encode(value);
        let (decoded, consumed) = vlq::decode(&encoded).unwrap();
        assert_eq!(decoded, value, "Failed for value {value}");
        assert_eq!(consumed, encoded.len());
    }
}

#[test]
fn test_simple_map_generic() {
    // Minimal test with just Map generic - checking for infinite loops
    let source = r#"const metadata = new Map<any, Map<string, any>>();"#;

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
    assert!(
        output.contains("Map"),
        "expected output to contain Map. output: {output}"
    );
}

#[test]
fn test_parameter_decorator_simple() {
    // Test with just parameter decorator on class method
    let source = r#"function inject(token: string) {
    return function(target: any, key: string, index: number) {};
}

class ApiController {
    getUsers(@inject("db") db: any) {
        return [];
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
    assert!(
        output.contains("ApiController"),
        "expected output to contain ApiController. output: {output}"
    );
}

#[test]
fn test_class_and_method_decorator() {
    // Test with class and method decorators
    let source = r#"function controller(path: string) {
    return function(target: any) { return target; };
}

function get(route: string) {
    return function(target: any, key: string, desc: PropertyDescriptor) { return desc; };
}

@controller("/api")
class ApiController {
    @get("/users")
    getUsers() {
        return [];
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
    assert!(
        output.contains("ApiController"),
        "expected output to contain ApiController. output: {output}"
    );
}

#[test]
fn test_full_decorator_combo() {
    // Test with class, method, and parameter decorators
    let source = r#"function controller(path: string) {
    return function(target: any) { return target; };
}

function get(route: string) {
    return function(target: any, key: string, desc: PropertyDescriptor) { return desc; };
}

function inject(token: string) {
    return function(target: any, key: string, index: number) {};
}

@controller("/api")
class ApiController {
    @get("/users")
    getUsers(@inject("db") db: any) {
        return [];
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
    assert!(
        output.contains("ApiController"),
        "expected output to contain ApiController. output: {output}"
    );
}

#[test]
fn test_full_decorator_combo_with_prop() {
    // Test with class, method, property, and parameter decorators (matching the ignored test)
    let source = r#"function controller(path: string) {
    return function(target: any) { return target; };
}

function get(route: string) {
    return function(target: any, key: string, desc: PropertyDescriptor) { return desc; };
}

function inject(token: string) {
    return function(target: any, key: string, index: number) {};
}

function prop(target: any, key: string) {}

@controller("/api")
class ApiController {
    @prop
    service: any;

    @get("/users")
    getUsers(@inject("db") db: any) {
        return [];
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
    assert!(
        output.contains("ApiController"),
        "expected output to contain ApiController. output: {output}"
    );
}

#[test]
fn test_two_methods_no_param_decorators() {
    // Test with just two methods - no parameter decorators
    let source = r#"function get(route: string) {
    return function(target: any, key: string, desc: PropertyDescriptor) { return desc; };
}

class ApiController {
    @get("/users")
    getUsers() {
        return [];
    }

    @get("/posts")
    getPosts() {
        return [];
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
    assert!(
        output.contains("ApiController"),
        "expected output to contain ApiController. output: {output}"
    );
}

#[test]
fn test_two_methods_with_param_decorators() {
    // Test with two methods and parameter decorators - the suspected issue
    let source = r#"function get(route: string) {
    return function(target: any, key: string, desc: PropertyDescriptor) { return desc; };
}

function inject(token: string) {
    return function(target: any, key: string, index: number) {};
}

class ApiController {
    @get("/users")
    getUsers(@inject("db") db: any) {
        return [];
    }

    @get("/posts")
    getPosts(@inject("db") db: any) {
        return [];
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
    assert!(
        output.contains("ApiController"),
        "expected output to contain ApiController. output: {output}"
    );
}

