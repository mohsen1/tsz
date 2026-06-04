use super::Printer;

use crate::output::source_writer::{DelimiterKind, SourcePosition};

use crate::safe_slice;

use tsz_parser::parser::node::{Node, NodeAccess};

use tsz_parser::parser::{NodeIndex, NodeList, syntax_kind_ext};

use tsz_scanner::SyntaxKind;

fn starts_with_keyword_token(text: &str, keyword: &str) -> bool {
    text.strip_prefix(keyword).is_some_and(|tail| {
        tail.chars()
            .next()
            .is_none_or(|ch| !(ch == '_' || ch == '$' || ch.is_ascii_alphanumeric()))
    })
}

fn strip_keyword_token<'a>(text: &'a str, keyword: &str) -> Option<&'a str> {
    text.strip_prefix(keyword).and_then(|tail| {
        tail.chars()
            .next()
            .is_none_or(|ch| !(ch == '_' || ch == '$' || ch.is_ascii_alphanumeric()))
            .then_some(tail)
    })
}

const fn is_identifier_continue(byte: u8) -> bool {
    byte == b'_' || byte == b'$' || byte.is_ascii_alphanumeric()
}

fn previous_identifier_token(text: &str, mut end: usize) -> Option<(&str, usize)> {
    let bytes = text.as_bytes();
    while end > 0 && matches!(bytes[end - 1], b' ' | b'\t' | b'\r' | b'\n') {
        end -= 1;
    }
    let token_end = end;
    while end > 0 && is_identifier_continue(bytes[end - 1]) {
        end -= 1;
    }
    (end < token_end).then(|| (&text[end..token_end], end))
}

include!("helpers_parts/part1.rs");
include!("helpers_parts/part2.rs");
