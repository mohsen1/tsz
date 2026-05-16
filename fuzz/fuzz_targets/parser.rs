#![no_main]

use libfuzzer_sys::fuzz_target;
use tsz_parser::ParserState;

fuzz_target!(|data: &[u8]| {
    let Ok(source) = std::str::from_utf8(data) else {
        return;
    };

    if source.len() > 64 * 1024 {
        return;
    }

    let mut parser = ParserState::new("fuzz.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let _ = parser.get_arena().get(root);
    let _ = parser.get_diagnostics().len();
});
