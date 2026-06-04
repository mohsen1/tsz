use super::{Server, TsServerRequest, TsServerResponse};

use tsz::lsp::position::LineMap;

include!("handlers_editing_parts/part1.rs");
include!("handlers_editing_parts/part2.rs");

/// Forward-scan `bytes[..end]` and collect byte ranges that should be ignored
/// when matching JSX angle brackets — namely string/template literals and
/// `//` / `/* ... */` comments. The returned ranges are half-open `[start, end)`
/// and include the surrounding quotes/comment delimiters.
///
/// This is intentionally a lightweight tokenizer rather than a full JSX
/// scanner: it is sufficient to keep `<` and `>` inside attribute strings
/// (and JSX-expression strings) from being treated as tag boundaries.
pub(crate) fn collect_skip_ranges(bytes: &[u8], end: usize) -> Vec<(usize, usize)> {
    let mut ranges: Vec<(usize, usize)> = Vec::new();
    let limit = end.min(bytes.len());
    let mut j = 0;
    while j < limit {
        match bytes[j] {
            quote @ (b'"' | b'\'' | b'`') => {
                let start = j;
                j += 1;
                while j < limit && bytes[j] != quote {
                    if bytes[j] == b'\\' && j + 1 < limit {
                        j += 2;
                    } else if bytes[j] == b'\n' && quote != b'`' {
                        // Unterminated single/double-quoted string: stop at
                        // newline so a stray quote does not swallow the rest
                        // of the file.
                        break;
                    } else {
                        j += 1;
                    }
                }
                if j < limit {
                    j += 1; // consume closing quote (or stop byte)
                }
                ranges.push((start, j));
            }
            b'/' if j + 1 < limit && bytes[j + 1] == b'/' => {
                let start = j;
                j += 2;
                while j < limit && bytes[j] != b'\n' {
                    j += 1;
                }
                ranges.push((start, j));
            }
            b'/' if j + 1 < limit && bytes[j + 1] == b'*' => {
                let start = j;
                j += 2;
                while j + 1 < limit && !(bytes[j] == b'*' && bytes[j + 1] == b'/') {
                    j += 1;
                }
                if j + 1 < limit {
                    j += 2; // consume closing */
                } else {
                    j = limit;
                }
                ranges.push((start, j));
            }
            _ => j += 1,
        }
    }
    ranges
}
