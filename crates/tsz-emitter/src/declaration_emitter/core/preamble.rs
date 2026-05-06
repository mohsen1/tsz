use super::DeclarationEmitter;

impl<'a> DeclarationEmitter<'a> {
    /// Emits detached copyright comments (`/*! ... */`) at the top of the .d.ts file.
    ///
    /// TSC preserves `/*!` comments (copyright notices) at the very start of the file
    /// in declaration output, even when `--removeComments` is set.
    pub(in crate::declaration_emitter) fn emit_detached_copyright_comments(
        &mut self,
        source_file: &tsz_parser::parser::node::SourceFileData,
    ) {
        // Find the position of the first statement
        let first_stmt_pos = source_file
            .statements
            .nodes
            .first()
            .and_then(|&idx| self.arena.get(idx))
            .map(|n| n.pos);

        for comment in &source_file.comments {
            // Only consider comments that appear before the first statement
            if let Some(stmt_pos) = first_stmt_pos
                && comment.pos >= stmt_pos
            {
                break;
            }

            // Only preserve /*! ... */ copyright comments
            if !comment.is_multi_line {
                continue;
            }
            let text = comment.get_text(&source_file.text);
            if !text.starts_with("/*!") {
                continue;
            }

            self.write(text);
            self.write_line();
        }
    }

    /// Emits triple-slash directives at the top of the .d.ts file.
    ///
    /// TypeScript uses triple-slash directives for:
    /// - File references: `/// <reference path="other.ts" />`
    /// - Type references: `/// <reference types="node" />`
    /// - Lib references: `/// <reference lib="es2015" />`
    /// - AMD directives: `/// <amd-module />`, `/// <amd-dependency />`
    ///
    /// These must appear at the very top of the file, before any imports or declarations.
    pub(in crate::declaration_emitter) fn emit_triple_slash_directives(
        &mut self,
        source_file: &tsz_parser::parser::node::SourceFileData,
    ) {
        for comment in &source_file.comments {
            let text = &source_file.text[comment.pos as usize..comment.end as usize];

            // Triple-slash directives start with ///
            if let Some(stripped) = text.strip_prefix("///") {
                let trimmed = stripped.trim_start();

                // Preserve `<amd-module>` and `<amd-dependency>` directives.
                // Also preserve `<reference>` directives that have `preserve="true"`.
                let should_emit = trimmed.starts_with("<amd-module")
                    || trimmed.starts_with("<amd-dependency")
                    || (trimmed.starts_with("<reference") && trimmed.contains("preserve=\"true\""));

                if should_emit {
                    // Normalize spacing to match tsc:
                    // 1. Ensure space after `///`: `///<reference` -> `/// <reference`
                    // 2. Ensure space before `/>`: `/>` -> ` />`
                    let mut normalized = if !stripped.starts_with(' ') {
                        format!("/// {}", stripped.trim_start())
                    } else {
                        text.to_string()
                    };
                    if normalized.ends_with("/>") && !normalized.ends_with(" />") {
                        let base = &normalized[..normalized.len() - 2];
                        normalized = format!("{base} />");
                    }
                    self.write(&normalized);
                    self.write_line();
                }
            }
        }
    }
}
