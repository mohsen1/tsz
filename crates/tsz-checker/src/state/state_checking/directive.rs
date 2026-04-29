//! Source-file directive checking: triple-slash references and AMD module names.

use crate::state::CheckerState;

impl<'a> CheckerState<'a> {
    /// Check triple-slash reference directives and emit TS6053 for missing files.
    ///
    /// Validates `/// <reference path="..." />` directives in TypeScript source files.
    /// If a referenced file doesn't exist, emits error 6053.
    /// Also emits TS1084 for malformed reference directive syntax.
    pub(crate) fn check_triple_slash_references(&mut self, file_name: &str, source_text: &str) {
        use crate::triple_slash_validator::{
            extract_reference_paths, find_malformed_reference_directives, validate_reference_path,
        };
        use std::collections::HashSet;
        use std::path::Path;

        // Check for malformed reference directive syntax (TS1084)
        let malformed = find_malformed_reference_directives(source_text);
        for (line_num, byte_offset) in &malformed {
            let line_length = source_text
                .lines()
                .nth(*line_num)
                .map_or(0, |l| l.trim().len() as u32);
            use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
            self.emit_error_at(
                *byte_offset as u32,
                line_length,
                diagnostic_messages::INVALID_REFERENCE_DIRECTIVE_SYNTAX,
                diagnostic_codes::INVALID_REFERENCE_DIRECTIVE_SYNTAX,
            );
        }

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

            // Try adding extensions if the filename part doesn't already have one.
            // Check the filename (after last /) for a dot, not the whole path,
            // since paths like "./idx" contain dots in directory components.
            let file_name_part = Path::new(reference_path)
                .file_name()
                .and_then(|f| f.to_str())
                .unwrap_or(reference_path);
            if !file_name_part.contains('.') {
                for ext in [".ts", ".tsx", ".d.ts"] {
                    let candidate = base.join(format!("{reference_path}{ext}"));
                    if known_files.contains(candidate.to_string_lossy().as_ref()) {
                        return true;
                    }
                }
            }
            false
        };

        let unresolved_extensions: Vec<&'static str> =
            if self.ctx.compiler_options.allow_js || self.ctx.is_js_file() {
                vec![
                    ".ts", ".tsx", ".d.ts", ".js", ".jsx", ".cts", ".d.cts", ".cjs", ".mts",
                    ".d.mts", ".mjs",
                ]
            } else {
                vec![".ts", ".tsx", ".d.ts", ".cts", ".d.cts", ".mts", ".d.mts"]
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

                use crate::diagnostics::{diagnostic_codes, format_message};
                let file_name_part = Path::new(&reference_path)
                    .file_name()
                    .and_then(|f| f.to_str())
                    .unwrap_or(reference_path.as_str());
                if !file_name_part.contains('.') {
                    let extensions = unresolved_extensions.join("', '");
                    let message = format_message(
                        "Could not resolve the path '{0}' with the extensions: '{1}'.",
                        &[&reference_path, &extensions],
                    );
                    self.emit_error_at(
                        pos,
                        length,
                        &message,
                        diagnostic_codes::COULD_NOT_RESOLVE_THE_PATH_WITH_THE_EXTENSIONS,
                    );
                    continue;
                }

                // Resolve the reference path relative to the source file's directory,
                // matching tsc behavior which reports absolute/resolved paths.
                // tsc normalizes Windows-style backslashes before resolution, so
                // `../../../foo` from `..\..\..\foo` resolves correctly on Unix.
                let forward_slash_path = reference_path.replace('\\', "/");
                let resolved = source_path
                    .parent()
                    .unwrap_or_else(|| Path::new(""))
                    .join(&forward_slash_path);

                // Normalize the path to resolve . and .. components.
                // tsc reports normalized paths like "/tmp/file.ts" not "/tmp/dir/../file.ts".
                let display_path = normalize_path(&resolved);
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

/// Normalize a path by resolving `.` and `..` components without requiring the file to exist.
///
/// This matches tsc behavior which reports clean paths like `/tmp/file.ts`
/// instead of `/tmp/dir/../file.ts` or `/tmp/./file.ts`.
fn normalize_path(path: &std::path::Path) -> String {
    use std::path::Component;

    let mut normalized = Vec::new();

    for component in path.components() {
        match component {
            Component::CurDir => {
                // Skip `.` components
            }
            Component::ParentDir => {
                // For `..`, pop the last component if possible
                if normalized
                    .last()
                    .is_some_and(|c| matches!(c, Component::Normal(_) | Component::CurDir))
                {
                    normalized.pop();
                } else {
                    // Can't go up (already at root or start with ..), keep the ..
                    normalized.push(component);
                }
            }
            _ => {
                normalized.push(component);
            }
        }
    }

    // Reconstruct the path
    let mut result = std::path::PathBuf::new();
    for component in normalized {
        result.push(component);
    }

    result.to_string_lossy().into_owned()
}
