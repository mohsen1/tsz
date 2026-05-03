use super::super::Printer;
use crate::emitter::ModuleKind;

impl<'a> Printer<'a> {
    fn referenced_declaration_files(
        &self,
        path: &str,
    ) -> impl Iterator<Item = &tsz_parser::parser::node::SourceFileData> {
        self.arena.source_files.iter().filter(move |source_file| {
            source_file.is_declaration_file
                && (source_file.file_name == path
                    || source_file.file_name.ends_with(&format!("/{path}")))
        })
    }

    fn extract_declare_module_name(line: &str) -> Option<&str> {
        let after_keyword = line
            .trim_start()
            .strip_prefix("declare module")?
            .trim_start();
        let quote = after_keyword.as_bytes().first().copied()?;
        if !matches!(quote, b'\'' | b'"') {
            return None;
        }
        let end = after_keyword[1..].find(quote as char)?;
        Some(&after_keyword[1..1 + end])
    }

    pub(super) fn should_preserve_bang_module_reference(
        &self,
        path: &str,
        source_text: &str,
    ) -> bool {
        if self.ctx.options.module != ModuleKind::AMD || !path.ends_with(".d.ts") {
            return false;
        }

        let mut saw_referenced_declaration_file = false;
        let declares_imported_bang_module =
            self.referenced_declaration_files(path).any(|source_file| {
                saw_referenced_declaration_file = true;
                source_file.text.lines().any(|line| {
                    let Some(module_name) = Self::extract_declare_module_name(line) else {
                        return false;
                    };
                    module_name.contains('!')
                        && (source_text.contains(&format!("\"{module_name}\""))
                            || source_text.contains(&format!("'{module_name}'")))
                })
            });

        if saw_referenced_declaration_file {
            declares_imported_bang_module
        } else {
            Self::source_imports_bang_module_specifier(source_text)
        }
    }

    fn source_imports_bang_module_specifier(source_text: &str) -> bool {
        source_text.lines().any(|line| {
            let trimmed = line.trim_start();
            (trimmed.starts_with("import ")
                || (trimmed.starts_with("export ")
                    && (trimmed.contains(" from ") || trimmed.contains("require(")))
                || trimmed.contains("require(")
                || trimmed.contains(" from "))
                && Self::quoted_text_contains_bang(line)
        })
    }

    fn quoted_text_contains_bang(text: &str) -> bool {
        let mut rest = text;
        while let Some(quote_start) = rest.find(['"', '\'']) {
            rest = &rest[quote_start..];
            let quote = rest.as_bytes()[0] as char;
            let Some(quote_end) = rest[1..].find(quote) else {
                return false;
            };
            if rest[1..1 + quote_end].contains('!') {
                return true;
            }
            rest = &rest[1 + quote_end + 1..];
        }
        false
    }
}
