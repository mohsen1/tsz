use tsz_common::ScriptTarget;
use tsz_scanner::SyntaxKind;
use tsz_scanner::scanner_impl::{ScannerState, TokenFlags};

// =============================================================================
// Helper: collect all tokens from source
// =============================================================================

fn scan_all_tokens(source: &str) -> Vec<SyntaxKind> {
    let mut scanner = ScannerState::new(source.to_string(), true);
    let mut tokens = Vec::new();
    loop {
        let token = scanner.scan();
        tokens.push(token);
        if token == SyntaxKind::EndOfFileToken {
            break;
        }
    }
    tokens
}

/// Scan a single token from source and return (kind, value)
fn scan_single(source: &str) -> (SyntaxKind, String) {
    let mut scanner = ScannerState::new(source.to_string(), true);
    let kind = scanner.scan();
    let value = scanner.get_token_value();
    (kind, value)
}

// =============================================================================
// 1. String Scanning
// =============================================================================

include!("scanner_comprehensive_tests_parts/part_00.rs");
include!("scanner_comprehensive_tests_parts/part_01.rs");
