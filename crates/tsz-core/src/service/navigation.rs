//! One immutable index shares bound declaration identity across service navigation.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::bind::{BoundDeclaration, DeclarationKind, Meaning, ScopeId};
use crate::program::{CapabilityTarget, CompileOutput, Program, ProgramFile};
use crate::semantics::{DeclarationDisplayParts, DeclarationDisplaySummary};
use crate::source::{DeclId, FileId, Span};

use super::{QuickInfo, TextSpan, normalize_path};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DefinitionInfo {
    pub file_name: String,
    pub text_span: TextSpan,
    pub kind: String,
    pub name: String,
    pub container_name: String,
    pub is_local: bool,
    pub is_ambient: bool,
    pub unverified: bool,
    pub failed_alias_resolution: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context_span: Option<TextSpan>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DefinitionAndBoundSpan {
    pub definitions: Vec<DefinitionInfo>,
    pub text_span: TextSpan,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SymbolDisplayPart {
    pub text: String,
    pub kind: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReferencedSymbolDefinition {
    pub container_kind: String,
    pub container_name: String,
    pub file_name: String,
    pub kind: String,
    pub name: String,
    pub text_span: TextSpan,
    pub display_parts: Vec<SymbolDisplayPart>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context_span: Option<TextSpan>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReferenceEntry {
    pub text_span: TextSpan,
    pub file_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context_span: Option<TextSpan>,
    pub is_write_access: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_definition: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReferencedSymbol {
    pub definition: ReferencedSymbolDefinition,
    pub references: Vec<ReferenceEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HighlightSpan {
    pub text_span: TextSpan,
    pub kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context_span: Option<TextSpan>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DocumentHighlights {
    pub file_name: String,
    pub highlight_spans: Vec<HighlightSpan>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RenameInfo {
    pub can_rename: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub full_display_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kind_modifiers: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trigger_span: Option<TextSpan>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub localized_error_message: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RenameLocation {
    pub file_name: String,
    pub text_span: TextSpan,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context_span: Option<TextSpan>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RenameResult {
    pub info: RenameInfo,
    pub locations: Vec<RenameLocation>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
enum SymbolKey {
    // Chosen only after a BoundDeclaration exists; arbitrary syntax spans never enter.
    Authored { file: FileId, start: u32, end: u32 },
}

#[derive(Debug, Clone)]
struct DeclarationMetadata {
    name: String,
    kind: &'static str,
    function_local: bool,
    is_local: bool,
    summary: DeclarationDisplaySummary,
}

#[derive(Debug, Clone)]
struct Occurrence {
    key: SymbolKey,
    file_name: String,
    span: TextSpan,
    context_span: Option<TextSpan>,
    is_write_access: bool,
    declaration: Option<DeclarationMetadata>,
}

#[derive(Debug, Default)]
pub(super) struct NavigationIndex {
    occurrences: Vec<Occurrence>,
    declaration_keys: BTreeMap<DeclId, SymbolKey>,
}

impl NavigationIndex {
    pub(super) fn build(output: &CompileOutput) -> Self {
        let program = &output.program;
        let mut index = Self::default();

        for file in &program.files {
            index.collect_bound_declarations(output, file);
        }
        for file in &program.files {
            index.collect_references(program, file);
        }
        index.occurrences.sort_by(|left, right| {
            (&left.file_name, left.span.start, left.span.length).cmp(&(
                &right.file_name,
                right.span.start,
                right.span.length,
            ))
        });
        index
    }

    pub(super) fn definition(&self, path: &str, offset: u32) -> Option<DefinitionAndBoundSpan> {
        let occurrence = self.occurrence_at(path, offset)?;
        let definitions = self
            .occurrences
            .iter()
            .filter(|location| location.key == occurrence.key)
            .filter_map(|location| location.declaration.as_ref().map(|value| (location, value)))
            .map(|(location, declaration)| DefinitionInfo {
                file_name: location.file_name.clone(),
                text_span: location.span,
                kind: declaration.kind.to_string(),
                name: declaration.name.clone(),
                container_name: String::new(),
                is_local: declaration.is_local,
                is_ambient: declaration.summary.ambient,
                unverified: false,
                failed_alias_resolution: false,
                context_span: location.context_span,
            })
            .collect::<Vec<_>>();
        Some(DefinitionAndBoundSpan {
            definitions,
            text_span: occurrence.span,
        })
    }

    pub(super) fn quick_info(&self, path: &str, offset: u32) -> Option<QuickInfo> {
        let (occurrence, _, declaration) = self.declaration_at(path, offset)?;
        let mut kind = declaration.summary.quick_info_kind?;
        let mut display = declaration.summary.display.clone();
        if declaration.kind == "local var" && declaration.function_local {
            kind = declaration.kind;
            display.replace_range(.."var".len(), "(local var)");
        }
        Some(QuickInfo {
            kind: kind.to_string(),
            text_span: occurrence.span,
            display,
        })
    }

    pub(super) fn references(&self, path: &str, offset: u32) -> Vec<ReferencedSymbol> {
        let Some((origin, location, declaration)) = self.declaration_at(path, offset) else {
            return Vec::new();
        };
        let mark_definitions = origin.declaration.is_some();
        let references = self
            .occurrences
            .iter()
            .filter(|occurrence| occurrence.key == origin.key)
            .map(|occurrence| ReferenceEntry {
                text_span: occurrence.span,
                file_name: occurrence.file_name.clone(),
                context_span: occurrence.context_span,
                is_write_access: occurrence.is_write_access,
                is_definition: mark_definitions.then_some(occurrence.declaration.is_some()),
            })
            .collect();
        vec![ReferencedSymbol {
            definition: ReferencedSymbolDefinition {
                container_kind: String::new(),
                container_name: String::new(),
                file_name: location.file_name.clone(),
                kind: declaration.kind.to_string(),
                name: declaration.summary.display.clone(),
                text_span: location.span,
                display_parts: service_display_parts(&declaration.summary, &declaration.name),
                context_span: location.context_span,
            },
            references,
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
        for occurrence in self
            .occurrences
            .iter()
            .filter(|occurrence| occurrence.key == origin.key)
        {
            if !requested.is_empty() && !requested.contains(&occurrence.file_name) {
                continue;
            }
            by_file
                .entry(occurrence.file_name.clone())
                .or_default()
                .push(HighlightSpan {
                    text_span: occurrence.span,
                    kind: if occurrence.is_write_access {
                        "writtenReference".to_string()
                    } else {
                        "reference".to_string()
                    },
                    context_span: occurrence.context_span,
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
            return RenameResult::failure();
        };
        RenameResult {
            info: RenameInfo {
                can_rename: true,
                display_name: Some(declaration.name.clone()),
                full_display_name: Some(declaration.name.clone()),
                kind: Some(declaration.kind.to_string()),
                kind_modifiers: Some(String::new()),
                trigger_span: Some(origin.span),
                localized_error_message: None,
            },
            locations: self
                .occurrences
                .iter()
                .filter(|occurrence| occurrence.key == origin.key)
                .map(|occurrence| RenameLocation {
                    file_name: occurrence.file_name.clone(),
                    text_span: occurrence.span,
                    context_span: occurrence.context_span,
                })
                .collect(),
        }
    }

    fn occurrence_at(&self, path: &str, offset: u32) -> Option<&Occurrence> {
        let normalized = normalize_path(path);
        self.occurrences
            .iter()
            .find(|occurrence| {
                occurrence.file_name == normalized
                    && occurrence.span.start <= offset
                    && offset < occurrence.span.start + occurrence.span.length
            })
            .or_else(|| {
                self.occurrences.iter().find(|occurrence| {
                    occurrence.file_name == normalized
                        && occurrence.span.length > 0
                        && occurrence.span.start + occurrence.span.length == offset
                })
            })
    }

    fn declaration_at(
        &self,
        path: &str,
        offset: u32,
    ) -> Option<(&Occurrence, &Occurrence, &DeclarationMetadata)> {
        let occurrence = self.occurrence_at(path, offset)?;
        let location = self
            .occurrences
            .iter()
            .find(|location| location.key == occurrence.key && location.declaration.is_some())?;
        let declaration = location.declaration.as_ref()?;
        Some((occurrence, location, declaration))
    }

    fn collect_bound_declarations(&mut self, output: &CompileOutput, file: &ProgramFile) {
        let program = &output.program;
        let summaries = &output.declaration_display_summaries;
        let file_name = normalize_path(&file.source.path.to_string_lossy());
        let module_file = file.is_external_module();

        for declaration in &file.bindings.declarations {
            // Type-member groups lack merged display provenance; do not fabricate results.
            if matches!(
                declaration.kind,
                DeclarationKind::TypeMember | DeclarationKind::AnonymousSignature
            ) || declaration.kind == DeclarationKind::FunctionExpression
                && declaration.name.is_empty()
            {
                continue;
            }
            let key = declaration_key(program, module_file, declaration);
            self.declaration_keys.insert(declaration.id, key.clone());

            let span = text_span(declaration.name_span);
            if self.occurrences.iter().any(|occurrence| {
                occurrence.declaration.is_some()
                    && occurrence.key == key
                    && occurrence.file_name == file_name
                    && occurrence.span == span
            }) {
                continue;
            }
            let mut summary = summaries.get(&declaration.id).cloned().unwrap_or_else(|| {
                let (kind, display) = fallback_metadata(declaration.kind, &declaration.name);
                DeclarationDisplaySummary {
                    kind,
                    context_span: None,
                    exported: false,
                    ambient: false,
                    display,
                    display_parts: DeclarationDisplayParts::Text,
                    quick_info_kind: None,
                }
            });
            summary.quick_info_kind = summary.quick_info_kind.filter(|_| {
                file.capability_scope_at(declaration.name_span.start)
                    .is_some_and(|scope| {
                        output
                            .capabilities
                            .claim(CapabilityTarget::QuickInfo, scope)
                            .is_claimed()
                    })
            });
            let context_span = summary.context_span.map(text_span);
            let is_local = declaration.scope != ScopeId(0)
                || declaration.kind == DeclarationKind::Import
                || (module_file && !summary.exported);
            let kind = match (declaration.kind, is_local, summary.kind) {
                (DeclarationKind::Variable, true, "var") => "local var",
                (_, _, kind) => kind,
            };
            let metadata = DeclarationMetadata {
                name: declaration.name.clone(),
                kind,
                function_local: file.bindings.scope_is_function_local(declaration.scope),
                is_local,
                summary,
            };
            self.occurrences.push(Occurrence {
                key,
                file_name: file_name.clone(),
                span,
                context_span,
                is_write_access: true,
                declaration: Some(metadata),
            });
        }
    }

    fn collect_references(&mut self, program: &Program, file: &ProgramFile) {
        let file_name = normalize_path(&file.source.path.to_string_lossy());
        for reference in file.bindings.reference_facts() {
            let Some(declaration) =
                reference.declaration(|name, meaning| program.resolve_global(name, meaning))
            else {
                continue;
            };
            let Some(key) = self.declaration_keys.get(&declaration).cloned() else {
                continue;
            };
            self.occurrences.push(Occurrence {
                key,
                file_name: file_name.clone(),
                span: text_span(reference.span),
                context_span: None,
                is_write_access: reference.is_write_access,
                declaration: None,
            });
        }
    }
}

impl RenameResult {
    pub(super) fn failure() -> Self {
        Self {
            info: RenameInfo {
                localized_error_message: Some("You cannot rename this element.".to_string()),
                ..RenameInfo::default()
            },
            locations: Vec::new(),
        }
    }
}

fn declaration_key(
    program: &Program,
    module_file: bool,
    declaration: &BoundDeclaration,
) -> SymbolKey {
    let canonical = if declaration.scope == ScopeId(0)
        && declaration.kind != DeclarationKind::Import
        && !module_file
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
            .map(|id| &program.files[id.file.0 as usize].bindings.declarations[id.local as usize])
            .unwrap_or(declaration)
    } else {
        declaration
    };
    let Span { file, start, end } = canonical.name_span;
    SymbolKey::Authored { file, start, end }
}

fn fallback_metadata(kind: DeclarationKind, name: &str) -> (&'static str, String) {
    match kind {
        DeclarationKind::Variable => ("var", format!("var {name}")),
        DeclarationKind::Parameter => ("parameter", format!("(parameter) {name}")),
        DeclarationKind::Import => ("alias", format!("(alias) {name}")),
        DeclarationKind::Function | DeclarationKind::FunctionExpression => {
            ("function", format!("function {name}"))
        }
        DeclarationKind::Class => ("class", format!("class {name}")),
        DeclarationKind::TypeAlias => ("type", format!("type {name}")),
        DeclarationKind::Interface => ("interface", format!("interface {name}")),
        DeclarationKind::TypeParameter => {
            ("type parameter", format!("(type parameter) {name} in type"))
        }
        DeclarationKind::TypeMember | DeclarationKind::JavaScriptPropertyAssignment => {
            ("property", format!("(property) {name}"))
        }
        DeclarationKind::AnonymousSignature => ("type", "(anonymous signature)".to_string()),
        DeclarationKind::UnmodeledHost => ("module", format!("module {name}")),
    }
}

fn service_display_parts(
    summary: &DeclarationDisplaySummary,
    name: &str,
) -> Vec<SymbolDisplayPart> {
    match &summary.display_parts {
        DeclarationDisplayParts::Text => vec![display_part(&summary.display, "text")],
        DeclarationDisplayParts::Variable(ty) => {
            let mut parts = vec![
                display_part(summary.kind, "keyword"),
                display_part(" ", "space"),
                display_part(name, "localName"),
            ];
            if let Some(ty) = ty {
                parts.extend([
                    display_part(":", "punctuation"),
                    display_part(" ", "space"),
                    display_part(&ty.text, ty.part_kind),
                ]);
            }
            parts
        }
        DeclarationDisplayParts::Function { parameters, result } => {
            let (Some(parameters), Some(result)) = (parameters, result) else {
                return vec![display_part(&summary.display, "text")];
            };
            let mut parts = vec![
                display_part("function", "keyword"),
                display_part(" ", "space"),
                display_part(name, "functionName"),
                display_part("(", "punctuation"),
            ];
            for (index, parameter) in parameters.iter().enumerate() {
                if index > 0 {
                    parts.extend([display_part(",", "punctuation"), display_part(" ", "space")]);
                }
                append_parameter_parts(&mut parts, parameter);
            }
            parts.extend([
                display_part(")", "punctuation"),
                display_part(":", "punctuation"),
                display_part(" ", "space"),
                display_part(&result.text, result.part_kind),
            ]);
            parts
        }
        DeclarationDisplayParts::Class => vec![
            display_part("class", "keyword"),
            display_part(" ", "space"),
            display_part(name, "className"),
        ],
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

fn append_parameter_parts(
    parts: &mut Vec<SymbolDisplayPart>,
    parameter: &crate::emit::display::RenderedParameter,
) {
    if parameter.rest {
        parts.push(display_part("...", "punctuation"));
    }
    parts.push(display_part(&parameter.name, "parameterName"));
    if parameter.optional {
        parts.push(display_part("?", "punctuation"));
    }
    parts.extend([
        display_part(":", "punctuation"),
        display_part(" ", "space"),
        display_part(&parameter.ty.text, parameter.ty.part_kind),
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
