//! Tests for printer module

use crate::parser::ParserState;
use crate::printer::{
    PrintOptions, Printer, StreamingPrinter, lower_and_print, print_to_string,
    print_with_source_map, safe_slice,
};

// =============================================================================
// Basic Print Tests
// =============================================================================

#[test]
fn test_print_to_string_basic() {
    let source = "const x = 42;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let output = print_to_string(&parser.arena, root, PrintOptions::default());
    assert!(output.contains("const x = 42"), "Output: {}", output);
}

#[test]
fn test_print_to_string_function() {
    let source = "function add(a: number, b: number): number { return a + b; }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let output = print_to_string(&parser.arena, root, PrintOptions::default());
    assert!(output.contains("function add"), "Output: {}", output);
    assert!(output.contains("return a + b"), "Output: {}", output);
    // Type annotations should be stripped
    assert!(!output.contains(": number"), "Output: {}", output);
}

#[test]
fn test_print_to_string_class() {
    let source = "class Foo { constructor() {} }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let output = print_to_string(&parser.arena, root, PrintOptions::default());
    assert!(output.contains("class Foo"), "Output: {}", output);
    assert!(output.contains("constructor"), "Output: {}", output);
}

// =============================================================================
// ES5 Transform Tests
// =============================================================================

#[test]
fn test_lower_and_print_es5_class() {
    let source = "class Foo { constructor() {} }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let result = lower_and_print(&parser.arena, root, PrintOptions::es5());
    // ES5 output should use function instead of class
    assert!(
        result.code.contains("function Foo"),
        "Output: {}",
        result.code
    );
}

#[test]
fn test_lower_and_print_es5_arrow() {
    let source = "const fn = (x) => x * 2;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let result = lower_and_print(&parser.arena, root, PrintOptions::es5());
    // ES5 output should convert arrow to function
    assert!(result.code.contains("function"), "Output: {}", result.code);
    assert!(!result.code.contains("=>"), "Output: {}", result.code);
}

#[test]
fn test_lower_and_print_es5_let_const() {
    let source = "let x = 1; const y = 2;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let result = lower_and_print(&parser.arena, root, PrintOptions::es5());
    // ES5 output should use var
    assert!(result.code.contains("var x"), "Output: {}", result.code);
    assert!(result.code.contains("var y"), "Output: {}", result.code);
}

// =============================================================================
// Source Map Tests
// =============================================================================

#[test]
fn test_print_with_source_map() {
    let source = "const x = 1;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrintOptions::default();
    options.source_map = true;

    let result = print_with_source_map(&parser.arena, root, source, "test.ts", "test.js", options);

    assert!(result.code.contains("const x = 1"), "Code: {}", result.code);
    assert!(
        result.source_map.is_some(),
        "Source map should be generated"
    );

    let map = result.source_map.unwrap();
    assert!(map.contains("\"sources\""), "Map: {}", map);
    assert!(map.contains("\"mappings\""), "Map: {}", map);
}

// =============================================================================
// Printer Struct Tests
// =============================================================================

#[test]
fn test_printer_struct() {
    let source = "const x = 1;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut printer = Printer::new(&parser.arena, PrintOptions::default());
    printer.set_source_text(source);
    printer.print(root);

    let result = printer.finish();
    assert!(result.code.contains("const x = 1"), "Code: {}", result.code);
}

// =============================================================================
// Safe Slice Tests
// =============================================================================

#[test]
fn test_safe_slice_normal() {
    let s = "hello world";
    assert_eq!(safe_slice::slice(s, 0, 5), "hello");
    assert_eq!(safe_slice::slice(s, 6, 11), "world");
    assert_eq!(safe_slice::slice(s, 0, 11), "hello world");
}

#[test]
fn test_safe_slice_out_of_bounds() {
    let s = "hello";
    assert_eq!(safe_slice::slice(s, 0, 100), "");
    assert_eq!(safe_slice::slice(s, 100, 200), "");
    assert_eq!(safe_slice::slice(s, 5, 3), ""); // start > end
}

#[test]
fn test_safe_slice_unicode() {
    let s = "hello 🦀 world";
    let crab_start = 6;
    let crab_end = 10; // 🦀 is 4 bytes

    // Valid boundaries
    assert_eq!(safe_slice::slice(s, 0, crab_start), "hello ");
    assert_eq!(safe_slice::slice_from(s, crab_end + 1), "world");

    // Invalid boundaries (mid-emoji)
    assert_eq!(safe_slice::slice(s, 7, 9), "");
}

#[test]
fn test_safe_slice_from_to() {
    let s = "hello world";
    assert_eq!(safe_slice::slice_from(s, 6), "world");
    assert_eq!(safe_slice::slice_to(s, 5), "hello");
    assert_eq!(safe_slice::slice_from(s, 100), "");
}

#[test]
fn test_safe_char_at() {
    let s = "hello 🦀";
    assert_eq!(safe_slice::char_at(s, 0), Some('h'));
    assert_eq!(safe_slice::char_at(s, 5), Some(' '));
    assert_eq!(safe_slice::char_at(s, 6), Some('🦀'));
    assert_eq!(safe_slice::char_at(s, 100), None);
    // Mid-character position should return None
    assert_eq!(safe_slice::char_at(s, 7), None);
}

#[test]
fn test_safe_byte_at() {
    let s = "hello";
    assert_eq!(safe_slice::byte_at(s, 0), Some(b'h'));
    assert_eq!(safe_slice::byte_at(s, 4), Some(b'o'));
    assert_eq!(safe_slice::byte_at(s, 5), None);
}

#[test]
fn test_boundary_helpers() {
    let s = "hello 🦀 world";

    // next_boundary finds valid UTF-8 boundaries
    assert_eq!(safe_slice::next_boundary(s, 0), 0);
    assert_eq!(safe_slice::next_boundary(s, 6), 6); // Start of emoji
    assert_eq!(safe_slice::next_boundary(s, 7), 10); // Mid-emoji -> end of emoji
    assert_eq!(safe_slice::next_boundary(s, 100), s.len());

    // prev_boundary finds valid UTF-8 boundaries
    assert_eq!(safe_slice::prev_boundary(s, 10), 10); // End of emoji
    assert_eq!(safe_slice::prev_boundary(s, 9), 6); // Mid-emoji -> start of emoji
    assert_eq!(safe_slice::prev_boundary(s, 0), 0);
}

// =============================================================================
// Streaming Writer Tests
// =============================================================================

#[test]
fn test_streaming_printer_basic() {
    let mut output = Vec::new();
    {
        let mut printer = StreamingPrinter::new(&mut output);
        printer.write("hello").unwrap();
        printer.write_char(' ').unwrap();
        printer.write("world").unwrap();
        printer.flush().unwrap();
    }
    assert_eq!(String::from_utf8(output).unwrap(), "hello world");
}

#[test]
fn test_streaming_printer_large_write() {
    let mut output = Vec::new();
    let large_text = "x".repeat(10000);
    {
        let mut printer = StreamingPrinter::with_buffer_size(&mut output, 1024);
        printer.write(&large_text).unwrap();
        printer.flush().unwrap();
    }
    assert_eq!(output.len(), 10000);
}

#[test]
fn test_streaming_printer_newlines() {
    let mut output = Vec::new();
    {
        let mut printer = StreamingPrinter::new(&mut output);
        printer.write("line1").unwrap();
        printer.write_line().unwrap();
        printer.write("line2").unwrap();
        printer.flush().unwrap();
    }
    assert_eq!(String::from_utf8(output).unwrap(), "line1\nline2");
}

// =============================================================================
// Print Options Tests
// =============================================================================

#[test]
fn test_print_options_es5() {
    let opts = PrintOptions::es5();
    assert!(matches!(
        opts.target,
        crate::emitter::ScriptTarget::ES5
    ));
}

#[test]
fn test_print_options_es6() {
    let opts = PrintOptions::es6();
    assert!(matches!(
        opts.target,
        crate::emitter::ScriptTarget::ES2015
    ));
}

#[test]
fn test_print_options_commonjs() {
    let opts = PrintOptions::commonjs();
    assert!(matches!(
        opts.module,
        crate::emitter::ModuleKind::CommonJS
    ));
}

#[test]
fn test_print_options_es5_commonjs() {
    let opts = PrintOptions::es5_commonjs();
    assert!(matches!(
        opts.target,
        crate::emitter::ScriptTarget::ES5
    ));
    assert!(matches!(
        opts.module,
        crate::emitter::ModuleKind::CommonJS
    ));
}

// =============================================================================
// Integration Tests
// =============================================================================

#[test]
fn test_complete_workflow() {
    let source = r#"
class Calculator {
    add(a: number, b: number): number {
        return a + b;
    }
}
const calc = new Calculator();
"#;

    let mut parser = ParserState::new("calc.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    // Test ES6+ output
    let es6_output = print_to_string(&parser.arena, root, PrintOptions::default());
    assert!(
        es6_output.contains("class Calculator"),
        "ES6: {}",
        es6_output
    );
    assert!(es6_output.contains("add(a, b)"), "ES6: {}", es6_output);

    // Test ES5 output
    let es5_result = lower_and_print(&parser.arena, root, PrintOptions::es5());
    assert!(
        es5_result.code.contains("function Calculator"),
        "ES5: {}",
        es5_result.code
    );
}

#[test]
fn test_emit_produces_valid_javascript() {
    // Simple variable declaration
    {
        let source = "let x = 1;";
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let output = print_to_string(&parser.arena, root, PrintOptions::default());
        assert!(output.contains("let x = 1"), "Output: {}", output);
    }

    // Function with types stripped
    {
        let source = "function greet(name: string): string { return 'Hello, ' + name; }";
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let output = print_to_string(&parser.arena, root, PrintOptions::default());
        assert!(
            output.contains("function greet(name)"),
            "Output: {}",
            output
        );
        assert!(
            !output.contains(": string"),
            "Types should be stripped: {}",
            output
        );
    }

    // Class
    {
        let source = "class Foo { x: number = 1; }";
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let output = print_to_string(&parser.arena, root, PrintOptions::default());
        assert!(output.contains("class Foo"), "Output: {}", output);
    }
}
