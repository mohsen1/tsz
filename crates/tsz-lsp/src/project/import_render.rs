//! Auto-import completion rendering.
//!
//! Candidate collection lives in `imports`; this module turns an already chosen
//! candidate into the completion item exposed through the LSP surface.

use crate::code_actions::{ImportCandidate, ImportCandidateKind};
use crate::completions::{CompletionItem, CompletionItemKind, sort_priority};
use crate::symbols::document_symbols::SymbolKind;

use super::Project;

impl Project {
    pub(crate) fn completion_from_import_candidate(
        &self,
        candidate: &ImportCandidate,
        from_file: &str,
        import_statement_completion: bool,
    ) -> CompletionItem {
        let detail = self.auto_import_detail(candidate);
        let documentation = self.auto_import_documentation(candidate);
        let completion_kind = self.auto_import_completion_kind(candidate);

        let mut item = CompletionItem::new(candidate.local_name.clone(), completion_kind)
            .with_detail(detail)
            .with_sort_text(if import_statement_completion {
                // Inside `import { | }`: TypeScript uses LocationPriority ("11") so
                // these rank above regular-code auto-import suggestions ("16").
                sort_priority::LOCATION_PRIORITY
            } else {
                sort_priority::AUTO_IMPORT
            })
            .with_has_action()
            .with_source(candidate.module_specifier.clone())
            .with_source_display(candidate.module_specifier.clone())
            .with_kind_modifiers("export".to_string());
        if let Some(doc) = documentation {
            item = item.with_documentation(doc);
        }
        if let Some(package_name) = Self::module_specifier_package_name(&candidate.module_specifier)
            && let Some(allowed) = self.allowed_dependency_package_names(from_file)
            && allowed.contains(package_name)
        {
            item = item.with_is_package_json_import();
        }
        item
    }

    fn auto_import_completion_kind(&self, candidate: &ImportCandidate) -> CompletionItemKind {
        match self.symbol_index.get_definition_kind(&candidate.local_name) {
            Some(SymbolKind::Class) => CompletionItemKind::Class,
            Some(SymbolKind::Method) => CompletionItemKind::Method,
            Some(SymbolKind::Property) | Some(SymbolKind::Field) | Some(SymbolKind::Key) => {
                CompletionItemKind::Property
            }
            Some(SymbolKind::Constant | SymbolKind::String | SymbolKind::Number) => {
                CompletionItemKind::Const
            }
            Some(SymbolKind::Constructor) => CompletionItemKind::Constructor,
            Some(SymbolKind::Enum) => CompletionItemKind::Enum,
            Some(SymbolKind::Interface) => CompletionItemKind::Interface,
            Some(SymbolKind::Function) | Some(SymbolKind::Event) | Some(SymbolKind::Operator) => {
                CompletionItemKind::Function
            }
            Some(SymbolKind::Module) | Some(SymbolKind::Namespace) | Some(SymbolKind::Package) => {
                CompletionItemKind::Module
            }
            Some(SymbolKind::TypeParameter) => CompletionItemKind::TypeParameter,
            Some(SymbolKind::Struct) => CompletionItemKind::TypeAlias,
            _ => CompletionItemKind::Variable,
        }
    }

    fn auto_import_detail(&self, candidate: &ImportCandidate) -> String {
        let prefix = if candidate.is_type_only {
            "auto-import type"
        } else {
            "auto-import"
        };

        match candidate.kind {
            ImportCandidateKind::Named { .. } => {
                format!("{} from {}", prefix, candidate.module_specifier)
            }
            ImportCandidateKind::Default => {
                format!("{} default from {}", prefix, candidate.module_specifier)
            }
            ImportCandidateKind::Namespace => {
                format!("{} namespace from {}", prefix, candidate.module_specifier)
            }
        }
    }

    fn auto_import_documentation(&self, candidate: &ImportCandidate) -> Option<String> {
        let import_kw = if candidate.is_type_only {
            "import type"
        } else {
            "import"
        };

        let snippet = match &candidate.kind {
            ImportCandidateKind::Named { export_name } => {
                format!(
                    "{} {{ {} }} from \"{}\";",
                    import_kw, export_name, candidate.module_specifier
                )
            }
            ImportCandidateKind::Default => {
                format!(
                    "{} {} from \"{}\";",
                    import_kw, candidate.local_name, candidate.module_specifier
                )
            }
            ImportCandidateKind::Namespace => {
                format!(
                    "{} * as {} from \"{}\";",
                    import_kw, candidate.local_name, candidate.module_specifier
                )
            }
        };

        Some(snippet)
    }
}
