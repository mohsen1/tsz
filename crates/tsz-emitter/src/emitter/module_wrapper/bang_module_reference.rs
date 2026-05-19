use super::super::Printer;
use crate::emitter::ModuleKind;
use tsz_parser::parser::node::NodeAccess;

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

    pub(super) fn should_preserve_bang_module_reference(&self, path: &str) -> bool {
        if self.ctx.options.module != ModuleKind::AMD || !path.ends_with(".d.ts") {
            return false;
        }

        let mut decl_files = self.referenced_declaration_files(path).peekable();
        if decl_files.peek().is_none() {
            return self.source_imports_bang_module_specifier();
        }

        decl_files.any(|source_file| {
            source_file.text.lines().any(|line| {
                let Some(module_name) = Self::extract_declare_module_name(line) else {
                    return false;
                };
                module_name.contains('!') && self.source_references_module_specifier(module_name)
            })
        })
    }

    fn source_references_module_specifier(&self, module_name: &str) -> bool {
        let specifier_matches = |idx| {
            self.arena
                .get_literal_text(idx)
                .is_some_and(|s| s == module_name)
        };
        self.arena
            .import_decls
            .iter()
            .map(|d| d.module_specifier)
            .chain(self.arena.export_decls.iter().map(|d| d.module_specifier))
            .any(specifier_matches)
    }

    fn source_imports_bang_module_specifier(&self) -> bool {
        let specifier_has_bang = |idx| {
            self.arena
                .get_literal_text(idx)
                .is_some_and(|s| s.contains('!'))
        };
        self.arena
            .import_decls
            .iter()
            .map(|d| d.module_specifier)
            .chain(self.arena.export_decls.iter().map(|d| d.module_specifier))
            .any(specifier_has_bang)
    }
}
