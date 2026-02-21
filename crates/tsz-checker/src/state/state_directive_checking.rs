//! Source-file directive checking: triple-slash references and AMD module names.

use crate::error_handler::ErrorHandler;
use crate::state::CheckerState;

impl<'a> CheckerState<'a> {
    /// Check triple-slash reference directives and emit TS6053 for missing files.
    ///
    /// Validates `/// <reference path="..." />` directives in TypeScript source files.
    /// If a referenced file doesn't exist, emits error 6053.
    pub(crate) fn check_triple_slash_references(&mut self, file_name: &str, source_text: &str) {
        use crate::triple_slash_validator::{extract_reference_paths, validate_reference_path};
        use std::collections::HashSet;
        use std::path::Path;

        let references = extract_reference_paths(source_text);
        if references.is_empty() {
            return;
        }

        let source_path = Path::new(file_name);

        let mut known_files: HashSet<String> = HashSet::new();
        if let Some(arenas) = self.ctx.all_arenas.as_ref() {
            for arena in arenas.iter() {
                for source_file in &arena.source_files {
                    known_files.insert(source_file.file_name.clone());
                }
            }
        } else {
            for source_file in &self.ctx.arena.source_files {
                known_files.insert(source_file.file_name.clone());
            }
        }

        let has_virtual_reference = |reference_path: &str| {
            let base = source_path.parent().unwrap_or_else(|| Path::new(""));
            if validate_reference_path(source_path, reference_path) {
                return true;
            }

            let direct_candidate = base.join(reference_path);
            if known_files.contains(direct_candidate.to_string_lossy().as_ref()) {
                return true;
            }

            if !reference_path.contains('.') {
                for ext in [".ts", ".tsx", ".d.ts"] {
                    let candidate = base.join(format!("{reference_path}{ext}"));
                    if known_files.contains(candidate.to_string_lossy().as_ref()) {
                        return true;
                    }
                }
            }
            false
        };

        for (reference_path, line_num, quote_offset) in references {
            if !has_virtual_reference(&reference_path) {
                // Calculate byte offset to the start of this line
                let mut line_start = 0u32;
                for (idx, line) in source_text.lines().enumerate() {
                    if idx == line_num {
                        break;
                    }
                    // +1 for the newline character
                    line_start += line.len() as u32 + 1;
                }

                // Point at the path value (after the opening quote)
                let pos = line_start + quote_offset as u32;
                // Span covers just the path value (not the quotes)
                let length = reference_path.len() as u32;

                // Resolve the reference path relative to the source file's directory,
                // matching tsc behavior which reports absolute/resolved paths.
                let resolved = source_path
                    .parent()
                    .unwrap_or_else(|| Path::new(""))
                    .join(&reference_path);
                let display_path = resolved.to_string_lossy();

                use crate::diagnostics::{diagnostic_codes, format_message};
                let message = format_message("File '{0}' not found.", &[&display_path]);
                self.emit_error_at(pos, length, &message, diagnostic_codes::FILE_NOT_FOUND);
            }
        }
    }

    /// Check for duplicate AMD module name assignments.
    ///
    /// Validates `/// <amd-module name="..." />` directives in TypeScript source files.
    /// If multiple AMD module name assignments are found, emits error TS2458.
    pub(crate) fn check_amd_module_names(&mut self, source_text: &str) {
        use crate::triple_slash_validator::extract_amd_module_names;

        let amd_modules = extract_amd_module_names(source_text);

        // Only emit error if there are multiple AMD module name assignments
        if amd_modules.len() <= 1 {
            return;
        }

        // Emit TS2458 error at the position of the second (and subsequent) directive(s)
        for (_, line_num) in amd_modules.iter().skip(1) {
            // Calculate the position of the error (start of the line)
            let mut pos = 0u32;
            for (idx, _) in source_text.lines().enumerate() {
                if idx == *line_num {
                    break;
                }
                pos += source_text.lines().nth(idx).map_or(0, |l| l.len() + 1) as u32;
            }

            // Find the actual directive on the line to get accurate position
            if let Some(line) = source_text.lines().nth(*line_num)
                && let Some(directive_start) = line.find("///")
            {
                pos += directive_start as u32;
            }

            let length = source_text
                .lines()
                .nth(*line_num)
                .map_or(0, |l| l.len() as u32);

            use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
            self.emit_error_at(
                pos,
                length,
                diagnostic_messages::AN_AMD_MODULE_CANNOT_HAVE_MULTIPLE_NAME_ASSIGNMENTS,
                diagnostic_codes::AN_AMD_MODULE_CANNOT_HAVE_MULTIPLE_NAME_ASSIGNMENTS,
            );
        }
    }
}
