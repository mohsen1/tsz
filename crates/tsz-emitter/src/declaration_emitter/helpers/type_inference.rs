//! Type inference for expressions, object literals, and enums

#[allow(unused_imports)]
use super::super::{DeclarationEmitter, ImportPlan, PlannedImportModule, PlannedImportSymbol};
#[allow(unused_imports)]
use crate::emitter::type_printer::TypePrinter;
#[allow(unused_imports)]
use crate::output::source_writer::{SourcePosition, SourceWriter, source_position_from_offset};
#[allow(unused_imports)]
use rustc_hash::{FxHashMap, FxHashSet};
#[allow(unused_imports)]
use std::sync::Arc;
#[allow(unused_imports)]
use tracing::debug;
#[allow(unused_imports)]
use tsz_binder::{BinderState, SymbolId, symbol_flags};
#[allow(unused_imports)]
use tsz_common::comments::{get_jsdoc_content, is_jsdoc_comment};
#[allow(unused_imports)]
use tsz_parser::parser::ParserState;
#[allow(unused_imports)]
use tsz_parser::parser::node::{Node, NodeAccess, NodeArena};
#[allow(unused_imports)]
use tsz_parser::parser::syntax_kind_ext;
#[allow(unused_imports)]
use tsz_parser::parser::{NodeIndex, NodeList};
#[allow(unused_imports)]
use tsz_scanner::SyntaxKind;

pub(in crate::declaration_emitter) struct CallableDeclParts<'b> {
    pub(in crate::declaration_emitter) modifiers: Option<&'b NodeList>,
    pub(in crate::declaration_emitter) type_parameters: Option<&'b NodeList>,
    pub(in crate::declaration_emitter) parameters: &'b NodeList,
    pub(in crate::declaration_emitter) type_annotation: NodeIndex,
    pub(in crate::declaration_emitter) body: NodeIndex,
}

impl<'a> DeclarationEmitter<'a> {
    pub(in crate::declaration_emitter) fn synthetic_class_extends_alias_source_type_text(
        &self,
        heritage: Option<&NodeList>,
    ) -> Option<String> {
        let heritage = heritage?;
        let (_, expr_idx) = self.non_nameable_extends_heritage_type(heritage)?;
        let expr_idx = self.skip_parenthesized_expression(expr_idx)?;
        let expr_node = self.arena.get(expr_idx)?;
        if expr_node.kind != syntax_kind_ext::CALL_EXPRESSION {
            return None;
        }

        let call = self.arena.get_call_expr(expr_node)?;
        let arguments = call.arguments.as_ref()?;
        for arg_idx in arguments.nodes.iter().copied() {
            let Some(arg_node) = self.arena.get(arg_idx) else {
                continue;
            };
            if arg_node.kind != syntax_kind_ext::ARROW_FUNCTION
                && arg_node.kind != syntax_kind_ext::FUNCTION_EXPRESSION
            {
                continue;
            }
            if let Some(type_text) =
                self.function_returned_local_class_constructor_type_text(arg_idx)
            {
                return Some(type_text);
            }
        }

        self.call_expression_returned_local_class_constructor_text(expr_idx, true)
    }

    pub(in crate::declaration_emitter) fn call_expression_returned_local_class_constructor_text(
        &self,
        expr_idx: NodeIndex,
        arrow_form: bool,
    ) -> Option<String> {
        let expr_node = self.arena.get(expr_idx)?;
        if expr_node.kind != syntax_kind_ext::CALL_EXPRESSION {
            return None;
        }
        let call = self.arena.get_call_expr(expr_node)?;
        let sym_id = self.value_reference_symbol(call.expression)?;
        let binder = self.binder?;
        let symbol = binder.symbols.get(sym_id)?;
        let source_arena = binder
            .symbol_arenas
            .get(&sym_id)
            .map(|arena| arena.as_ref())
            .unwrap_or(self.arena);
        if !std::ptr::eq(source_arena, self.arena) {
            return None;
        }

        for decl_idx in symbol.declarations.iter().copied() {
            let Some(func) = self.callable_function_from_symbol_decl(self.arena, decl_idx) else {
                continue;
            };
            let (class_idx, base_param_index) =
                self.function_returned_local_class_extends_parameter(func)?;
            let args = call.arguments.as_ref()?;
            let base_arg = args.nodes.get(base_param_index).copied()?;
            let base_type_text =
                self.direct_value_reference_typeof_text(base_arg)
                    .or_else(|| {
                        self.nameable_constructor_expression_text(base_arg)
                            .map(|name| format!("typeof {name}"))
                    })?;
            return self.local_class_constructor_type_text_from_ast(
                class_idx,
                Some(&base_type_text),
                arrow_form,
            );
        }

        None
    }

    fn function_returned_local_class_extends_parameter(
        &self,
        func: &tsz_parser::parser::node::FunctionData,
    ) -> Option<(NodeIndex, usize)> {
        let body_node = self.arena.get(func.body)?;
        let block = self.arena.get_block(body_node)?;

        let returned = block
            .statements
            .nodes
            .iter()
            .copied()
            .find_map(|stmt_idx| {
                let stmt_node = self.arena.get(stmt_idx)?;
                if stmt_node.kind != syntax_kind_ext::RETURN_STATEMENT {
                    return None;
                }
                let ret = self.arena.get_return_statement(stmt_node)?;
                if !ret.expression.is_some() {
                    return None;
                }
                self.skip_parenthesized_expression(ret.expression)
            })?;

        let returned_node = self.arena.get(returned)?;
        let class_idx = if returned_node.kind == syntax_kind_ext::CLASS_EXPRESSION {
            returned
        } else if returned_node.kind == SyntaxKind::Identifier as u16 {
            let returned_name = self.get_identifier_text(returned)?;
            block.statements.nodes.iter().copied().find(|&stmt_idx| {
                let Some(stmt_node) = self.arena.get(stmt_idx) else {
                    return false;
                };
                if stmt_node.kind != syntax_kind_ext::CLASS_DECLARATION {
                    return false;
                }
                self.arena
                    .get_class(stmt_node)
                    .and_then(|class| self.get_identifier_text(class.name))
                    .as_deref()
                    == Some(returned_name.as_str())
            })?
        } else {
            return None;
        };

        let class_node = self.arena.get(class_idx)?;
        let class = self.arena.get_class(class_node)?;
        let heritage_clauses = class.heritage_clauses.as_ref()?;
        for clause_idx in heritage_clauses.nodes.iter().copied() {
            let heritage = self.arena.get_heritage_clause_at(clause_idx)?;
            if heritage.token != SyntaxKind::ExtendsKeyword as u16 {
                continue;
            }
            let base_idx = heritage.types.nodes.first().copied()?;
            let base_node = self.arena.get(base_idx)?;
            let base_expr = self
                .arena
                .get_expr_type_args(base_node)
                .map(|expr| expr.expression)
                .unwrap_or(base_idx);
            let base_name = self.get_identifier_text(base_expr)?;
            for (idx, param_idx) in func.parameters.nodes.iter().copied().enumerate() {
                let param_node = self.arena.get(param_idx)?;
                let param = self.arena.get_parameter(param_node)?;
                if self.get_identifier_text(param.name).as_deref() == Some(base_name.as_str()) {
                    return Some((class_idx, idx));
                }
            }
        }

        None
    }

    fn function_returned_local_class_constructor_type_text(
        &self,
        func_idx: NodeIndex,
    ) -> Option<String> {
        let func_node = self.arena.get(func_idx)?;
        let func = self.arena.get_function(func_node)?;
        let body_node = self.arena.get(func.body)?;
        let block = self.arena.get_block(body_node)?;

        let returned = block
            .statements
            .nodes
            .iter()
            .copied()
            .find_map(|stmt_idx| {
                let stmt_node = self.arena.get(stmt_idx)?;
                if stmt_node.kind != syntax_kind_ext::RETURN_STATEMENT {
                    return None;
                }
                let ret = self.arena.get_return_statement(stmt_node)?;
                if !ret.expression.is_some() {
                    return None;
                }
                self.skip_parenthesized_expression(ret.expression)
            })?;

        let returned_node = self.arena.get(returned)?;
        if returned_node.kind == syntax_kind_ext::CLASS_EXPRESSION {
            return self.class_constructor_object_type_text_from_ast(returned);
        }

        if returned_node.kind != SyntaxKind::Identifier as u16 {
            return None;
        }
        let returned_name = self.get_identifier_text(returned)?;

        block.statements.nodes.iter().copied().find_map(|stmt_idx| {
            let stmt_node = self.arena.get(stmt_idx)?;
            if stmt_node.kind != syntax_kind_ext::CLASS_DECLARATION {
                return None;
            }
            let class = self.arena.get_class(stmt_node)?;
            (self.get_identifier_text(class.name).as_deref() == Some(returned_name.as_str()))
                .then(|| self.local_class_constructor_type_text_from_ast(stmt_idx, None, false))
                .flatten()
        })
    }

    fn class_constructor_object_type_text_from_ast(&self, class_idx: NodeIndex) -> Option<String> {
        self.local_class_constructor_type_text_from_ast(class_idx, None, false)
    }

    fn local_class_constructor_type_text_from_ast(
        &self,
        class_idx: NodeIndex,
        base_type_text: Option<&str>,
        arrow_form: bool,
    ) -> Option<String> {
        let class_node = self.arena.get(class_idx)?;
        let class = self.arena.get_class(class_node)?;

        let mut params_text = String::new();
        if let Some(ctor_idx) = class.members.nodes.iter().copied().find(|&member_idx| {
            self.arena
                .get(member_idx)
                .is_some_and(|node| node.kind == syntax_kind_ext::CONSTRUCTOR)
        }) {
            let ctor = self
                .arena
                .get(ctor_idx)
                .and_then(|node| self.arena.get_constructor(node))?;
            let mut scratch = self.scratch_declaration_emitter();
            scratch.in_constructor_params = true;
            scratch.emit_parameters_with_body(&ctor.parameters, ctor.body);
            scratch.in_constructor_params = false;
            params_text = scratch.writer.take_output();
        }
        if params_text.is_empty() && base_type_text.is_some() {
            params_text = "...args: any[]".to_string();
        }

        let mut member_scratch = self.scratch_declaration_emitter();
        member_scratch.indent_level = if arrow_form {
            self.indent_level + 1
        } else {
            self.indent_level + 2
        };
        for member_idx in class.members.nodes.iter().copied() {
            let Some(member_node) = self.arena.get(member_idx) else {
                continue;
            };
            if member_node.kind == syntax_kind_ext::CONSTRUCTOR {
                continue;
            }
            member_scratch.emit_class_member(member_idx);
        }
        let members = member_scratch.writer.take_output();
        let members = Self::strip_abstract_member_modifiers(members.trim_end());
        let members = members.as_str();

        let constructor_type = if arrow_form {
            let is_abstract = self
                .arena
                .has_modifier(&class.modifiers, SyntaxKind::AbstractKeyword);
            let prefix = if is_abstract { "abstract new " } else { "new " };
            if members.is_empty() {
                format!("{prefix}({params_text}) => {{}}")
            } else {
                format!("{prefix}({params_text}) => {{\n{members}\n}}")
            }
        } else if members.is_empty() {
            format!("{{\n    new ({params_text}): {{}};\n}}")
        } else {
            format!("{{\n    new ({params_text}): {{\n{members}\n    }};\n}}")
        };

        if let Some(base_type_text) = base_type_text {
            if arrow_form {
                Some(format!("({constructor_type}) & {base_type_text}"))
            } else {
                Some(format!("{constructor_type} & {base_type_text}"))
            }
        } else {
            Some(constructor_type)
        }
    }

    fn strip_abstract_member_modifiers(members: &str) -> String {
        members
            .lines()
            .map(|line| {
                let trimmed = line.trim_start();
                if let Some(rest) = trimmed.strip_prefix("abstract ") {
                    let indent_len = line.len() - trimmed.len();
                    format!("{}{}", &line[..indent_len], rest)
                } else {
                    line.to_string()
                }
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn type_annotation_text_from_arena_node(
        &self,
        source_arena: &NodeArena,
        type_annotation: NodeIndex,
    ) -> Option<String> {
        let printed = self
            .get_node_type(type_annotation)
            .map(|type_id| self.print_type_id(type_id));
        let type_text = if std::ptr::eq(source_arena, self.arena) {
            self.preferred_annotation_name_text(type_annotation)
                .or_else(|| self.emit_type_node_text(type_annotation))
        } else {
            self.source_slice_from_arena(source_arena, type_annotation)
                .or_else(|| self.emit_type_node_text_from_arena(source_arena, type_annotation))
        }?;
        let type_text = if std::ptr::eq(source_arena, self.arena) {
            if printed.as_deref().is_some_and(|printed| printed != "any")
                && let Some(raw_type_text) = self.local_type_annotation_text(type_annotation)
                && Self::type_text_starts_with_string_intrinsic(&raw_type_text)
            {
                raw_type_text
            } else {
                match printed {
                    Some(printed)
                        if printed != "any"
                            && (!printed.contains("any") || type_text.contains("any"))
                            && printed.contains("typeof ")
                            && !type_text.contains("typeof ") =>
                    {
                        printed.replace("typeof ", "")
                    }
                    Some(printed)
                        if printed != "any"
                            && (!printed.contains("any") || type_text.contains("any")) =>
                    {
                        printed
                    }
                    _ => type_text,
                }
            }
        } else {
            let rewritten = self.qualify_foreign_imported_names_in_text(source_arena, &type_text);
            let expands_mapped_object =
                Self::contains_portable_mapped_object_text(rewritten.as_str());
            let rewritten = self
                .expand_portable_intersection_type_text(source_arena, &rewritten)
                .unwrap_or(rewritten);
            match printed {
                Some(ref printed)
                    if printed != "any"
                        && !printed.contains("any")
                        && !expands_mapped_object
                        && (!Self::type_text_contains_import_type(&rewritten)
                            || Self::type_text_contains_import_type(printed)) =>
                {
                    printed.clone()
                }
                _ => rewritten,
            }
        };
        let trimmed = type_text.trim_end();
        let trimmed = trimmed.strip_suffix(';').unwrap_or(trimmed).trim_end();
        let trimmed = trimmed.strip_suffix('=').unwrap_or(trimmed).trim_end();
        Some(trimmed.to_string())
    }

    pub(in crate::declaration_emitter) fn contains_portable_mapped_object_text(text: &str) -> bool {
        Self::split_top_level_intersection_parts(text)
            .iter()
            .any(|part| {
                let trimmed = part.trim().trim_end_matches(';').trim();
                trimmed
                    .strip_prefix('{')
                    .and_then(|inner| inner.strip_suffix('}'))
                    .is_some_and(|inner| {
                        inner.trim_start().starts_with('[')
                            && Self::type_text_contains_import_type(inner)
                    })
            })
    }

    pub(in crate::declaration_emitter) fn expand_portable_intersection_type_text(
        &self,
        source_arena: &NodeArena,
        text: &str,
    ) -> Option<String> {
        let parts = Self::split_top_level_intersection_parts(text);
        if parts.len() <= 1 {
            return self.expand_portable_object_type_text(source_arena, text);
        }

        let mut changed = false;
        let expanded_parts: Vec<String> = parts
            .into_iter()
            .map(|part| {
                if let Some(expanded) =
                    self.expand_portable_object_type_text(source_arena, part.trim())
                {
                    changed = true;
                    expanded
                } else {
                    part.trim().to_string()
                }
            })
            .collect();

        changed.then(|| expanded_parts.join(" & "))
    }

    fn split_top_level_intersection_parts(text: &str) -> Vec<String> {
        let bytes = text.as_bytes();
        let mut brace_depth = 0usize;
        let mut bracket_depth = 0usize;
        let mut paren_depth = 0usize;
        let mut angle_depth = 0usize;
        let mut part_start = 0usize;
        let mut parts = Vec::new();
        let mut i = 0usize;
        while i < bytes.len() {
            match bytes[i] {
                b'{' => brace_depth += 1,
                b'}' => brace_depth = brace_depth.saturating_sub(1),
                b'[' => bracket_depth += 1,
                b']' => bracket_depth = bracket_depth.saturating_sub(1),
                b'(' => paren_depth += 1,
                b')' => paren_depth = paren_depth.saturating_sub(1),
                b'<' => angle_depth += 1,
                b'>' => angle_depth = angle_depth.saturating_sub(1),
                b'&' if brace_depth == 0
                    && bracket_depth == 0
                    && paren_depth == 0
                    && angle_depth == 0 =>
                {
                    let part = text
                        .get(part_start..i)
                        .map(str::trim)
                        .unwrap_or_default()
                        .to_string();
                    if !part.is_empty() {
                        parts.push(part);
                    }
                    part_start = i + 1;
                }
                _ => {}
            }
            i += 1;
        }
        let tail = text
            .get(part_start..)
            .map(str::trim)
            .unwrap_or_default()
            .to_string();
        if !tail.is_empty() {
            parts.push(tail);
        }
        parts
    }

    fn expand_portable_object_type_text(
        &self,
        source_arena: &NodeArena,
        text: &str,
    ) -> Option<String> {
        let trimmed = text.trim().trim_end_matches(';').trim();
        let inner = trimmed.strip_prefix('{')?.strip_suffix('}')?.trim();
        if inner.starts_with('[') {
            return self.expand_portable_mapped_object_text(source_arena, inner);
        }
        (!inner.is_empty()).then(|| format!("{{\n    {};\n}}", inner.trim().trim_end_matches(';')))
    }

    pub(in crate::declaration_emitter) fn expand_portable_mapped_object_text_in_current_context(
        &self,
        text: &str,
    ) -> Option<String> {
        Self::contains_portable_mapped_object_text(text)
            .then(|| self.expand_portable_intersection_type_text(self.arena, text))
            .flatten()
    }

    pub(in crate::declaration_emitter) fn expand_portable_mapped_object_text(
        &self,
        source_arena: &NodeArena,
        inner: &str,
    ) -> Option<String> {
        let in_pos = inner.find(" in ")?;
        let after_in = inner.get(in_pos + 4..)?;
        let close_bracket = after_in.find(']')?;
        let key_ref = after_in.get(..close_bracket)?.trim();
        let after_bracket = after_in.get(close_bracket + 1..)?.trim_start();
        let after_optional = after_bracket.strip_prefix("?:")?.trim_start();
        let value_type = after_optional.trim_end().trim_end_matches(';').trim();
        let (module_specifier, export_name) = Self::parse_import_type_reference(key_ref)?;
        let keys = self.expand_imported_string_union_alias_keys(
            source_arena,
            &module_specifier,
            &export_name,
        )?;
        let members: Vec<String> = keys
            .into_iter()
            .map(|key| format!("    {key}?: {value_type} | undefined;"))
            .collect();
        Some(format!("{{\n{}\n}}", members.join("\n")))
    }

    fn parse_import_type_reference(text: &str) -> Option<(String, String)> {
        let (start, module_specifier, tail) = Self::next_import_type_text(text)?;
        if start != 0 {
            return None;
        }
        let export_name = tail.trim().strip_prefix('.')?.to_string();
        Some((module_specifier, export_name))
    }

    fn expand_imported_string_union_alias_keys(
        &self,
        source_arena: &NodeArena,
        module_specifier: &str,
        export_name: &str,
    ) -> Option<Vec<String>> {
        let binder = self.binder?;
        let source_path = self
            .arena_to_path
            .get(&(source_arena as *const NodeArena as usize))
            .cloned()
            .or_else(|| {
                self.arena_source_file(source_arena)
                    .map(|source_file| source_file.file_name.clone())
            })
            .or_else(|| self.current_file_path.clone())?;

        let mut module_paths =
            self.matching_module_export_paths(binder, &source_path, module_specifier);
        if module_paths.is_empty()
            && !module_specifier.starts_with('.')
            && !module_specifier.starts_with('/')
        {
            module_paths = self.package_root_index_module_export_paths(binder, module_specifier);
        }

        for module_path in module_paths {
            let Some(exports) = binder.module_exports.get(module_path) else {
                continue;
            };
            let Some(export_sym_id) = exports.get(export_name) else {
                continue;
            };
            if let Some((foreign_arena, alias_type_node)) =
                self.exported_type_alias_type_node_in_module_path(module_path, export_name)
                && let Some(keys) = self.expand_string_literals_from_type_node_in_arena(
                    foreign_arena,
                    alias_type_node,
                    &FxHashMap::default(),
                    0,
                )
            {
                return Some(keys);
            }
            if let Some(keys) =
                self.expand_string_literals_from_type_alias_file(module_path, export_name)
            {
                return Some(keys);
            }
            if let Some(keys) =
                self.with_symbol_declarations(export_sym_id, |foreign_arena, decl_idx| {
                    let mut current = decl_idx;
                    for _ in 0..4 {
                        let decl_node = foreign_arena.get(current)?;
                        if let Some(alias) = foreign_arena.get_type_alias(decl_node) {
                            return self.expand_string_literals_from_type_node_in_arena(
                                foreign_arena,
                                alias.type_node,
                                &FxHashMap::default(),
                                0,
                            );
                        }
                        current = foreign_arena.parent_of(current)?;
                        if !current.is_some() {
                            return None;
                        }
                    }
                    None
                })
            {
                return Some(keys);
            }
        }

        None
    }

    fn package_root_index_module_export_paths<'b>(
        &self,
        binder: &'b BinderState,
        module_specifier: &str,
    ) -> Vec<&'b str> {
        let mut matches: Vec<_> = binder
            .module_exports
            .keys()
            .filter_map(|module_path| {
                let package_root =
                    Self::deepest_node_modules_package_root_path(module_path, module_specifier)?;
                let suffix = module_path.strip_prefix(&package_root)?;
                matches!(
                    suffix.trim_start_matches(['/', '\\']),
                    "index.d.ts" | "index.ts"
                )
                .then_some(module_path.as_str())
            })
            .collect();
        matches.sort_by(|left, right| right.len().cmp(&left.len()).then_with(|| left.cmp(right)));
        matches
    }

    fn deepest_node_modules_package_root_path(
        module_path: &str,
        module_specifier: &str,
    ) -> Option<String> {
        use std::path::{Component, Path, PathBuf};

        let components: Vec<_> = Path::new(module_path).components().collect();
        let pkg_len = if module_specifier.starts_with('@') {
            2
        } else {
            1
        };
        components
            .iter()
            .enumerate()
            .filter_map(|(idx, component)| {
                matches!(component, Component::Normal(part) if part.to_str() == Some("node_modules"))
                    .then_some(idx)
            })
            .filter_map(|nm_idx| {
                let pkg_start = nm_idx + 1;
                if components.len() < pkg_start + pkg_len {
                    return None;
                }
                let package_name = components[pkg_start..pkg_start + pkg_len]
                    .iter()
                    .filter_map(|component| match component {
                        Component::Normal(part) => part.to_str(),
                        _ => None,
                    })
                    .collect::<Vec<_>>()
                    .join("/");
                (package_name == module_specifier).then(|| {
                    components[..pkg_start + pkg_len]
                        .iter()
                        .fold(PathBuf::new(), |mut path, component| {
                            path.push(component.as_os_str());
                            path
                        })
                        .to_string_lossy()
                        .into_owned()
                })
            })
            .max_by_key(|path| path.len())
    }

    fn expand_string_literals_from_type_alias_file(
        &self,
        module_path: &str,
        export_name: &str,
    ) -> Option<Vec<String>> {
        let source = std::fs::read_to_string(module_path).ok()?;
        let mut parser = ParserState::new(module_path.to_string(), source);
        let _root = parser.parse_source_file();
        let alias_type_node = self
            .find_type_alias_type_node_in_arena(&parser.arena, export_name)
            .or_else(|| self.type_alias_type_node_by_name_in_arena(&parser.arena, export_name))?;
        self.expand_string_literals_from_type_node_in_arena(
            &parser.arena,
            alias_type_node,
            &FxHashMap::default(),
            0,
        )
    }

    fn exported_type_alias_type_node_in_module_path<'arena>(
        &'arena self,
        module_path: &str,
        export_name: &str,
    ) -> Option<(&'arena NodeArena, NodeIndex)> {
        if self.arena_matches_module_path(self.arena, module_path)
            && let Some(type_node) =
                self.find_type_alias_type_node_in_arena(self.arena, export_name)
        {
            return Some((self.arena, type_node));
        }

        for arena in self.global_symbol_arenas.values() {
            if self.arena_matches_module_path(arena.as_ref(), module_path)
                && let Some(type_node) =
                    self.find_type_alias_type_node_in_arena(arena.as_ref(), export_name)
            {
                return Some((arena.as_ref(), type_node));
            }
        }

        let binder = self.binder?;
        for arenas in binder.declaration_arenas.values() {
            for arena in arenas {
                if self.arena_matches_module_path(arena.as_ref(), module_path)
                    && let Some(type_node) =
                        self.find_type_alias_type_node_in_arena(arena.as_ref(), export_name)
                {
                    return Some((arena.as_ref(), type_node));
                }
            }
        }

        for arena in binder.symbol_arenas.values() {
            if self.arena_matches_module_path(arena.as_ref(), module_path)
                && let Some(type_node) =
                    self.find_type_alias_type_node_in_arena(arena.as_ref(), export_name)
            {
                return Some((arena.as_ref(), type_node));
            }
        }

        None
    }

    fn type_alias_type_node_by_name_in_arena(
        &self,
        arena: &NodeArena,
        export_name: &str,
    ) -> Option<NodeIndex> {
        for node_id in 0..arena.len() {
            let node_idx = NodeIndex(u32::try_from(node_id).ok()?);
            let node = arena.get(node_idx)?;
            let Some(alias) = arena.get_type_alias(node) else {
                continue;
            };
            if self
                .identifier_text_from_arena(arena, alias.name)
                .is_some_and(|name| name == export_name)
            {
                return Some(alias.type_node);
            }
        }

        None
    }

    fn arena_matches_module_path(&self, arena: &NodeArena, module_path: &str) -> bool {
        let arena_ptr = arena as *const NodeArena as usize;
        self.arena_to_path
            .get(&arena_ptr)
            .is_some_and(|path| path == module_path)
            || self
                .arena_source_file(arena)
                .is_some_and(|source_file| source_file.file_name == module_path)
    }

    fn type_reference_name_text_from_arena(
        &self,
        arena: &NodeArena,
        name_idx: NodeIndex,
    ) -> Option<String> {
        let name_node = arena.get(name_idx)?;
        if name_node.kind == SyntaxKind::Identifier as u16 {
            return self.identifier_text_from_arena(arena, name_idx);
        }
        if name_node.kind == syntax_kind_ext::QUALIFIED_NAME {
            let qualified = arena.get_qualified_name(name_node)?;
            return self.identifier_text_from_arena(arena, qualified.right);
        }
        None
    }

    pub(in crate::declaration_emitter) fn find_type_alias_type_node_in_arena(
        &self,
        arena: &NodeArena,
        name: &str,
    ) -> Option<NodeIndex> {
        let source_file = self.arena_source_file(arena)?;
        for &stmt_idx in &source_file.statements.nodes {
            let stmt_node = arena.get(stmt_idx)?;
            let Some(alias) = arena.get_type_alias(stmt_node) else {
                continue;
            };
            if self
                .identifier_text_from_arena(arena, alias.name)
                .as_deref()
                == Some(name)
            {
                return Some(alias.type_node);
            }
        }
        None
    }

    fn expand_string_literals_from_type_node_in_arena(
        &self,
        arena: &NodeArena,
        type_node: NodeIndex,
        substitutions: &FxHashMap<String, String>,
        depth: usize,
    ) -> Option<Vec<String>> {
        if depth > 32 {
            return None;
        }

        let node = arena.get(type_node)?;
        match node.kind {
            k if k == syntax_kind_ext::UNION_TYPE => {
                let composite = arena.get_composite_type(node)?;
                let mut result = Vec::new();
                for &child in &composite.types.nodes {
                    result.extend(self.expand_string_literals_from_type_node_in_arena(
                        arena,
                        child,
                        substitutions,
                        depth + 1,
                    )?);
                }
                Some(result)
            }
            k if k == syntax_kind_ext::LITERAL_TYPE => {
                let literal = arena.get_literal_type(node)?;
                let literal_node = arena.get(literal.literal)?;
                if literal_node.kind == SyntaxKind::StringLiteral as u16 {
                    Some(vec![arena.get_literal(literal_node)?.text.clone()])
                } else {
                    None
                }
            }
            k if k == syntax_kind_ext::TYPE_REFERENCE => {
                let type_ref = arena.get_type_ref(node)?;
                let name = self.type_reference_name_text_from_arena(arena, type_ref.type_name)?;
                if let Some(value) = substitutions.get(&name) {
                    return Some(vec![value.clone()]);
                }
                if let Some(alias_type) = self.find_type_alias_type_node_in_arena(arena, &name) {
                    return self.expand_string_literals_from_type_node_in_arena(
                        arena,
                        alias_type,
                        substitutions,
                        depth + 1,
                    );
                }
                let source_file = self.arena_source_file(arena)?;
                for &stmt_idx in &source_file.statements.nodes {
                    let stmt_node = arena.get(stmt_idx)?;
                    let Some(import) = arena.get_import_decl(stmt_node) else {
                        continue;
                    };
                    let Some(module_node) = arena.get(import.module_specifier) else {
                        continue;
                    };
                    let Some(module_lit) = arena.get_literal(module_node) else {
                        continue;
                    };
                    let Some(clause_node) = arena.get(import.import_clause) else {
                        continue;
                    };
                    let Some(clause) = arena.get_import_clause(clause_node) else {
                        continue;
                    };
                    let Some(bindings_node) = arena.get(clause.named_bindings) else {
                        continue;
                    };
                    let Some(bindings) = arena.get_named_imports(bindings_node) else {
                        continue;
                    };
                    for &spec_idx in &bindings.elements.nodes {
                        let spec_node = arena.get(spec_idx)?;
                        let specifier = arena.get_specifier(spec_node)?;
                        let local_name = self.identifier_text_from_arena(arena, specifier.name)?;
                        if local_name != name {
                            continue;
                        }
                        let imported_name = if specifier.property_name.is_some() {
                            self.identifier_text_from_arena(arena, specifier.property_name)
                                .unwrap_or(local_name)
                        } else {
                            local_name
                        };
                        return self.expand_imported_string_union_alias_keys(
                            arena,
                            module_lit.text.as_str(),
                            &imported_name,
                        );
                    }
                }
                None
            }
            k if k == SyntaxKind::Identifier as u16 => {
                let name = self.identifier_text_from_arena(arena, type_node)?;
                if let Some(value) = substitutions.get(&name) {
                    return Some(vec![value.clone()]);
                }
                let alias_type = self.find_type_alias_type_node_in_arena(arena, &name)?;
                self.expand_string_literals_from_type_node_in_arena(
                    arena,
                    alias_type,
                    substitutions,
                    depth + 1,
                )
            }
            _ => None,
        }
    }

    fn declared_type_annotation_text_for_symbol(&self, sym_id: SymbolId) -> Option<String> {
        self.with_symbol_declarations(sym_id, |source_arena, decl_idx| {
            let decl_idx = Self::annotation_bearing_declaration_from_arena(source_arena, decl_idx)
                .unwrap_or(decl_idx);
            let decl_node = source_arena.get(decl_idx)?;
            let type_annotation = source_arena
                .get_variable_declaration(decl_node)
                .map(|decl| decl.type_annotation)
                .or_else(|| {
                    source_arena
                        .get_property_decl(decl_node)
                        .map(|decl| decl.type_annotation)
                })
                .or_else(|| {
                    source_arena
                        .get_parameter(decl_node)
                        .map(|param| param.type_annotation)
                })
                .filter(|type_idx| type_idx.is_some())?;
            self.type_annotation_text_from_arena_node(source_arena, type_annotation)
        })
    }

    fn annotation_bearing_declaration_from_arena(
        arena: &NodeArena,
        decl_idx: NodeIndex,
    ) -> Option<NodeIndex> {
        let mut current = decl_idx;
        for _ in 0..12 {
            let node = arena.get(current)?;
            if arena.get_variable_declaration(node).is_some()
                || arena.get_property_decl(node).is_some()
                || arena.get_parameter(node).is_some()
                || arena.get_interface(node).is_some()
                || arena.get_class(node).is_some()
                || arena.get_type_alias(node).is_some()
            {
                return Some(current);
            }
            let parent = arena.parent_of(current)?;
            if parent.is_none() {
                break;
            }
            current = parent;
        }
        None
    }

    pub(in crate::declaration_emitter) fn emit_type_node_text_from_arena(
        &self,
        source_arena: &NodeArena,
        type_idx: NodeIndex,
    ) -> Option<String> {
        source_arena.get(type_idx)?;

        let mut scratch = if let (Some(type_cache), Some(type_interner), Some(binder)) =
            (&self.type_cache, self.type_interner, self.binder)
        {
            DeclarationEmitter::with_type_info(
                source_arena,
                type_cache.clone(),
                type_interner,
                binder,
            )
        } else {
            DeclarationEmitter::new(source_arena)
        };

        let source_file = self.arena_source_file(source_arena);
        scratch.source_is_declaration_file = source_file
            .map(|source_file| source_file.is_declaration_file)
            .unwrap_or(self.source_is_declaration_file);
        scratch.source_is_js_file = self.source_is_js_file;
        scratch.current_source_file_idx = source_file
            .and_then(|_| {
                source_arena
                    .nodes
                    .iter()
                    .position(|node| source_arena.get_source_file(node).is_some())
                    .and_then(|idx| u32::try_from(idx).ok())
                    .map(NodeIndex)
            })
            .or(self.current_source_file_idx);
        scratch.source_file_text = source_file.map(|source_file| source_file.text.clone());
        scratch.current_file_path = self
            .arena_to_path
            .get(&(source_arena as *const NodeArena as usize))
            .cloned()
            .or_else(|| source_file.map(|source_file| source_file.file_name.clone()))
            .or_else(|| self.current_file_path.clone());
        scratch.current_arena = self.current_arena.clone();
        scratch.arena_to_path = self.arena_to_path.clone();
        scratch.emit_type(type_idx);
        Some(scratch.writer.take_output())
    }

    fn explicit_asserted_type_node_from_arena(
        arena: &NodeArena,
        expr_idx: NodeIndex,
    ) -> Option<NodeIndex> {
        let mut current = expr_idx;

        for _ in 0..100 {
            let node = arena.get(current)?;
            if node.kind == syntax_kind_ext::PARENTHESIZED_EXPRESSION
                && let Some(paren) = arena.get_parenthesized(node)
            {
                current = paren.expression;
                continue;
            }
            if node.kind == syntax_kind_ext::NON_NULL_EXPRESSION
                && let Some(unary) = arena.get_unary_expr_ex(node)
            {
                current = unary.expression;
                continue;
            }
            if node.kind == syntax_kind_ext::BINARY_EXPRESSION
                && let Some(binary) = arena.get_binary_expr(node)
                && binary.operator_token == SyntaxKind::CommaToken as u16
            {
                current = binary.right;
                continue;
            }

            let assertion = arena.get_type_assertion(node)?;
            let asserted_type = arena.get(assertion.type_node)?;
            if asserted_type.kind == SyntaxKind::ConstKeyword as u16 {
                return None;
            }
            return Some(assertion.type_node);
        }

        None
    }

    fn declaration_type_symbol_from_type_node(
        &self,
        arena: &NodeArena,
        type_idx: NodeIndex,
    ) -> Option<SymbolId> {
        let binder = self.binder?;
        let type_node = arena.get(type_idx)?;
        match type_node.kind {
            k if k == syntax_kind_ext::TYPE_REFERENCE => {
                let type_ref = arena.get_type_ref(type_node)?;
                if std::ptr::eq(arena, self.arena)
                    && let Some(name) = self.get_identifier_text(type_ref.type_name)
                    && let Some(sym_id) = self.resolve_identifier_symbol(type_ref.type_name, &name)
                {
                    Some(sym_id)
                } else {
                    binder.get_node_symbol(type_ref.type_name)
                }
            }
            k if k == SyntaxKind::Identifier as u16 || k == syntax_kind_ext::QUALIFIED_NAME => {
                if std::ptr::eq(arena, self.arena)
                    && let Some(name) = self.get_identifier_text(type_idx)
                    && let Some(sym_id) = self.resolve_identifier_symbol(type_idx, &name)
                {
                    Some(sym_id)
                } else {
                    binder.get_node_symbol(type_idx).or_else(|| {
                        self.identifier_text_from_arena(arena, type_idx)
                            .and_then(|name| binder.symbols.find_by_name(&name))
                    })
                }
            }
            _ => None,
        }
    }

    fn property_access_declared_type_annotation_text(&self, expr_idx: NodeIndex) -> Option<String> {
        let binder = self.binder?;
        let expr_node = self.arena.get(expr_idx)?;
        if expr_node.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            return None;
        }

        let access = self.arena.get_access_expr(expr_node)?;
        let member_name = self.get_identifier_text(access.name_or_argument)?;
        let base_sym_id = self.value_reference_symbol(access.expression)?;

        self.with_symbol_declarations(base_sym_id, |source_arena, decl_idx| {
            let decl_idx = Self::annotation_bearing_declaration_from_arena(source_arena, decl_idx)
                .unwrap_or(decl_idx);
            let decl_node = source_arena.get(decl_idx)?;
            let declared_type = source_arena
                .get_variable_declaration(decl_node)
                .and_then(|decl| {
                    if decl.type_annotation.is_some() {
                        Some(decl.type_annotation)
                    } else if decl.initializer.is_some() {
                        Self::explicit_asserted_type_node_from_arena(source_arena, decl.initializer)
                    } else {
                        None
                    }
                })
                .or_else(|| {
                    source_arena.get_parameter(decl_node).and_then(|param| {
                        if param.type_annotation.is_some() {
                            Some(param.type_annotation)
                        } else {
                            None
                        }
                    })
                })
                .or_else(|| {
                    source_arena.get_property_decl(decl_node).and_then(|decl| {
                        if decl.type_annotation.is_some() {
                            Some(decl.type_annotation)
                        } else if decl.initializer.is_some() {
                            Self::explicit_asserted_type_node_from_arena(
                                source_arena,
                                decl.initializer,
                            )
                        } else {
                            None
                        }
                    })
                })?;

            let declared_type_sym_id =
                self.declaration_type_symbol_from_type_node(source_arena, declared_type)?;
            let declared_type_sym_id = self
                .resolve_portability_import_alias(declared_type_sym_id, binder)
                .unwrap_or(declared_type_sym_id);
            let declared_type_sym_id =
                self.resolve_portability_declaration_symbol(declared_type_sym_id, binder);
            self.type_member_declared_type_annotation_text(declared_type_sym_id, &member_name)
        })
    }

    pub(in crate::declaration_emitter) fn type_member_declared_type_annotation_text(
        &self,
        type_sym_id: SymbolId,
        member_name: &str,
    ) -> Option<String> {
        let binder = self.binder?;
        let member_sym_id = binder
            .symbols
            .get(type_sym_id)
            .and_then(|symbol| symbol.members.as_ref())
            .and_then(|members| members.get(member_name));
        let printed_member_type = member_sym_id.and_then(|member_sym_id| {
            self.type_cache
                .as_ref()
                .and_then(|cache| cache.symbol_types.get(&member_sym_id))
                .copied()
                .map(|type_id| self.print_type_id(type_id))
        });

        self.with_symbol_declarations(type_sym_id, |source_arena, decl_idx| {
            let decl_idx = Self::annotation_bearing_declaration_from_arena(source_arena, decl_idx)
                .unwrap_or(decl_idx);
            let decl_node = source_arena.get(decl_idx)?;
            let mut members: Vec<NodeIndex> = Vec::new();
            if let Some(interface) = source_arena.get_interface(decl_node) {
                members.extend(interface.members.nodes.iter().copied());
            }
            if let Some(class_decl) = source_arena.get_class(decl_node) {
                members.extend(class_decl.members.nodes.iter().copied());
            }
            if let Some(type_alias) = source_arena.get_type_alias(decl_node)
                && let Some(type_node) = source_arena.get(type_alias.type_node)
                && type_node.kind == syntax_kind_ext::TYPE_LITERAL
                && let Some(type_literal) = source_arena.get_type_literal(type_node)
            {
                members.extend(type_literal.members.nodes.iter().copied());
            }

            for member_idx in members {
                let Some(member_node) = source_arena.get(member_idx) else {
                    continue;
                };
                if let Some(signature) = source_arena.get_signature(member_node)
                    && self
                        .property_name_text_from_arena(source_arena, signature.name)
                        .as_deref()
                        == Some(member_name)
                    && signature.type_annotation.is_some()
                {
                    let raw = self.type_annotation_text_from_arena_node(
                        source_arena,
                        signature.type_annotation,
                    );
                    if let Some(printed) = printed_member_type.as_ref() {
                        let printed =
                            self.qualify_foreign_imported_names_in_text(source_arena, printed);
                        if !printed.contains("any")
                            && (raw.as_ref().is_none_or(|raw| raw.contains("[k in"))
                                || !printed.contains("[k in"))
                        {
                            return Some(printed);
                        }
                    }
                    return raw;
                }
                if let Some(prop_decl) = source_arena.get_property_decl(member_node)
                    && self
                        .property_name_text_from_arena(source_arena, prop_decl.name)
                        .as_deref()
                        == Some(member_name)
                    && prop_decl.type_annotation.is_some()
                {
                    let raw = self.type_annotation_text_from_arena_node(
                        source_arena,
                        prop_decl.type_annotation,
                    );
                    if let Some(printed) = printed_member_type.as_ref() {
                        let printed =
                            self.qualify_foreign_imported_names_in_text(source_arena, printed);
                        if !printed.contains("any")
                            && (raw.as_ref().is_none_or(|raw| raw.contains("[k in"))
                                || !printed.contains("[k in"))
                        {
                            return Some(printed);
                        }
                    }
                    return raw;
                }
                if let Some(accessor) = source_arena.get_accessor(member_node)
                    && self
                        .property_name_text_from_arena(source_arena, accessor.name)
                        .as_deref()
                        == Some(member_name)
                    && accessor.type_annotation.is_some()
                {
                    let raw = self.type_annotation_text_from_arena_node(
                        source_arena,
                        accessor.type_annotation,
                    );
                    if let Some(printed) = printed_member_type.as_ref() {
                        let printed =
                            self.qualify_foreign_imported_names_in_text(source_arena, printed);
                        if !printed.contains("any")
                            && (raw.as_ref().is_none_or(|raw| raw.contains("[k in"))
                                || !printed.contains("[k in"))
                        {
                            return Some(printed);
                        }
                    }
                    return raw;
                }
            }

            if let Some(interface) = source_arena.get_interface(decl_node)
                && let Some(heritage_clauses) = interface.heritage_clauses.as_ref()
            {
                for &heritage_idx in &heritage_clauses.nodes {
                    let Some(heritage_node) = source_arena.get(heritage_idx) else {
                        continue;
                    };
                    let Some(heritage) = source_arena.get_heritage(heritage_node) else {
                        continue;
                    };
                    for &base_idx in &heritage.types.nodes {
                        let Some(base_node) = source_arena.get(base_idx) else {
                            continue;
                        };
                        let base_expr = source_arena
                            .get_expr_type_args(base_node)
                            .map_or(base_idx, |expr| expr.expression);
                        let Some(base_sym_id) =
                            self.declaration_type_symbol_from_type_node(source_arena, base_expr)
                        else {
                            continue;
                        };
                        if let Some(type_text) =
                            self.type_member_declared_type_annotation_text(base_sym_id, member_name)
                        {
                            return Some(type_text);
                        }
                    }
                }
            }

            None
        })
    }

    pub(crate) fn with_symbol_declarations<T>(
        &self,
        sym_id: SymbolId,
        mut f: impl FnMut(&NodeArena, NodeIndex) -> Option<T>,
    ) -> Option<T> {
        let binder = self.binder?;
        let symbol = binder.symbols.get(sym_id)?;
        for decl_idx in symbol.declarations.iter().copied() {
            if let Some(result) = self
                .arena
                .get(decl_idx)
                .and_then(|_| f(self.arena, decl_idx))
            {
                return Some(result);
            }
            if let Some(arenas) = binder.declaration_arenas.get(&(sym_id, decl_idx)) {
                for arena in arenas {
                    if let Some(result) = arena
                        .get(decl_idx)
                        .and_then(|_| f(arena.as_ref(), decl_idx))
                    {
                        return Some(result);
                    }
                }
            }
            if let Some(arena) = binder.symbol_arenas.get(&sym_id)
                && let Some(result) = arena
                    .get(decl_idx)
                    .and_then(|_| f(arena.as_ref(), decl_idx))
            {
                return Some(result);
            }
            if let Some(arena) = self.global_symbol_arenas.get(&sym_id)
                && let Some(result) = arena
                    .get(decl_idx)
                    .and_then(|_| f(arena.as_ref(), decl_idx))
            {
                return Some(result);
            }
        }

        None
    }

    fn replace_whole_words_in_text(text: &str, replacements: &[(String, String)]) -> String {
        if replacements.is_empty() {
            return text.to_string();
        }

        let protected_spans = Self::protected_type_text_literal_spans(text);
        let mut protected_idx = 0usize;
        let mut result = String::with_capacity(text.len() + 16);
        let bytes = text.as_bytes();
        let text_len = bytes.len();
        let mut last_copied = 0usize;
        let mut i = 0;
        while i < text_len {
            while protected_idx < protected_spans.len() && protected_spans[protected_idx].1 <= i {
                protected_idx += 1;
            }
            if let Some((start, end)) = protected_spans.get(protected_idx).copied()
                && start <= i
                && i < end
            {
                i = end;
                continue;
            }

            let mut best_match: Option<(&str, usize)> = None;
            for (word, replacement) in replacements {
                let word_bytes = word.as_bytes();
                let word_len = word_bytes.len();
                if word_len == 0 || i + word_len > text_len {
                    continue;
                }
                if &bytes[i..i + word_len] != word_bytes {
                    continue;
                }
                let before_ok = i == 0 || !Self::is_ident_char_in_text(bytes[i - 1]);
                let after_ok =
                    i + word_len >= text_len || !Self::is_ident_char_in_text(bytes[i + word_len]);
                let qualified_member = i > 0 && bytes[i - 1] == b'.';
                if !before_ok || !after_ok || qualified_member {
                    continue;
                }
                if best_match.is_none_or(|(_, best_len)| word_len > best_len) {
                    best_match = Some((replacement.as_str(), word_len));
                }
            }

            if let Some((replacement, word_len)) = best_match {
                result.push_str(&text[last_copied..i]);
                result.push_str(replacement);
                i += word_len;
                last_copied = i;
                continue;
            }
            i += 1;
        }
        result.push_str(&text[last_copied..]);
        result
    }

    pub(in crate::declaration_emitter) fn contains_whole_word_in_text(
        text: &str,
        word: &str,
    ) -> bool {
        let bytes = text.as_bytes();
        let word_bytes = word.as_bytes();
        let word_len = word_bytes.len();
        let text_len = bytes.len();
        let protected_spans = Self::protected_type_text_literal_spans(text);
        let mut protected_idx = 0usize;
        let mut i = 0;
        while i < text_len {
            while protected_idx < protected_spans.len() && protected_spans[protected_idx].1 <= i {
                protected_idx += 1;
            }
            if let Some((start, end)) = protected_spans.get(protected_idx).copied()
                && start <= i
                && i < end
            {
                i = end;
                continue;
            }

            if i + word_len <= text_len && &bytes[i..i + word_len] == word_bytes {
                let before_ok = i == 0 || !Self::is_ident_char_in_text(bytes[i - 1]);
                let after_ok =
                    i + word_len >= text_len || !Self::is_ident_char_in_text(bytes[i + word_len]);
                let qualified_member = i > 0 && bytes[i - 1] == b'.';
                if before_ok && after_ok && !qualified_member {
                    return true;
                }
            }
            i += 1;
        }
        false
    }

    fn protected_type_text_literal_spans(text: &str) -> Vec<(usize, usize)> {
        fn skip_quoted(bytes: &[u8], mut i: usize, quote: u8) -> usize {
            i += 1;
            let mut escaped = false;
            while i < bytes.len() {
                if escaped {
                    escaped = false;
                    i += 1;
                    continue;
                }
                if bytes[i] == b'\\' {
                    escaped = true;
                    i += 1;
                    continue;
                }
                i += 1;
                if bytes[i - 1] == quote {
                    break;
                }
            }
            i
        }

        fn scan_template(bytes: &[u8], start: usize, spans: &mut Vec<(usize, usize)>) -> usize {
            let mut segment_start = start;
            let mut i = start + 1;
            while i < bytes.len() {
                match bytes[i] {
                    b'\\' => {
                        i = (i + 2).min(bytes.len());
                    }
                    b'`' => {
                        spans.push((segment_start, i + 1));
                        return i + 1;
                    }
                    b'$' if bytes.get(i + 1) == Some(&b'{') => {
                        spans.push((segment_start, i + 2));
                        i = scan_template_placeholder(bytes, i + 2, spans);
                        segment_start = i.saturating_sub(1);
                    }
                    _ => {
                        i += 1;
                    }
                }
            }
            spans.push((segment_start, bytes.len()));
            bytes.len()
        }

        fn scan_template_placeholder(
            bytes: &[u8],
            mut i: usize,
            spans: &mut Vec<(usize, usize)>,
        ) -> usize {
            let mut brace_depth = 1usize;
            while i < bytes.len() {
                match bytes[i] {
                    b'\'' | b'"' => {
                        let end = skip_quoted(bytes, i, bytes[i]);
                        spans.push((i, end));
                        i = end;
                    }
                    b'`' => {
                        i = scan_template(bytes, i, spans);
                    }
                    b'{' => {
                        brace_depth += 1;
                        i += 1;
                    }
                    b'}' => {
                        brace_depth = brace_depth.saturating_sub(1);
                        i += 1;
                        if brace_depth == 0 {
                            return i;
                        }
                    }
                    _ => {
                        i += 1;
                    }
                }
            }
            i
        }

        let bytes = text.as_bytes();
        let mut spans = Vec::new();
        let mut i = 0usize;
        while i < bytes.len() {
            match bytes[i] {
                b'\'' | b'"' => {
                    let end = skip_quoted(bytes, i, bytes[i]);
                    spans.push((i, end));
                    i = end;
                }
                b'`' => {
                    i = scan_template(bytes, i, &mut spans);
                }
                _ => {
                    i += 1;
                }
            }
        }
        spans
    }

    const fn is_ident_char_in_text(b: u8) -> bool {
        b.is_ascii_alphanumeric() || b == b'_' || b == b'$'
    }

    pub(crate) fn identifier_text_from_arena(
        &self,
        arena: &NodeArena,
        idx: NodeIndex,
    ) -> Option<String> {
        let node = arena.get(idx)?;
        arena
            .get_identifier(node)
            .map(|ident| ident.escaped_text.clone())
    }

    fn property_name_text_from_arena(&self, arena: &NodeArena, idx: NodeIndex) -> Option<String> {
        let node = arena.get(idx)?;
        if node.kind == SyntaxKind::Identifier as u16 {
            return self.identifier_text_from_arena(arena, idx);
        }
        if node.kind == SyntaxKind::StringLiteral as u16
            || node.kind == SyntaxKind::NumericLiteral as u16
        {
            let literal = arena.get_literal(node)?;
            return Some(literal.text.clone());
        }
        None
    }

    fn object_rest_binding_excluded_names(&self, identifier_idx: NodeIndex) -> Option<Vec<String>> {
        let sym_id = self.value_reference_symbol(identifier_idx)?;
        let binder = self.binder?;
        let symbol = binder.symbols.get(sym_id)?;

        for decl_idx in symbol.declarations.iter().copied() {
            let parent_idx = self.arena.parent_of(decl_idx)?;
            let parent_node = self.arena.get(parent_idx)?;
            let binding = self.arena.get_binding_element(parent_node)?;
            if !binding.dot_dot_dot_token || binding.name != decl_idx {
                continue;
            }

            let pattern_idx = self.arena.parent_of(parent_idx)?;
            let pattern_node = self.arena.get(pattern_idx)?;
            let pattern = self.arena.get_binding_pattern(pattern_node)?;
            let mut excluded = Vec::new();
            for &element_idx in &pattern.elements.nodes {
                let Some(element_node) = self.arena.get(element_idx) else {
                    continue;
                };
                let Some(element) = self.arena.get_binding_element(element_node) else {
                    continue;
                };
                if element.dot_dot_dot_token {
                    continue;
                }
                let name_idx = if element.property_name.is_some() {
                    element.property_name
                } else {
                    element.name
                };
                if let Some(name) = self.property_name_text_from_arena(self.arena, name_idx) {
                    excluded.push(name);
                }
            }
            return Some(excluded);
        }

        None
    }

    fn omit_object_type_text_properties(type_text: &str, excluded_names: &[String]) -> String {
        if !type_text.trim_start().starts_with('{') || excluded_names.is_empty() {
            return type_text.to_string();
        }

        type_text
            .lines()
            .filter(|line| {
                let trimmed = line.trim_start();
                !excluded_names.iter().any(|name| {
                    trimmed
                        .strip_prefix(name)
                        .is_some_and(|rest| rest.starts_with(':') || rest.starts_with("?:"))
                })
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn qualify_foreign_exported_names_in_text(
        &self,
        source_arena: &NodeArena,
        source_path: &str,
        text: &str,
        excluded_names: &[String],
    ) -> String {
        let Some(current_path) = self.current_file_path.as_deref() else {
            return text.to_string();
        };
        if self.paths_refer_to_same_source_file(current_path, source_path) {
            return text.to_string();
        }

        let rel_path =
            self.strip_ts_extensions(&self.calculate_relative_path(current_path, source_path));
        let Some(source_file) = self.arena_source_file(source_arena) else {
            return text.to_string();
        };

        let mut replacements = Vec::new();
        for &stmt_idx in &source_file.statements.nodes {
            let Some(stmt_node) = source_arena.get(stmt_idx) else {
                continue;
            };
            let target_node = source_arena
                .get_export_decl(stmt_node)
                .and_then(|export| source_arena.get(export.export_clause))
                .unwrap_or(stmt_node);
            let export_name = if let Some(decl) = source_arena.get_interface(target_node) {
                (source_arena.has_modifier(&decl.modifiers, SyntaxKind::ExportKeyword)
                    || source_arena.get_export_decl(stmt_node).is_some())
                .then_some(decl.name)
            } else if let Some(decl) = source_arena.get_type_alias(target_node) {
                (source_arena.has_modifier(&decl.modifiers, SyntaxKind::ExportKeyword)
                    || source_arena.get_export_decl(stmt_node).is_some())
                .then_some(decl.name)
            } else if let Some(decl) = source_arena.get_class(target_node) {
                (source_arena.has_modifier(&decl.modifiers, SyntaxKind::ExportKeyword)
                    || source_arena.get_export_decl(stmt_node).is_some())
                .then_some(decl.name)
            } else if let Some(decl) = source_arena.get_enum(target_node) {
                (source_arena.has_modifier(&decl.modifiers, SyntaxKind::ExportKeyword)
                    || source_arena.get_export_decl(stmt_node).is_some())
                .then_some(decl.name)
            } else {
                None
            }
            .and_then(|name_idx| self.identifier_text_from_arena(source_arena, name_idx));

            let Some(export_name) = export_name else {
                continue;
            };
            if excluded_names.iter().any(|name| name == &export_name) {
                continue;
            }
            let qualified = format!("import(\"{rel_path}\").{export_name}");
            replacements.push((export_name, qualified));
        }

        Self::replace_whole_words_in_text(text, &replacements)
    }

    fn type_text_contains_unqualified_foreign_value_export(
        &self,
        source_arena: &NodeArena,
        source_path: &str,
        text: &str,
    ) -> bool {
        let Some(current_path) = self.current_file_path.as_deref() else {
            return false;
        };
        if self.paths_refer_to_same_source_file(current_path, source_path) {
            return false;
        }

        let Some(source_file) = self.arena_source_file(source_arena) else {
            return false;
        };

        source_file
            .statements
            .nodes
            .iter()
            .copied()
            .any(|stmt_idx| {
                let Some(stmt_node) = source_arena.get(stmt_idx) else {
                    return false;
                };
                let export_name = if let Some(decl) = source_arena.get_function(stmt_node) {
                    source_arena
                        .has_modifier(&decl.modifiers, SyntaxKind::ExportKeyword)
                        .then_some(decl.name)
                } else if let Some(var_stmt) = source_arena.get_variable(stmt_node) {
                    if !source_arena.has_modifier(&var_stmt.modifiers, SyntaxKind::ExportKeyword) {
                        None
                    } else {
                        var_stmt.declarations.nodes.first().and_then(|decl_idx| {
                            let decl_node = source_arena.get(*decl_idx)?;
                            let decl = source_arena.get_variable_declaration(decl_node)?;
                            Some(decl.name)
                        })
                    }
                } else {
                    None
                }
                .and_then(|name_idx| self.identifier_text_from_arena(source_arena, name_idx));

                export_name.is_some_and(|name| Self::contains_whole_word_in_text(text, &name))
            })
    }

    fn qualify_foreign_imported_names_in_text(
        &self,
        source_arena: &NodeArena,
        text: &str,
    ) -> String {
        let Some(source_file) = self.arena_source_file(source_arena) else {
            return text.to_string();
        };

        let mut replacements = Vec::new();
        for &stmt_idx in &source_file.statements.nodes {
            let Some(stmt_node) = source_arena.get(stmt_idx) else {
                continue;
            };
            let Some(import) = source_arena.get_import_decl(stmt_node) else {
                continue;
            };
            let Some(module_node) = source_arena.get(import.module_specifier) else {
                continue;
            };
            let Some(module_lit) = source_arena.get_literal(module_node) else {
                continue;
            };
            let module_specifier = module_lit.text.as_str();
            let Some(clause_node) = source_arena.get(import.import_clause) else {
                continue;
            };
            let Some(clause) = source_arena.get_import_clause(clause_node) else {
                continue;
            };

            if clause.name.is_some()
                && let Some(local_name) = self.identifier_text_from_arena(source_arena, clause.name)
            {
                let qualified = format!("import(\"{module_specifier}\").default");
                replacements.push((local_name, qualified));
            }

            if clause.named_bindings.is_some()
                && let Some(bindings_node) = source_arena.get(clause.named_bindings)
                && let Some(bindings) = source_arena.get_named_imports(bindings_node)
            {
                if bindings.name.is_some() && bindings.elements.nodes.is_empty() {
                    if let Some(local_name) =
                        self.identifier_text_from_arena(source_arena, bindings.name)
                    {
                        let qualified = format!("typeof import(\"{module_specifier}\")");
                        replacements.push((local_name, qualified));
                    }
                } else {
                    for &spec_idx in &bindings.elements.nodes {
                        let Some(spec_node) = source_arena.get(spec_idx) else {
                            continue;
                        };
                        let Some(specifier) = source_arena.get_specifier(spec_node) else {
                            continue;
                        };
                        let Some(local_name) =
                            self.identifier_text_from_arena(source_arena, specifier.name)
                        else {
                            continue;
                        };
                        let imported_name = if specifier.property_name.is_some() {
                            self.identifier_text_from_arena(source_arena, specifier.property_name)
                                .unwrap_or(local_name.clone())
                        } else {
                            local_name.clone()
                        };
                        let qualified = format!("import(\"{module_specifier}\").{imported_name}");
                        replacements.push((local_name, qualified));
                    }
                }
            }
        }

        Self::replace_whole_words_in_text(text, &replacements)
    }

    /// Get the type of a node from the type cache, if available.
    pub(crate) fn get_node_type(&self, node_id: NodeIndex) -> Option<tsz_solver::types::TypeId> {
        if let (Some(cache), _) = (&self.type_cache, &self.type_interner) {
            cache.node_types.get(&node_id.0).copied()
        } else {
            None
        }
    }

    /// Try to find type for a function by looking up both the declaration node and name node.
    /// The binder may map the function declaration node rather than the name identifier,
    /// so we try both.
    pub(crate) fn get_type_via_symbol_for_func(
        &self,
        func_idx: NodeIndex,
        name_node: NodeIndex,
    ) -> Option<tsz_solver::types::TypeId> {
        let cache = self.type_cache.as_ref()?;
        let binder = self.binder?;
        // Try the name node first, then the function declaration node itself
        let symbol_id = binder
            .get_node_symbol(name_node)
            .or_else(|| binder.get_node_symbol(func_idx))?;
        cache.symbol_types.get(&symbol_id).copied()
    }

    pub(crate) fn get_type_via_symbol(
        &self,
        node_id: NodeIndex,
    ) -> Option<tsz_solver::types::TypeId> {
        let binder = self.binder?;
        let symbol_id = binder.get_node_symbol(node_id)?;
        let symbol = binder.symbols.get(symbol_id)?;
        symbol
            .declarations
            .iter()
            .copied()
            .find_map(|decl_idx| self.get_node_type_or_names(&[decl_idx]))
    }

    /// Look up the cached type for a node via its symbol in `symbol_types`.
    /// Unlike `get_type_via_symbol`, this directly queries `symbol_types` without
    /// recursing through declarations — necessary for parameters whose types are
    /// stored by `cache_parameter_types` in `symbol_types` rather than `node_types`.
    pub(crate) fn get_symbol_cached_type(
        &self,
        node_id: NodeIndex,
    ) -> Option<tsz_solver::types::TypeId> {
        let cache = self.type_cache.as_ref()?;
        let binder = self.binder?;
        let sym_id = binder.get_node_symbol(node_id)?;
        cache.symbol_types.get(&sym_id).copied()
    }

    pub(crate) fn infer_fallback_type_text(&self, node_id: NodeIndex) -> Option<String> {
        self.infer_fallback_type_text_at(node_id, self.indent_level)
    }

    pub(in crate::declaration_emitter) fn infer_fallback_type_text_at(
        &self,
        node_id: NodeIndex,
        depth: u32,
    ) -> Option<String> {
        if !node_id.is_some() {
            return None;
        }

        let node = self.arena.get(node_id)?;
        match node.kind {
            k if k == SyntaxKind::NumericLiteral as u16 => Some("number".to_string()),
            k if k == SyntaxKind::StringLiteral as u16 => Some("string".to_string()),
            k if k == SyntaxKind::RegularExpressionLiteral as u16 => Some("RegExp".to_string()),
            k if k == SyntaxKind::NoSubstitutionTemplateLiteral as u16
                || k == syntax_kind_ext::TEMPLATE_EXPRESSION =>
            {
                Some("string".to_string())
            }
            k if k == SyntaxKind::TrueKeyword as u16 || k == SyntaxKind::FalseKeyword as u16 => {
                Some("boolean".to_string())
            }
            k if k == SyntaxKind::NullKeyword as u16
                || k == SyntaxKind::UndefinedKeyword as u16 =>
            {
                Some("any".to_string())
            }
            k if k == syntax_kind_ext::NON_NULL_EXPRESSION => {
                let unary = self.arena.get_unary_expr_ex(node)?;
                self.preferred_expression_type_text(unary.expression)
                    .or_else(|| self.infer_fallback_type_text_at(unary.expression, depth + 1))
            }
            k if k == syntax_kind_ext::ARROW_FUNCTION
                || k == syntax_kind_ext::FUNCTION_EXPRESSION =>
            {
                self.function_expression_type_text_from_ast(node_id)
            }
            k if k == syntax_kind_ext::NEW_EXPRESSION => {
                self.preferred_expression_type_text(node_id)
            }
            k if k == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION => {
                self.infer_object_literal_type_text_at(node_id, depth)
            }
            k if k == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION => self
                .preferred_expression_type_text(node_id)
                .or_else(|| Some("any[]".to_string())),
            k if k == syntax_kind_ext::BINARY_EXPRESSION => self
                .infer_arithmetic_binary_type_text(node_id, depth)
                .or_else(|| {
                    self.get_node_type(node_id)
                        .map(|type_id| self.print_type_id(type_id))
                }),
            _ => self
                .get_node_type(node_id)
                .map(|type_id| self.print_type_id(type_id)),
        }
    }

    /// Infer the type of an arithmetic binary expression for declaration emit.
    /// For numeric operators (`+`, `-`, `*`, `/`, `%`, `**`, bitwise), if both
    /// operands resolve to `number`, the result is `number`.
    /// For `+` specifically, if either operand is `string`, the result is `string`.
    pub(in crate::declaration_emitter) fn infer_arithmetic_binary_type_text(
        &self,
        node_id: NodeIndex,
        depth: u32,
    ) -> Option<String> {
        if depth > 8 {
            return None;
        }
        let node = self.arena.get(node_id)?;
        let binary = self.arena.get_binary_expr(node)?;
        let op = binary.operator_token;

        let is_numeric_op = op == SyntaxKind::MinusToken as u16
            || op == SyntaxKind::AsteriskToken as u16
            || op == SyntaxKind::AsteriskAsteriskToken as u16
            || op == SyntaxKind::SlashToken as u16
            || op == SyntaxKind::PercentToken as u16
            || op == SyntaxKind::LessThanLessThanToken as u16
            || op == SyntaxKind::GreaterThanGreaterThanToken as u16
            || op == SyntaxKind::GreaterThanGreaterThanGreaterThanToken as u16
            || op == SyntaxKind::AmpersandToken as u16
            || op == SyntaxKind::BarToken as u16
            || op == SyntaxKind::CaretToken as u16;

        let is_plus = op == SyntaxKind::PlusToken as u16;

        if !is_numeric_op && !is_plus {
            return None;
        }

        // Purely numeric operators always produce number
        if is_numeric_op {
            return Some("number".to_string());
        }

        // For `+`, resolve both operands
        let left_type = self.infer_operand_type_text(binary.left, depth + 1)?;
        let right_type = self.infer_operand_type_text(binary.right, depth + 1)?;

        if left_type == "string" || right_type == "string" {
            Some("string".to_string())
        } else if left_type == "number" && right_type == "number" {
            Some("number".to_string())
        } else {
            None
        }
    }

    /// Resolve the primitive type of an operand for arithmetic type inference.
    pub(in crate::declaration_emitter) fn infer_operand_type_text(
        &self,
        node_id: NodeIndex,
        depth: u32,
    ) -> Option<String> {
        // Try preferred expression first (finds declared types)
        if let Some(text) = self.preferred_expression_type_text(node_id) {
            return Some(text);
        }
        // Then try structural fallback
        self.infer_fallback_type_text_at(node_id, depth)
    }

    pub(crate) fn preferred_expression_type_text(&self, expr_idx: NodeIndex) -> Option<String> {
        if let Some(asserted_type_text) = self.explicit_asserted_type_text(expr_idx) {
            return Some(asserted_type_text);
        }

        let expr_idx = self.skip_parenthesized_expression(expr_idx)?;
        let expr_node = self.arena.get(expr_idx)?;

        match expr_node.kind {
            k if k == SyntaxKind::Identifier as u16
                || k == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION =>
            {
                if expr_node.kind == SyntaxKind::Identifier as u16
                    && self.identifier_is_object_rest_binding(expr_idx)
                    && let Some(type_id) = self
                        .get_node_type_or_names(&[expr_idx])
                        .or_else(|| self.get_type_via_symbol(expr_idx))
                    && type_id != tsz_solver::types::TypeId::ANY
                    && type_id != tsz_solver::types::TypeId::ERROR
                    && let Some(interner) = self.type_interner
                    && tsz_solver::type_queries::is_object_like_type(interner, type_id)
                {
                    return Some(self.print_type_id(type_id));
                }
                if expr_node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                    && self.get_node_type(expr_idx) == Some(tsz_solver::types::TypeId::ANY)
                {
                    return Some("any".to_string());
                }
                let type_text = self
                    .reference_declared_type_annotation_text(expr_idx)
                    .or_else(|| self.value_reference_symbol_type_text(expr_idx))
                    .or_else(|| self.undefined_identifier_type_text(expr_idx));
                if expr_node.kind == SyntaxKind::Identifier as u16
                    && let Some(type_text) = type_text
                {
                    if let Some(excluded_names) = self.object_rest_binding_excluded_names(expr_idx)
                    {
                        return Some(Self::omit_object_type_text_properties(
                            &type_text,
                            &excluded_names,
                        ));
                    }
                    if let Some(type_id) = self.reference_declared_type_id(expr_idx)
                        && self.should_expand_named_application_for_inferred_declaration(type_id)
                    {
                        return Some(self.print_type_id_for_inferred_declaration(type_id));
                    }
                    return Some(type_text);
                }
                type_text
            }
            k if k == syntax_kind_ext::CALL_EXPRESSION => {
                let reused_type_text = self.call_expression_reused_type_text(expr_idx);
                let reused_type_uses_function_local_alias =
                    reused_type_text.as_deref().is_some_and(|type_text| {
                        self.type_text_starts_with_function_local_type_alias(type_text)
                    });
                if reused_type_text.is_some()
                    && let Some(type_id) = self.get_node_type_or_names(&[expr_idx])
                    && type_id != tsz_solver::types::TypeId::ANY
                    && type_id != tsz_solver::types::TypeId::ERROR
                    && (reused_type_uses_function_local_alias
                        || self.should_expand_named_application_for_inferred_declaration(type_id)
                        || self.type_contains_conditional_alias_application_for_inferred_emit(
                            type_id, 0,
                        ))
                {
                    let printed = self.print_type_id_for_inferred_declaration(type_id);
                    if let Some(call) = self.arena.get_call_expr(expr_node) {
                        if let Some((alias_name, module_specifier)) =
                            self.call_receiver_default_import_alias(call.expression)
                        {
                            return Some(Self::rewrite_import_type_export_to_default_alias(
                                &printed,
                                &alias_name,
                                &module_specifier,
                            ));
                        }
                    }
                    return Some(printed);
                }
                reused_type_text
            }
            k if k == syntax_kind_ext::TAGGED_TEMPLATE_EXPRESSION => {
                self.tagged_template_declared_return_type_text(expr_idx)
            }
            k if k == syntax_kind_ext::NEW_EXPRESSION => {
                self.nameable_new_expression_type_text(expr_idx)
            }
            k if k == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION => {
                self.class_static_computed_index_access_type_text(expr_idx)
            }
            k if k == syntax_kind_ext::CLASS_EXPRESSION => {
                let ast_type_text = self.class_expression_constructor_type_text_from_ast(expr_idx);
                if ast_type_text
                    .as_ref()
                    .is_some_and(|type_text| type_text.contains(" & "))
                {
                    ast_type_text
                } else {
                    self.get_node_type_or_names(&[expr_idx])
                        .map(|type_id| self.print_type_id(type_id))
                        .filter(|type_text| type_text != "any")
                        .or(ast_type_text)
                }
            }
            k if k == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION => {
                self.array_literal_expression_type_text(expr_idx)
            }
            k if k == syntax_kind_ext::ARROW_FUNCTION
                || k == syntax_kind_ext::FUNCTION_EXPRESSION =>
            {
                self.function_expression_type_text_from_ast(expr_idx)
            }
            k if k == syntax_kind_ext::BINARY_EXPRESSION => {
                self.short_circuit_expression_type_text(expr_idx)
            }
            _ => None,
        }
    }

    pub(in crate::declaration_emitter) fn super_method_call_return_type_text(
        &self,
        expr_idx: NodeIndex,
    ) -> Option<String> {
        let expr_node = self.arena.get(expr_idx)?;
        if expr_node.kind != syntax_kind_ext::CALL_EXPRESSION {
            return None;
        }
        let call = self.arena.get_call_expr(expr_node)?;
        let access_node = self.arena.get(call.expression)?;
        if access_node.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            return None;
        }
        let access = self.arena.get_access_expr(access_node)?;
        if self
            .arena
            .get(access.expression)
            .is_none_or(|node| node.kind != SyntaxKind::SuperKeyword as u16)
        {
            return None;
        }
        let method_name = self.get_identifier_text(access.name_or_argument)?;
        let is_static_context = self
            .enclosing_method_for_node(expr_idx)
            .is_some_and(|method| self.arena.is_static(&method.modifiers));
        let method_idx =
            self.super_method_declaration(expr_idx, &method_name, is_static_context)?;
        let method_node = self.arena.get(method_idx)?;
        let method = self.arena.get_method_decl(method_node)?;
        self.method_source_return_type_text(method_idx, method)
    }

    fn super_method_declaration(
        &self,
        expr_idx: NodeIndex,
        method_name: &str,
        is_static_context: bool,
    ) -> Option<NodeIndex> {
        let class_idx = self.enclosing_class_for_node(expr_idx)?;
        let class_node = self.arena.get(class_idx)?;
        let class = self.arena.get_class(class_node)?;
        let base_expr = self.class_extends_expression(class)?;
        let base_sym = self.value_reference_symbol(base_expr)?;
        let binder = self.binder?;
        let symbol = binder.symbols.get(base_sym)?;

        for decl_idx in symbol.declarations.iter().copied() {
            let Some(decl_node) = self.arena.get(decl_idx) else {
                continue;
            };
            let Some(base_class) = self.arena.get_class(decl_node) else {
                continue;
            };
            if let Some(method_idx) =
                self.class_method_named(base_class, method_name, is_static_context)
            {
                return Some(method_idx);
            }
        }

        None
    }

    fn method_source_return_type_text(
        &self,
        method_idx: NodeIndex,
        method: &tsz_parser::parser::node::MethodDeclData,
    ) -> Option<String> {
        if method.type_annotation.is_some() {
            return self.emit_type_node_text(method.type_annotation);
        }
        if method.body.is_some() {
            if self.body_returns_void(method.body) {
                return Some("void".to_string());
            }
            if let Some(type_text) = self.function_body_preferred_return_type_text(method.body) {
                return Some(type_text);
            }
        }

        let method_type_id = self
            .get_node_type_or_names(&[method_idx, method.name])
            .or_else(|| self.get_type_via_symbol_for_func(method_idx, method.name))?;
        let Some(interner) = self.type_interner else {
            return Some(self.print_type_id(method_type_id));
        };
        tsz_solver::type_queries::get_return_type(interner, method_type_id)
            .map(|return_type| self.print_type_id(return_type))
            .or_else(|| Some(self.print_type_id(method_type_id)))
    }

    fn enclosing_method_for_node(
        &self,
        node_idx: NodeIndex,
    ) -> Option<&tsz_parser::parser::node::MethodDeclData> {
        let mut current = node_idx;
        for _ in 0..32 {
            let parent_idx = self.arena.parent_of(current)?;
            if !parent_idx.is_some() {
                return None;
            }
            let parent_node = self.arena.get(parent_idx)?;
            if self.arena.get_source_file(parent_node).is_some()
                || self.arena.get_class(parent_node).is_some()
            {
                return None;
            }
            if let Some(method) = self.arena.get_method_decl(parent_node) {
                return Some(method);
            }
            current = parent_idx;
        }
        None
    }

    fn enclosing_class_for_node(&self, node_idx: NodeIndex) -> Option<NodeIndex> {
        let mut current = node_idx;
        for _ in 0..32 {
            let parent_idx = self.arena.parent_of(current)?;
            if !parent_idx.is_some() {
                return None;
            }
            let parent_node = self.arena.get(parent_idx)?;
            if self.arena.get_source_file(parent_node).is_some() {
                return None;
            }
            if self.arena.get_class(parent_node).is_some() {
                return Some(parent_idx);
            }
            current = parent_idx;
        }
        None
    }

    fn class_extends_expression(
        &self,
        class: &tsz_parser::parser::node::ClassData,
    ) -> Option<NodeIndex> {
        let heritage_clauses = class.heritage_clauses.as_ref()?;
        for clause_idx in heritage_clauses.nodes.iter().copied() {
            let heritage = self.arena.get_heritage_clause_at(clause_idx)?;
            if heritage.token != SyntaxKind::ExtendsKeyword as u16 {
                continue;
            }
            let base_idx = heritage.types.nodes.first().copied()?;
            let base_node = self.arena.get(base_idx)?;
            return self
                .arena
                .get_expr_type_args(base_node)
                .map(|expr| expr.expression)
                .or(Some(base_idx));
        }
        None
    }

    fn class_method_named(
        &self,
        class: &tsz_parser::parser::node::ClassData,
        method_name: &str,
        is_static: bool,
    ) -> Option<NodeIndex> {
        class.members.nodes.iter().copied().find(|&member_idx| {
            let Some(member_node) = self.arena.get(member_idx) else {
                return false;
            };
            let Some(method) = self.arena.get_method_decl(member_node) else {
                return false;
            };
            self.arena.is_static(&method.modifiers) == is_static
                && self.get_identifier_text(method.name).as_deref() == Some(method_name)
        })
    }

    fn class_expression_constructor_type_text_from_ast(
        &self,
        expr_idx: NodeIndex,
    ) -> Option<String> {
        let expr_node = self.arena.get(expr_idx)?;
        let class = self.arena.get_class(expr_node)?;
        let extends_parameter_type_text =
            self.class_expression_extends_parameter_type_text(expr_idx, class);

        let mut params_text = String::new();
        if let Some(ctor_idx) = class.members.nodes.iter().copied().find(|&member_idx| {
            self.arena
                .get(member_idx)
                .is_some_and(|node| node.kind == syntax_kind_ext::CONSTRUCTOR)
        }) {
            let ctor = self
                .arena
                .get(ctor_idx)
                .and_then(|node| self.arena.get_constructor(node))?;
            let mut scratch = self.scratch_declaration_emitter();
            scratch.in_constructor_params = true;
            scratch.emit_parameters_with_body(&ctor.parameters, ctor.body);
            scratch.in_constructor_params = false;
            params_text = scratch.writer.take_output();
        }
        if params_text.is_empty() && extends_parameter_type_text.is_some() {
            params_text = "...args: any[]".to_string();
        }

        let mut member_scratch = self.scratch_declaration_emitter();
        member_scratch.indent_level = self.indent_level + 2;
        for member_idx in class.members.nodes.iter().copied() {
            let Some(member_node) = self.arena.get(member_idx) else {
                continue;
            };
            if member_node.kind == syntax_kind_ext::CONSTRUCTOR {
                continue;
            }
            member_scratch.emit_class_member(member_idx);
        }
        let members = member_scratch.writer.take_output();
        let members = members.trim_end();

        let constructor_type = if members.is_empty() {
            format!("{{\n    new ({params_text}): {{}};\n}}")
        } else {
            format!("{{\n    new ({params_text}): {{\n{members}\n    }};\n}}")
        };

        if let Some(base_type_text) = extends_parameter_type_text {
            Some(format!("{constructor_type} & {base_type_text}"))
        } else {
            Some(constructor_type)
        }
    }

    fn class_expression_extends_parameter_type_text(
        &self,
        expr_idx: NodeIndex,
        class: &tsz_parser::parser::node::ClassData,
    ) -> Option<String> {
        let enclosing_func = self.enclosing_function_for_node(expr_idx)?;
        let heritage_clauses = class.heritage_clauses.as_ref()?;
        for clause_idx in heritage_clauses.nodes.iter().copied() {
            let heritage = self.arena.get_heritage_clause_at(clause_idx)?;
            if heritage.token != SyntaxKind::ExtendsKeyword as u16 {
                continue;
            }
            let base_idx = heritage.types.nodes.first().copied()?;
            let base_node = self.arena.get(base_idx)?;
            let base_expr = self
                .arena
                .get_expr_type_args(base_node)
                .map(|expr| expr.expression)
                .unwrap_or(base_idx);
            if let Some(type_text) = self.function_parameter_type_text(enclosing_func, base_expr) {
                return Some(type_text);
            }
        }

        None
    }

    fn enclosing_function_for_node(
        &self,
        node_idx: NodeIndex,
    ) -> Option<&tsz_parser::parser::node::FunctionData> {
        let mut current = node_idx;
        for _ in 0..32 {
            let parent_idx = self.arena.parent_of(current)?;
            if !parent_idx.is_some() {
                return None;
            }
            let parent_node = self.arena.get(parent_idx)?;
            if self.arena.get_source_file(parent_node).is_some() {
                return None;
            }
            if let Some(func) = self.arena.get_function(parent_node) {
                return Some(func);
            }
            current = parent_idx;
        }

        None
    }

    pub(in crate::declaration_emitter) fn scratch_declaration_emitter(
        &self,
    ) -> DeclarationEmitter<'a> {
        let mut scratch = if let (Some(type_cache), Some(type_interner), Some(binder)) =
            (&self.type_cache, self.type_interner, self.binder)
        {
            DeclarationEmitter::with_type_info(
                self.arena,
                type_cache.clone(),
                type_interner,
                binder,
            )
        } else {
            DeclarationEmitter::new(self.arena)
        };

        scratch.source_is_declaration_file = self.source_is_declaration_file;
        scratch.source_is_js_file = self.source_is_js_file;
        scratch.current_source_file_idx = self.current_source_file_idx;
        scratch.source_file_text = self.source_file_text.clone();
        scratch.current_file_path = self.current_file_path.clone();
        scratch.current_arena = self.current_arena.clone();
        scratch.arena_to_path = self.arena_to_path.clone();
        scratch
    }

    pub(in crate::declaration_emitter) fn declaration_emittable_type_text(
        &self,
        initializer: NodeIndex,
        type_id: tsz_solver::types::TypeId,
        printed_type_text: &str,
    ) -> String {
        let initializer = self.skip_parenthesized_non_null_and_comma(initializer);

        if type_id == tsz_solver::types::TypeId::ANY
            && let Some(type_text) = self.data_view_new_expression_type_text(initializer)
        {
            return type_text;
        }

        if self.object_literal_prefers_syntax_type_text(initializer)
            && let Some(type_text) =
                self.rewrite_object_literal_computed_member_type_text(initializer, type_id)
        {
            return type_text;
        }

        if let Some(typeof_text) =
            self.typeof_prefix_for_value_entity(initializer, true, Some(type_id))
        {
            return typeof_text;
        }

        if (type_id == tsz_solver::types::TypeId::ANY
            || type_id == tsz_solver::types::TypeId::ERROR)
            && self
                .arena
                .get(initializer)
                .is_some_and(|node| node.kind == syntax_kind_ext::CALL_EXPRESSION)
            && let Some(type_text) = self.preferred_expression_type_text(initializer)
        {
            return type_text;
        }

        if type_id != tsz_solver::types::TypeId::ANY
            && type_id != tsz_solver::types::TypeId::ERROR
            && self
                .arena
                .get(initializer)
                .is_some_and(|node| node.kind == syntax_kind_ext::CALL_EXPRESSION)
        {
            if let Some(type_text) = self.preferred_expression_type_text(initializer) {
                let type_text = Self::strip_synthetic_anonymous_object_members(&type_text);
                let type_text = self
                    .expand_portable_mapped_object_text_in_current_context(&type_text)
                    .unwrap_or(type_text);
                return self.rewrite_call_receiver_default_import_aliases(initializer, type_text);
            }
            let type_text = Self::strip_synthetic_anonymous_object_members(printed_type_text);
            let type_text = self
                .expand_portable_mapped_object_text_in_current_context(&type_text)
                .unwrap_or(type_text);
            return self.rewrite_call_receiver_default_import_aliases(initializer, type_text);
        }

        if (type_id != tsz_solver::types::TypeId::ANY
            || !self.initializer_is_new_expression(initializer))
            && let Some(type_text) = self.preferred_expression_type_text(initializer)
        {
            let type_text = Self::strip_synthetic_anonymous_object_members(&type_text);
            if let Some(expanded) =
                self.expand_portable_mapped_object_text_in_current_context(&type_text)
            {
                return expanded;
            }
            return self
                .enum_value_index_access_alias_type_text(&type_text)
                .unwrap_or(type_text);
        }

        let type_text = Self::strip_synthetic_anonymous_object_members(printed_type_text);
        if let Some(expanded) =
            self.expand_portable_mapped_object_text_in_current_context(&type_text)
        {
            return expanded;
        }
        self.enum_value_index_access_alias_type_text(&type_text)
            .unwrap_or(type_text)
    }

    pub(in crate::declaration_emitter) fn strip_synthetic_anonymous_object_members(
        type_text: &str,
    ) -> String {
        if let Some(unwrapped) = Self::unwrap_synthetic_anonymous_object_type(type_text) {
            return unwrapped;
        }
        type_text.to_string()
    }

    fn unwrap_synthetic_anonymous_object_type(type_text: &str) -> Option<String> {
        let trimmed = type_text.trim();
        let inner = trimmed.strip_prefix('{')?.trim_start();
        let member = inner.strip_prefix(':')?.trim();
        let member = if member.ends_with('}') {
            let without_outer = member.strip_suffix('}').unwrap_or(member).trim_end();
            if without_outer.ends_with(';') {
                without_outer
            } else {
                member
            }
        } else {
            member
        };
        let member = member.strip_suffix(';').unwrap_or(member).trim();
        if member.is_empty() {
            return None;
        }
        if member.starts_with('{') {
            if let Some(unwrapped) = Self::unwrap_synthetic_anonymous_object_type(member) {
                return Some(unwrapped);
            }
        }
        Some(member.to_string())
    }

    pub(in crate::declaration_emitter) fn explicit_asserted_type_text(
        &self,
        expr_idx: NodeIndex,
    ) -> Option<String> {
        let mut current = expr_idx;

        for _ in 0..100 {
            let node = self.arena.get(current)?;
            if node.kind == syntax_kind_ext::PARENTHESIZED_EXPRESSION
                && let Some(paren) = self.arena.get_parenthesized(node)
            {
                current = paren.expression;
                continue;
            }
            if node.kind == syntax_kind_ext::NON_NULL_EXPRESSION
                && let Some(unary) = self.arena.get_unary_expr_ex(node)
            {
                current = unary.expression;
                continue;
            }
            if node.kind == syntax_kind_ext::BINARY_EXPRESSION
                && let Some(binary) = self.arena.get_binary_expr(node)
                && binary.operator_token == SyntaxKind::CommaToken as u16
            {
                current = binary.right;
                continue;
            }

            if node.kind == syntax_kind_ext::SATISFIES_EXPRESSION {
                return None;
            }

            let assertion = self.arena.get_type_assertion(node)?;
            let asserted_type = self.arena.get(assertion.type_node)?;
            if asserted_type.kind == SyntaxKind::ConstKeyword as u16 {
                return None;
            }
            if let Some(alias_text) =
                self.local_asserted_type_alias_text(current, assertion.type_node)
            {
                return Some(alias_text);
            }
            return self.emit_type_node_text_normalized(assertion.type_node);
        }

        None
    }

    pub(in crate::declaration_emitter) fn declaration_type_is_uninformative(
        &self,
        candidates: &[NodeIndex],
    ) -> bool {
        self.get_node_type_or_names(candidates)
            .is_none_or(|type_id| {
                type_id == tsz_solver::types::TypeId::ANY
                    || type_id == tsz_solver::types::TypeId::ERROR
                    || type_id == tsz_solver::types::TypeId::UNKNOWN
            })
    }

    pub(in crate::declaration_emitter) fn as_const_assertion_type_text(
        &self,
        expr_idx: NodeIndex,
    ) -> Option<String> {
        let expr_node = self.arena.get(expr_idx)?;
        if expr_node.kind != syntax_kind_ext::AS_EXPRESSION {
            return None;
        }
        let assertion = self.arena.get_type_assertion(expr_node)?;
        if !self.type_assertion_is_const(assertion.type_node) {
            return None;
        }

        self.const_asserted_expression_type_text(assertion.expression, self.indent_level)
    }

    pub(in crate::declaration_emitter) fn angle_bracket_const_assertion_type_text(
        &self,
        expr_idx: NodeIndex,
    ) -> Option<String> {
        let expr_node = self.arena.get(expr_idx)?;
        if expr_node.kind != syntax_kind_ext::TYPE_ASSERTION {
            return None;
        }

        let assertion = self.arena.get_type_assertion(expr_node)?;
        if !self.type_assertion_is_const(assertion.type_node) {
            return None;
        }

        self.const_asserted_expression_type_text(assertion.expression, self.indent_level)
    }

    fn type_assertion_is_const(&self, type_node_idx: NodeIndex) -> bool {
        self.arena
            .get(type_node_idx)
            .is_some_and(|asserted_type| asserted_type.kind == SyntaxKind::ConstKeyword as u16)
            || self
                .get_identifier_text(type_node_idx)
                .or_else(|| self.emit_type_node_text(type_node_idx))
                .as_deref()
                == Some("const")
    }

    fn const_asserted_expression_type_text(
        &self,
        expr_idx: NodeIndex,
        depth: u32,
    ) -> Option<String> {
        let expr_node = self.arena.get(expr_idx)?;
        match expr_node.kind {
            k if k == syntax_kind_ext::PARENTHESIZED_EXPRESSION => {
                self.arena.get_parenthesized(expr_node).and_then(|paren| {
                    self.const_asserted_expression_type_text(paren.expression, depth)
                })
            }
            k if k == SyntaxKind::StringLiteral as u16
                || k == SyntaxKind::NumericLiteral as u16
                || k == SyntaxKind::BigIntLiteral as u16
                || k == SyntaxKind::TrueKeyword as u16
                || k == SyntaxKind::FalseKeyword as u16
                || k == SyntaxKind::NoSubstitutionTemplateLiteral as u16
                || (k == syntax_kind_ext::PREFIX_UNARY_EXPRESSION
                    && self.is_negative_literal(expr_node)) =>
            {
                self.const_literal_initializer_text(expr_idx)
            }
            k if k == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION => {
                self.const_asserted_array_literal_type_text(expr_idx, depth)
            }
            k if k == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION => {
                self.const_asserted_object_literal_type_text(expr_idx, depth)
            }
            _ => self
                .get_node_type_or_names(&[expr_idx])
                .map(|type_id| self.print_type_id_for_inferred_declaration(type_id))
                .or_else(|| self.infer_fallback_type_text_at(expr_idx, depth)),
        }
    }

    fn const_asserted_array_literal_type_text(
        &self,
        expr_idx: NodeIndex,
        depth: u32,
    ) -> Option<String> {
        let expr_node = self.arena.get(expr_idx)?;
        let array = self.arena.get_literal_expr(expr_node)?;
        let mut parts = Vec::with_capacity(array.elements.nodes.len());

        for &element_idx in &array.elements.nodes {
            let element_node = self.arena.get(element_idx)?;
            if element_node.kind == syntax_kind_ext::SPREAD_ELEMENT {
                let spread = self.arena.get_spread(element_node)?;
                let spread_type = self
                    .get_node_type_or_names(&[spread.expression])
                    .map(|type_id| self.print_type_id_for_inferred_declaration(type_id))
                    .or_else(|| self.infer_fallback_type_text_at(spread.expression, depth + 1))
                    .unwrap_or_else(|| "any[]".to_string());
                parts.push(format!("...{spread_type}"));
                continue;
            }

            let element_depth = if element_node.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION {
                depth
            } else {
                depth + 1
            };
            parts.push(
                self.const_asserted_expression_type_text(element_idx, element_depth)
                    .unwrap_or_else(|| "any".to_string()),
            );
        }

        Some(format!("readonly [{}]", parts.join(", ")))
    }

    fn const_asserted_object_literal_type_text(
        &self,
        expr_idx: NodeIndex,
        depth: u32,
    ) -> Option<String> {
        let expr_node = self.arena.get(expr_idx)?;
        let object = self.arena.get_literal_expr(expr_node)?;
        let mut members = Vec::new();

        for &member_idx in &object.elements.nodes {
            let Some(member_node) = self.arena.get(member_idx) else {
                continue;
            };
            if member_node.kind == syntax_kind_ext::SPREAD_ASSIGNMENT {
                return None;
            }

            let Some(name_idx) = self.object_literal_member_name_idx(member_node) else {
                continue;
            };
            let name = self.object_literal_member_name_text(name_idx)?;
            if name.is_empty() || name == ":" {
                continue;
            }

            if let Some(method) = self.arena.get_method_decl(member_node) {
                let type_text = self
                    .function_expression_type_text_from_ast(member_idx)
                    .or_else(|| {
                        self.get_node_type_or_names(&[member_idx])
                            .map(|type_id| self.print_type_id(type_id))
                    })
                    .unwrap_or_else(|| {
                        if method.parameters.nodes.is_empty() {
                            "() => void".to_string()
                        } else {
                            "any".to_string()
                        }
                    });
                members.push(format!("readonly {name}: {type_text};"));
                continue;
            }

            let Some(initializer) = self.object_literal_member_initializer(member_node) else {
                continue;
            };
            let type_text = self
                .const_asserted_expression_type_text(initializer, depth + 1)
                .unwrap_or_else(|| "any".to_string());
            members.push(format!("readonly {name}: {type_text};"));
        }

        if members.is_empty() {
            return Some("{}".to_string());
        }

        let member_indent = "    ".repeat((depth + 1) as usize);
        let closing_indent = "    ".repeat(depth as usize);
        let lines = members
            .into_iter()
            .map(|member| format!("{member_indent}{member}"))
            .collect::<Vec<_>>();
        Some(format!("{{\n{}\n{closing_indent}}}", lines.join("\n")))
    }

    fn local_asserted_type_alias_text(
        &self,
        assertion_expr_idx: NodeIndex,
        type_node_idx: NodeIndex,
    ) -> Option<String> {
        let name = self.simple_type_reference_name_text(type_node_idx)?;
        let alias_decl_idx =
            self.find_enclosing_block_type_alias_declaration(assertion_expr_idx, &name)?;
        let alias_node = self.arena.get(alias_decl_idx)?;
        let alias = self.arena.get_type_alias(alias_node)?;
        let mut alias_text = self.emit_type_node_text_normalized(alias.type_node)?;
        let substitutions = self
            .type_alias_application_substitutions(alias.type_parameters.as_ref(), type_node_idx);
        if !substitutions.is_empty() {
            alias_text = Self::replace_whole_words_in_text(&alias_text, &substitutions);
        }
        if alias_text.contains("typeof ") {
            return Some(alias_text);
        }

        if Self::type_text_contains_mapped_type_literal(&alias_text) {
            return Some(
                self.expand_enclosing_block_type_aliases_in_text(
                    assertion_expr_idx,
                    &alias_text,
                    &name,
                )
                .unwrap_or(alias_text),
            );
        }

        alias_text
            .trim_start()
            .starts_with('{')
            .then(|| Self::normalize_local_type_literal_accessor_text(&alias_text))
    }

    fn type_text_contains_mapped_type_literal(text: &str) -> bool {
        text.contains("[") && text.contains(" in ") && text.contains("]:")
    }

    fn expand_enclosing_block_type_aliases_in_text(
        &self,
        from_idx: NodeIndex,
        type_text: &str,
        excluded_name: &str,
    ) -> Option<String> {
        let aliases = self.enclosing_block_type_alias_replacements(from_idx, excluded_name)?;
        Some(Self::replace_whole_words_in_text(type_text, &aliases))
    }

    fn enclosing_block_type_alias_replacements(
        &self,
        from_idx: NodeIndex,
        excluded_name: &str,
    ) -> Option<Vec<(String, String)>> {
        let mut current_idx = from_idx;
        while let Some(ext) = self.arena.get_extended(current_idx) {
            let parent_idx = ext.parent;
            if !parent_idx.is_some() {
                return None;
            }
            let parent_node = self.arena.get(parent_idx)?;
            if parent_node.kind == syntax_kind_ext::BLOCK {
                let block = self.arena.get_block(parent_node)?;
                let replacements = block
                    .statements
                    .nodes
                    .iter()
                    .copied()
                    .filter_map(|stmt_idx| {
                        let stmt_node = self.arena.get(stmt_idx)?;
                        if stmt_node.kind != syntax_kind_ext::TYPE_ALIAS_DECLARATION {
                            return None;
                        }
                        let alias = self.arena.get_type_alias(stmt_node)?;
                        let name = self.get_identifier_text(alias.name)?;
                        if name == excluded_name {
                            return None;
                        }
                        let alias_text = self.emit_type_node_text_normalized(alias.type_node)?;
                        Some((name, alias_text))
                    })
                    .collect();
                return Some(replacements);
            }
            current_idx = parent_idx;
        }
        None
    }

    fn simple_type_reference_name_text(&self, type_node_idx: NodeIndex) -> Option<String> {
        let type_node = self.arena.get(type_node_idx)?;
        if type_node.kind == SyntaxKind::Identifier as u16 {
            return self.get_identifier_text(type_node_idx);
        }
        if type_node.kind == syntax_kind_ext::TYPE_REFERENCE {
            let type_ref = self.arena.get_type_ref(type_node)?;
            return self.type_reference_name_text(type_ref.type_name);
        }
        None
    }

    pub(in crate::declaration_emitter) fn truncation_candidate_type_node(
        &self,
        expr_idx: NodeIndex,
    ) -> Option<NodeIndex> {
        let mut current = expr_idx;

        for _ in 0..100 {
            let node = self.arena.get(current)?;
            if let Some(assertion) = self.arena.get_type_assertion(node) {
                let asserted_type = self.arena.get(assertion.type_node)?;
                if asserted_type.kind == SyntaxKind::ConstKeyword as u16 {
                    return None;
                }
                return Some(assertion.type_node);
            }

            if node.kind != syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION {
                return None;
            }

            let access = self.arena.get_access_expr(node)?;
            let argument = self.arena.get(access.name_or_argument)?;
            let literal = self.arena.get_literal(argument)?;
            if argument.kind != SyntaxKind::NumericLiteral as u16 || literal.text != "0" {
                return None;
            }

            let array_node = self.arena.get(access.expression)?;
            let literal_expr = self.arena.get_literal_expr(array_node)?;
            if array_node.kind != syntax_kind_ext::ARRAY_LITERAL_EXPRESSION
                || literal_expr.elements.nodes.len() != 1
            {
                return None;
            }

            current = literal_expr.elements.nodes[0];
        }

        None
    }

    pub(in crate::declaration_emitter) fn truncation_candidate_type_text(
        &self,
        expr_idx: NodeIndex,
    ) -> Option<String> {
        let type_node = self.truncation_candidate_type_node(expr_idx)?;
        if let Some(type_id) = self.get_node_type_or_names(&[type_node]) {
            let printed = self.print_type_id(type_id);
            if printed != "any" {
                return Some(printed);
            }
        }
        self.emit_type_node_text(type_node)
    }

    pub(in crate::declaration_emitter) fn estimated_truncation_candidate_length(
        &self,
        expr_idx: NodeIndex,
    ) -> Option<usize> {
        let type_node = self.truncation_candidate_type_node(expr_idx)?;
        self.estimate_serialized_type_length(type_node, &FxHashMap::default(), 0)
    }

    pub(in crate::declaration_emitter) fn estimate_serialized_type_length(
        &self,
        type_node: NodeIndex,
        substitutions: &FxHashMap<String, String>,
        depth: usize,
    ) -> Option<usize> {
        if depth > 32 {
            return None;
        }

        let node = self.arena.get(type_node)?;
        match node.kind {
            k if k == syntax_kind_ext::MAPPED_TYPE => {
                let mapped = self.arena.get_mapped_type(node)?;
                let type_param = self.arena.get_type_parameter_at(mapped.type_parameter)?;
                let type_param_name = self.get_identifier_text(type_param.name)?;
                let constraint = if type_param.constraint != NodeIndex::NONE {
                    type_param.constraint
                } else {
                    return None;
                };
                let keys = self.expand_string_literals_from_type_node(
                    constraint,
                    substitutions,
                    depth + 1,
                )?;
                let mut total = 4usize;
                for key in keys {
                    let mut next = substitutions.clone();
                    next.insert(type_param_name.clone(), key.clone());
                    let value_len =
                        self.estimate_serialized_type_length(mapped.type_node, &next, depth + 1)?;
                    total = total
                        .saturating_add(self.serialized_property_name_length(&key))
                        .saturating_add(2)
                        .saturating_add(value_len)
                        .saturating_add(2);
                }
                Some(total)
            }
            k if k == syntax_kind_ext::TEMPLATE_LITERAL_TYPE => {
                let expansions = self.expand_string_literals_from_type_node(
                    type_node,
                    substitutions,
                    depth + 1,
                )?;
                let mut total = 0usize;
                for (idx, value) in expansions.iter().enumerate() {
                    if idx > 0 {
                        total = total.saturating_add(3);
                    }
                    total = total.saturating_add(value.len() + 2);
                }
                Some(total)
            }
            k if k == syntax_kind_ext::TYPE_REFERENCE => {
                let type_ref = self.arena.get_type_ref(node)?;
                let name = self.type_reference_name_text(type_ref.type_name)?;
                if let Some(value) = substitutions.get(&name) {
                    return Some(value.len() + 2);
                }
                let alias_type = self.find_local_type_alias_type_node(&name)?;
                self.estimate_serialized_type_length(alias_type, substitutions, depth + 1)
            }
            k if k == syntax_kind_ext::UNION_TYPE => {
                let composite = self.arena.get_composite_type(node)?;
                let mut total = 0usize;
                for (idx, child) in composite.types.nodes.iter().enumerate() {
                    if idx > 0 {
                        total = total.saturating_add(3);
                    }
                    total = total.saturating_add(self.estimate_serialized_type_length(
                        *child,
                        substitutions,
                        depth + 1,
                    )?);
                }
                Some(total)
            }
            k if k == syntax_kind_ext::LITERAL_TYPE => {
                let literal = self.arena.get_literal_type(node)?;
                let literal_node = self.arena.get(literal.literal)?;
                match literal_node.kind {
                    k if k == SyntaxKind::StringLiteral as u16 => {
                        Some(self.arena.get_literal(literal_node)?.text.len() + 2)
                    }
                    _ => None,
                }
            }
            k if k == SyntaxKind::Identifier as u16 => {
                let name = self.get_identifier_text(type_node)?;
                if let Some(value) = substitutions.get(&name) {
                    return Some(value.len() + 2);
                }
                let alias_type = self.find_local_type_alias_type_node(&name)?;
                self.estimate_serialized_type_length(alias_type, substitutions, depth + 1)
            }
            _ => None,
        }
    }

    pub(in crate::declaration_emitter) fn expand_string_literals_from_type_node(
        &self,
        type_node: NodeIndex,
        substitutions: &FxHashMap<String, String>,
        depth: usize,
    ) -> Option<Vec<String>> {
        if depth > 32 {
            return None;
        }

        let node = self.arena.get(type_node)?;
        match node.kind {
            k if k == syntax_kind_ext::LITERAL_TYPE => {
                let literal = self.arena.get_literal_type(node)?;
                let literal_node = self.arena.get(literal.literal)?;
                if literal_node.kind != SyntaxKind::StringLiteral as u16 {
                    return None;
                }
                Some(vec![self.arena.get_literal(literal_node)?.text.clone()])
            }
            k if k == syntax_kind_ext::UNION_TYPE => {
                let composite = self.arena.get_composite_type(node)?;
                let mut result = Vec::new();
                for child in &composite.types.nodes {
                    result.extend(self.expand_string_literals_from_type_node(
                        *child,
                        substitutions,
                        depth + 1,
                    )?);
                }
                Some(result)
            }
            k if k == syntax_kind_ext::TEMPLATE_LITERAL_TYPE => {
                let template = self.arena.get_template_literal_type(node)?;
                let head = self.arena.get(template.head)?;
                let head_text = self
                    .arena
                    .get_literal(head)
                    .map(|lit| lit.text.clone())
                    .unwrap_or_default();
                let mut results = vec![head_text];
                for span in &template.template_spans.nodes {
                    let data = self.arena.get_template_span_at(*span)?;
                    let expansions = self.expand_string_literals_from_type_node(
                        data.expression,
                        substitutions,
                        depth + 1,
                    )?;
                    let suffix = self
                        .arena
                        .get(data.literal)
                        .and_then(|literal| self.arena.get_literal(literal))
                        .map(|lit| lit.text.clone())
                        .unwrap_or_default();
                    let mut next =
                        Vec::with_capacity(results.len().saturating_mul(expansions.len()));
                    for prefix in &results {
                        for expansion in &expansions {
                            let mut combined = String::with_capacity(
                                prefix.len() + expansion.len() + suffix.len(),
                            );
                            combined.push_str(prefix);
                            combined.push_str(expansion);
                            combined.push_str(&suffix);
                            next.push(combined);
                        }
                    }
                    results = next;
                }
                Some(results)
            }
            k if k == syntax_kind_ext::TYPE_REFERENCE => {
                let type_ref = self.arena.get_type_ref(node)?;
                let name = self.type_reference_name_text(type_ref.type_name)?;
                if let Some(value) = substitutions.get(&name) {
                    return Some(vec![value.clone()]);
                }
                let alias_type = self.find_local_type_alias_type_node(&name)?;
                self.expand_string_literals_from_type_node(alias_type, substitutions, depth + 1)
            }
            k if k == SyntaxKind::Identifier as u16 => {
                let name = self.get_identifier_text(type_node)?;
                if let Some(value) = substitutions.get(&name) {
                    return Some(vec![value.clone()]);
                }
                let alias_type = self.find_local_type_alias_type_node(&name)?;
                self.expand_string_literals_from_type_node(alias_type, substitutions, depth + 1)
            }
            _ => None,
        }
    }

    pub(in crate::declaration_emitter) fn find_local_type_alias_type_node(
        &self,
        name: &str,
    ) -> Option<NodeIndex> {
        let binder = self.binder?;
        let symbol = binder
            .file_locals
            .get(name)
            .or_else(|| binder.current_scope.get(name))?;
        let declaration = binder.symbols.get(symbol)?.declarations.first().copied()?;
        let declaration_node = self.arena.get(declaration)?;
        self.arena
            .get_type_alias(declaration_node)
            .map(|alias| alias.type_node)
    }

    fn enum_value_index_access_alias_type_text(&self, type_text: &str) -> Option<String> {
        let mut inner = type_text.trim();
        let mut array_suffix = String::new();
        while let Some(next) = inner.strip_suffix("[]") {
            array_suffix.push_str("[]");
            inner = next.trim_end();
        }

        let (alias, key_alias) = inner.split_once("[keyof ")?;
        let alias = alias.trim();
        let key_alias = key_alias.strip_suffix(']')?.trim();
        if alias != key_alias || !Self::is_simple_identifier_text(alias) {
            return None;
        }

        let enum_name = self.typeof_enum_alias_target_name(alias)?;
        Some(format!("{enum_name}{array_suffix}"))
    }

    fn typeof_enum_alias_target_name(&self, alias: &str) -> Option<String> {
        let alias_type_node = self.find_local_type_alias_type_node(alias)?;
        let alias_type = self.arena.get(alias_type_node)?;
        if alias_type.kind != syntax_kind_ext::TYPE_QUERY {
            return None;
        }
        let query = self.arena.get_type_query(alias_type)?;
        let enum_name = self.type_reference_name_text(query.expr_name)?;
        self.local_enum_declaration_exists(&enum_name)
            .then_some(enum_name)
    }

    fn local_enum_declaration_exists(&self, name: &str) -> bool {
        let Some(binder) = self.binder else {
            return false;
        };
        let Some(symbol) = binder
            .file_locals
            .get(name)
            .or_else(|| binder.current_scope.get(name))
        else {
            return false;
        };
        let Some(symbol_data) = binder.symbols.get(symbol) else {
            return false;
        };
        symbol_data.declarations.iter().copied().any(|decl_idx| {
            self.arena
                .get(decl_idx)
                .is_some_and(|node| self.arena.get_enum(node).is_some())
        })
    }

    pub(in crate::declaration_emitter) fn is_simple_identifier_text(text: &str) -> bool {
        let mut chars = text.chars();
        let Some(first) = chars.next() else {
            return false;
        };
        (first == '_' || first == '$' || first.is_ascii_alphabetic())
            && chars.all(|ch| ch == '_' || ch == '$' || ch.is_ascii_alphanumeric())
    }

    pub(in crate::declaration_emitter) fn type_reference_name_text(
        &self,
        name_idx: NodeIndex,
    ) -> Option<String> {
        let name_node = self.arena.get(name_idx)?;
        if name_node.kind == SyntaxKind::Identifier as u16 {
            return self.get_identifier_text(name_idx);
        }
        if name_node.kind == syntax_kind_ext::QUALIFIED_NAME {
            let qualified = self.arena.get_qualified_name(name_node)?;
            return self.get_identifier_text(qualified.right);
        }
        None
    }

    pub(in crate::declaration_emitter) fn skip_parenthesized_expression(
        &self,
        expr_idx: NodeIndex,
    ) -> Option<NodeIndex> {
        let mut current = expr_idx;
        loop {
            let node = self.arena.get(current)?;
            if node.kind != syntax_kind_ext::PARENTHESIZED_EXPRESSION {
                return Some(current);
            }
            current = self.arena.get_unary_expr_ex(node)?.expression;
        }
    }

    pub(in crate::declaration_emitter) fn arena_source_file<'arena>(
        &self,
        arena: &'arena tsz_parser::parser::node::NodeArena,
    ) -> Option<&'arena tsz_parser::parser::node::SourceFileData> {
        arena
            .nodes
            .iter()
            .rev()
            .find_map(|node| arena.get_source_file(node))
    }

    pub(in crate::declaration_emitter) fn source_slice_from_arena(
        &self,
        arena: &tsz_parser::parser::node::NodeArena,
        node_idx: NodeIndex,
    ) -> Option<String> {
        let node = arena.get(node_idx)?;
        let source_file = self.arena_source_file(arena)?;
        let text = source_file.text.as_ref();
        let start = usize::try_from(node.pos).ok()?;
        let end = usize::try_from(node.end).ok()?;
        text.get(start..end).map(str::to_string)
    }

    pub(crate) fn rescued_asserts_parameter_type_text(
        &self,
        param_idx: NodeIndex,
    ) -> Option<String> {
        let param_node = self.arena.get(param_idx)?;
        let param = self.arena.get_parameter(param_node)?;
        let type_node = self.arena.get(param.type_annotation)?;
        let type_ref = self.arena.get_type_ref(type_node)?;
        if type_ref.type_arguments.is_some() {
            return None;
        }
        let type_name = self.arena.get(type_ref.type_name)?;
        let ident = self.arena.get_identifier(type_name)?;
        if ident.escaped_text != "asserts" {
            return None;
        }

        let rescued = self.scan_asserts_parameter_type_text(type_node.pos)?;
        let normalized = rescued.split_whitespace().collect::<Vec<_>>().join(" ");
        (normalized != "asserts").then_some(normalized)
    }

    pub(in crate::declaration_emitter) fn scan_asserts_parameter_type_text(
        &self,
        start: u32,
    ) -> Option<String> {
        let text = self.source_file_text.as_deref()?;
        let bytes = text.as_bytes();
        let start = usize::try_from(start).ok()?;
        if start >= bytes.len() {
            return None;
        }

        let mut i = start;
        let mut paren_depth = 0usize;
        let mut bracket_depth = 0usize;
        let mut brace_depth = 0usize;
        let mut angle_depth = 0usize;

        while i < bytes.len() {
            match bytes[i] {
                b'(' => paren_depth += 1,
                b')' => {
                    if paren_depth == 0
                        && bracket_depth == 0
                        && brace_depth == 0
                        && angle_depth == 0
                    {
                        break;
                    }
                    paren_depth = paren_depth.saturating_sub(1);
                }
                b'[' => bracket_depth += 1,
                b']' => bracket_depth = bracket_depth.saturating_sub(1),
                b'{' => brace_depth += 1,
                b'}' => brace_depth = brace_depth.saturating_sub(1),
                b'<' => angle_depth += 1,
                b'>' => angle_depth = angle_depth.saturating_sub(1),
                b',' | b'=' | b';'
                    if paren_depth == 0
                        && bracket_depth == 0
                        && brace_depth == 0
                        && angle_depth == 0 =>
                {
                    break;
                }
                _ => {}
            }
            i += 1;
        }

        let rescued = text.get(start..i)?.trim().to_string();
        (!rescued.is_empty()).then_some(rescued)
    }

    pub(in crate::declaration_emitter) fn undefined_identifier_type_text(
        &self,
        expr_idx: NodeIndex,
    ) -> Option<String> {
        (self.get_identifier_text(expr_idx).as_deref() == Some("undefined"))
            .then(|| "any".to_string())
    }

    pub(in crate::declaration_emitter) fn reference_declared_type_annotation_text(
        &self,
        expr_idx: NodeIndex,
    ) -> Option<String> {
        let binder = self.binder?;
        let raw_sym_id = self.value_reference_symbol(expr_idx)?;
        let sym_id = self
            .resolve_portability_import_alias(raw_sym_id, binder)
            .unwrap_or_else(|| self.resolve_portability_declaration_symbol(raw_sym_id, binder));

        self.declared_type_annotation_text_for_symbol(sym_id)
            .or_else(|| self.property_access_declared_type_annotation_text(expr_idx))
    }

    pub(in crate::declaration_emitter) fn value_reference_symbol_type_text(
        &self,
        expr_idx: NodeIndex,
    ) -> Option<String> {
        let sym_id = self.value_reference_symbol(expr_idx)?;
        let binder = self.binder?;
        let cache = self.type_cache.as_ref()?;
        let resolved_sym_id = self
            .resolve_portability_import_alias(sym_id, binder)
            .unwrap_or_else(|| self.resolve_portability_symbol(sym_id, binder));
        let symbol = binder.symbols.get(resolved_sym_id)?;

        for decl_idx in symbol.declarations.iter().copied() {
            let Some(decl_node) = self.arena.get(decl_idx) else {
                continue;
            };

            if let Some(prop_decl) = self.arena.get_property_decl(decl_node)
                && let Some(type_id) = self.get_node_type_or_names(&[decl_idx, prop_decl.name])
            {
                let effective_type = if self
                    .arena
                    .has_modifier(&prop_decl.modifiers, SyntaxKind::ReadonlyKeyword)
                {
                    type_id
                } else {
                    self.type_interner
                        .map(|interner| {
                            tsz_solver::operations::widening::widen_literal_type(interner, type_id)
                        })
                        .unwrap_or(type_id)
                };
                return Some(self.print_type_id(effective_type));
            }

            if let Some(accessor) = self.arena.get_accessor(decl_node)
                && let Some(type_id) = self.get_node_type_or_names(&[decl_idx, accessor.name])
            {
                return Some(self.print_type_id(type_id));
            }
        }

        let type_id = cache.symbol_types.get(&resolved_sym_id).copied()?;
        Some(self.print_type_id(type_id))
    }

    pub(in crate::declaration_emitter) fn local_type_annotation_text(
        &self,
        type_idx: NodeIndex,
    ) -> Option<String> {
        let text = self.source_file_text.as_deref()?;
        let node = self.arena.get(type_idx)?;
        let start = usize::try_from(node.pos).ok()?;
        let end = usize::try_from(node.end).ok()?;
        let slice = text.get(start..end)?.trim();
        (!slice.is_empty()).then(|| slice.to_string())
    }

    pub(in crate::declaration_emitter) fn preferred_annotation_name_text(
        &self,
        type_idx: NodeIndex,
    ) -> Option<String> {
        let raw = self.local_type_annotation_text(type_idx)?;
        Self::simple_type_reference_name(&raw).map(|_| raw)
    }

    pub(in crate::declaration_emitter) fn call_expression_declared_return_type_text(
        &self,
        expr_idx: NodeIndex,
    ) -> Option<String> {
        let expr_node = self.arena.get(expr_idx)?;
        if expr_node.kind != syntax_kind_ext::CALL_EXPRESSION {
            return None;
        }

        let call = self.arena.get_call_expr(expr_node)?;
        let binder = self.binder?;
        let raw_sym_id = self.value_reference_symbol(call.expression)?;
        let imported_module = self
            .imported_value_module_specifier(raw_sym_id, binder)
            .or_else(|| self.imported_value_module_specifier_from_syntax(call.expression));
        let sym_id = self
            .resolve_portability_import_alias(raw_sym_id, binder)
            .or_else(|| {
                imported_module.as_deref().and_then(|module_specifier| {
                    self.imported_value_export_symbol_from_syntax(
                        call.expression,
                        module_specifier,
                        binder,
                    )
                })
            })
            .unwrap_or_else(|| self.resolve_portability_symbol(raw_sym_id, binder));
        let explicit_type_args = self.type_argument_list_source_text(call.type_arguments.as_ref());
        self.with_symbol_declarations(sym_id, |source_arena, decl_idx| {
            let decl_node = source_arena.get(decl_idx)?;
            let callable = Self::callable_decl_parts_from_node(source_arena, decl_node)?;
            let source_file = self.arena_source_file(source_arena)?;
            let is_ambient_function =
                source_file.is_declaration_file || source_arena.is_declare_ref(callable.modifiers);
            let is_source_overload_signature = callable.body.is_none()
                && callable
                    .type_parameters
                    .is_some_and(|params| !params.nodes.is_empty());
            let is_source_with_return_annotation =
                callable.body.is_some() && callable.type_annotation.is_some();
            if (!is_ambient_function
                && !is_source_overload_signature
                && !is_source_with_return_annotation)
                || !callable.type_annotation.is_some()
                || !self.function_signature_accepts_call_arguments(
                    source_arena,
                    callable.parameters,
                    call,
                )
            {
                return None;
            }

            let mut type_text = self
                .emit_type_node_text_from_arena(source_arena, callable.type_annotation)
                .or_else(|| self.source_slice_from_arena(source_arena, callable.type_annotation))?
                .trim_end()
                .trim_end_matches(';')
                .trim_end()
                .to_string();

            let mut type_param_names = Vec::new();
            let mut type_param_substitutions = Vec::new();
            let mut type_param_fallbacks = Vec::new();
            if let Some(type_params) = callable.type_parameters {
                for &param_idx in &type_params.nodes {
                    if let Some(param_node) = source_arena.get(param_idx)
                        && let Some(param) = source_arena.get_type_parameter(param_node)
                        && let Some(name_text) =
                            self.identifier_text_from_arena(source_arena, param.name)
                    {
                        let fallback = if param.default.is_some() {
                            self.emit_type_node_text_from_arena(source_arena, param.default)
                                .or_else(|| {
                                    self.source_slice_from_arena(source_arena, param.default)
                                })
                        } else if param.constraint.is_some() {
                            self.emit_type_node_text_from_arena(source_arena, param.constraint)
                                .or_else(|| {
                                    self.source_slice_from_arena(source_arena, param.constraint)
                                })
                        } else {
                            None
                        };
                        if let Some(fallback) = fallback {
                            type_param_fallbacks.push((name_text.clone(), fallback));
                        }
                        type_param_names.push(name_text);
                    }
                }

                if !explicit_type_args.is_empty() {
                    for (name_text, arg_text) in
                        type_param_names.iter().zip(explicit_type_args.iter())
                    {
                        type_param_substitutions.push((name_text.clone(), arg_text.clone()));
                    }
                } else {
                    type_param_substitutions.extend(
                        self.infer_call_type_param_substitutions_from_arguments(
                            source_arena,
                            callable.parameters,
                            call,
                            &type_param_names,
                        ),
                    );
                }
            }
            for (name_text, fallback_text) in &type_param_fallbacks {
                if type_param_substitutions
                    .iter()
                    .any(|(substituted, _)| substituted == name_text)
                    || !Self::contains_whole_word_in_text(&type_text, name_text)
                {
                    continue;
                }
                let fallback_text =
                    Self::replace_whole_words_in_text(fallback_text, &type_param_substitutions);
                type_param_substitutions.push((name_text.clone(), fallback_text));
            }
            if explicit_type_args.is_empty()
                && type_param_substitutions.is_empty()
                && type_param_names
                    .iter()
                    .any(|name| Self::contains_whole_word_in_text(&type_text, name))
            {
                return None;
            }
            type_text = Self::replace_whole_words_in_text(&type_text, &type_param_substitutions);
            if type_param_names
                .iter()
                .any(|name| Self::contains_whole_word_in_text(&type_text, name))
            {
                return None;
            }
            if Self::leading_type_reference_name(&type_text)
                .is_some_and(Self::is_builtin_conditional_utility_type_name)
                && let Some(type_id) = self.get_node_type_or_names(&[expr_idx])
            {
                return Some(self.print_type_id_expanded_for_inferred_declaration(type_id));
            }
            if let Some(expanded) =
                self.event_like_correlated_alias_return_text(source_arena, &type_text, call)
            {
                type_text = expanded;
            } else if let Some(expanded) =
                Self::expand_tuple_item_lookup_mapped_type_text(&type_text)
            {
                type_text = expanded;
            }

            let source_path = self.get_symbol_source_path(sym_id, binder).or_else(|| {
                self.arena_to_path
                    .get(&(source_arena as *const NodeArena as usize))
                    .cloned()
            });
            type_text = self.qualify_foreign_imported_names_in_text(source_arena, &type_text);
            if let (Some(source_path), Some(module_specifier)) =
                (source_path.as_deref(), imported_module.as_deref())
                && let Some(rewritten) = self.rewrite_typeof_import_default_return_type(
                    source_path,
                    module_specifier,
                    &type_text,
                    binder,
                )
            {
                type_text = rewritten;
            }
            if let Some(module_specifier) = imported_module.as_deref() {
                type_text = self.qualify_ambient_module_exported_names_in_text(
                    source_arena,
                    module_specifier,
                    &type_text,
                    &type_param_names,
                );
                if !Self::type_text_contains_import_type(&type_text)
                    && let Some(root_name) = Self::leading_type_reference_name(&type_text)
                    && !type_param_names.iter().any(|name| name == root_name)
                    && self.imported_module_exports_name(binder, module_specifier, root_name)
                {
                    type_text = format!(
                        "import(\"{module_specifier}\").{}{}",
                        root_name,
                        &type_text[root_name.len()..]
                    );
                }
            }
            if let Some(source_path) = source_path.as_deref() {
                if !Self::type_text_contains_import_type(&type_text) {
                    type_text = self.qualify_foreign_exported_names_in_text(
                        source_arena,
                        source_path,
                        &type_text,
                        &type_param_names,
                    );
                }
                if self
                    .current_file_path
                    .as_deref()
                    .is_some_and(|current_path| {
                        !self.paths_refer_to_same_source_file(current_path, source_path)
                            && type_text.starts_with("typeof ")
                            && !Self::type_text_contains_import_type(&type_text)
                    })
                {
                    return None;
                }
                if self.type_text_contains_unqualified_foreign_value_export(
                    source_arena,
                    source_path,
                    &type_text,
                ) {
                    return None;
                }
            }
            Some(type_text)
        })
    }

    fn is_builtin_conditional_utility_type_name(name: &str) -> bool {
        matches!(name, "Exclude" | "Extract" | "NonNullable")
    }

    fn rewrite_typeof_import_default_return_type(
        &self,
        source_path: &str,
        imported_module: &str,
        type_text: &str,
        binder: &BinderState,
    ) -> Option<String> {
        let import_text = type_text.trim().strip_prefix("typeof ")?;
        let (start, module_specifier, tail) = Self::next_import_type_text(import_text)?;
        if start != 0 || tail.trim() != ".default" {
            return None;
        }

        let target_module_path = self
            .matching_module_export_paths(binder, source_path, &module_specifier)
            .into_iter()
            .next()?;
        let default_sym = binder
            .module_exports
            .get(target_module_path)?
            .get("default")?;
        let default_sym = self.resolve_portability_symbol(default_sym, binder);
        let declared_type = self.declared_type_annotation_text_for_symbol(default_sym)?;
        let public_module =
            Self::combine_public_module_specifier(imported_module, &module_specifier)?;
        let exported_name = Self::leading_type_reference_name(&declared_type)?;
        if binder
            .module_exports
            .get(target_module_path)
            .is_some_and(|exports| exports.get(exported_name).is_some())
        {
            return Some(format!(
                "import(\"{public_module}\").{}{}",
                exported_name,
                &declared_type[exported_name.len()..]
            ));
        }

        None
    }

    fn combine_public_module_specifier(base: &str, relative: &str) -> Option<String> {
        if base.starts_with('.') || base.starts_with('/') {
            return None;
        }
        let mut parts = base.split('/').collect::<Vec<_>>();
        if parts.is_empty() {
            return None;
        }
        let package_len = if parts[0].starts_with('@') { 2 } else { 1 };
        if parts.len() < package_len {
            return None;
        }
        if parts.len() > package_len {
            parts.pop();
        }

        for segment in relative.split('/') {
            match segment {
                "" | "." => {}
                ".." if parts.len() > package_len => {
                    parts.pop();
                }
                ".." => return None,
                text => parts.push(text),
            }
        }

        Some(parts.join("/"))
    }

    fn imported_value_module_specifier(
        &self,
        sym_id: SymbolId,
        binder: &BinderState,
    ) -> Option<String> {
        self.import_symbol_map
            .get(&sym_id)
            .cloned()
            .or_else(|| binder.symbols.get(sym_id)?.import_module.clone())
    }

    fn imported_value_module_specifier_from_syntax(&self, expr_idx: NodeIndex) -> Option<String> {
        let local_name = self.get_identifier_text(expr_idx)?;
        let source_file = self
            .current_source_file_idx
            .and_then(|source_file_idx| self.arena.get(source_file_idx))
            .and_then(|node| self.arena.get_source_file(node))
            .or_else(|| self.arena_source_file(self.arena))?;

        for &stmt_idx in &source_file.statements.nodes {
            let Some(stmt_node) = self.arena.get(stmt_idx) else {
                continue;
            };
            let Some(import) = self.arena.get_import_decl(stmt_node) else {
                continue;
            };
            let Some(module_node) = self.arena.get(import.module_specifier) else {
                continue;
            };
            let Some(module_lit) = self.arena.get_literal(module_node) else {
                continue;
            };
            let Some(clause_node) = self.arena.get(import.import_clause) else {
                continue;
            };
            let Some(clause) = self.arena.get_import_clause(clause_node) else {
                continue;
            };

            if clause.name.is_some()
                && self.get_identifier_text(clause.name).as_deref() == Some(local_name.as_str())
            {
                return Some(module_lit.text.clone());
            }

            if clause.named_bindings.is_some()
                && let Some(bindings_node) = self.arena.get(clause.named_bindings)
                && let Some(bindings) = self.arena.get_named_imports(bindings_node)
            {
                if bindings.name.is_some()
                    && self.get_identifier_text(bindings.name).as_deref()
                        == Some(local_name.as_str())
                {
                    return Some(module_lit.text.clone());
                }
                for &spec_idx in &bindings.elements.nodes {
                    let Some(spec_node) = self.arena.get(spec_idx) else {
                        continue;
                    };
                    let Some(specifier) = self.arena.get_specifier(spec_node) else {
                        continue;
                    };
                    if self.get_identifier_text(specifier.name).as_deref()
                        == Some(local_name.as_str())
                    {
                        return Some(module_lit.text.clone());
                    }
                }
            }
        }

        None
    }

    fn imported_value_export_symbol_from_syntax(
        &self,
        expr_idx: NodeIndex,
        module_specifier: &str,
        binder: &BinderState,
    ) -> Option<SymbolId> {
        let export_name =
            self.imported_value_export_name_from_syntax(expr_idx, module_specifier)?;
        if let Some(sym_id) = binder
            .module_exports
            .get(module_specifier)
            .and_then(|exports| exports.get(&export_name))
        {
            return Some(sym_id);
        }

        let module_paths = if module_specifier.starts_with('.') || module_specifier.starts_with('/')
        {
            let current_path = self.current_file_path.as_deref()?;
            self.matching_module_export_paths(binder, current_path, module_specifier)
        } else {
            let mut paths: Vec<_> = binder
                .module_exports
                .keys()
                .filter_map(|module_path| {
                    (self.node_modules_path_matches_import_specifier(module_path, module_specifier)
                        || self.node_modules_package_path_matches_import_specifier(
                            module_path,
                            module_specifier,
                        ))
                    .then_some(module_path.as_str())
                })
                .collect();
            paths.sort();
            paths
        };
        for module_path in module_paths {
            if let Some(sym_id) = binder
                .module_exports
                .get(module_path)
                .and_then(|exports| exports.get(&export_name))
            {
                return Some(sym_id);
            }
        }

        None
    }

    fn imported_value_export_name_from_syntax(
        &self,
        expr_idx: NodeIndex,
        module_specifier: &str,
    ) -> Option<String> {
        let local_name = self.get_identifier_text(expr_idx)?;
        let source_file = self
            .current_source_file_idx
            .and_then(|source_file_idx| self.arena.get(source_file_idx))
            .and_then(|node| self.arena.get_source_file(node))
            .or_else(|| self.arena_source_file(self.arena))?;

        for &stmt_idx in &source_file.statements.nodes {
            let Some(stmt_node) = self.arena.get(stmt_idx) else {
                continue;
            };
            let Some(import) = self.arena.get_import_decl(stmt_node) else {
                continue;
            };
            let Some(module_node) = self.arena.get(import.module_specifier) else {
                continue;
            };
            let Some(module_lit) = self.arena.get_literal(module_node) else {
                continue;
            };
            if module_lit.text != module_specifier {
                continue;
            }

            let Some(clause_node) = self.arena.get(import.import_clause) else {
                continue;
            };
            let Some(clause) = self.arena.get_import_clause(clause_node) else {
                continue;
            };

            if clause.name.is_some()
                && self.get_identifier_text(clause.name).as_deref() == Some(local_name.as_str())
            {
                return Some("default".to_string());
            }

            if clause.named_bindings.is_some()
                && let Some(bindings_node) = self.arena.get(clause.named_bindings)
                && let Some(bindings) = self.arena.get_named_imports(bindings_node)
            {
                if bindings.name.is_some() {
                    continue;
                }
                for &spec_idx in &bindings.elements.nodes {
                    let Some(spec_node) = self.arena.get(spec_idx) else {
                        continue;
                    };
                    let Some(specifier) = self.arena.get_specifier(spec_node) else {
                        continue;
                    };
                    if self.get_identifier_text(specifier.name).as_deref()
                        != Some(local_name.as_str())
                    {
                        continue;
                    }
                    return self
                        .get_identifier_text(specifier.property_name)
                        .or_else(|| self.get_identifier_text(specifier.name));
                }
            }
        }

        None
    }

    fn node_modules_package_path_matches_import_specifier(
        &self,
        module_path: &str,
        module_specifier: &str,
    ) -> bool {
        use std::path::{Component, Path};

        let components: Vec<_> = Path::new(module_path).components().collect();
        let Some(nm_idx) = components.iter().position(|component| {
            matches!(component, Component::Normal(part) if part.to_str() == Some("node_modules"))
        }) else {
            return false;
        };

        let pkg_start = nm_idx + 1;
        if components.len() == pkg_start + 1
            && let Component::Normal(part) = components[pkg_start]
            && let Some(file_name) = part.to_str()
            && let Some(runtime_path) = self.declaration_runtime_relative_path(file_name)
        {
            let runtime_path = runtime_path.trim_start_matches("./");
            let package_name = runtime_path
                .strip_suffix(".js")
                .unwrap_or(runtime_path)
                .trim_end_matches("/index");
            return module_specifier == package_name;
        }

        let pkg_len = if components.get(pkg_start).is_some_and(|component| {
            matches!(component, Component::Normal(part) if part.to_str().is_some_and(|text| text.starts_with('@')))
        }) {
            2
        } else {
            1
        };
        if components.len() < pkg_start + pkg_len {
            return false;
        }

        let package_name = components[pkg_start..pkg_start + pkg_len]
            .iter()
            .filter_map(|component| match component {
                Component::Normal(part) => part.to_str(),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("/");

        let relative_path = components[pkg_start + pkg_len..]
            .iter()
            .filter_map(|component| match component {
                Component::Normal(part) => part.to_str(),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("/");
        let Some(runtime_subpath) = self.declaration_runtime_relative_path(&relative_path) else {
            return false;
        };
        let mut runtime_subpath = runtime_subpath.trim_start_matches("./").to_string();
        if runtime_subpath.ends_with("/index.js") {
            runtime_subpath.truncate(runtime_subpath.len() - "/index.js".len());
        } else if runtime_subpath == "index.js" {
            runtime_subpath.clear();
        }

        if runtime_subpath.is_empty() {
            module_specifier == package_name
        } else {
            module_specifier == format!("{package_name}/{runtime_subpath}")
        }
    }

    fn imported_module_exports_name(
        &self,
        binder: &BinderState,
        module_specifier: &str,
        export_name: &str,
    ) -> bool {
        if binder
            .module_exports
            .get(module_specifier)
            .is_some_and(|exports| exports.get(export_name).is_some())
        {
            return true;
        }

        if let Some(current_path) = self.current_file_path.as_deref() {
            for module_path in
                self.matching_module_export_paths(binder, current_path, module_specifier)
            {
                if binder
                    .module_exports
                    .get(module_path)
                    .is_some_and(|exports| exports.get(export_name).is_some())
                {
                    return true;
                }
            }
        }

        if !module_specifier.starts_with('.') && !module_specifier.starts_with('/') {
            return binder.module_exports.iter().any(|(module_path, exports)| {
                (self.node_modules_path_matches_import_specifier(module_path, module_specifier)
                    || self.node_modules_package_path_matches_import_specifier(
                        module_path,
                        module_specifier,
                    ))
                    && exports.get(export_name).is_some()
            });
        }

        false
    }

    fn leading_type_reference_name(type_text: &str) -> Option<&str> {
        let trimmed = type_text.trim_start();
        if Self::type_text_starts_with_import_type(trimmed) || trimmed.starts_with("typeof ") {
            return None;
        }
        let end = trimmed
            .char_indices()
            .find_map(|(idx, ch)| {
                (!(ch == '_' || ch == '$' || ch.is_ascii_alphanumeric())).then_some(idx)
            })
            .unwrap_or(trimmed.len());
        if end == 0 {
            return None;
        }
        let name = &trimmed[..end];
        name.chars()
            .next()
            .is_some_and(|ch| ch == '_' || ch == '$' || ch.is_ascii_alphabetic())
            .then_some(name)
    }

    fn type_text_starts_with_string_intrinsic(type_text: &str) -> bool {
        matches!(
            Self::leading_type_reference_name(type_text),
            Some("Uppercase" | "Lowercase" | "Capitalize" | "Uncapitalize")
        )
    }

    pub(in crate::declaration_emitter) fn function_signature_accepts_call_arguments(
        &self,
        source_arena: &NodeArena,
        parameters: &NodeList,
        call: &tsz_parser::parser::node::CallExprData,
    ) -> bool {
        let arg_count = call.arguments.as_ref().map_or(0, |args| args.nodes.len());
        let mut required_count = 0usize;
        let mut has_rest = false;

        for &param_idx in &parameters.nodes {
            let Some(param_node) = source_arena.get(param_idx) else {
                continue;
            };
            let Some(param) = source_arena.get_parameter(param_node) else {
                continue;
            };
            has_rest |= param.dot_dot_dot_token;
            if !param.dot_dot_dot_token
                && !param.question_token
                && param.initializer == NodeIndex::NONE
            {
                required_count += 1;
            }
        }

        arg_count >= required_count && (has_rest || arg_count <= parameters.nodes.len())
    }

    pub(in crate::declaration_emitter) fn callable_decl_parts_from_node<'b>(
        source_arena: &'b NodeArena,
        decl_node: &'b Node,
    ) -> Option<CallableDeclParts<'b>> {
        if let Some(func) = source_arena.get_function(decl_node) {
            return Some(CallableDeclParts {
                modifiers: func.modifiers.as_ref(),
                type_parameters: func.type_parameters.as_ref(),
                parameters: &func.parameters,
                type_annotation: func.type_annotation,
                body: func.body,
            });
        }

        if let Some(method) = source_arena.get_method_decl(decl_node) {
            return Some(CallableDeclParts {
                modifiers: method.modifiers.as_ref(),
                type_parameters: method.type_parameters.as_ref(),
                parameters: &method.parameters,
                type_annotation: method.type_annotation,
                body: method.body,
            });
        }

        None
    }

    fn qualify_ambient_module_exported_names_in_text(
        &self,
        source_arena: &NodeArena,
        module_specifier: &str,
        text: &str,
        excluded_names: &[String],
    ) -> String {
        let Some(source_file) = self.arena_source_file(source_arena) else {
            return text.to_string();
        };

        let mut replacements = Vec::new();
        for &stmt_idx in &source_file.statements.nodes {
            self.collect_ambient_module_export_replacements(
                source_arena,
                stmt_idx,
                module_specifier,
                excluded_names,
                &mut replacements,
            );
        }

        Self::replace_whole_words_in_text(text, &replacements)
    }

    fn collect_ambient_module_export_replacements(
        &self,
        source_arena: &NodeArena,
        module_idx: NodeIndex,
        module_specifier: &str,
        excluded_names: &[String],
        replacements: &mut Vec<(String, String)>,
    ) {
        let Some(module_node) = source_arena.get(module_idx) else {
            return;
        };
        let Some(module) = source_arena.get_module(module_node) else {
            return;
        };

        let Some(name_node) = source_arena.get(module.name) else {
            return;
        };
        if name_node.kind != SyntaxKind::StringLiteral as u16 {
            return;
        }
        let Some(literal) = source_arena.get_literal(name_node) else {
            return;
        };
        if literal.text != module_specifier {
            return;
        }

        let Some(body_node) = source_arena.get(module.body) else {
            return;
        };
        if source_arena.get_module(body_node).is_some() {
            self.collect_ambient_module_export_replacements(
                source_arena,
                module.body,
                module_specifier,
                excluded_names,
                replacements,
            );
            return;
        }

        let Some(block) = source_arena.get_module_block(body_node) else {
            return;
        };
        let Some(statements) = block.statements.as_ref() else {
            return;
        };

        for &stmt_idx in &statements.nodes {
            let Some(stmt_node) = source_arena.get(stmt_idx) else {
                continue;
            };
            let export_name = if let Some(decl) = source_arena.get_interface(stmt_node) {
                Some(decl.name)
            } else if let Some(decl) = source_arena.get_type_alias(stmt_node) {
                Some(decl.name)
            } else if let Some(decl) = source_arena.get_class(stmt_node) {
                Some(decl.name)
            } else if let Some(decl) = source_arena.get_enum(stmt_node) {
                Some(decl.name)
            } else {
                source_arena.get_function(stmt_node).map(|decl| decl.name)
            }
            .and_then(|name_idx| self.identifier_text_from_arena(source_arena, name_idx));

            let Some(export_name) = export_name else {
                continue;
            };
            if excluded_names.iter().any(|name| name == &export_name) {
                continue;
            }
            let qualified = format!("import(\"{module_specifier}\").{export_name}");
            replacements.push((export_name, qualified));
        }
    }

    pub(in crate::declaration_emitter) fn call_expression_source_return_type_text(
        &self,
        expr_idx: NodeIndex,
    ) -> Option<String> {
        let expr_node = self.arena.get(expr_idx)?;
        if expr_node.kind != syntax_kind_ext::CALL_EXPRESSION {
            return None;
        }

        let call = self.arena.get_call_expr(expr_node)?;
        let sym_id = self.value_reference_symbol(call.expression)?;
        let binder = self.binder?;
        let symbol = binder.symbols.get(sym_id)?;
        let source_arena = binder
            .symbol_arenas
            .get(&sym_id)
            .map(|arena| arena.as_ref())
            .unwrap_or(self.arena);

        let mut function_decl_count = 0usize;
        for decl_idx in symbol.declarations.iter().copied() {
            let Some(func) = self.callable_function_from_symbol_decl(source_arena, decl_idx) else {
                continue;
            };
            if func
                .type_parameters
                .as_ref()
                .is_some_and(|params| !params.nodes.is_empty())
            {
                continue;
            }
            function_decl_count += 1;
            if function_decl_count > 1 {
                return None;
            }
        }

        for decl_idx in symbol.declarations.iter().copied() {
            let Some(func) = self.callable_function_from_symbol_decl(source_arena, decl_idx) else {
                continue;
            };
            if func
                .type_parameters
                .as_ref()
                .is_some_and(|params| !params.nodes.is_empty())
            {
                continue;
            }
            if func.type_annotation.is_some() {
                if let Some(type_text) =
                    self.source_slice_from_arena(source_arena, func.type_annotation)
                    && self.source_return_type_annotation_is_reusable(
                        source_arena,
                        func.type_annotation,
                    )
                {
                    return Some(
                        type_text
                            .trim_end()
                            .trim_end_matches(';')
                            .trim_end()
                            .to_string(),
                    );
                }
            } else if func.body.is_some()
                && let Some(type_text) = {
                    let mut scratch = if std::ptr::eq(source_arena, self.arena)
                        && let (Some(type_cache), Some(type_interner), Some(binder)) =
                            (&self.type_cache, self.type_interner, self.binder)
                    {
                        DeclarationEmitter::with_type_info(
                            source_arena,
                            type_cache.clone(),
                            type_interner,
                            binder,
                        )
                    } else {
                        DeclarationEmitter::new(source_arena)
                    };
                    let source_file = self.arena_source_file(source_arena)?;
                    scratch.source_is_declaration_file = source_file.is_declaration_file;
                    scratch.source_is_js_file = scratch.source_file_is_js(source_file);
                    scratch.current_source_file_idx = self.current_source_file_idx;
                    scratch.source_file_text = Some(source_file.text.clone());
                    scratch.current_file_path = self.current_file_path.clone();
                    scratch.current_arena = self.current_arena.clone();
                    scratch.arena_to_path = self.arena_to_path.clone();
                    scratch.indent_level = self.indent_level;
                    let type_text = scratch.source_function_return_type_text(func)?;
                    let type_text =
                        scratch.substitute_call_result_parameter_type_queries(func, &type_text);
                    let (type_text, _) =
                        scratch.function_return_type_text_for_declaration_scope(func, &type_text);
                    Some(type_text)
                }
            {
                return Some(Self::strip_synthetic_anonymous_object_members(&type_text));
            }
        }

        None
    }

    fn substitute_call_result_parameter_type_queries(
        &self,
        func: &tsz_parser::parser::node::FunctionData,
        source_type_text: &str,
    ) -> String {
        if !source_type_text.contains("typeof ") {
            return source_type_text.to_string();
        }

        let mut text = source_type_text.to_string();
        for param_idx in func.parameters.nodes.iter().copied() {
            let Some(param_node) = self.arena.get(param_idx) else {
                continue;
            };
            let Some(param) = self.arena.get_parameter(param_node) else {
                continue;
            };
            let Some(param_name) = self.get_identifier_text(param.name) else {
                continue;
            };
            let Some(param_type_text) = self.function_parameter_type_text(func, param.name) else {
                continue;
            };
            if !Self::type_text_can_substitute_type_query_parameter(&param_type_text) {
                continue;
            }
            text = Self::replace_typeof_identifier(&text, &param_name, &param_type_text).0;
        }
        text
    }

    fn type_text_can_substitute_type_query_parameter(type_text: &str) -> bool {
        let trimmed = type_text.trim();
        if Self::simple_type_reference_name(trimmed).is_some() {
            return true;
        }
        if matches!(trimmed, "true" | "false" | "null" | "undefined") {
            return true;
        }
        if trimmed.parse::<f64>().is_ok() {
            return true;
        }
        if trimmed.len() >= 2 {
            let bytes = trimmed.as_bytes();
            return (bytes[0] == b'"' && bytes[trimmed.len() - 1] == b'"')
                || (bytes[0] == b'\'' && bytes[trimmed.len() - 1] == b'\'');
        }
        false
    }

    fn callable_function_from_symbol_decl<'b>(
        &self,
        source_arena: &'b NodeArena,
        decl_idx: NodeIndex,
    ) -> Option<&'b tsz_parser::parser::node::FunctionData> {
        if let Some(func) = source_arena
            .get(decl_idx)
            .and_then(|node| source_arena.get_function(node))
        {
            return Some(func);
        }

        let mut current = decl_idx;
        for _ in 0..8 {
            let node = source_arena.get(current)?;
            if let Some(var_decl) = source_arena.get_variable_declaration(node) {
                let initializer_node = source_arena.get(var_decl.initializer)?;
                if initializer_node.kind == syntax_kind_ext::ARROW_FUNCTION
                    || initializer_node.kind == syntax_kind_ext::FUNCTION_EXPRESSION
                {
                    return source_arena.get_function(initializer_node);
                }
            }
            current = source_arena.parent_of(current)?;
        }

        None
    }

    fn source_function_return_type_text(
        &self,
        func: &tsz_parser::parser::node::FunctionData,
    ) -> Option<String> {
        let body_node = self.arena.get(func.body)?;
        if body_node.kind == syntax_kind_ext::BLOCK {
            return self.function_body_preferred_return_type_text(func.body);
        }

        self.preferred_expression_type_text(func.body)
            .or_else(|| self.infer_fallback_type_text_at(func.body, 0))
            .filter(|text| !text.is_empty() && text != "any")
    }

    fn source_return_type_annotation_is_reusable(
        &self,
        source_arena: &NodeArena,
        type_annotation: NodeIndex,
    ) -> bool {
        let Some(binder) = self.binder else {
            return true;
        };
        let Some(type_node) = source_arena.get(type_annotation) else {
            return true;
        };
        if type_node.kind != syntax_kind_ext::TYPE_REFERENCE {
            return true;
        }
        let Some(type_ref) = source_arena.get_type_ref(type_node) else {
            return true;
        };
        let Some(name_node) = source_arena.get(type_ref.type_name) else {
            return true;
        };
        if name_node.kind != SyntaxKind::Identifier as u16 {
            return true;
        }

        let Some(sym_id) = binder
            .get_node_symbol(type_ref.type_name)
            .or_else(|| binder.resolve_identifier(source_arena, type_ref.type_name))
        else {
            return true;
        };
        let Some(symbol) = binder.symbols.get(sym_id) else {
            return true;
        };
        let parent_id = symbol.parent;
        if parent_id == SymbolId::NONE
            || self.enclosing_namespace_symbol == Some(parent_id)
            || symbol.has_any_flags(symbol_flags::ENUM_MEMBER)
        {
            return true;
        }
        let Some(parent) = binder.symbols.get(parent_id) else {
            return true;
        };
        if !parent.has_any_flags(symbol_flags::NAMESPACE | symbol_flags::ENUM) {
            return true;
        }
        if !symbol.is_exported && !symbol.has_any_flags(symbol_flags::EXPORT_VALUE) {
            return false;
        }
        parent.is_exported || parent.has_any_flags(symbol_flags::EXPORT_VALUE)
    }

    pub(in crate::declaration_emitter) fn tagged_template_declared_return_type_text(
        &self,
        expr_idx: NodeIndex,
    ) -> Option<String> {
        let expr_node = self.arena.get(expr_idx)?;
        if expr_node.kind != syntax_kind_ext::TAGGED_TEMPLATE_EXPRESSION {
            return None;
        }

        let tagged = self.arena.get_tagged_template(expr_node)?;
        let sym_id = self.value_reference_symbol(tagged.tag)?;
        let binder = self.binder?;
        let symbol = binder.symbols.get(sym_id)?;
        let source_arena = binder.symbol_arenas.get(&sym_id)?;
        let source_file = self.arena_source_file(source_arena.as_ref())?;
        if !source_file.is_declaration_file {
            return None;
        }

        for decl_idx in symbol.declarations.iter().copied() {
            let Some(decl_node) = source_arena.get(decl_idx) else {
                continue;
            };
            let Some(func) = source_arena.get_function(decl_node) else {
                continue;
            };
            if func.type_annotation.is_none() {
                continue;
            }
            if let Some(type_text) =
                self.source_slice_from_arena(source_arena.as_ref(), func.type_annotation)
            {
                return Some(
                    type_text
                        .trim_end()
                        .trim_end_matches(';')
                        .trim_end()
                        .to_string(),
                );
            }
        }

        None
    }

    pub(in crate::declaration_emitter) fn nameable_new_expression_type_text(
        &self,
        expr_idx: NodeIndex,
    ) -> Option<String> {
        let expr_node = self.arena.get(expr_idx)?;
        if expr_node.kind != syntax_kind_ext::NEW_EXPRESSION {
            return None;
        }

        let new_expr = self.arena.get_call_expr(expr_node)?;
        let base_text = self.declaration_constructor_expression_text(new_expr.expression)?;
        let type_args = self.type_argument_list_source_text(new_expr.type_arguments.as_ref());
        if type_args.is_empty() {
            Some(base_text)
        } else {
            Some(format!("{base_text}<{}>", type_args.join(", ")))
        }
    }

    pub(in crate::declaration_emitter) fn type_argument_list_source_text(
        &self,
        type_args: Option<&NodeList>,
    ) -> Vec<String> {
        let Some(list) = type_args else {
            return Vec::new();
        };

        list.nodes
            .iter()
            .enumerate()
            .filter_map(|(index, &arg)| {
                let node = self.arena.get(arg)?;
                let mut text = self.get_source_slice_no_semi(node.pos, node.end)?;
                if self.first_type_argument_needs_parentheses(arg, index == 0) {
                    text = format!("({text})");
                }
                Some(text)
            })
            .collect()
    }

    pub(crate) fn first_type_argument_needs_parentheses(
        &self,
        type_arg_idx: NodeIndex,
        is_first: bool,
    ) -> bool {
        if !is_first {
            return false;
        }

        self.arena
            .get(type_arg_idx)
            .and_then(|node| self.arena.get_function_type(node))
            .is_some_and(|func| {
                !func
                    .type_parameters
                    .as_ref()
                    .is_none_or(|params| params.nodes.is_empty())
            })
    }

    pub(in crate::declaration_emitter) fn declaration_constructor_expression_text(
        &self,
        expr_idx: NodeIndex,
    ) -> Option<String> {
        let expr_idx = self.skip_parenthesized_expression(expr_idx)?;
        if self.is_module_exports_reference(expr_idx) {
            let source_file_idx = self.current_source_file_idx?;
            let source_file_node = self.arena.get(source_file_idx)?;
            let source_file = self.arena.get_source_file(source_file_node)?;
            if source_file
                .statements
                .nodes
                .iter()
                .copied()
                .any(|stmt_idx| {
                    self.js_anonymous_export_equals_class_expression_initializer(stmt_idx)
                        .is_some()
                })
            {
                return Some(r#"import(".")"#.to_string());
            }
        }
        let expr_node = self.arena.get(expr_idx)?;

        match expr_node.kind {
            k if k == SyntaxKind::Identifier as u16 => {
                self.identifier_constructor_reference_text(expr_idx)
            }
            k if k == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION => {
                let access = self.arena.get_access_expr(expr_node)?;
                let lhs = self.declaration_constructor_expression_text(access.expression)?;
                let rhs = self.get_identifier_text(access.name_or_argument)?;
                Some(format!("{lhs}.{rhs}"))
            }
            _ => None,
        }
    }

    pub(in crate::declaration_emitter) fn identifier_constructor_reference_text(
        &self,
        expr_idx: NodeIndex,
    ) -> Option<String> {
        let ident = self.get_identifier_text(expr_idx)?;
        let binder = self.binder?;
        let sym_id = self.resolve_identifier_symbol(expr_idx, &ident)?;
        let symbol = binder.symbols.get(sym_id)?;

        for decl_idx in symbol.declarations.iter().copied() {
            let decl_node = self.arena.get(decl_idx)?;
            if decl_node.kind != syntax_kind_ext::IMPORT_EQUALS_DECLARATION {
                if self.inside_non_ambient_namespace
                    && let Some(import_type) =
                        self.require_property_initializer_import_type(decl_node)
                {
                    return Some(import_type);
                }
                continue;
            }
            let import_eq = self.arena.get_import_decl(decl_node)?;
            let target_node = self.arena.get(import_eq.module_specifier)?;
            if target_node.kind == SyntaxKind::StringLiteral as u16 {
                return Some(ident);
            }
            return Some(ident);
        }

        Some(ident)
    }

    fn require_property_initializer_import_type(&self, decl_node: &Node) -> Option<String> {
        let (module, export_name) = self.require_property_initializer_parts(decl_node)?;
        Some(format!("import(\"{module}\").{export_name}"))
    }

    pub(in crate::declaration_emitter) fn resolve_identifier_symbol(
        &self,
        expr_idx: NodeIndex,
        ident: &str,
    ) -> Option<SymbolId> {
        let binder = self.binder?;
        let no_libs: &[Arc<BinderState>] = &[];
        binder
            .get_node_symbol(expr_idx)
            .or_else(|| {
                binder.resolve_name_with_filter(ident, self.arena, expr_idx, no_libs, |_| true)
            })
            .or_else(|| binder.file_locals.get(ident))
    }

    pub(in crate::declaration_emitter) fn array_literal_expression_type_text(
        &self,
        expr_idx: NodeIndex,
    ) -> Option<String> {
        let expr_node = self.arena.get(expr_idx)?;
        let array = self.arena.get_literal_expr(expr_node)?;
        if array.elements.nodes.is_empty() {
            return Some("any[]".to_string());
        }

        let mut element_types = Vec::with_capacity(array.elements.nodes.len());
        for elem_idx in array.elements.nodes.iter().copied() {
            // When strictNullChecks is off, skip null/undefined/void elements
            // so they don't pollute the array element type (tsc widens them away).
            if !self.strict_null_checks {
                if let Some(elem_node) = self.arena.get(elem_idx) {
                    let k = elem_node.kind;
                    if k == SyntaxKind::NullKeyword as u16
                        || k == SyntaxKind::UndefinedKeyword as u16
                    {
                        continue;
                    }
                    // Also skip void expressions (e.g., void 0)
                    if self.is_void_expression(elem_node) {
                        continue;
                    }
                }
                // Skip elements whose inferred type is null/undefined
                if let Some(type_id) = self.get_node_type_or_names(&[elem_idx])
                    && matches!(
                        type_id,
                        tsz_solver::types::TypeId::NULL
                            | tsz_solver::types::TypeId::UNDEFINED
                            | tsz_solver::types::TypeId::VOID
                    )
                {
                    continue;
                }
            }
            let elem_type = self.preferred_expression_type_text(elem_idx).or_else(|| {
                self.get_node_type_or_names(&[elem_idx])
                    .map(|type_id| self.print_type_id(type_id))
            })?;
            element_types.push(elem_type);
        }

        // If any element type is `any`, the whole union collapses to `any`
        // (matches tsc: T | any = any for all T).
        if element_types.iter().any(|t| t == "any") {
            return Some("any[]".to_string());
        }

        let mut distinct = Vec::new();
        for ty in element_types {
            if !distinct.iter().any(|existing| existing == &ty) {
                distinct.push(ty);
            }
        }
        Self::expand_object_union_arms_from_sibling_properties(&mut distinct);

        // tsc orders union members by `TypeFlags` when printing: for the
        // primitive intrinsics the rank is Any < Unknown < String < Number
        // < Boolean < BigInt < Symbol. Our solver-inferred array-element
        // union was otherwise rendered in construction order, so
        // `var a = [1, "hello"]` printed as `(number | string)[]` instead
        // of tsc's `(string | number)[]`. Apply a stable sort that reorders
        // known primitives while keeping non-primitive members in their
        // original relative order (a comparator that returns Equal for
        // them preserves insertion order under a stable sort).
        fn primitive_rank(name: &str) -> Option<u32> {
            match name {
                "any" => Some(1),
                "unknown" => Some(2),
                "string" => Some(4),
                "number" => Some(8),
                "boolean" => Some(16),
                "bigint" => Some(64),
                "symbol" => Some(4096),
                "object" => Some(33_554_432),
                _ => None,
            }
        }
        distinct.sort_by(|a, b| match (primitive_rank(a), primitive_rank(b)) {
            (Some(ra), Some(rb)) => ra.cmp(&rb),
            _ => std::cmp::Ordering::Equal,
        });

        let elem_text = if distinct.len() == 1 {
            distinct.pop()?
        } else {
            distinct.join(" | ")
        };
        let needs_parens =
            elem_text.contains("=>") || elem_text.contains('|') || elem_text.contains('&');
        if needs_parens {
            Some(format!("({elem_text})[]"))
        } else {
            Some(format!("{elem_text}[]"))
        }
    }

    fn expand_object_union_arms_from_sibling_properties(types: &mut [String]) {
        if types.len() <= 1 {
            return;
        }

        let has_empty_arm = types.iter().any(|ty| ty.trim() == "{}");
        let object_arm_names = types
            .iter()
            .map(|ty| {
                let trimmed = ty.trim();
                if !trimmed.starts_with('{') || !trimmed.ends_with('}') {
                    return None;
                }
                Some(Self::object_type_top_level_member_names(ty, true))
            })
            .collect::<Vec<_>>();

        let mut property_names = Vec::<String>::new();
        for names in object_arm_names.iter().flatten() {
            for name in names {
                if !property_names.iter().any(|existing| existing == name) {
                    property_names.push(name.clone());
                }
            }
        }
        if property_names.is_empty() {
            return;
        }

        if has_empty_arm {
            let members = property_names
                .into_iter()
                .map(|name| format!("    {name}?: undefined;"))
                .collect::<Vec<_>>()
                .join("\n");
            let replacement = format!("{{\n{members}\n}}");
            for ty in types.iter_mut() {
                if ty.trim() == "{}" {
                    *ty = replacement.clone();
                }
            }
            return;
        }

        if !types
            .iter()
            .any(|ty| Self::object_type_has_top_level_method(ty))
        {
            return;
        }

        for (ty, present_names) in types.iter_mut().zip(object_arm_names) {
            let Some(present_names) = present_names else {
                continue;
            };
            let missing_names = property_names
                .iter()
                .filter(|name| !present_names.iter().any(|present| present == *name))
                .cloned()
                .collect::<Vec<_>>();
            if !missing_names.is_empty() {
                *ty = Self::append_optional_undefined_members(ty, &missing_names);
            }
        }
    }

    fn append_optional_undefined_members(type_text: &str, missing_names: &[String]) -> String {
        let missing_members = missing_names
            .iter()
            .map(|name| format!("    {name}?: undefined;"))
            .collect::<Vec<_>>();

        if type_text.trim() == "{}" {
            return format!("{{\n{}\n}}", missing_members.join("\n"));
        }

        let mut lines = type_text.lines().map(str::to_string).collect::<Vec<_>>();
        let insert_at = lines
            .iter()
            .rposition(|line| line.trim() == "}")
            .unwrap_or(lines.len());
        lines.splice(insert_at..insert_at, missing_members);
        lines.join("\n")
    }

    fn object_type_top_level_member_names(type_text: &str, include_methods: bool) -> Vec<String> {
        let trimmed = type_text.trim();
        if !trimmed.starts_with('{') || !trimmed.ends_with('}') || trimmed == "{}" {
            return Vec::new();
        }

        let mut depth = 0usize;
        let mut names = Vec::new();
        for line in trimmed.lines() {
            if depth == 1
                && let Some(name) = Self::object_type_member_name_from_line(line, include_methods)
            {
                names.push(name);
            }
            depth = Self::update_object_text_brace_depth(depth, line);
        }
        names
    }

    fn object_type_has_top_level_method(type_text: &str) -> bool {
        let trimmed = type_text.trim();
        if !trimmed.starts_with('{') || !trimmed.ends_with('}') || trimmed == "{}" {
            return false;
        }

        let mut depth = 0usize;
        for line in trimmed.lines() {
            if depth == 1
                && Self::object_type_member_name_from_line(line, true).is_some_and(|name| {
                    let trimmed_line = line.trim_start();
                    trimmed_line.starts_with(&format!("{name}("))
                        || trimmed_line.starts_with(&format!("{name}?("))
                })
            {
                return true;
            }
            depth = Self::update_object_text_brace_depth(depth, line);
        }
        false
    }

    fn update_object_text_brace_depth(depth: usize, line: &str) -> usize {
        let mut depth = depth;
        let mut quote: Option<u8> = None;
        let mut escaped = false;
        for byte in line.bytes() {
            if let Some(active_quote) = quote {
                if escaped {
                    escaped = false;
                } else if byte == b'\\' {
                    escaped = true;
                } else if byte == active_quote {
                    quote = None;
                }
                continue;
            }

            match byte {
                b'\'' | b'"' => quote = Some(byte),
                b'{' => depth += 1,
                b'}' => depth = depth.saturating_sub(1),
                _ => {}
            }
        }
        depth
    }

    fn object_type_member_name_from_line(line: &str, include_methods: bool) -> Option<String> {
        if !include_methods {
            return Self::object_type_property_name_from_line(line);
        }

        let line = line.trim().trim_end_matches(';').trim();
        if line.is_empty() || line == "{" || line == "}" || line.starts_with('[') {
            return None;
        }

        let colon = Self::top_level_property_type_colon(line)?;
        let name = line
            .get(..colon)?
            .trim()
            .strip_prefix("readonly ")
            .unwrap_or_else(|| line.get(..colon).unwrap_or_default().trim())
            .trim()
            .trim_end_matches('?')
            .trim();

        if name.contains('(') {
            if !include_methods {
                return None;
            }
            return Self::object_type_method_name_from_prefix(name);
        }

        (!name.is_empty()).then(|| name.to_string())
    }

    fn object_type_method_name_from_prefix(prefix: &str) -> Option<String> {
        let paren = prefix.find('(')?;
        let name = prefix.get(..paren)?.trim().trim_end_matches('?').trim();
        if name.is_empty() || name == "new" || name.contains(' ') {
            return None;
        }
        Some(name.to_string())
    }

    fn object_type_property_name_from_line(line: &str) -> Option<String> {
        let line = line.trim().trim_end_matches(';').trim();
        if line.is_empty() || line == "{" || line == "}" || line.starts_with('[') {
            return None;
        }
        let colon = Self::top_level_property_type_colon(line)?;
        let name = line
            .get(..colon)?
            .trim()
            .strip_prefix("readonly ")
            .unwrap_or_else(|| line.get(..colon).unwrap_or_default().trim())
            .trim()
            .trim_end_matches('?')
            .trim();
        if name.contains('(') {
            return None;
        }
        (!name.is_empty()).then(|| name.to_string())
    }

    fn top_level_property_type_colon(line: &str) -> Option<usize> {
        let mut quote: Option<u8> = None;
        let mut escaped = false;
        let mut paren_depth = 0usize;
        let mut bracket_depth = 0usize;
        for (index, byte) in line.bytes().enumerate() {
            if let Some(active_quote) = quote {
                if escaped {
                    escaped = false;
                } else if byte == b'\\' {
                    escaped = true;
                } else if byte == active_quote {
                    quote = None;
                }
                continue;
            }

            match byte {
                b'\'' | b'"' => quote = Some(byte),
                b'(' => paren_depth += 1,
                b')' => paren_depth = paren_depth.saturating_sub(1),
                b'[' => bracket_depth += 1,
                b']' => bracket_depth = bracket_depth.saturating_sub(1),
                b':' if paren_depth == 0 && bracket_depth == 0 => return Some(index),
                _ => {}
            }
        }
        None
    }

    pub(in crate::declaration_emitter) fn short_circuit_expression_type_text(
        &self,
        expr_idx: NodeIndex,
    ) -> Option<String> {
        let expr_node = self.arena.get(expr_idx)?;
        let binary = self.arena.get_binary_expr(expr_node)?;
        if binary.operator_token != SyntaxKind::BarBarToken as u16 {
            return None;
        }
        if !self.expression_is_always_truthy_for_decl_emit(binary.left) {
            return None;
        }

        self.preferred_expression_type_text(binary.left)
            .or_else(|| {
                self.get_node_type_or_names(&[binary.left])
                    .map(|type_id| self.print_type_id(type_id))
            })
    }

    pub(in crate::declaration_emitter) fn emit_type_node_text(
        &self,
        type_idx: NodeIndex,
    ) -> Option<String> {
        self.emit_type_node_text_impl(type_idx, true)
    }

    // Like `emit_type_node_text` but omits `source_file_text` from the scratch
    // emitter so that string literals are normalized to double quotes.
    // tsc normalizes quotes in type assertions (e.g. `x as T<'a'>` → `T<"a">`).
    pub(in crate::declaration_emitter) fn emit_type_node_text_normalized(
        &self,
        type_idx: NodeIndex,
    ) -> Option<String> {
        self.emit_type_node_text_impl(type_idx, false)
    }

    fn emit_type_node_text_impl(
        &self,
        type_idx: NodeIndex,
        preserve_source_quotes: bool,
    ) -> Option<String> {
        self.arena.get(type_idx)?;

        let mut scratch = if let (Some(type_cache), Some(type_interner), Some(binder)) =
            (&self.type_cache, self.type_interner, self.binder)
        {
            DeclarationEmitter::with_type_info(
                self.arena,
                type_cache.clone(),
                type_interner,
                binder,
            )
        } else {
            DeclarationEmitter::new(self.arena)
        };

        scratch.source_is_declaration_file = self.source_is_declaration_file;
        scratch.source_is_js_file = self.source_is_js_file;
        scratch.current_source_file_idx = self.current_source_file_idx;
        if preserve_source_quotes {
            scratch.source_file_text = self.source_file_text.clone();
        }
        scratch.current_file_path = self.current_file_path.clone();
        scratch.current_arena = self.current_arena.clone();
        scratch.arena_to_path = self.arena_to_path.clone();
        scratch.emit_type(type_idx);
        Some(scratch.writer.take_output())
    }

    pub(in crate::declaration_emitter) fn expression_is_always_truthy_for_decl_emit(
        &self,
        expr_idx: NodeIndex,
    ) -> bool {
        let Some(expr_idx) = self.skip_parenthesized_expression(expr_idx) else {
            return false;
        };
        let Some(expr_node) = self.arena.get(expr_idx) else {
            return false;
        };

        match expr_node.kind {
            k if k == syntax_kind_ext::NEW_EXPRESSION
                || k == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION
                || k == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
                || k == syntax_kind_ext::FUNCTION_EXPRESSION
                || k == syntax_kind_ext::ARROW_FUNCTION
                || k == syntax_kind_ext::CLASS_EXPRESSION =>
            {
                true
            }
            k if k == syntax_kind_ext::BINARY_EXPRESSION => {
                self.arena.get_binary_expr(expr_node).is_some_and(|binary| {
                    binary.operator_token == SyntaxKind::BarBarToken as u16
                        && self.expression_is_always_truthy_for_decl_emit(binary.left)
                })
            }
            _ => false,
        }
    }

    pub(in crate::declaration_emitter) fn function_body_preferred_return_type_text(
        &self,
        body_idx: NodeIndex,
    ) -> Option<String> {
        let body_node = self.arena.get(body_idx)?;
        let block = self.arena.get_block(body_node)?;
        if let Some(type_text) =
            self.function_body_numeric_literal_return_union_type_text(&block.statements)
        {
            return Some(type_text);
        }
        let mut preferred = None;
        if self.collect_unique_return_type_text_from_block(&block.statements, &mut preferred) {
            preferred
        } else {
            None
        }
    }

    pub(in crate::declaration_emitter) fn should_prefer_source_return_type_text(
        &self,
        source_type_text: &str,
        inferred_return_type: tsz_solver::types::TypeId,
    ) -> bool {
        if Self::numeric_literal_union_widens_to_number(
            source_type_text,
            &self.print_type_id(inferred_return_type),
        ) {
            return true;
        }
        if source_type_text.contains("{\n    new ")
            && source_type_text.contains(" & ")
            && self.print_type_id(inferred_return_type) != source_type_text
        {
            return true;
        }
        if Self::type_text_starts_with_import_type(source_type_text)
            && self.print_type_id(inferred_return_type) != source_type_text
        {
            return true;
        }
        if !source_type_text.contains("typeof ") {
            return Self::type_text_contains_mapped_type_literal(source_type_text)
                && self.print_type_id(inferred_return_type) != source_type_text;
        }
        !self.print_type_id(inferred_return_type).contains("typeof ")
    }

    pub(in crate::declaration_emitter) fn source_return_type_is_function_type_param(
        &self,
        func: &tsz_parser::parser::node::FunctionData,
        source_type_text: &str,
    ) -> bool {
        let Some(ref type_params) = func.type_parameters else {
            return false;
        };
        let Some(name) = Self::simple_type_reference_name(source_type_text) else {
            return false;
        };
        self.collect_type_param_names(type_params)
            .iter()
            .any(|type_param| type_param == &name)
    }

    pub(in crate::declaration_emitter) fn function_return_type_text_for_declaration_scope(
        &self,
        func: &tsz_parser::parser::node::FunctionData,
        source_type_text: &str,
    ) -> (String, bool) {
        let (text, substituted_parameter_type_query) =
            self.substitute_function_parameter_type_queries(func, source_type_text);
        let text = self.rewrite_returned_auto_accessor_parameter_unknowns(func, &text);
        let Some(ref type_params) = func.type_parameters else {
            return (text, substituted_parameter_type_query);
        };
        if type_params.nodes.is_empty() {
            return (text, substituted_parameter_type_query);
        }

        let outer_names = self.collect_type_param_names(type_params);
        let text = Self::rename_shadowed_type_params_in_text(&text, &outer_names);
        (
            Self::rename_shadowed_infer_type_params_in_text(&text, &outer_names),
            substituted_parameter_type_query,
        )
    }

    pub(in crate::declaration_emitter) fn inferred_function_return_type_text(
        &self,
        func: &tsz_parser::parser::node::FunctionData,
        return_type_id: tsz_solver::types::TypeId,
    ) -> String {
        let text = if let Some(ref type_params) = func.type_parameters
            && !type_params.nodes.is_empty()
        {
            self.print_type_id_with_outer_type_params(return_type_id, type_params)
        } else {
            self.print_type_id(return_type_id)
        };
        self.rewrite_returned_auto_accessor_parameter_unknowns(func, &text)
    }

    pub(in crate::declaration_emitter) fn rewrite_returned_auto_accessor_parameter_unknowns(
        &self,
        func: &tsz_parser::parser::node::FunctionData,
        source_type_text: &str,
    ) -> String {
        let source_type_text = self
            .simplify_uniform_object_keyof_index_access_text(source_type_text)
            .unwrap_or_else(|| source_type_text.to_string());
        if !source_type_text.contains(": unknown;") {
            return source_type_text;
        }

        let Some(class_expr_idx) = self.direct_returned_class_expression(func.body) else {
            return source_type_text;
        };
        let Some(class_node) = self.arena.get(class_expr_idx) else {
            return source_type_text;
        };
        let Some(class) = self.arena.get_class(class_node) else {
            return source_type_text;
        };

        let mut rewritten = source_type_text;
        for member_idx in class.members.nodes.iter().copied() {
            let Some(member_node) = self.arena.get(member_idx) else {
                continue;
            };
            let Some(prop) = self.arena.get_property_decl(member_node) else {
                continue;
            };
            if !self
                .arena
                .has_modifier(&prop.modifiers, SyntaxKind::AccessorKeyword)
            {
                continue;
            }
            if !prop.initializer.is_some() {
                continue;
            }
            let Some(name_text) = self.get_identifier_text(prop.name) else {
                continue;
            };
            let Some(type_text) = self.function_parameter_type_text(func, prop.initializer) else {
                continue;
            };
            if type_text == "unknown" {
                continue;
            }

            let get_unknown = format!("get {name_text}(): unknown;");
            let get_replacement = format!("get {name_text}(): {type_text};");
            rewritten = rewritten.replace(&get_unknown, &get_replacement);

            let set_unknown = format!("set {name_text}(arg: unknown);");
            let set_replacement = format!("set {name_text}(arg: {type_text});");
            rewritten = rewritten.replace(&set_unknown, &set_replacement);
        }

        rewritten
    }

    fn direct_returned_class_expression(&self, body_idx: NodeIndex) -> Option<NodeIndex> {
        let body_node = self.arena.get(body_idx)?;
        let block = self.arena.get_block(body_node)?;
        let mut returned_class = None;
        for stmt_idx in block.statements.nodes.iter().copied() {
            let Some(stmt_node) = self.arena.get(stmt_idx) else {
                continue;
            };
            if stmt_node.kind != syntax_kind_ext::RETURN_STATEMENT {
                continue;
            }
            let ret = self.arena.get_return_statement(stmt_node)?;
            if !ret.expression.is_some() {
                return None;
            }
            let expr_idx = self.skip_parenthesized_expression(ret.expression)?;
            let expr_node = self.arena.get(expr_idx)?;
            if expr_node.kind != syntax_kind_ext::CLASS_EXPRESSION {
                return None;
            }
            if returned_class.replace(expr_idx).is_some() {
                return None;
            }
        }
        returned_class
    }

    fn substitute_function_parameter_type_queries(
        &self,
        func: &tsz_parser::parser::node::FunctionData,
        source_type_text: &str,
    ) -> (String, bool) {
        if !source_type_text.contains("typeof ")
            || !source_type_text.contains(" extends ")
            || !source_type_text.contains('?')
        {
            return (source_type_text.to_string(), false);
        }

        let mut text = source_type_text.to_string();
        let mut replaced_any = false;
        for param_idx in func.parameters.nodes.iter().copied() {
            let Some(param_node) = self.arena.get(param_idx) else {
                continue;
            };
            let Some(param) = self.arena.get_parameter(param_node) else {
                continue;
            };
            let Some(param_name) = self.get_identifier_text(param.name) else {
                continue;
            };
            let Some(param_type_text) = self.function_parameter_type_text(func, param.name) else {
                continue;
            };
            if Self::simple_type_reference_name(&param_type_text).is_none() {
                continue;
            }
            let (replaced_text, replaced) =
                Self::replace_typeof_identifier(&text, &param_name, &param_type_text);
            text = replaced_text;
            replaced_any |= replaced;
        }
        (text, replaced_any)
    }

    fn replace_typeof_identifier(
        text: &str,
        identifier: &str,
        replacement: &str,
    ) -> (String, bool) {
        let query = format!("typeof {identifier}");
        let bytes = text.as_bytes();
        let query_bytes = query.as_bytes();
        let mut result = String::with_capacity(text.len());
        let mut replaced = false;
        let mut i = 0usize;
        while i < bytes.len() {
            if i + query_bytes.len() <= bytes.len()
                && &bytes[i..i + query_bytes.len()] == query_bytes
                && (i == 0 || !Self::is_ident_char(bytes[i - 1]))
            {
                let after = i + query_bytes.len();
                let after_ok = after == bytes.len()
                    || (!Self::is_ident_char(bytes[after])
                        && bytes[after] != b'.'
                        && bytes[after] != b'<');
                if after_ok {
                    result.push_str(replacement);
                    i = after;
                    replaced = true;
                    continue;
                }
            }
            result.push(bytes[i] as char);
            i += 1;
        }
        (result, replaced)
    }

    pub(in crate::declaration_emitter) fn collect_unique_return_type_text_from_block(
        &self,
        statements: &NodeList,
        preferred: &mut Option<String>,
    ) -> bool {
        statements.nodes.iter().copied().all(|stmt_idx| {
            self.collect_unique_return_type_text_from_statement(stmt_idx, preferred)
        })
    }

    pub(in crate::declaration_emitter) fn collect_unique_return_type_text_from_statement(
        &self,
        stmt_idx: NodeIndex,
        preferred: &mut Option<String>,
    ) -> bool {
        let Some(stmt_node) = self.arena.get(stmt_idx) else {
            return true;
        };

        match stmt_node.kind {
            k if k == syntax_kind_ext::RETURN_STATEMENT => {
                let Some(ret) = self.arena.get_return_statement(stmt_node) else {
                    return false;
                };
                let type_text = if !ret.expression.is_some() {
                    // `return;` with no expression contributes `void` to the
                    // function's return type — tsc's inference for a bare
                    // return is equivalent to `return undefined` with
                    // widening to `void`. Matches declFileTypeAnnotationBuiltInType.
                    "void".to_string()
                } else if let Some(text) = self
                    .preferred_expression_type_text(ret.expression)
                    .filter(|text| !text.is_empty() && text != "any")
                {
                    text
                } else if let Some(text) = self
                    .return_expression_identifier(ret.expression)
                    .and_then(|identifier_idx| {
                        self.reference_declared_type_annotation_text(identifier_idx)
                    })
                    .filter(|text| text == "any")
                {
                    text
                } else if let Some(text) = self
                    .local_variable_initializer_type_text(ret.expression)
                    .filter(|text| !text.is_empty())
                {
                    text
                } else if let Some(text) = self
                    .infer_fallback_type_text_at(ret.expression, 0)
                    .filter(|text| !text.is_empty())
                {
                    text
                } else {
                    return false;
                };
                if let Some(existing) = preferred.as_ref() {
                    existing == &type_text
                } else {
                    *preferred = Some(type_text);
                    true
                }
            }
            k if k == syntax_kind_ext::BLOCK => {
                self.arena.get_block(stmt_node).is_some_and(|block| {
                    self.collect_unique_return_type_text_from_block(&block.statements, preferred)
                })
            }
            k if k == syntax_kind_ext::IF_STATEMENT => self
                .arena
                .get_if_statement(stmt_node)
                .is_some_and(|if_data| {
                    if if_data.else_statement.is_none() {
                        let mut ignored = preferred.clone();
                        return self.collect_unique_return_type_text_from_statement(
                            if_data.then_statement,
                            &mut ignored,
                        );
                    }
                    self.collect_unique_return_type_text_from_statement(
                        if_data.then_statement,
                        preferred,
                    ) && self.collect_unique_return_type_text_from_statement(
                        if_data.else_statement,
                        preferred,
                    )
                }),
            k if k == syntax_kind_ext::TRY_STATEMENT => {
                self.arena.get_try(stmt_node).is_some_and(|try_data| {
                    self.collect_unique_return_type_text_from_statement(
                        try_data.try_block,
                        preferred,
                    ) && try_data.catch_clause.is_some()
                        && self.collect_unique_return_type_text_from_statement(
                            try_data.catch_clause,
                            preferred,
                        )
                        && try_data.finally_block.is_some()
                        && self.collect_unique_return_type_text_from_statement(
                            try_data.finally_block,
                            preferred,
                        )
                })
            }
            k if k == syntax_kind_ext::CATCH_CLAUSE => self
                .arena
                .get_catch_clause(stmt_node)
                .is_some_and(|catch_data| {
                    self.collect_unique_return_type_text_from_statement(catch_data.block, preferred)
                }),
            k if k == syntax_kind_ext::CASE_CLAUSE || k == syntax_kind_ext::DEFAULT_CLAUSE => {
                self.arena.get_case_clause(stmt_node).is_some_and(|clause| {
                    self.collect_unique_return_type_text_from_block(&clause.statements, preferred)
                })
            }
            k if k == syntax_kind_ext::SWITCH_STATEMENT => {
                self.arena.get_switch(stmt_node).is_some_and(|switch_data| {
                    self.arena
                        .get(switch_data.case_block)
                        .and_then(|case_block_node| self.arena.get_block(case_block_node))
                        .is_some_and(|block| {
                            self.collect_unique_return_type_text_from_block(
                                &block.statements,
                                preferred,
                            )
                        })
                })
            }
            _ => true,
        }
    }

    pub(in crate::declaration_emitter) fn local_variable_initializer_type_text(
        &self,
        expr_idx: NodeIndex,
    ) -> Option<String> {
        let expr_node = self.arena.get(expr_idx)?;
        if expr_node.kind != SyntaxKind::Identifier as u16 {
            return None;
        }
        let sym_id = self.value_reference_symbol(expr_idx)?;
        let binder = self.binder?;
        let symbol = binder.symbols.get(sym_id)?;
        for decl_idx in symbol.declarations.iter().copied() {
            let decl_node = self.arena.get(decl_idx)?;
            let Some(var_decl) = self.arena.get_variable_declaration(decl_node) else {
                continue;
            };
            if !var_decl.initializer.is_some() {
                continue;
            }
            if let Some(type_text) = self
                .preferred_expression_type_text(var_decl.initializer)
                .filter(|text| text != "any" && text != "unknown" && !text.contains("any"))
                .or_else(|| self.as_const_assertion_type_text(var_decl.initializer))
                .or_else(|| self.infer_fallback_type_text_at(var_decl.initializer, 0))
            {
                return Some(type_text);
            }
        }
        None
    }

    fn function_expression_type_text_from_ast(&self, expr_idx: NodeIndex) -> Option<String> {
        let expr_node = self.arena.get(expr_idx)?;
        if expr_node.kind != syntax_kind_ext::ARROW_FUNCTION
            && expr_node.kind != syntax_kind_ext::FUNCTION_EXPRESSION
        {
            return None;
        }
        let func = self.arena.get_function(expr_node)?;

        let mut scratch = if let (Some(type_cache), Some(type_interner), Some(binder)) =
            (&self.type_cache, self.type_interner, self.binder)
        {
            DeclarationEmitter::with_type_info(
                self.arena,
                type_cache.clone(),
                type_interner,
                binder,
            )
        } else {
            DeclarationEmitter::new(self.arena)
        };
        scratch.source_is_declaration_file = self.source_is_declaration_file;
        scratch.source_is_js_file = self.source_is_js_file;
        scratch.current_source_file_idx = self.current_source_file_idx;
        scratch.source_file_text = self.source_file_text.clone();
        scratch.current_file_path = self.current_file_path.clone();
        scratch.current_arena = self.current_arena.clone();
        scratch.arena_to_path = self.arena_to_path.clone();
        scratch.indent_level = self.indent_level;
        scratch.normalize_string_literal_type_quotes = true;

        if let Some(ref type_params) = func.type_parameters
            && !type_params.nodes.is_empty()
        {
            scratch.emit_type_parameters(type_params);
        }
        scratch.write("(");
        scratch.emit_parameters_with_body(&func.parameters, func.body);
        scratch.write(") => ");
        if func.type_annotation.is_some() {
            scratch.emit_type(func.type_annotation);
        } else if func.body.is_some() && scratch.body_returns_void(func.body) {
            scratch.write("void");
        } else if let Some(return_type) = scratch.expression_body_parameter_return_type_text(func) {
            scratch.write(&return_type);
        } else if func.body.is_some()
            && let Some(return_type) = scratch
                .preferred_expression_type_text(func.body)
                .or_else(|| scratch.infer_fallback_type_text_at(func.body, 0))
                .filter(|text| !text.is_empty() && text != "any")
        {
            scratch.write(&return_type);
        } else if let Some(return_type) =
            scratch.function_body_preferred_return_type_text(func.body)
        {
            scratch.write(&return_type);
        } else {
            scratch.write("any");
        }
        Some(scratch.writer.take_output())
    }

    fn expression_body_parameter_return_type_text(
        &self,
        func: &tsz_parser::parser::node::FunctionData,
    ) -> Option<String> {
        let body_node = self.arena.get(func.body)?;
        if body_node.kind == syntax_kind_ext::BLOCK {
            return None;
        }
        let body_name = self.get_identifier_text(func.body)?;
        for &param_idx in &func.parameters.nodes {
            let param_node = self.arena.get(param_idx)?;
            let param = self.arena.get_parameter(param_node)?;
            if self.get_identifier_text(param.name).as_deref() != Some(body_name.as_str()) {
                continue;
            }
            if param.type_annotation.is_some() {
                return self.emit_type_node_text_normalized(param.type_annotation);
            }
            return self
                .get_node_type(param.name)
                .map(|type_id| self.print_type_id(type_id));
        }
        None
    }

    pub(in crate::declaration_emitter) fn infer_object_literal_type_text_at(
        &self,
        object_expr_idx: NodeIndex,
        depth: u32,
    ) -> Option<String> {
        let object_node = self.arena.get(object_expr_idx)?;
        let object = self.arena.get_literal_expr(object_node)?;

        // Pre-scan: collect setter and getter names for accessor pair handling
        let mut setter_names = rustc_hash::FxHashSet::<String>::default();
        let mut getter_names = rustc_hash::FxHashSet::<String>::default();
        for &idx in &object.elements.nodes {
            if let Some(n) = self.arena.get(idx) {
                if n.kind == syntax_kind_ext::SET_ACCESSOR {
                    if let Some(acc) = self.arena.get_accessor(n)
                        && let Some(name) = self.object_literal_member_name_text(acc.name)
                    {
                        setter_names.insert(name);
                    }
                } else if n.kind == syntax_kind_ext::GET_ACCESSOR
                    && let Some(acc) = self.arena.get_accessor(n)
                    && let Some(name) = self.object_literal_member_name_text(acc.name)
                {
                    getter_names.insert(name);
                }
            }
        }

        let mut members = Vec::new();
        for &member_idx in &object.elements.nodes {
            let Some(member_node) = self.arena.get(member_idx) else {
                continue;
            };
            let Some(name_idx) = self.object_literal_member_name_idx(member_node) else {
                continue;
            };
            let Some(name) = self.object_literal_member_name_text(name_idx) else {
                continue;
            };
            if name.is_empty() || name == ":" {
                continue;
            }

            if let Some(member_text) = self.infer_object_member_type_text_named_at(
                member_idx,
                &name,
                depth + 1,
                getter_names.contains(&name),
                setter_names.contains(&name),
            ) {
                if member_text.trim_start().starts_with(':') {
                    continue;
                }
                if !self.remove_comments {
                    for jsdoc in self.leading_jsdoc_comment_chain_for_pos(member_node.pos) {
                        members.push(Self::format_object_member_jsdoc_text(&jsdoc));
                    }
                }
                members.push(member_text);
            }
        }

        if members.is_empty() {
            Some("{}".to_string())
        } else {
            // Format as multi-line to match tsc's .d.ts output
            let member_indent = "    ".repeat((depth + 1) as usize);
            let closing_indent = "    ".repeat(depth as usize);
            let formatted_members: Vec<String> = members
                .iter()
                .map(|m| Self::format_object_member_entry(&member_indent, m))
                .collect();
            Some(format!(
                "{{\n{}\n{closing_indent}}}",
                formatted_members.join("\n")
            ))
        }
    }

    pub(in crate::declaration_emitter) fn object_literal_value_typeof_type_text(
        &self,
        object_expr_idx: NodeIndex,
        depth: u32,
    ) -> Option<String> {
        let object_node = self.arena.get(object_expr_idx)?;
        let object = self.arena.get_literal_expr(object_node)?;
        let mut saw_typeof = false;
        let mut members = Vec::new();

        for &member_idx in &object.elements.nodes {
            let Some(member_node) = self.arena.get(member_idx) else {
                continue;
            };
            let name_idx = self.object_literal_member_name_idx(member_node)?;
            let name_text = self.object_literal_member_name_text(name_idx)?;
            if name_text.is_empty() || name_text == ":" {
                return None;
            }

            let value_idx = if let Some(data) = self.arena.get_shorthand_property(member_node) {
                data.name
            } else if let Some(data) = self.arena.get_property_assignment(member_node) {
                data.initializer
            } else {
                return None;
            };

            let type_text = self
                .direct_value_reference_typeof_text(value_idx)
                .or_else(|| {
                    self.preferred_object_member_initializer_type_text(value_idx, depth + 1)
                })?;
            saw_typeof |= type_text.contains("typeof ");
            members.push(Self::format_object_member_type_text(
                &name_text, &type_text, depth,
            ));
        }

        if !saw_typeof || members.is_empty() {
            return None;
        }

        let member_indent = "    ".repeat((depth + 1) as usize);
        let closing_indent = "    ".repeat(depth as usize);
        let formatted_members: Vec<String> = members
            .iter()
            .map(|member| Self::format_object_member_entry(&member_indent, member))
            .collect();
        Some(format!(
            "{{\n{}\n{closing_indent}}}",
            formatted_members.join("\n")
        ))
    }

    pub(in crate::declaration_emitter) fn infer_object_member_type_text_named_at(
        &self,
        member_idx: NodeIndex,
        name: &str,
        depth: u32,
        getter_exists: bool,
        setter_exists: bool,
    ) -> Option<String> {
        let member_node = self.arena.get(member_idx)?;

        match member_node.kind {
            k if k == syntax_kind_ext::PROPERTY_ASSIGNMENT => {
                let data = self.arena.get_property_assignment(member_node)?;
                let type_text = self
                    .preferred_object_member_initializer_type_text(data.initializer, depth)
                    .unwrap_or_else(|| "any".to_string());
                Some(Self::format_object_member_type_text(
                    name, &type_text, depth,
                ))
            }
            k if k == syntax_kind_ext::SHORTHAND_PROPERTY_ASSIGNMENT => {
                let data = self.arena.get_shorthand_property(member_node)?;
                // For `{ foo }` the value reference is the name identifier itself.
                // For `{ foo = expr }` (CoverInitializedName) the assignment
                // initializer holds the default value.
                let initializer = if data.object_assignment_initializer == NodeIndex::NONE {
                    data.name
                } else {
                    data.object_assignment_initializer
                };
                let type_text = self
                    .preferred_object_member_initializer_type_text(initializer, depth)
                    .unwrap_or_else(|| "any".to_string());
                Some(Self::format_object_member_type_text(
                    name, &type_text, depth,
                ))
            }
            k if k == syntax_kind_ext::GET_ACCESSOR => {
                let data = self.arena.get_accessor(member_node)?;
                // Infer return type: explicit annotation > body inference > any
                let type_text = self
                    .infer_fallback_type_text_at(data.type_annotation, depth)
                    .or_else(|| self.function_body_preferred_return_type_text(data.body))
                    .unwrap_or_else(|| "any".to_string());
                let readonly = if setter_exists { "" } else { "readonly " };
                Some(format!("{readonly}{name}: {type_text}"))
            }
            k if k == syntax_kind_ext::SET_ACCESSOR => {
                if getter_exists {
                    return None;
                }

                let data = self.arena.get_accessor(member_node)?;
                let type_text = data
                    .parameters
                    .nodes
                    .first()
                    .and_then(|&p_idx| self.arena.get(p_idx))
                    .and_then(|p_node| self.arena.get_parameter(p_node))
                    .and_then(|param| {
                        self.infer_fallback_type_text_at(param.type_annotation, depth)
                    })
                    .unwrap_or_else(|| "any".to_string());
                Some(format!("{name}: {type_text}"))
            }
            k if k == syntax_kind_ext::METHOD_DECLARATION => {
                let data = self.arena.get_method_decl(member_node)?;
                if self.object_literal_method_uses_property_syntax(data) {
                    self.method_function_type_text(member_idx, data, depth)
                        .map(|type_text| format!("{name}: {type_text}"))
                } else {
                    self.method_signature_type_text_named_at(member_idx, data, name, depth)
                }
            }
            _ => None,
        }
    }

    fn method_signature_type_text_named_at(
        &self,
        method_idx: NodeIndex,
        method: &tsz_parser::parser::node::MethodDeclData,
        name: &str,
        depth: u32,
    ) -> Option<String> {
        let mut scratch = self.scratch_declaration_emitter();
        scratch.indent_level = depth;
        scratch.write(name);
        if method.question_token {
            scratch.write("?");
        }

        let jsdoc_template_params = if method
            .type_parameters
            .as_ref()
            .is_none_or(|type_params| type_params.nodes.is_empty())
        {
            self.jsdoc_template_params_for_node(method_idx)
        } else {
            Vec::new()
        };
        if let Some(ref type_params) = method.type_parameters {
            if !type_params.nodes.is_empty() {
                scratch.emit_type_parameters(type_params);
            } else if !jsdoc_template_params.is_empty() {
                scratch.emit_jsdoc_template_parameters(&jsdoc_template_params);
            }
        } else if !jsdoc_template_params.is_empty() {
            scratch.emit_jsdoc_template_parameters(&jsdoc_template_params);
        }

        scratch.write("(");
        scratch.emit_parameters_with_body(&method.parameters, method.body);
        scratch.write("): ");
        scratch.emit_method_function_type_return(method_idx, method);
        let type_text = scratch.writer.take_output();
        (!type_text.trim().is_empty()).then_some(type_text)
    }

    fn object_literal_method_uses_property_syntax(
        &self,
        method: &tsz_parser::parser::node::MethodDeclData,
    ) -> bool {
        let Some(name_node) = self.arena.get(method.name) else {
            return false;
        };
        if name_node.kind != syntax_kind_ext::COMPUTED_PROPERTY_NAME {
            return false;
        }
        if self
            .resolved_computed_property_name_text(method.name)
            .is_some()
            || self.computed_property_name_is_symbol_access(method.name)
            || self.computed_property_name_is_literal_key(method.name)
        {
            return false;
        }

        let computed_key_requires_property_syntax = self
            .arena
            .get_computed_property(name_node)
            .and_then(|cp| self.get_node_type_or_names(&[cp.expression, method.name]))
            .is_none_or(|type_id| {
                type_id == tsz_solver::types::TypeId::ANY
                    || self.type_interner.is_some_and(|interner| {
                        !tsz_solver::type_queries::is_type_usable_as_property_name(
                            interner, type_id,
                        )
                    })
            });

        method.question_token || computed_key_requires_property_syntax
    }

    fn computed_property_name_is_literal_key(&self, name_idx: NodeIndex) -> bool {
        let Some(name_node) = self.arena.get(name_idx) else {
            return false;
        };
        if name_node.kind != syntax_kind_ext::COMPUTED_PROPERTY_NAME {
            return false;
        }

        let Some(computed) = self.arena.get_computed_property(name_node) else {
            return false;
        };
        let expr_idx = self
            .arena
            .skip_parenthesized_and_assertions_and_comma(computed.expression);
        let Some(expr_node) = self.arena.get(expr_idx) else {
            return false;
        };

        expr_node.kind == SyntaxKind::StringLiteral as u16
            || expr_node.kind == SyntaxKind::NumericLiteral as u16
            || expr_node.kind == syntax_kind_ext::PREFIX_UNARY_EXPRESSION
    }

    fn computed_property_name_is_symbol_access(&self, name_idx: NodeIndex) -> bool {
        let Some(name_node) = self.arena.get(name_idx) else {
            return false;
        };
        if name_node.kind != syntax_kind_ext::COMPUTED_PROPERTY_NAME {
            return false;
        }

        let Some(computed) = self.arena.get_computed_property(name_node) else {
            return false;
        };
        let expr_idx = self.skip_parenthesized_non_null_and_comma(computed.expression);
        let Some(expr_node) = self.arena.get(expr_idx) else {
            return false;
        };
        if expr_node.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            return false;
        }

        let Some(access) = self.arena.get_access_expr(expr_node) else {
            return false;
        };
        self.get_identifier_text(access.expression).as_deref() == Some("Symbol")
    }

    fn format_object_member_type_text(name: &str, type_text: &str, depth: u32) -> String {
        if !type_text.contains('\n') {
            return format!("{name}: {type_text}");
        }

        let _ = depth;
        format!("{name}: {type_text}")
    }

    fn format_object_member_jsdoc_text(jsdoc: &str) -> String {
        let trimmed = jsdoc.trim();
        if !trimmed.contains('\n') {
            return format!("/** {trimmed} */");
        }

        let mut result = String::from("/**");
        for line in trimmed.lines() {
            result.push('\n');
            if line.trim().is_empty() {
                result.push_str(" *");
            } else {
                result.push_str(" * ");
                result.push_str(line.trim());
            }
        }
        result.push_str("\n */");
        result
    }

    pub(in crate::declaration_emitter) fn add_returned_object_member_comments_to_type_text(
        &self,
        initializer: NodeIndex,
        type_text: &str,
    ) -> String {
        if self.remove_comments || !type_text.contains("{\n") {
            return type_text.to_string();
        }
        let Some(object_idx) =
            self.function_initializer_unique_returned_object_literal(initializer)
        else {
            return type_text.to_string();
        };
        let Some(object_node) = self.arena.get(object_idx) else {
            return type_text.to_string();
        };
        let Some(object) = self.arena.get_literal_expr(object_node) else {
            return type_text.to_string();
        };

        let mut commented_members = Vec::new();
        for &member_idx in &object.elements.nodes {
            let Some(member_node) = self.arena.get(member_idx) else {
                continue;
            };
            let Some(name_idx) = self.object_literal_member_name_idx(member_node) else {
                continue;
            };
            let Some(name_text) = self.object_literal_member_name_text(name_idx) else {
                continue;
            };
            let comments = self.leading_jsdoc_comment_chain_for_pos(member_node.pos);
            if !comments.is_empty() {
                commented_members.push((name_text, comments));
            }
        }
        if commented_members.is_empty() {
            return type_text.to_string();
        }

        let mut lines: Vec<String> = type_text.lines().map(str::to_string).collect();
        for (name_text, comments) in commented_members.into_iter().rev() {
            let Some(line_idx) = lines.iter().position(|line| {
                let trimmed = line.trim_start();
                Self::object_literal_property_name_prefixes(&name_text)
                    .into_iter()
                    .any(|prefix| {
                        trimmed.starts_with(&prefix)
                            || trimmed.starts_with(&format!("readonly {prefix}"))
                    })
                    || trimmed.starts_with(&format!("{name_text}("))
            }) else {
                continue;
            };
            if line_idx > 0 && lines[line_idx - 1].trim_start().starts_with("/**") {
                continue;
            }
            let indent_len = lines[line_idx].len() - lines[line_idx].trim_start().len();
            let indent = " ".repeat(indent_len);
            let mut comment_lines = Vec::new();
            for jsdoc in comments {
                for line in Self::format_object_member_jsdoc_text(&jsdoc).lines() {
                    comment_lines.push(format!("{indent}{line}"));
                }
            }
            lines.splice(line_idx..line_idx, comment_lines);
        }

        if type_text.ends_with('\n') {
            format!("{}\n", lines.join("\n"))
        } else {
            lines.join("\n")
        }
    }

    fn function_initializer_unique_returned_object_literal(
        &self,
        initializer: NodeIndex,
    ) -> Option<NodeIndex> {
        let init_node = self.arena.get(initializer)?;
        if init_node.kind != syntax_kind_ext::ARROW_FUNCTION
            && init_node.kind != syntax_kind_ext::FUNCTION_EXPRESSION
        {
            return None;
        }
        let func = self.arena.get_function(init_node)?;
        let body_node = self.arena.get(func.body)?;
        if body_node.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION {
            return Some(func.body);
        }
        let block = self.arena.get_block(body_node)?;
        let mut returned = None;
        for &stmt_idx in &block.statements.nodes {
            let stmt_node = self.arena.get(stmt_idx)?;
            if stmt_node.kind != syntax_kind_ext::RETURN_STATEMENT {
                continue;
            }
            let ret = self.arena.get_return_statement(stmt_node)?;
            if !ret.expression.is_some() {
                continue;
            }
            let expr = self.skip_parenthesized_non_null_and_comma(ret.expression);
            let expr_node = self.arena.get(expr)?;
            if expr_node.kind != syntax_kind_ext::OBJECT_LITERAL_EXPRESSION {
                return None;
            }
            if returned.replace(expr).is_some() {
                return None;
            }
        }
        returned
    }

    fn format_object_member_entry(member_indent: &str, member_text: &str) -> String {
        let mut lines = member_text.lines();
        let first = lines.next().unwrap_or(member_text);
        let mut result = String::new();
        result.push_str(member_indent);
        result.push_str(first);
        for line in lines {
            result.push('\n');
            result.push_str(line);
        }
        if !result.trim_start().starts_with("/**") && !result.trim_end().ends_with(';') {
            result.push(';');
        }
        result
    }

    pub(in crate::declaration_emitter) fn object_literal_member_name_text(
        &self,
        name_idx: NodeIndex,
    ) -> Option<String> {
        self.resolved_computed_property_name_text(name_idx)
            .or_else(|| self.infer_property_name_text(name_idx))
    }

    pub(in crate::declaration_emitter) fn resolved_computed_property_name_text(
        &self,
        name_idx: NodeIndex,
    ) -> Option<String> {
        let name_node = self.arena.get(name_idx)?;
        if name_node.kind != syntax_kind_ext::COMPUTED_PROPERTY_NAME {
            return None;
        }

        let computed = self.arena.get_computed_property(name_node)?;
        let expr_idx = self
            .arena
            .skip_parenthesized_and_assertions_and_comma(computed.expression);
        let expr_node = self.arena.get(expr_idx)?;

        match expr_node.kind {
            k if k == SyntaxKind::Identifier as u16
                || k == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION =>
            {
                if let Some(interner) = self.type_interner
                    && let Some(type_id) = self.get_node_type_or_names(&[expr_idx])
                    && let Some(literal) = tsz_solver::visitor::literal_value(interner, type_id)
                {
                    return Some(Self::format_property_name_literal_value(&literal, interner));
                }
                // Fallback: an enum member access (e.g. `[E.A]`) is a valid
                // property-name source even when the type cache hasn't
                // produced a `Literal` form for it. Detecting it via the
                // binder lets the caller keep method/getter syntax instead
                // of degrading to `[E.A]: () => T`.
                self.enum_member_access_name_text(expr_idx)
            }
            _ => None,
        }
    }

    /// If `expr_idx` is a value reference whose symbol is an enum member,
    /// return the member's escaped name. This is used as a fallback to keep
    /// method-like dts syntax for `[E.A]() {}` even when the type system
    /// hasn't produced a literal type for the access expression.
    pub(in crate::declaration_emitter) fn enum_member_access_name_text(
        &self,
        expr_idx: NodeIndex,
    ) -> Option<String> {
        let binder = self.binder?;
        let sym_id = self.value_reference_symbol(expr_idx)?;
        let symbol = binder.symbols.get(sym_id)?;
        if symbol.flags & tsz_binder::symbol_flags::ENUM_MEMBER == 0 {
            return None;
        }
        Some(symbol.escaped_name.clone())
    }

    pub(in crate::declaration_emitter) fn format_property_name_literal_value(
        literal: &tsz_solver::types::LiteralValue,
        interner: &tsz_solver::TypeInterner,
    ) -> String {
        match literal {
            tsz_solver::types::LiteralValue::String(atom) => {
                Self::format_property_name_literal_text(&interner.resolve_atom(*atom))
            }
            tsz_solver::types::LiteralValue::Number(n) => Self::format_js_number(n.0),
            tsz_solver::types::LiteralValue::Boolean(b) => b.to_string(),
            tsz_solver::types::LiteralValue::BigInt(atom) => {
                format!("{}n", interner.resolve_atom(*atom))
            }
        }
    }

    pub(in crate::declaration_emitter) fn format_property_name_literal_text(text: &str) -> String {
        if Self::is_unquoted_property_name(text) {
            text.to_string()
        } else {
            format!("\"{}\"", super::escape_string_for_double_quote(text))
        }
    }

    pub(in crate::declaration_emitter) fn is_unquoted_property_name(text: &str) -> bool {
        let mut chars = text.chars();
        let Some(first) = chars.next() else {
            return false;
        };

        if !(first == '_' || first == '$' || first.is_ascii_alphabetic()) {
            return false;
        }

        chars.all(|ch| ch == '_' || ch == '$' || ch.is_ascii_alphanumeric())
    }

    pub(in crate::declaration_emitter) fn preferred_object_member_initializer_type_text(
        &self,
        initializer: NodeIndex,
        depth: u32,
    ) -> Option<String> {
        let type_id = self.get_node_type_or_names(&[initializer]);
        if let Some(typeof_text) = self.typeof_prefix_for_value_entity(initializer, true, type_id) {
            return Some(typeof_text);
        }
        if let Some(enum_type_text) = self.enum_member_widened_type_text(initializer) {
            return Some(enum_type_text);
        }
        self.preferred_expression_type_text(initializer)
            .or_else(|| self.infer_fallback_type_text_at(initializer, depth))
    }

    pub(in crate::declaration_emitter) fn enum_member_widened_type_text(
        &self,
        expr_idx: NodeIndex,
    ) -> Option<String> {
        let expr_idx = self.skip_parenthesized_non_null_and_comma(expr_idx);
        let expr_node = self.arena.get(expr_idx)?;
        let access = self.arena.get_access_expr(expr_node)?;
        let binder = self.binder?;

        let member_sym_id = self.value_reference_symbol(expr_idx)?;
        let member_symbol = binder.symbols.get(member_sym_id)?;
        if !member_symbol.has_any_flags(symbol_flags::ENUM_MEMBER) {
            return None;
        }

        let enum_expr = self.skip_parenthesized_non_null_and_comma(access.expression);
        let enum_sym_id = self.value_reference_symbol(enum_expr)?;
        let enum_symbol = binder.symbols.get(enum_sym_id)?;
        if !enum_symbol.has_any_flags(symbol_flags::ENUM)
            || enum_symbol.has_any_flags(symbol_flags::ENUM_MEMBER)
        {
            return None;
        }

        self.nameable_constructor_expression_text(enum_expr)
    }

    pub(in crate::declaration_emitter) fn infer_property_name_text(
        &self,
        node_id: NodeIndex,
    ) -> Option<String> {
        let node = self.arena.get(node_id)?;
        if node.kind == syntax_kind_ext::COMPUTED_PROPERTY_NAME {
            let computed = self.arena.get_computed_property(node)?;
            let expr_idx = self
                .arena
                .skip_parenthesized_and_assertions_and_comma(computed.expression);
            let expr_node = self.arena.get(expr_idx)?;
            match expr_node.kind {
                k if k == SyntaxKind::StringLiteral as u16 => {
                    let literal = self.arena.get_literal(expr_node)?;
                    let quote = self.original_quote_char(expr_node);
                    return Some(format!("{}{}{}", quote, literal.text, quote));
                }
                k if k == SyntaxKind::NumericLiteral as u16 => {
                    let literal = self.arena.get_literal(expr_node)?;
                    return Some(Self::normalize_numeric_literal(literal.text.as_ref()));
                }
                k if k == syntax_kind_ext::PREFIX_UNARY_EXPRESSION => {
                    let unary = self.arena.get_unary_expr(expr_node)?;
                    let operand_idx = self
                        .arena
                        .skip_parenthesized_and_assertions_and_comma(unary.operand);
                    let operand_node = self.arena.get(operand_idx)?;
                    if operand_node.kind != SyntaxKind::NumericLiteral as u16 {
                        return None;
                    }
                    let literal = self.arena.get_literal(operand_node)?;
                    let normalized = Self::normalize_numeric_literal(literal.text.as_ref());
                    return match unary.operator {
                        k if k == SyntaxKind::MinusToken as u16 => Some(format!("[-{normalized}]")),
                        k if k == SyntaxKind::PlusToken as u16 => Some(normalized),
                        _ => None,
                    };
                }
                k if k == SyntaxKind::Identifier as u16
                    || k == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION =>
                {
                    // Use the COMPUTED_PROPERTY_NAME node's source slice.
                    // The node.end may extend past `]` into trailing `:`
                    // (property colon) or `(` (getter/method params), so
                    // trim to the closing `]` to avoid `::` or `(` leaking.
                    if let Some(mut s) = self.get_source_slice(node.pos, node.end) {
                        // Find the last `]` and truncate after it
                        if let Some(bracket_pos) = s.rfind(']') {
                            s.truncate(bracket_pos + 1);
                        } else {
                            // No brackets — trim trailing punctuation
                            while s.ends_with(':') || s.ends_with('(') {
                                s.pop();
                                s = s.trim_end().to_string();
                            }
                        }
                        if !s.is_empty() {
                            return Some(s);
                        }
                    }
                    return None;
                }
                _ => return None,
            }
        }
        if let Some(ident) = self.arena.get_identifier(node) {
            return Some(ident.escaped_text.clone());
        }
        if let Some(literal) = self.arena.get_literal(node) {
            let quote = self.original_quote_char(node);
            return Some(format!("{}{}{}", quote, literal.text, quote));
        }
        self.get_source_slice(node.pos, node.end)
    }

    pub(in crate::declaration_emitter) fn skip_parenthesized_non_null_and_comma(
        &self,
        mut idx: NodeIndex,
    ) -> NodeIndex {
        for _ in 0..100 {
            let Some(node) = self.arena.get(idx) else {
                return idx;
            };
            if node.kind == syntax_kind_ext::PARENTHESIZED_EXPRESSION
                && let Some(paren) = self.arena.get_parenthesized(node)
            {
                idx = paren.expression;
                continue;
            }
            if node.kind == syntax_kind_ext::NON_NULL_EXPRESSION
                && let Some(unary) = self.arena.get_unary_expr_ex(node)
            {
                idx = unary.expression;
                continue;
            }
            if node.kind == syntax_kind_ext::BINARY_EXPRESSION
                && let Some(binary) = self.arena.get_binary_expr(node)
                && binary.operator_token == SyntaxKind::CommaToken as u16
            {
                idx = binary.right;
                continue;
            }
            return idx;
        }
        idx
    }

    pub(in crate::declaration_emitter) fn object_literal_prefers_syntax_type_text(
        &self,
        initializer: NodeIndex,
    ) -> bool {
        let Some(init_node) = self.arena.get(initializer) else {
            return false;
        };
        if init_node.kind != syntax_kind_ext::OBJECT_LITERAL_EXPRESSION {
            return false;
        }
        let Some(object) = self.arena.get_literal_expr(init_node) else {
            return false;
        };

        object
            .elements
            .nodes
            .iter()
            .copied()
            .any(|member_idx| self.object_literal_member_needs_syntax_override(member_idx))
    }

    pub(in crate::declaration_emitter) fn rewrite_object_literal_computed_member_type_text(
        &self,
        initializer: NodeIndex,
        type_id: tsz_solver::types::TypeId,
    ) -> Option<String> {
        let init_node = self.arena.get(initializer)?;
        if init_node.kind != syntax_kind_ext::OBJECT_LITERAL_EXPRESSION {
            return None;
        }
        let object = self.arena.get_literal_expr(init_node)?;

        let mut setter_names = rustc_hash::FxHashSet::<String>::default();
        let mut getter_names = rustc_hash::FxHashSet::<String>::default();
        for &idx in &object.elements.nodes {
            if let Some(n) = self.arena.get(idx) {
                if n.kind == syntax_kind_ext::SET_ACCESSOR {
                    if let Some(acc) = self.arena.get_accessor(n)
                        && let Some(name) = self.object_literal_member_name_text(acc.name)
                    {
                        setter_names.insert(name);
                    }
                } else if n.kind == syntax_kind_ext::GET_ACCESSOR
                    && let Some(acc) = self.arena.get_accessor(n)
                    && let Some(name) = self.object_literal_member_name_text(acc.name)
                {
                    getter_names.insert(name);
                }
            }
        }

        let mut computed_members = Vec::new();
        let mut overridden_members = Vec::new();
        let mut concrete_member_names = Vec::new();
        let mut only_numeric_like = true;
        let mut has_non_emittable_computed_members = false;
        let mut synthetic_number_index_member = None;
        let mut negative_numeric_computed_names = Vec::new();
        let mut computed_method_value_types = Vec::new();

        for &member_idx in &object.elements.nodes {
            let Some(member_node) = self.arena.get(member_idx) else {
                continue;
            };
            let name_idx = if let Some(data) = self.arena.get_property_assignment(member_node) {
                Some(data.name)
            } else if let Some(data) = self.arena.get_shorthand_property(member_node) {
                Some(data.name)
            } else if let Some(data) = self.arena.get_accessor(member_node) {
                Some(data.name)
            } else {
                self.arena
                    .get_method_decl(member_node)
                    .map(|data| data.name)
            };
            let Some(name_idx) = name_idx else {
                continue;
            };
            let Some(name_node) = self.arena.get(name_idx) else {
                continue;
            };
            if !self.object_literal_member_needs_syntax_override(member_idx) {
                continue;
            }

            if name_node.kind == syntax_kind_ext::COMPUTED_PROPERTY_NAME
                && let Some(source_name_text) = self.get_source_slice(name_node.pos, name_node.end)
                && let Some(key_text) =
                    Self::negative_numeric_computed_property_key_text(&source_name_text)
            {
                negative_numeric_computed_names
                    .push((key_text.to_string(), source_name_text.trim().to_string()));
            }

            let Some(mut name_text) = self.object_literal_member_name_text(name_idx) else {
                if name_node.kind == syntax_kind_ext::COMPUTED_PROPERTY_NAME {
                    has_non_emittable_computed_members = true;
                    if synthetic_number_index_member.is_none() {
                        synthetic_number_index_member = self
                            .infer_object_member_type_text_named_at(
                                member_idx,
                                "[x: number]",
                                self.indent_level + 1,
                                false,
                                false,
                            );
                    }
                }
                continue;
            };
            let preserve_computed_syntax = name_node.kind
                == syntax_kind_ext::COMPUTED_PROPERTY_NAME
                && self
                    .resolved_computed_property_name_text(name_idx)
                    .is_none();
            if preserve_computed_syntax
                && let Some(source_name_text) = self.get_source_slice(name_node.pos, name_node.end)
                && Self::is_negative_numeric_computed_property_name_text(&source_name_text)
            {
                name_text = source_name_text.trim().to_string();
            }
            concrete_member_names.push(name_text.clone());
            let Some(member_text) = self.infer_object_member_type_text_named_at(
                member_idx,
                &name_text,
                self.indent_level + 1,
                getter_names.contains(&name_text),
                setter_names.contains(&name_text),
            ) else {
                continue;
            };
            if preserve_computed_syntax {
                if member_node.kind == syntax_kind_ext::METHOD_DECLARATION {
                    if let Some(value_type) = Self::object_literal_property_value_type(&member_text)
                    {
                        computed_method_value_types.push(value_type.to_string());
                    }
                }
                only_numeric_like &= Self::is_numeric_property_name_text(&name_text);
                computed_members.push((name_text, member_text));
            } else {
                overridden_members.push((name_text, member_text));
            }
        }

        if computed_members.is_empty() && overridden_members.is_empty() {
            return None;
        }

        if overridden_members
            .iter()
            .any(|(_, member_text)| member_text.contains('\n'))
        {
            return self.infer_object_literal_type_text_at(initializer, self.indent_level);
        }

        let printed = self.print_type_id(type_id);
        let mut lines: Vec<String> = printed.lines().map(str::to_string).collect();
        if lines.len() < 2 {
            return Some(printed);
        }

        if has_non_emittable_computed_members {
            let index_signature_value_types: Vec<String> = lines
                .iter()
                .filter_map(|line| {
                    Self::broad_object_index_signature_value_type(line).map(str::to_string)
                })
                .collect();
            if !index_signature_value_types.is_empty() {
                computed_members.retain(|(_, member_text)| {
                    Self::object_literal_property_value_type(member_text).is_none_or(
                        |member_value_type| {
                            !index_signature_value_types
                                .iter()
                                .any(|index_value_type| index_value_type == member_value_type)
                        },
                    )
                });
            }
        } else {
            let computed_value_types: Vec<String> = computed_members
                .iter()
                .filter_map(|(name_text, member_text)| {
                    Self::is_symbol_observer_computed_property_name_text(name_text)
                        .then(|| Self::object_literal_property_value_type(member_text))
                        .flatten()
                        .map(str::to_string)
                })
                .collect();
            if !computed_value_types.is_empty() {
                lines.retain(|line| {
                    let Some(index_value_type) =
                        Self::broad_object_index_signature_value_type(line)
                    else {
                        return true;
                    };
                    !computed_value_types
                        .iter()
                        .any(|member_value_type| member_value_type == index_value_type)
                });
            }
            if !computed_method_value_types.is_empty() {
                lines.retain(|line| {
                    let Some(index_value_type) =
                        Self::broad_object_index_signature_value_type(line)
                    else {
                        return true;
                    };
                    !computed_method_value_types
                        .iter()
                        .any(|member_value_type| member_value_type == index_value_type)
                });
            }
        }

        if only_numeric_like {
            if has_non_emittable_computed_members {
                for line in &mut lines {
                    let trimmed = line.trim_start();
                    if trimmed.starts_with("[x: string]:") {
                        *line = line.replacen("[x: string]:", "[x: number]:", 1);
                    } else if trimmed.starts_with("readonly [x: string]:") {
                        *line = line.replacen("readonly [x: string]:", "readonly [x: number]:", 1);
                    }
                }
                lines.retain(|line| {
                    let trimmed = line.trim_start();
                    if Self::is_numeric_like_object_property_line(trimmed)
                        && !Self::object_literal_line_matches_any_name(
                            trimmed,
                            &concrete_member_names,
                        )
                    {
                        return false;
                    }
                    true
                });
                let has_number_index = lines
                    .iter()
                    .any(|line| line.trim_start().starts_with("[x: number]:"));
                if !has_number_index
                    && let Some(member_text) = synthetic_number_index_member.as_deref()
                {
                    let member_indent = "    ".repeat((self.indent_level + 1) as usize);
                    lines.insert(1, format!("{member_indent}{member_text};"));
                }
            } else {
                lines.retain(|line| !line.trim_start().starts_with("[x: string]:"));
            }
        }

        let indent = "    ".repeat((self.indent_level + 1) as usize);
        for (name_text, member_text) in overridden_members {
            let replacement = format!("{indent}{member_text};");
            if let Some(existing_idx) = lines.iter().position(|line| {
                Self::object_literal_property_line_matches(line, &name_text, &replacement)
            }) {
                lines[existing_idx] = replacement;
            } else {
                let insert_at = lines.len().saturating_sub(1);
                lines.insert(insert_at, replacement);
            }
        }

        let insert_at = lines.len().saturating_sub(1);
        let mut actual_insertions = 0usize;
        for (name_text, member_text) in computed_members {
            let line = format!("{indent}{member_text};");
            let line_trimmed = line.trim();
            if Self::object_literal_method_line_matches_name(line_trimmed, &name_text)
                && lines.iter().any(|existing| {
                    Self::object_literal_method_line_matches_name(existing.trim(), &name_text)
                })
            {
                continue;
            }
            let exact_exists = lines.iter().any(|existing| existing.trim() == line_trimmed);
            if let Some(existing_idx) = lines.iter().position(|existing| {
                existing.trim() != line_trimmed
                    && Self::object_literal_property_line_matches(existing, &name_text, &line)
            }) {
                if exact_exists {
                    lines.remove(existing_idx);
                } else {
                    lines[existing_idx] = line;
                }
            } else {
                if !exact_exists {
                    lines.insert(insert_at + actual_insertions, line);
                    actual_insertions += 1;
                }
            }
        }

        if !negative_numeric_computed_names.is_empty() {
            for line in &mut lines {
                for (key_text, source_name_text) in &negative_numeric_computed_names {
                    if Self::replace_object_literal_property_line_name(
                        line,
                        key_text,
                        source_name_text,
                    ) {
                        break;
                    }
                }
            }
        }

        Some(lines.join("\n"))
    }

    fn object_literal_method_line_matches_name(existing: &str, name_text: &str) -> bool {
        let without_readonly = existing
            .strip_prefix("readonly ")
            .unwrap_or(existing)
            .trim_start();
        without_readonly.starts_with(&format!("{name_text}("))
            || without_readonly.starts_with(&format!("{name_text}<"))
            || without_readonly.starts_with(&format!("{name_text}?("))
            || without_readonly.starts_with(&format!("{name_text}?<"))
    }

    pub(in crate::declaration_emitter) fn object_literal_property_line_matches(
        existing: &str,
        name_text: &str,
        replacement: &str,
    ) -> bool {
        let trimmed = existing.trim();
        if trimmed == replacement.trim() {
            return true;
        }

        for prefix in Self::object_literal_property_name_prefixes(name_text) {
            if trimmed.starts_with(&prefix) || trimmed.starts_with(&format!("readonly {prefix}")) {
                return true;
            }
        }

        false
    }

    fn is_symbol_observer_computed_property_name_text(name_text: &str) -> bool {
        name_text.trim_start().starts_with("[Symbol.observer]")
    }

    fn is_negative_numeric_computed_property_name_text(name_text: &str) -> bool {
        Self::negative_numeric_computed_property_key_text(name_text).is_some()
    }

    fn negative_numeric_computed_property_key_text(name_text: &str) -> Option<&str> {
        let inner = name_text
            .trim()
            .strip_prefix("[-")
            .and_then(|name| name.strip_suffix(']'))?;

        inner.parse::<f64>().ok()?;
        name_text.trim().strip_prefix('[')?.strip_suffix(']')
    }

    fn replace_object_literal_property_line_name(
        line: &mut String,
        key_text: &str,
        replacement_name: &str,
    ) -> bool {
        let leading_len = line.len() - line.trim_start().len();
        let trimmed = &line[leading_len..];
        let candidates = [
            format!("\"{key_text}\":"),
            format!("'{key_text}':"),
            format!("{key_text}:"),
        ];
        for candidate in candidates {
            if trimmed.starts_with(&candidate) {
                let replacement = format!("{replacement_name}:");
                line.replace_range(leading_len..leading_len + candidate.len(), &replacement);
                return true;
            }
        }
        false
    }

    pub(in crate::declaration_emitter) fn object_literal_line_matches_any_name(
        existing: &str,
        names: &[String],
    ) -> bool {
        names.iter().any(|name| {
            Self::object_literal_property_name_prefixes(name)
                .into_iter()
                .any(|prefix| {
                    existing.starts_with(&prefix)
                        || existing.starts_with(&format!("readonly {prefix}"))
                })
        })
    }

    pub(in crate::declaration_emitter) fn object_literal_property_name_prefixes(
        name_text: &str,
    ) -> Vec<String> {
        let mut prefixes = vec![format!("{name_text}:")];

        if let Some(unquoted) = name_text
            .strip_prefix('"')
            .and_then(|name| name.strip_suffix('"'))
            .or_else(|| {
                name_text
                    .strip_prefix('\'')
                    .and_then(|name| name.strip_suffix('\''))
            })
        {
            prefixes.push(format!("\"{unquoted}\":"));
            prefixes.push(format!("'{unquoted}':"));
        }

        if let Some(negative_numeric) = name_text
            .strip_prefix("[-")
            .and_then(|name| name.strip_suffix(']'))
        {
            prefixes.push(format!("\"-{negative_numeric}\":"));
            prefixes.push(format!("'-{negative_numeric}':"));
            prefixes.push(format!("-{negative_numeric}:"));
        }

        prefixes
    }

    pub(in crate::declaration_emitter) fn object_literal_member_needs_syntax_override(
        &self,
        member_idx: NodeIndex,
    ) -> bool {
        let Some(member_node) = self.arena.get(member_idx) else {
            return false;
        };
        let Some(name_idx) = self.object_literal_member_name_idx(member_node) else {
            return false;
        };
        if self
            .arena
            .get(name_idx)
            .is_some_and(|name_node| name_node.kind == syntax_kind_ext::COMPUTED_PROPERTY_NAME)
        {
            return true;
        }

        let Some(initializer) = self.object_literal_member_initializer(member_node) else {
            return false;
        };
        if self
            .arena
            .get(initializer)
            .is_some_and(|node| node.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION)
            && self.object_literal_prefers_syntax_type_text(initializer)
        {
            return true;
        }
        if self.explicit_asserted_type_text(initializer).is_some() {
            return true;
        }
        if self
            .preferred_expression_type_text(initializer)
            .is_some_and(|text| {
                !text.is_empty()
                    && text != "any"
                    && (Self::type_text_contains_import_type(&text) || text.starts_with("typeof "))
            })
        {
            return true;
        }
        let type_id = self.get_node_type_or_names(&[initializer]);
        self.typeof_prefix_for_value_entity(initializer, true, type_id)
            .is_some()
            || self.enum_member_widened_type_text(initializer).is_some()
    }

    pub(in crate::declaration_emitter) fn object_literal_member_name_idx(
        &self,
        member_node: &Node,
    ) -> Option<NodeIndex> {
        if let Some(data) = self.arena.get_property_assignment(member_node) {
            return Some(data.name);
        }
        if let Some(data) = self.arena.get_shorthand_property(member_node) {
            return Some(data.name);
        }
        if let Some(data) = self.arena.get_accessor(member_node) {
            return Some(data.name);
        }
        self.arena
            .get_method_decl(member_node)
            .map(|data| data.name)
    }

    pub(in crate::declaration_emitter) fn object_literal_member_initializer(
        &self,
        member_node: &Node,
    ) -> Option<NodeIndex> {
        if let Some(data) = self.arena.get_property_assignment(member_node) {
            return Some(data.initializer);
        }
        // Shorthand `{ foo }` has no separate initializer node; the value
        // reference IS the name identifier. `{ foo = expr }` (CoverInitializedName)
        // is the only shape where `object_assignment_initializer` is non-`NONE`.
        self.arena.get_shorthand_property(member_node).map(|data| {
            if data.object_assignment_initializer == NodeIndex::NONE {
                data.name
            } else {
                data.object_assignment_initializer
            }
        })
    }

    pub(in crate::declaration_emitter) fn is_numeric_property_name_text(name: &str) -> bool {
        name.parse::<f64>().is_ok()
            || (name.starts_with("[-")
                && name.ends_with(']')
                && name[2..name.len().saturating_sub(1)].parse::<f64>().is_ok())
    }

    pub(in crate::declaration_emitter) fn is_numeric_like_object_property_line(line: &str) -> bool {
        let Some((name, _)) = line.split_once(':') else {
            return false;
        };
        let trimmed = name.trim().trim_start_matches("readonly ").trim();
        let normalized = trimmed
            .strip_prefix('"')
            .and_then(|s| s.strip_suffix('"'))
            .or_else(|| {
                trimmed
                    .strip_prefix('\'')
                    .and_then(|s| s.strip_suffix('\''))
            })
            .unwrap_or(trimmed);
        normalized.parse::<f64>().is_ok()
    }
}

#[cfg(test)]
#[path = "type_inference_tests.rs"]
mod tests;
