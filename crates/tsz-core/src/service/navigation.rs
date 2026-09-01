//! One immutable index shares bound declaration identity across service navigation.
use super::{QuickInfo, TextSpan, normalize_path};
use crate::bind::{BoundDeclaration, DeclarationKind, Meaning, ScopeId};
use crate::program::{
    CapabilityAnalysis, CapabilityTarget, CompileOutput, DeclarationDisplayParts,
    DeclarationDisplaySummary, Program, ProgramFile, RenderedParameter, SemanticCompletion,
};
use crate::source::{DeclId, FileId, Span, display_path};
use crate::text::quote_string;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
macro_rules! navigation_record {
    ($name:ident $(, $extra:ident)* { $($(#[$meta:meta])* $field:ident: $ty:ty),* $(,)? }) => {
        #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize $(, $extra)*)]
        #[serde(rename_all = "camelCase")]
        pub struct $name { $($(#[$meta])* pub $field: $ty),* }
    };
}

navigation_record!(DefinitionInfo {
    file_name: String, text_span: TextSpan, kind: String, name: String,
    container_kind: String, container_name: String,
    is_local: bool, is_ambient: bool, unverified: bool,
    failed_alias_resolution: bool,
    #[serde(skip_serializing_if = "Option::is_none")] context_span: Option<TextSpan>,
});
navigation_record!(DefinitionAndBoundSpan { definitions: Vec<DefinitionInfo>, text_span: TextSpan });
navigation_record!(SymbolDisplayPart {
    text: String,
    kind: String
});
navigation_record!(ReferencedSymbolDefinition {
    container_kind: String, container_name: String, file_name: String, kind: String, name: String,
    text_span: TextSpan, display_parts: Vec<SymbolDisplayPart>,
    #[serde(skip_serializing_if = "Option::is_none")] context_span: Option<TextSpan>,
});
navigation_record!(ReferenceEntry {
    text_span: TextSpan, file_name: String,
    #[serde(skip_serializing_if = "Option::is_none")] context_span: Option<TextSpan>,
    is_write_access: bool,
    #[serde(skip_serializing_if = "Option::is_none")] is_definition: Option<bool>,
});
navigation_record!(ReferencedSymbol { definition: ReferencedSymbolDefinition, references: Vec<ReferenceEntry> });
navigation_record!(HighlightSpan {
    text_span: TextSpan, kind: String,
    #[serde(skip_serializing_if = "Option::is_none")] context_span: Option<TextSpan>,
});
navigation_record!(DocumentHighlights { file_name: String, highlight_spans: Vec<HighlightSpan> });
navigation_record!(RenameInfo, Default {
    can_rename: bool,
    #[serde(skip_serializing_if = "Option::is_none")] display_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")] full_display_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")] kind: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")] kind_modifiers: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")] trigger_span: Option<TextSpan>,
    #[serde(skip_serializing_if = "Option::is_none")] localized_error_message: Option<String>,
});
navigation_record!(RenameLocation {
    file_name: String, text_span: TextSpan,
    #[serde(skip_serializing_if = "Option::is_none")] context_span: Option<TextSpan>,
});
navigation_record!(RenameResult { info: RenameInfo, locations: Vec<RenameLocation> });

#[derive(Debug, Clone)]
struct Occurrence {
    key: Span,
    file: FileId,
    span: TextSpan,
    is_write_access: bool,
    declaration: Option<DeclId>,
}

#[derive(Debug)]
pub(super) struct NavigationIndex<'a> {
    output: &'a CompileOutput,
    occurrences: Vec<Occurrence>,
}

impl<'a> NavigationIndex<'a> {
    pub(super) fn build(output: &'a CompileOutput) -> Self {
        let program = &output.program;
        let mut index = Self {
            output,
            occurrences: Vec::new(),
        };

        for file in &program.files {
            index.collect_bound_declarations(file);
        }
        for file in &program.files {
            index.collect_references(program, file);
        }
        index.occurrences.sort_by(|left, right| {
            (left.file, left.span.start, left.span.length).cmp(&(
                right.file,
                right.span.start,
                right.span.length,
            ))
        });
        index
    }

    pub(super) fn definition(&self, path: &str, offset: u32) -> Option<DefinitionAndBoundSpan> {
        let occurrence = self.occurrence_at(path, offset)?;
        Some(DefinitionAndBoundSpan {
            definitions: self.definitions_for_query(occurrence),
            text_span: occurrence.span,
        })
    }
    pub(super) fn type_definition(&self, path: &str, offset: u32) -> Vec<DefinitionInfo> {
        let Some((_, _, declaration)) = self.declaration_at(path, offset) else {
            return Vec::new();
        };
        let Some(keys) = self.type_definition_keys(declaration) else {
            return Vec::new();
        };
        self.definitions_for_keys(keys, None, None)
    }
    pub(super) fn quick_info(&self, path: &str, offset: u32) -> Option<QuickInfo> {
        let (occurrence, _, declaration) = self.declaration_at(path, offset)?;
        let (file, bound, summary) = self.declaration(declaration);
        let kind = declaration_kind(file, bound, summary);
        let mut quick_info_kind = summary.kind;
        let mut display = summary.display.clone();
        if kind == "local var" && file.bindings.scope_is_function_local(bound.scope) {
            quick_info_kind = kind;
            display.replace_range(.."var".len(), "(local var)");
        }
        Some(QuickInfo {
            kind: quick_info_kind.to_string(),
            text_span: occurrence.span,
            display,
        })
    }

    pub(super) fn references(&self, path: &str, offset: u32) -> Vec<ReferencedSymbol> {
        let Some((origin, location, declaration)) = self.declaration_at(path, offset) else {
            return Vec::new();
        };
        let (file, bound, summary) = self.declaration(declaration);
        let mark_definitions = origin.declaration.is_some();
        vec![ReferencedSymbol {
            definition: ReferencedSymbolDefinition {
                container_kind: String::new(),
                container_name: String::new(),
                file_name: self.file_name(location.file),
                kind: declaration_kind(file, bound, summary).to_string(),
                name: summary.display.clone(),
                text_span: location.span,
                display_parts: service_display_parts(summary, &bound.name),
                context_span: self.context_span(location),
            },
            references: self
                .occurrences_for_key(origin.key)
                .map(|occurrence| ReferenceEntry {
                    text_span: occurrence.span,
                    file_name: self.file_name(occurrence.file),
                    context_span: self.context_span(occurrence),
                    is_write_access: occurrence.is_write_access,
                    is_definition: mark_definitions.then_some(occurrence.declaration.is_some()),
                })
                .collect(),
        }]
    }

    pub(super) fn document_highlights(
        &self,
        path: &str,
        offset: u32,
        files_to_search: &[String],
    ) -> Vec<DocumentHighlights> {
        let Some(origin) = self.occurrence_at(path, offset) else {
            return Vec::new();
        };
        let requested = files_to_search
            .iter()
            .map(|path| normalize_path(path))
            .collect::<BTreeSet<_>>();
        let mut by_file: BTreeMap<String, Vec<HighlightSpan>> = BTreeMap::new();
        for occurrence in self.occurrences_for_key(origin.key) {
            let file_name = self.file_name(occurrence.file);
            if !requested.is_empty() && !requested.contains(&file_name) {
                continue;
            }
            by_file.entry(file_name).or_default().push(HighlightSpan {
                text_span: occurrence.span,
                kind: if occurrence.is_write_access {
                    "writtenReference".to_string()
                } else {
                    "reference".to_string()
                },
                context_span: self.context_span(occurrence),
            });
        }
        by_file
            .into_iter()
            .map(|(file_name, highlight_spans)| DocumentHighlights {
                file_name,
                highlight_spans,
            })
            .collect()
    }

    pub(super) fn rename(&self, path: &str, offset: u32) -> RenameResult {
        let Some((origin, _, declaration)) = self.declaration_at(path, offset) else {
            return RenameResult {
                info: RenameInfo {
                    localized_error_message: Some("You cannot rename this element.".to_string()),
                    ..RenameInfo::default()
                },
                locations: Vec::new(),
            };
        };
        let (file, bound, summary) = self.declaration(declaration);
        let name = bound.name.clone();
        RenameResult {
            info: RenameInfo {
                can_rename: true,
                display_name: Some(name.clone()),
                full_display_name: self
                    .definition(path, offset)
                    .as_ref()
                    .and_then(|definition| self.module_qualified_name(definition))
                    .or(Some(name)),
                kind: Some(declaration_kind(file, bound, summary).to_string()),
                kind_modifiers: Some(String::new()),
                trigger_span: Some(origin.span),
                localized_error_message: None,
            },
            locations: self
                .occurrences_for_key(origin.key)
                .map(|occurrence| RenameLocation {
                    file_name: self.file_name(occurrence.file),
                    text_span: occurrence.span,
                    context_span: self.context_span(occurrence),
                })
                .collect(),
        }
    }

    pub(super) fn query_is_claimed(
        &self,
        target: CapabilityTarget,
        path: &str,
        offset: u32,
        files_to_search: &[String],
    ) -> bool {
        let Some(file) = super::compiled_file(self.output, &normalize_path(path)) else {
            return false;
        };
        if !self
            .output
            .capabilities
            .navigation_query_is_claimed(target, file, offset)
        {
            return false;
        }
        if !self.query_completion(target, path, offset).is_complete() {
            return false;
        }
        let Some(origin) = self.occurrence_at(path, offset) else {
            return true;
        };
        let keys = if target == CapabilityTarget::TypeDefinition {
            self.declaration_at(path, offset)
                .and_then(|(_, _, declaration)| self.type_definition_keys(declaration))
                .unwrap_or_default()
        } else {
            match target {
                CapabilityTarget::Definition => self.definition_keys(origin),
                _ => vec![origin.key],
            }
        };
        self.occurrences
            .iter()
            .filter(move |occurrence| keys.contains(&occurrence.key))
            .filter(move |occurrence| match target {
                CapabilityTarget::QuickInfo
                | CapabilityTarget::Definition
                | CapabilityTarget::TypeDefinition => occurrence.declaration.is_some(),
                CapabilityTarget::Highlights => {
                    files_to_search.is_empty()
                        || files_to_search
                            .iter()
                            .any(|path| normalize_path(path) == self.file_name(occurrence.file))
                }
                CapabilityTarget::References | CapabilityTarget::Rename => true,
                _ => false,
            })
            .all(|occurrence| {
                let file = &self.output.program.files[occurrence.file.0 as usize];
                self.output.capabilities.navigation_query_is_claimed(
                    target,
                    file,
                    occurrence.span.start,
                )
            })
    }

    pub(super) fn query_completion(
        &self,
        target: CapabilityTarget,
        path: &str,
        offset: u32,
    ) -> SemanticCompletion {
        let Some(origin) = self.occurrence_at(path, offset) else {
            return SemanticCompletion::Complete;
        };
        self.occurrences_for_key(origin.key)
            .filter_map(|location| location.declaration)
            .map(|declaration| match target {
                CapabilityTarget::QuickInfo => {
                    self.declaration(declaration).2.quick_info_completion
                }
                CapabilityTarget::TypeDefinition => self
                    .type_definition_keys(declaration)
                    .map_or(SemanticCompletion::Deferred, |_| {
                        self.declaration(declaration).2.type_definition_completion
                    }),
                CapabilityTarget::References => {
                    self.declaration(declaration).2.references_completion
                }
                _ => SemanticCompletion::Complete,
            })
            .max()
            .unwrap_or({
                if matches!(
                    target,
                    CapabilityTarget::QuickInfo
                        | CapabilityTarget::TypeDefinition
                        | CapabilityTarget::References
                ) {
                    SemanticCompletion::Deferred
                } else {
                    SemanticCompletion::Complete
                }
            })
    }

    fn occurrence_at(&self, path: &str, offset: u32) -> Option<&Occurrence> {
        let file = super::compiled_file(self.output, &normalize_path(path))?
            .source
            .id;
        self.occurrences
            .iter()
            .filter(|occurrence| {
                occurrence.file == file
                    && (occurrence.span.start <= offset
                        && offset < occurrence.span.start + occurrence.span.length
                        || occurrence.span.length > 0
                            && occurrence.span.start + occurrence.span.length == offset)
            })
            .min_by_key(|occurrence| occurrence.span.length)
    }

    fn declaration_at(
        &self,
        path: &str,
        offset: u32,
    ) -> Option<(&Occurrence, &Occurrence, DeclId)> {
        let occurrence = self.occurrence_at(path, offset)?;
        let location = self
            .occurrences_for_key(occurrence.key)
            .find(|location| location.declaration.is_some())?;
        let declaration = location.declaration?;
        Some((occurrence, location, declaration))
    }

    fn occurrences_for_key(&self, key: Span) -> impl Iterator<Item = &Occurrence> {
        self.occurrences
            .iter()
            .filter(move |occurrence| occurrence.key == key)
    }

    fn definitions_for_keys(
        &self,
        keys: impl IntoIterator<Item = Span>,
        container_name: Option<&str>,
        common_kind: Option<&str>,
    ) -> Vec<DefinitionInfo> {
        let mut definitions = Vec::new();
        for key in keys {
            for location in self.occurrences_for_key(key) {
                let Some(declaration) = location.declaration else {
                    continue;
                };
                let (file, bound, summary) = self.declaration(declaration);
                let definition = DefinitionInfo {
                    file_name: self.file_name(location.file),
                    text_span: location.span,
                    kind: common_kind
                        .unwrap_or_else(|| declaration_kind(file, bound, summary))
                        .to_string(),
                    name: bound.name.clone(),
                    container_kind: String::new(),
                    container_name: container_name.unwrap_or_default().to_string(),
                    is_local: declaration_is_local(file, bound, summary),
                    is_ambient: summary.ambient,
                    unverified: false,
                    failed_alias_resolution: false,
                    context_span: self.context_span(location),
                };
                if !definitions.contains(&definition) {
                    definitions.push(definition);
                }
            }
        }
        if container_name.is_some() {
            definitions.sort_by(|left, right| {
                (&left.file_name, left.text_span.start, left.text_span.length).cmp(&(
                    &right.file_name,
                    right.text_span.start,
                    right.text_span.length,
                ))
            });
        }
        definitions
    }

    fn definitions_for_query(&self, origin: &Occurrence) -> Vec<DefinitionInfo> {
        let Some(declaration) = self.declaration_for_key(origin.key) else {
            return Vec::new();
        };
        let Some((keys, module, common_kind)) = self.definition_alias(declaration) else {
            return self.definitions_for_keys([origin.key], None, None);
        };
        let container_name = quote_string(module);
        self.definitions_for_keys(keys, Some(&container_name), common_kind)
    }

    fn definition_keys(&self, origin: &Occurrence) -> Vec<Span> {
        self.declaration_for_key(origin.key)
            .and_then(|declaration| self.definition_alias(declaration))
            .map_or_else(|| vec![origin.key], |(keys, _, _)| keys)
    }

    fn type_definition_keys(&self, declaration: DeclId) -> Option<Vec<Span>> {
        let summary = self.declaration(declaration).2;
        if !summary.type_definition_completion.is_complete() {
            return None;
        }
        summary
            .type_definition_targets
            .iter()
            .map(|target| {
                let key = declaration_key(&self.output.program, *target);
                self.declaration_for_key(key).map(|_| key)
            })
            .collect()
    }

    fn declaration_for_key(&self, key: Span) -> Option<DeclId> {
        self.occurrences_for_key(key)
            .find_map(|occurrence| occurrence.declaration)
    }

    fn declaration(
        &self,
        id: DeclId,
    ) -> (&ProgramFile, &BoundDeclaration, &DeclarationDisplaySummary) {
        let file = &self.output.program.files[id.file.0 as usize];
        let declaration = &file.bindings.declarations[id.local as usize];
        let summary = self
            .output
            .declaration_display_summaries
            .get(&id)
            .expect("every indexed declaration has one checker display result");
        (file, declaration, summary)
    }

    fn file_name(&self, file: FileId) -> String {
        display_path(&self.output.program.files[file.0 as usize].source.path)
    }

    fn context_span(&self, occurrence: &Occurrence) -> Option<TextSpan> {
        occurrence.declaration.and_then(|id| {
            let (_, declaration, summary) = self.declaration(id);
            (declaration.kind != DeclarationKind::AnonymousType)
                .then(|| summary.context_span.map(text_span))
                .flatten()
        })
    }

    fn module_qualified_name(&self, result: &DefinitionAndBoundSpan) -> Option<String> {
        let definition = result.definitions.iter().find(|item| !item.is_local)?;
        super::compiled_file(self.output, &definition.file_name)
            .filter(|file| file.is_external_module())?;
        Some(format!(
            "{}.{}",
            quote_string(remove_source_extension(&definition.file_name)),
            definition.name
        ))
    }

    fn definition_alias(
        &self,
        declaration: DeclId,
    ) -> Option<(Vec<Span>, &str, Option<&'static str>)> {
        let program = &self.output.program;
        let (file, bound, _) = self.declaration(declaration);
        let (value, r#type) = program.import_alias_targets(bound.id)?;
        let mut targets = [r#type, value]
            .into_iter()
            .flatten()
            .map(|target| declaration_key(program, target))
            .filter(|target| self.declaration_for_key(*target).is_some())
            .collect::<Vec<_>>();
        targets.sort_unstable_by_key(|target| (target.file, target.start, target.end));
        targets.dedup();
        if targets.is_empty() {
            return None;
        }
        let crate::syntax::StatementKind::Import(import) = &file
            .syntax
            .statements
            .iter()
            .find(|statement| statement.id == bound.owner)?
            .kind
        else {
            return None;
        };
        let common_kind = value
            .map(|target| declaration_key(program, target))
            .and_then(|target| self.declaration_for_key(target))
            .map(|target| {
                let (file, bound, summary) = self.declaration(target);
                declaration_kind(file, bound, summary)
            });
        Some((targets, &import.module_specifier, common_kind))
    }

    fn collect_bound_declarations(&mut self, file: &ProgramFile) {
        let program = &self.output.program;

        for declaration in &file.bindings.declarations {
            if !CapabilityAnalysis::navigation_declaration_has_identity(declaration) {
                continue;
            }
            self.output
                .declaration_display_summaries
                .get(&declaration.id)
                .expect("every bound declaration has one checker display result");
            let key = declaration_key(program, declaration.id);

            let span = text_span(declaration.name_span);
            if self.occurrences.iter().any(|occurrence| {
                occurrence.declaration.is_some()
                    && occurrence.key == key
                    && occurrence.file == file.source.id
                    && occurrence.span == span
            }) {
                continue;
            }
            self.occurrences.push(Occurrence {
                key,
                file: file.source.id,
                span,
                is_write_access: true,
                declaration: Some(declaration.id),
            });
        }
    }

    fn collect_references(&mut self, program: &Program, file: &ProgramFile) {
        for reference in file.bindings.reference_facts() {
            let Some(declaration) =
                reference.declaration(|name, meaning| program.resolve_global(name, meaning))
            else {
                continue;
            };
            let key = declaration_key(program, declaration);
            if self.declaration_for_key(key).is_none() {
                continue;
            }
            self.occurrences.push(Occurrence {
                key,
                file: file.source.id,
                span: text_span(reference.span),
                is_write_access: reference.is_write_access,
                declaration: None,
            });
        }
    }
}

fn declaration_key(program: &Program, id: DeclId) -> Span {
    let Some(file) = program.files.get(id.file.0 as usize) else {
        return Span::new(id.file, 0, 0);
    };
    let declaration = &file.bindings.declarations[id.local as usize];
    if declaration.scope == ScopeId(0)
        && declaration.kind != DeclarationKind::Import
        && !file.is_external_module()
    {
        let has_class = program
            .global_values
            .get(&declaration.name)
            .is_some_and(|declarations| {
                declarations.iter().any(|id| {
                    program.files[id.file.0 as usize].bindings.declarations[id.local as usize].kind
                        == DeclarationKind::Class
                })
            });
        let declarations = if has_class || declaration.kind == DeclarationKind::UnmodeledHost {
            program.global_values.get(&declaration.name)
        } else {
            match declaration.meaning {
                Meaning::Value => program.global_values.get(&declaration.name),
                Meaning::Type => program.global_types.get(&declaration.name),
            }
        };
        declarations
            .and_then(|declarations| declarations.first())
            .map(|id| {
                program.files[id.file.0 as usize].bindings.declarations[id.local as usize].name_span
            })
            .unwrap_or(declaration.name_span)
    } else {
        declaration.name_span
    }
}

fn declaration_is_local(
    file: &ProgramFile,
    declaration: &BoundDeclaration,
    summary: &DeclarationDisplaySummary,
) -> bool {
    declaration.kind != DeclarationKind::AnonymousType
        && (declaration.scope != ScopeId(0)
            || declaration.kind == DeclarationKind::Import
            || (file.is_external_module() && !summary.exported))
}

fn declaration_kind(
    file: &ProgramFile,
    declaration: &BoundDeclaration,
    summary: &DeclarationDisplaySummary,
) -> &'static str {
    if declaration.kind == DeclarationKind::Variable
        && declaration_is_local(file, declaration, summary)
        && summary.kind == "var"
    {
        "local var"
    } else {
        summary.kind
    }
}

pub(super) fn remove_source_extension(path: &str) -> &str {
    [
        ".d.ts", ".d.mts", ".d.cts", ".mjs", ".mts", ".cjs", ".cts", ".ts", ".js", ".tsx", ".jsx",
        ".json",
    ]
    .into_iter()
    .find_map(|extension| path.strip_suffix(extension))
    .unwrap_or(path)
}

fn service_display_parts(
    summary: &DeclarationDisplaySummary,
    name: &str,
) -> Vec<SymbolDisplayPart> {
    match &summary.display_parts {
        DeclarationDisplayParts::Text => vec![display_part(&summary.display, "text")],
        DeclarationDisplayParts::Variable(ty) => {
            let mut parts = named_display_parts(summary.kind, "keyword", name, "localName");
            if let Some(ty) = ty {
                append_type_parts(&mut parts, ty);
            }
            parts
        }
        DeclarationDisplayParts::Function { parameters, result } => {
            let (Some(parameters), Some(result)) = (parameters, result) else {
                return vec![display_part(&summary.display, "text")];
            };
            let mut parts = named_display_parts("function", "keyword", name, "functionName");
            parts.push(display_part("(", "punctuation"));
            for (index, parameter) in parameters.iter().enumerate() {
                if index > 0 {
                    parts.extend([display_part(",", "punctuation"), display_part(" ", "space")]);
                }
                append_parameter_parts(&mut parts, parameter);
            }
            parts.push(display_part(")", "punctuation"));
            append_type_parts(&mut parts, result);
            parts
        }
        DeclarationDisplayParts::Class => {
            named_display_parts("class", "keyword", name, "className")
        }
        DeclarationDisplayParts::Parameter(parameter) => {
            let mut parts = vec![
                display_part("(", "punctuation"),
                display_part("parameter", "text"),
                display_part(")", "punctuation"),
                display_part(" ", "space"),
            ];
            if let Some(parameter) = parameter {
                append_parameter_parts(&mut parts, parameter);
            } else {
                parts.push(display_part(name, "parameterName"));
            }
            parts
        }
    }
}

fn append_parameter_parts(parts: &mut Vec<SymbolDisplayPart>, parameter: &RenderedParameter) {
    if parameter.rest {
        parts.push(display_part("...", "punctuation"));
    }
    parts.push(display_part(&parameter.name, "parameterName"));
    if parameter.optional {
        parts.push(display_part("?", "punctuation"));
    }
    append_type_parts(parts, &parameter.ty);
}

fn named_display_parts(
    keyword: &str,
    keyword_kind: &str,
    name: &str,
    name_kind: &str,
) -> Vec<SymbolDisplayPart> {
    vec![
        display_part(keyword, keyword_kind),
        display_part(" ", "space"),
        display_part(name, name_kind),
    ]
}

fn append_type_parts(parts: &mut Vec<SymbolDisplayPart>, ty: &crate::program::RenderedType) {
    parts.extend([
        display_part(":", "punctuation"),
        display_part(" ", "space"),
        display_part(&ty.text, ty.part_kind),
    ]);
}

fn display_part(text: &str, kind: &str) -> SymbolDisplayPart {
    SymbolDisplayPart {
        text: text.to_string(),
        kind: kind.to_string(),
    }
}

const fn text_span(span: Span) -> TextSpan {
    TextSpan {
        start: span.start,
        length: span.len(),
    }
}
