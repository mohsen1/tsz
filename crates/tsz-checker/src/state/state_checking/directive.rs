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
            extract_reference_paths, find_malformed_reference_directives,
            reference_path_probe_extensions, validate_reference_path,
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

        let allow_js_references = self.ctx.compiler_options.allow_js || self.ctx.is_js_file();
        let has_virtual_reference = |reference_path: &str| {
            let base = source_path.parent().unwrap_or_else(|| Path::new(""));
            if validate_reference_path(source_path, reference_path, allow_js_references) {
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
                for ext in reference_path_probe_extensions(allow_js_references) {
                    let candidate = base.join(format!("{reference_path}{ext}"));
                    if known_files.contains(candidate.to_string_lossy().as_ref()) {
                        return true;
                    }
                }
            }
            false
        };

        let unresolved_extensions = if self.ctx.compiler_options.allow_js || self.ctx.is_js_file() {
            tsz_common::file_extensions::TSC_TS_JS_RESOLUTION_EXTENSIONS
        } else {
            tsz_common::file_extensions::TSC_TS_RESOLUTION_EXTENSIONS
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
                    let display_path = if reference_path.is_empty() {
                        let resolved = source_path.parent().unwrap_or_else(|| Path::new(""));
                        display_reference_path(resolved, self.ctx.current_directory.as_deref())
                    } else {
                        reference_path.clone()
                    };
                    let message = format_message(
                        "Could not resolve the path '{0}' with the extensions: '{1}'.",
                        &[&display_path, &extensions],
                    );
                    self.emit_error_at(
                        pos,
                        length,
                        &message,
                        diagnostic_codes::COULD_NOT_RESOLVE_THE_PATH_WITH_THE_EXTENSIONS,
                    );
                    continue;
                }

                // Resolve the reference path relative to the source file's directory.
                // tsc normalizes Windows-style backslashes before resolution, so
                // `../../../foo` from `..\..\..\foo` resolves correctly on Unix.
                let forward_slash_path = reference_path.replace('\\', "/");
                let resolved = source_path
                    .parent()
                    .unwrap_or_else(|| Path::new(""))
                    .join(&forward_slash_path);

                // tsc 7.0.2 prints the directive text exactly as written in
                // BOTH explicit-file and project modes; the resolved path is
                // used only for the existence probe.
                let _ = &resolved;
                let message = format_message("File '{0}' not found.", &[reference_path.as_str()]);
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

/// Render a resolved triple-slash reference path for a TS6053/TS6054 message.
///
/// Explicit-file `tsc entry.ts` checks keep source names relative to
/// `host.getCurrentDirectory()`, so a reference path resolved from one prints
/// relative to that directory (e.g. `nested/missing.d.ts`, `../up.d.ts`).
/// Project/config checks store resolved source names, so the driver leaves the
/// current directory unset and this returns the normalized resolved path. `.`
/// and `..` components are always collapsed.
fn display_reference_path(resolved: &std::path::Path, current_dir: Option<&str>) -> String {
    let normalized = normalize_path(resolved);
    let Some(current_dir) = current_dir.filter(|dir| !dir.is_empty()) else {
        return normalized;
    };
    let normalized_path = std::path::Path::new(&normalized);
    // Only relativize an absolute resolved path against the (absolute) current
    // directory. A path that is already relative is resolved relative to the
    // source file, which `tsc` also keeps relative, so it is left untouched.
    if !normalized_path.is_absolute() {
        return normalized;
    }
    relative_to(normalized_path, std::path::Path::new(current_dir)).unwrap_or(normalized)
}

fn reference_path_is_absolute(reference_path: &str) -> bool {
    let normalized = reference_path.replace('\\', "/");
    if normalized.starts_with('/') {
        return true;
    }
    let bytes = normalized.as_bytes();
    bytes.len() >= 3 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':' && bytes[2] == b'/'
}

/// Compute `path` relative to `base`, mirroring `tsc`'s
/// `getRelativePathFromDirectory`. Returns `None` when the two share no common
/// root, so the caller keeps the absolute form. The result always uses forward
/// slashes, matching `tsc`'s diagnostic paths.
fn relative_to(path: &std::path::Path, base: &std::path::Path) -> Option<String> {
    use std::path::Component;

    let path_components: Vec<Component<'_>> = path.components().collect();
    let base_components: Vec<Component<'_>> = base.components().collect();
    let common_len = path_components
        .iter()
        .zip(base_components.iter())
        .take_while(|(a, b)| a == b)
        .count();
    if common_len == 0 {
        return None;
    }

    let mut result = std::path::PathBuf::new();
    for _ in common_len..base_components.len() {
        result.push("..");
    }
    for component in &path_components[common_len..] {
        result.push(component);
    }

    let rel = result.to_string_lossy().replace('\\', "/");
    Some(if rel.is_empty() { ".".to_string() } else { rel })
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

#[cfg(all(test, unix))]
mod tests {
    use super::display_reference_path;
    use std::path::Path;

    // Unix-only: the relativization depends on POSIX absolute-path semantics
    // (`/...` is absolute). The cross-platform behavior is exercised end-to-end
    // by `tsz-cli`'s `triple_slash_reference_not_found_relative_path_tests`.

    #[test]
    fn sibling_reference_is_relative_to_cwd() {
        let resolved = Path::new("/proj/app/missing-a.d.ts");
        assert_eq!(
            display_reference_path(resolved, Some("/proj/app")),
            "missing-a.d.ts"
        );
    }

    #[test]
    fn parent_escaping_reference_keeps_single_dotdot() {
        let resolved = Path::new("/proj/up-missing.d.ts");
        assert_eq!(
            display_reference_path(resolved, Some("/proj/app")),
            "../up-missing.d.ts"
        );
    }

    #[test]
    fn subdirectory_reference_keeps_prefix() {
        let resolved = Path::new("/proj/app/sub/deep-missing.d.ts");
        assert_eq!(
            display_reference_path(resolved, Some("/proj/app")),
            "sub/deep-missing.d.ts"
        );
    }

    #[test]
    fn dot_components_are_collapsed_before_relativizing() {
        let resolved = Path::new("/proj/app/sub/../x.d.ts");
        assert_eq!(
            display_reference_path(resolved, Some("/proj/app")),
            "x.d.ts"
        );
    }

    #[test]
    fn without_current_directory_path_stays_absolute() {
        let resolved = Path::new("/proj/app/missing-a.d.ts");
        assert_eq!(
            display_reference_path(resolved, None),
            "/proj/app/missing-a.d.ts"
        );
        assert_eq!(
            display_reference_path(resolved, Some("")),
            "/proj/app/missing-a.d.ts"
        );
    }

    #[test]
    fn already_relative_path_is_left_untouched() {
        // A relative resolved path (source file stored relative) is already in
        // the form tsc keeps; do not attempt to relativize it further.
        let resolved = Path::new("app/missing-a.d.ts");
        assert_eq!(
            display_reference_path(resolved, Some("/proj/app")),
            "app/missing-a.d.ts"
        );
    }

    #[test]
    fn absolute_reference_literal_is_detected_portably() {
        assert!(super::reference_path_is_absolute("/tmp/missing.d.ts"));
        assert!(super::reference_path_is_absolute("C:/tmp/missing.d.ts"));
        assert!(super::reference_path_is_absolute("C:\\tmp\\missing.d.ts"));
        assert!(!super::reference_path_is_absolute("../missing.d.ts"));
        assert!(!super::reference_path_is_absolute("nested/missing.d.ts"));
    }
}
