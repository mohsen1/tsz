use super::*;
use tsz_common::common::ScriptTarget;
use tsz_parser::parser::ParserState;

#[test]
fn test_safe_slice_basic() {
    let s = "hello world";
    assert_eq!(safe_slice::slice(s, 0, 5), "hello");
    assert_eq!(safe_slice::slice(s, 6, 11), "world");
}

#[test]
fn test_safe_slice_empty() {
    let s = "hello";
    assert_eq!(safe_slice::slice(s, 10, 20), "");
    assert_eq!(safe_slice::slice(s, 5, 3), "");
}

#[test]
fn test_safe_slice_unicode() {
    let s = "hello 🦀 world";
    // The crab emoji is 4 bytes
    let crab_start = 6;
    let crab_end = 10;

    // Safe slice should work with valid boundaries
    assert_eq!(safe_slice::slice(s, 0, crab_start), "hello ");
    assert_eq!(safe_slice::slice(s, crab_end + 1, s.len()), "world");

    // Invalid boundary should return empty
    assert_eq!(safe_slice::slice(s, 7, 9), ""); // Mid-emoji
}

#[test]
fn test_safe_slice_from_to() {
    let s = "hello";
    assert_eq!(safe_slice::slice_from(s, 2), "llo");
    assert_eq!(safe_slice::slice_to(s, 3), "hel");
    assert_eq!(safe_slice::slice_from(s, 10), "");
}

#[test]
fn test_char_at() {
    let s = "hello 🦀";
    assert_eq!(safe_slice::char_at(s, 0), Some('h'));
    assert_eq!(safe_slice::char_at(s, 6), Some('🦀'));
    assert_eq!(safe_slice::char_at(s, 100), None);
}

#[test]
fn test_byte_at() {
    let s = "hello";
    assert_eq!(safe_slice::byte_at(s, 0), Some(b'h'));
    assert_eq!(safe_slice::byte_at(s, 4), Some(b'o'));
    assert_eq!(safe_slice::byte_at(s, 10), None);
}

#[test]
fn test_print_options() {
    let opts = PrintOptions::es5();
    assert!(matches!(opts.target, ScriptTarget::ES5));

    let opts = PrintOptions::commonjs();
    assert!(matches!(opts.module, ModuleKind::CommonJS));

    let opts = PrintOptions::es5_commonjs();
    assert!(matches!(opts.target, ScriptTarget::ES5));
    assert!(matches!(opts.module, ModuleKind::CommonJS));
}

#[test]
fn test_streaming_writer() {
    let mut output = Vec::new();
    {
        let mut printer = StreamingPrinter::new(&mut output);
        printer
            .write("hello")
            .expect("writing to Vec<u8> should not fail");
        printer
            .write(" ")
            .expect("writing to Vec<u8> should not fail");
        printer
            .write("world")
            .expect("writing to Vec<u8> should not fail");
        printer
            .flush()
            .expect("flushing to Vec<u8> should not fail");
    }
    assert_eq!(
        String::from_utf8(output).expect("output should be valid UTF-8"),
        "hello world"
    );
}

#[test]
fn test_es6_generator_param_named_yield_keeps_identifier_text() {
    let source = "function* foo(a = yield, yield) {}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let output = lower_and_print(&parser.arena, root, PrintOptions::es6()).code;
    assert_eq!(output, "function* foo(a = yield, yield) { }\n");
}

#[test]
fn test_optional_catch_binding_downlevel_to_param() {
    let source = "try {\n} catch {\n}\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let output = lower_and_print(
        &parser.arena,
        root,
        PrintOptions {
            target: ScriptTarget::ES2018,
            ..Default::default()
        },
    )
    .code;
    assert!(output.contains("catch (_unused)"));

    let output_es2020 = lower_and_print(
        &parser.arena,
        root,
        PrintOptions {
            target: ScriptTarget::ES2020,
            ..Default::default()
        },
    )
    .code;
    assert!(!output_es2020.contains("catch (_unused)"));
}

#[test]
fn test_exponentiation_downlevel_to_math_pow() {
    let source = "const x = 2 ** 3;\nlet y = 2;\ny **= 3;\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let output = lower_and_print(
        &parser.arena,
        root,
        PrintOptions {
            target: ScriptTarget::ES2015,
            ..Default::default()
        },
    )
    .code;

    assert!(output.contains("Math.pow(2, 3)"));
    assert!(output.contains("y = Math.pow(y, 3)"));
}

#[test]
fn test_optional_call_downlevel_to_conditional() {
    let source = "const fn = () => 1;\nconst obj = { m() { return this; } };\nfn?.();\nobj?.m();\nobj.m?.();\nobj?.m?.();\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let output = lower_and_print(
        &parser.arena,
        root,
        PrintOptions {
            target: ScriptTarget::ES2019,
            ..Default::default()
        },
    )
    .code;
    assert!(output.contains("fn === null || fn === void 0 ? void 0 : fn()"));
    assert!(output.matches(".call(").count() >= 2);
    assert!(!output.contains("?.("));
}

#[test]
fn test_optional_call_es2020_syntax_preserved() {
    let source = "const fn = () => 1;\nconst obj = { m() { return this; } };\nfn?.();\nobj?.m();\nobj.m?.();\nobj?.m?.();\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let output = lower_and_print(
        &parser.arena,
        root,
        PrintOptions {
            target: ScriptTarget::ES2020,
            ..Default::default()
        },
    )
    .code;
    assert!(output.contains("fn?.()"));
    assert!(output.contains("obj?.m()"));
    assert!(output.contains("obj.m?.()"));
    assert!(output.contains("obj?.m?.()"));
    assert!(!output.contains("void 0"));
}

#[test]
fn test_optional_call_spread_downlevel_es5() {
    let source = "const fn = function (...args) { return args; };\nconst obj = { m(...args) { return args; } };\nfn?.(...[1], 2);\nobj?.m(...[1], 2);\nobj.m?.(...[1], 2);\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let output = lower_and_print(
        &parser.arena,
        root,
        PrintOptions {
            target: ScriptTarget::ES5,
            ..Default::default()
        },
    )
    .code;
    assert!(output.contains(".__spreadArray"));
    assert!(output.contains(".apply(void 0,"));
    assert!(output.contains(".call.apply"));
    assert!(!output.contains("?.("));
}

#[test]
fn test_for_await_of_target_es2018_preserved() {
    let source = "async function f() {\n    const iterable = [];\n    for await (const x of iterable) {\n        console.log(x);\n    }\n}\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let output = lower_and_print(
        &parser.arena,
        root,
        PrintOptions {
            target: ScriptTarget::ES2018,
            ..Default::default()
        },
    )
    .code;

    assert!(output.contains("for await (const x of iterable)"));
    assert!(!output.contains("__asyncValues"));
}

#[test]
fn test_for_await_of_target_es2017_downlevel_to_await() {
    let source = "const iterable = [];\nasync function f() {\n    for await (const x of iterable) {\n        console.log(x);\n    }\n}\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let output = lower_and_print(
        &parser.arena,
        root,
        PrintOptions {
            target: ScriptTarget::ES2017,
            ..Default::default()
        },
    )
    .code;

    assert!(output.contains("__asyncValues"));
    assert!(output.contains("for (var"));
    assert!(output.contains(".next()"));
    assert!(output.contains("await"));
    assert!(!output.contains("yield iterable_1.next()"));
}

#[test]
fn test_for_await_of_target_es2016_downlevel_to_yield() {
    let source = "const iterable = [];\nasync function f() {\n    for await (const x of iterable) {\n        console.log(x);\n    }\n}\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let output = lower_and_print(
        &parser.arena,
        root,
        PrintOptions {
            target: ScriptTarget::ES2016,
            ..Default::default()
        },
    )
    .code;

    assert!(output.contains("__awaiter"));
    assert!(output.contains("__asyncValues"));
    assert!(output.contains("yield"));
    assert!(output.contains(".next()"));
}

#[test]
fn test_nested_for_await_of_targets_nested_return_temps() {
    let source = "async function f() {\n    for await (const a of xs) {\n        for await (const b of ys) {\n            console.log(a, b);\n        }\n    }\n}\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let output = lower_and_print(
        &parser.arena,
        root,
        PrintOptions {
            target: ScriptTarget::ES2016,
            ..Default::default()
        },
    )
    .code;

    assert!(output.contains("for (var"));
    assert!(output.contains("__asyncValues"));
    assert!(output.contains("var e_1"));
    assert!(output.contains("e_2"));
    assert!(output.contains("_a ="));
}
