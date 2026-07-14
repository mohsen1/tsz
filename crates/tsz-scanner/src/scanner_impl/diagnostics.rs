//! Scanner diagnostic plumbing and merge-conflict trivia scanning.
//!
//! Holds the `ScannerState` accessors and push helpers for `ScannerDiagnostic`
//! plus the merge-conflict marker trivia scanner, which emits a diagnostic and
//! skips past `<<<<<<<`/`=======`/`>>>>>>>`/`|||||||` runs.
use super::*;

impl ScannerState {
    /// Get the regex flag errors detected during scanning.
    #[must_use]
    pub fn get_regex_flag_errors(&self) -> &[RegexFlagError] {
        &self.regex_flag_errors
    }

    /// Get general scanner diagnostics (e.g., conflict marker errors).
    #[must_use]
    pub fn get_scanner_diagnostics(&self) -> &[ScannerDiagnostic] {
        &self.scanner_diagnostics
    }

    /// Clear accumulated scanner diagnostics. Used by `ParserState::reset` so a
    /// reused parser doesn't carry stale scanner-side errors into a new parse.
    /// `set_text` does NOT clear them — callers like the LSP that re-text the
    /// scanner across edits without going through `ParserState` may want the
    /// previous diagnostics to remain accessible.
    pub fn clear_scanner_diagnostics(&mut self) {
        self.scanner_diagnostics.clear();
    }

    /// Push a no-argument scanner diagnostic: the common `(pos, length, message,
    /// code)` form, routed through [`Self::push_diag_args`] with empty `args`.
    pub(crate) fn push_diag(
        &mut self,
        pos: usize,
        length: usize,
        message: &'static str,
        code: u32,
    ) {
        self.push_diag_args(pos, length, message, code, Vec::new());
    }

    /// Push a scanner diagnostic carrying message-template arguments; the single
    /// `ScannerDiagnostic` construction site for both `push_diag` forms.
    pub(crate) fn push_diag_args(
        &mut self,
        pos: usize,
        length: usize,
        message: &'static str,
        code: u32,
        args: Vec<String>,
    ) {
        self.scanner_diagnostics.push(ScannerDiagnostic {
            pos,
            length,
            message,
            code,
            args,
        });
    }

    /// Merge conflict marker length (7 characters: `<<<<<<<`, `=======`, etc.)
    const MERGE_CONFLICT_MARKER_LENGTH: usize = 7;

    /// Check if the current position is a merge conflict marker.
    /// A conflict marker must be at the start of a line, consist of 7 identical
    /// characters (`<`, `=`, `>`, or `|`), and for non-`=` markers, be followed
    /// by a space.
    pub(super) fn is_conflict_marker_trivia(&self) -> bool {
        let pos = self.pos;
        // Must be at start of line (pos == 0 or preceded by line break)
        if pos > 0 && !is_line_break(self.char_code_unchecked(pos - 1)) {
            return false;
        }
        // Must have room for 7 characters
        if pos + Self::MERGE_CONFLICT_MARKER_LENGTH >= self.end {
            return false;
        }
        let ch = self.char_code_unchecked(pos);
        // All 7 characters must be the same
        for i in 1..Self::MERGE_CONFLICT_MARKER_LENGTH {
            if self.char_code_unchecked(pos + i) != ch {
                return false;
            }
        }
        // For `=======`: no additional check needed
        // For `<<<<<<<`, `>>>>>>>`, `|||||||`: must be followed by a space
        ch == CharacterCodes::EQUALS
            || (pos + Self::MERGE_CONFLICT_MARKER_LENGTH < self.end
                && self.char_code_unchecked(pos + Self::MERGE_CONFLICT_MARKER_LENGTH)
                    == CharacterCodes::SPACE)
    }

    /// Scan past a conflict marker, emitting a TS1185 diagnostic.
    /// For `<` and `>` markers: skip to end of line.
    /// For `|` and `=` markers: skip until the next `=======` or `>>>>>>>` marker.
    pub(super) fn scan_conflict_marker_trivia(&mut self) {
        // Emit TS1185: "Merge conflict marker encountered."
        self.push_diag(
            self.pos,
            Self::MERGE_CONFLICT_MARKER_LENGTH,
            diagnostic_messages::MERGE_CONFLICT_MARKER_ENCOUNTERED,
            diagnostic_codes::MERGE_CONFLICT_MARKER_ENCOUNTERED,
        );

        let ch = self.char_code_unchecked(self.pos);
        if ch == CharacterCodes::LESS_THAN || ch == CharacterCodes::GREATER_THAN {
            // `<<<<<<<` or `>>>>>>>`: skip to end of line
            while self.pos < self.end && !is_line_break(self.char_code_unchecked(self.pos)) {
                self.pos += 1;
            }
        } else {
            // `|||||||` or `=======`: skip until next `=======` or `>>>>>>>` marker
            while self.pos < self.end {
                let current_char = self.char_code_unchecked(self.pos);
                if (current_char == CharacterCodes::EQUALS
                    || current_char == CharacterCodes::GREATER_THAN)
                    && current_char != ch
                    && self.is_conflict_marker_trivia()
                {
                    break;
                }
                self.pos += 1;
            }
        }
    }
}
