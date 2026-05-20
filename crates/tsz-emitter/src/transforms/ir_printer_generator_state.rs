//! Generator state-name helpers for the IR printer.

use super::IRPrinter;

impl IRPrinter<'_> {
    pub fn generator_state_name_for_hoisted<T: AsRef<str>>(hoisted_vars: &[T]) -> &'static str {
        const TEMP_NAMES: [&str; 26] = [
            "_a", "_b", "_c", "_d", "_e", "_f", "_g", "_h", "_i", "_j", "_k", "_l", "_m", "_n",
            "_o", "_p", "_q", "_r", "_s", "_t", "_u", "_v", "_w", "_x", "_y", "_z",
        ];
        const RESERVED_TEMP_INDEXES: [usize; 2] = [8, 13];

        let max_hoisted_temp = hoisted_vars
            .iter()
            .filter_map(|name| {
                let name = name.as_ref();
                // `_i` and `_n` are dedicated `TempFlags` names in `tsc`'s
                // temp allocator. They can appear in lowered loops, but they
                // should not influence the ordinary generator-state name.
                if name == "_i" || name == "_n" {
                    return None;
                }
                TEMP_NAMES.iter().position(|temp| *temp == name)
            })
            .max();
        let mut next_index = max_hoisted_temp.map_or(0, |idx| idx + 1);
        while RESERVED_TEMP_INDEXES.contains(&next_index) && next_index + 1 < TEMP_NAMES.len() {
            next_index += 1;
        }
        TEMP_NAMES[next_index.min(TEMP_NAMES.len() - 1)]
    }

    pub(super) fn rename_colliding_outer_generator_state(
        output: &str,
        generator_this: &str,
    ) -> String {
        let Some(generator_start) = output.find("__generator(") else {
            return output.to_string();
        };
        let Some(relative_fn_start) = output[generator_start..].find("function (") else {
            return output.to_string();
        };
        let fn_start = generator_start + relative_fn_start;
        let state_start = fn_start + "function (".len();
        let Some(relative_state_end) = output[state_start..].find(')') else {
            return output.to_string();
        };
        let state_end = state_start + relative_state_end;
        let state_name = &output[state_start..state_end];
        if state_name != generator_this {
            return output.to_string();
        }

        let new_state = Self::next_generator_state_name(state_name);
        let Some(relative_body_open) = output[state_end..].find('{') else {
            return output.to_string();
        };
        let body_open = state_end + relative_body_open;
        let Some(body_close) = Self::matching_brace(output, body_open) else {
            return output.to_string();
        };

        let nested_ranges = Self::nested_function_ranges(output, body_open + 1, body_close);
        let mut rewritten = String::with_capacity(output.len());
        rewritten.push_str(&output[..state_start]);
        rewritten.push_str(new_state);
        let mut cursor = state_end;
        let mut i = body_open + 1;

        for (nested_start, nested_end) in nested_ranges {
            while i < nested_start {
                if Self::state_property_at(output, i, state_name) {
                    rewritten.push_str(&output[cursor..i]);
                    rewritten.push_str(new_state);
                    cursor = i + state_name.len();
                    i = cursor;
                } else {
                    i += 1;
                }
            }
            i = nested_end;
        }

        while i < body_close {
            if Self::state_property_at(output, i, state_name) {
                rewritten.push_str(&output[cursor..i]);
                rewritten.push_str(new_state);
                cursor = i + state_name.len();
                i = cursor;
            } else {
                i += 1;
            }
        }

        rewritten.push_str(&output[cursor..]);
        rewritten
    }

    fn next_generator_state_name(current: &str) -> &'static str {
        const TEMP_NAMES: [&str; 26] = [
            "_a", "_b", "_c", "_d", "_e", "_f", "_g", "_h", "_i", "_j", "_k", "_l", "_m", "_n",
            "_o", "_p", "_q", "_r", "_s", "_t", "_u", "_v", "_w", "_x", "_y", "_z",
        ];
        TEMP_NAMES
            .iter()
            .copied()
            .find(|name| *name != current)
            .unwrap_or("_a")
    }

    fn state_property_at(output: &str, idx: usize, state_name: &str) -> bool {
        let Some(rest) = output.get(idx..) else {
            return false;
        };
        if !rest.starts_with(state_name) {
            return false;
        }
        let next = idx + state_name.len();
        output.as_bytes().get(next) == Some(&b'.')
            && idx
                .checked_sub(1)
                .and_then(|prev| output.as_bytes().get(prev))
                .is_none_or(|byte| !byte.is_ascii_alphanumeric() && *byte != b'_')
    }

    fn nested_function_ranges(output: &str, start: usize, end: usize) -> Vec<(usize, usize)> {
        let mut ranges = Vec::new();
        let mut i = start;
        while i < end {
            let Some(rest) = output.get(i..) else {
                break;
            };
            if rest.starts_with("function (") || rest.starts_with("function(") {
                let Some(relative_open) = rest.find('{') else {
                    break;
                };
                let open = i + relative_open;
                if open >= end {
                    break;
                }
                if let Some(close) = Self::matching_brace(output, open) {
                    let nested_end = (close + 1).min(end);
                    ranges.push((i, nested_end));
                    i = nested_end;
                    continue;
                }
            }
            i += 1;
        }
        ranges
    }

    fn matching_brace(output: &str, open: usize) -> Option<usize> {
        let bytes = output.as_bytes();
        if bytes.get(open) != Some(&b'{') {
            return None;
        }
        let mut depth = 0usize;
        for (idx, byte) in bytes.iter().enumerate().skip(open) {
            match byte {
                b'{' => depth += 1,
                b'}' => {
                    depth = depth.saturating_sub(1);
                    if depth == 0 {
                        return Some(idx);
                    }
                }
                _ => {}
            }
        }
        None
    }
}
