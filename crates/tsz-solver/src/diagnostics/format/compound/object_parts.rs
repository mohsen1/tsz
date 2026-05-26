//! Object-like part formatting helpers.

use super::super::TypeFormatter;

impl<'a> TypeFormatter<'a> {
    fn collapse_truncated_tail_part(part: &str) -> String {
        let Some((prefix, ty)) = part.split_once(": ") else {
            return part.to_string();
        };
        if ty.starts_with('{') {
            return format!("{prefix}: {{ ...; }}");
        }
        part.to_string()
    }

    /// Format object-like parts with tsc-style long-type truncation:
    /// keep a long prefix and the last member, inserting `... N more ...`.
    ///
    /// This is used for both plain object literals and object-with-index displays.
    /// tsc starts truncating only on larger member counts (roughly 22+), and
    /// preserves the tail member (often useful symbol members such as
    /// `[Symbol.unscopables]`).
    pub(super) fn format_object_parts(&self, parts: Vec<String>) -> String {
        if parts.is_empty() {
            return "{}".to_string();
        }

        // Match tsc's higher truncation threshold (small/medium objects display fully).
        const TRUNCATE_THRESHOLD: usize = 22;
        if parts.len() < TRUNCATE_THRESHOLD {
            return format!("{{ {}; }}", parts.join("; "));
        }

        let is_string_apparent_member_list = parts
            .first()
            .is_some_and(|part| part.starts_with("toString:"))
            && parts
                .iter()
                .any(|part| part.starts_with("[Symbol.iterator]"));
        // Keep at most this many leading members before the omitted-count marker.
        let max_head_parts = if is_string_apparent_member_list {
            1
        } else {
            17
        };
        // Soft budget for head text. Long member signatures (for example,
        // `toLocaleString` overloads) reduce the number of retained heads.
        const MAX_HEAD_CHARS: usize = 380;

        let total = parts.len();
        let tail_index = parts
            .iter()
            .rposition(|part| part.starts_with("[Symbol.") || part.starts_with("readonly [Symbol."))
            .filter(|&idx| idx > 0)
            .unwrap_or(total - 1);
        let tail = Self::collapse_truncated_tail_part(&parts[tail_index]);
        let max_head_chars = if tail_index == total - 1 {
            MAX_HEAD_CHARS
        } else {
            255
        };
        let mut head_count = 0usize;
        let mut used_chars = 0usize;

        for (idx, part) in parts.iter().enumerate().take(tail_index) {
            if head_count >= max_head_parts {
                break;
            }
            let part_cost = if head_count == 0 {
                part.len()
            } else {
                // "; " separator
                part.len() + 2
            };
            let next_used = used_chars + part_cost;
            let remaining_after = total - (idx + 1) - 1; // tail excluded
            let omitted_digits = remaining_after.max(1).to_string().len();
            // Reserve space for `; ... N more ...; <tail>`
            let reserve_for_marker = 2 + 4 + omitted_digits + 9;
            let reserve_for_tail = 2 + tail.len();

            // Keep at least two head parts when available; after that, enforce budget.
            if head_count >= 2 && next_used + reserve_for_marker + reserve_for_tail > max_head_chars
            {
                break;
            }

            used_chars = next_used;
            head_count += 1;
        }

        // Ensure progress even with extremely long first members.
        if head_count == 0 {
            head_count = 1;
        }

        let omitted = total.saturating_sub(head_count + 1);
        if omitted == 0 {
            return format!("{{ {}; }}", parts.join("; "));
        }

        let mut display_parts = Vec::with_capacity(head_count + 2);
        display_parts.extend(parts.iter().take(head_count).cloned());
        display_parts.push(format!("... {omitted} more ..."));
        display_parts.push(tail);
        format!("{{ {}; }}", display_parts.join("; "))
    }
}
