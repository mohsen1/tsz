use super::*;
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
