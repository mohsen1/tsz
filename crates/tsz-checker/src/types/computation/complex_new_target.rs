//! New-expression target validation and abstract constructor detection.
//!
//! Split from `complex.rs` to keep files under the 2000-LOC guard.
//! Contains:
//! - Abstract constructor detection in type nodes and declared targets
//! - Import-based abstract class detection
//! - `check_new_expression_target` — validates `new` expression targets
//! - `new_target_is_class_symbol` — checks whether a `new` target resolves to a class

use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    pub(crate) fn type_node_contains_abstract_constructor(
        &self,
        type_idx: NodeIndex,
        visited_aliases: &mut rustc_hash::FxHashSet<NodeIndex>,
    ) -> bool {
        use tsz_binder::symbol_flags;
        use tsz_parser::parser::syntax_kind_ext;

        let Some(node) = self.ctx.arena.get(type_idx) else {
            return false;
        };

        if let Some(query) = self.ctx.arena.get_type_query(node) {
            return self
                .class_symbol_from_expression(query.expr_name)
                .and_then(|sym_id| self.ctx.binder.get_symbol(sym_id))
                .is_some_and(|symbol| (symbol.flags & symbol_flags::ABSTRACT) != 0);
        }

        if let Some(composite) = self.ctx.arena.get_composite_type(node) {
            return composite.types.nodes.iter().any(|&member| {
                self.type_node_contains_abstract_constructor(member, visited_aliases)
            });
        }

        if let Some(wrapped) = self.ctx.arena.get_wrapped_type(node) {
            return self
                .type_node_contains_abstract_constructor(wrapped.type_node, visited_aliases);
        }

        if let Some(type_ref) = self.ctx.arena.get_type_ref(node) {
            let Some(sym_id) = self
                .resolve_identifier_symbol(type_ref.type_name)
                .or_else(|| {
                    self.ctx
                        .binder
                        .resolve_identifier(self.ctx.arena, type_ref.type_name)
                })
            else {
                return false;
            };
            let Some(symbol) = self.ctx.binder.get_symbol(sym_id) else {
                return false;
            };

            if (symbol.flags & symbol_flags::TYPE_ALIAS) == 0 {
                return false;
            }

            for &decl_idx in &symbol.declarations {
                if !visited_aliases.insert(decl_idx) {
                    continue;
                }
                let Some(decl_node) = self.ctx.arena.get(decl_idx) else {
                    continue;
                };
                if decl_node.kind != syntax_kind_ext::TYPE_ALIAS_DECLARATION {
                    continue;
                }
                if let Some(alias) = self.ctx.arena.get_type_alias(decl_node)
                    && self
                        .type_node_contains_abstract_constructor(alias.type_node, visited_aliases)
                {
                    return true;
                }
            }
        }

        false
    }

    pub(crate) fn declared_new_target_contains_abstract_constructor(
        &mut self,
        expr_idx: NodeIndex,
    ) -> bool {
        let Some(sym_id) = self
            .resolve_identifier_symbol(expr_idx)
            .or_else(|| self.ctx.binder.resolve_identifier(self.ctx.arena, expr_idx))
        else {
            return false;
        };
        let Some(symbol) = self.ctx.binder.get_symbol(sym_id) else {
            return false;
        };
        let Some(decl_node) = self.ctx.arena.get(symbol.value_declaration) else {
            return false;
        };

        if let Some(var_decl) = self.ctx.arena.get_variable_declaration(decl_node)
            && var_decl.type_annotation.is_some()
        {
            let mut visited_aliases = rustc_hash::FxHashSet::default();
            return self.type_node_contains_abstract_constructor(
                var_decl.type_annotation,
                &mut visited_aliases,
            );
        }

        if let Some(param) = self.ctx.arena.get_parameter(decl_node)
            && param.type_annotation.is_some()
        {
            let mut visited_aliases = rustc_hash::FxHashSet::default();
            return self.type_node_contains_abstract_constructor(
                param.type_annotation,
                &mut visited_aliases,
            );
        }

        false
    }

    pub(crate) fn imported_value_is_abstract_class(
        &self,
        module_name: &str,
        export_name: &str,
    ) -> bool {
        let export_sym_id = self
            .resolve_cross_file_export(module_name, export_name)
            .or_else(|| {
                let target_idx = self.ctx.resolve_import_target(module_name)?;
                let target_binder = self.ctx.get_binder_for_file(target_idx)?;
                let target_arena = self.ctx.get_arena_for_file(target_idx as u32);
                let file_name = target_arena.source_files.first()?.file_name.clone();
                target_binder
                    .module_exports
                    .get(&file_name)
                    .and_then(|exports| exports.get(export_name))
            });
        let Some(export_sym_id) = export_sym_id else {
            return false;
        };
        let Some(target_idx) = self
            .ctx
            .resolve_symbol_file_index(export_sym_id)
            .or_else(|| self.ctx.resolve_import_target(module_name))
        else {
            return false;
        };
        let Some(target_binder) = self.ctx.get_binder_for_file(target_idx) else {
            return false;
        };
        let target_arena = self.ctx.get_arena_for_file(target_idx as u32);
        let Some(export_symbol) = target_binder.get_symbol(export_sym_id) else {
            return false;
        };
        let decl_idx = if export_symbol.value_declaration.is_some() {
            export_symbol.value_declaration
        } else {
            *export_symbol
                .declarations
                .first()
                .unwrap_or(&NodeIndex::NONE)
        };
        target_arena
            .get(decl_idx)
            .filter(|decl| decl.kind == tsz_parser::parser::syntax_kind_ext::CLASS_DECLARATION)
            .and_then(|decl| target_arena.get_class(decl))
            .is_some_and(|class| self.has_abstract_modifier(&class.modifiers))
    }

    /// Validate the target of a `new` expression: reject type-only symbols and
    /// abstract classes. Returns `Some(TypeId)` if the expression should bail early.
    pub(crate) fn check_new_expression_target(
        &mut self,
        new_idx: NodeIndex,
        expr_idx: NodeIndex,
    ) -> Option<TypeId> {
        use crate::diagnostics::diagnostic_codes;
        use tsz_binder::symbol_flags;
        use tsz_scanner::SyntaxKind;

        // Primitive type keywords in constructor position (`new number[]`) are
        // type-only and should report TS2693.
        if let Some(expr_node) = self.ctx.arena.get(expr_idx) {
            let keyword_name = match expr_node.kind {
                k if k == SyntaxKind::NumberKeyword as u16 => Some("number"),
                k if k == SyntaxKind::StringKeyword as u16 => Some("string"),
                k if k == SyntaxKind::BooleanKeyword as u16 => Some("boolean"),
                k if k == SyntaxKind::SymbolKeyword as u16 => Some("symbol"),
                k if k == SyntaxKind::VoidKeyword as u16 => Some("void"),
                k if k == SyntaxKind::UndefinedKeyword as u16 => Some("undefined"),
                k if k == SyntaxKind::NullKeyword as u16 => Some("null"),
                k if k == SyntaxKind::AnyKeyword as u16 => Some("any"),
                k if k == SyntaxKind::UnknownKeyword as u16 => Some("unknown"),
                k if k == SyntaxKind::NeverKeyword as u16 => Some("never"),
                k if k == SyntaxKind::ObjectKeyword as u16 => Some("object"),
                k if k == SyntaxKind::BigIntKeyword as u16 => Some("bigint"),
                _ => None,
            };
            if let Some(keyword_name) = keyword_name {
                self.report_wrong_meaning_diagnostic(
                    keyword_name,
                    expr_idx,
                    crate::query_boundaries::name_resolution::NameLookupKind::Type,
                );
                return Some(TypeId::ERROR);
            }
        }

        let ident = self.ctx.arena.get_identifier_at(expr_idx)?;
        let class_name = &ident.escaped_text;

        let sym_id = self
            .resolve_identifier_symbol(expr_idx)
            .or_else(|| self.ctx.binder.resolve_identifier(self.ctx.arena, expr_idx))
            .or_else(|| self.ctx.binder.get_node_symbol(expr_idx))
            .or_else(|| self.ctx.binder.file_locals.get(class_name))
            .or_else(|| self.ctx.binder.get_symbols().find_by_name(class_name))?;
        let symbol = self.ctx.binder.get_symbol(sym_id).or_else(|| {
            self.ctx
                .resolve_symbol_file_index(sym_id)
                .and_then(|file_idx| self.ctx.get_binder_for_file(file_idx))
                .and_then(|binder| binder.get_symbol(sym_id))
        })?;

        if self.alias_resolves_to_type_only(sym_id) {
            self.report_wrong_meaning_diagnostic(
                class_name,
                expr_idx,
                crate::query_boundaries::name_resolution::NameLookupKind::Type,
            );
            return Some(TypeId::ERROR);
        }

        let has_type = (symbol.flags & symbol_flags::TYPE) != 0;
        let has_value = (symbol.flags & symbol_flags::VALUE) != 0;
        let is_type_alias = (symbol.flags & symbol_flags::TYPE_ALIAS) != 0;

        if !has_value && (is_type_alias || has_type) {
            // Type parameters only shadow in type contexts, not value contexts.
            // `new A()` where `A` is a type param shadowing an outer class should
            // resolve to the outer class, not emit TS2693.
            let is_type_param_only =
                (symbol.flags & symbol_flags::TYPE_PARAMETER) != 0 && !has_value;
            if is_type_param_only {
                let lib_binders = self.get_lib_binders();
                let has_outer_value = self
                    .ctx
                    .binder
                    .resolve_identifier_with_filter(self.ctx.arena, expr_idx, &lib_binders, |sid| {
                        self.ctx
                            .binder
                            .get_symbol_with_libs(sid, &lib_binders)
                            .is_some_and(|s| s.flags & symbol_flags::VALUE != 0)
                    })
                    .is_some();
                if has_outer_value {
                    // Fall through — the new expression will use the outer value
                    return None;
                }
            }
            self.report_wrong_meaning_diagnostic(
                class_name,
                expr_idx,
                crate::query_boundaries::name_resolution::NameLookupKind::Type,
            );
            return Some(TypeId::ERROR);
        }
        let symbol_decl_idx = if symbol.value_declaration.is_some() {
            symbol.value_declaration
        } else {
            *symbol.declarations.first().unwrap_or(&NodeIndex::NONE)
        };
        if let Some(decl_node) = self.ctx.arena.get(symbol_decl_idx)
            && decl_node.kind == tsz_parser::parser::syntax_kind_ext::IMPORT_CLAUSE
            && let Some(ext) = self.ctx.arena.get_extended(symbol_decl_idx)
            && ext.parent.is_some()
            && let Some(import_decl_node) = self.ctx.arena.get(ext.parent)
            && let Some(import_decl) = self.ctx.arena.get_import_decl(import_decl_node)
            && import_decl.module_specifier.is_some()
            && let Some(spec_node) = self.ctx.arena.get(import_decl.module_specifier)
            && let Some(spec_lit) = self.ctx.arena.get_literal(spec_node)
            && self.imported_value_is_abstract_class(&spec_lit.text, "default")
        {
            self.error_at_node(
                new_idx,
                "Cannot create an instance of an abstract class.",
                diagnostic_codes::CANNOT_CREATE_AN_INSTANCE_OF_AN_ABSTRACT_CLASS,
            );
            return Some(TypeId::ERROR);
        }
        if let Some(module_name) = symbol.import_module.as_deref() {
            let export_name = symbol.import_name.as_deref().unwrap_or(class_name);
            if self.imported_value_is_abstract_class(module_name, export_name) {
                self.error_at_node(
                    new_idx,
                    "Cannot create an instance of an abstract class.",
                    diagnostic_codes::CANNOT_CREATE_AN_INSTANCE_OF_AN_ABSTRACT_CLASS,
                );
                return Some(TypeId::ERROR);
            }
        }
        let resolved_sym_id = if (symbol.flags & symbol_flags::ALIAS) != 0 {
            self.resolve_alias_symbol(sym_id, &mut Vec::new())
                .unwrap_or(sym_id)
        } else {
            sym_id
        };
        let resolved_symbol = self
            .ctx
            .binder
            .get_symbol(resolved_sym_id)
            .or_else(|| {
                self.ctx
                    .resolve_symbol_file_index(resolved_sym_id)
                    .and_then(|file_idx| self.ctx.get_binder_for_file(file_idx))
                    .and_then(|binder| binder.get_symbol(resolved_sym_id))
            })
            .unwrap_or(symbol);
        let resolved_symbol_is_abstract_class = (resolved_symbol.flags
            & (symbol_flags::CLASS | symbol_flags::ABSTRACT))
            == (symbol_flags::CLASS | symbol_flags::ABSTRACT)
            || {
                let target_file_idx = self.ctx.resolve_symbol_file_index(resolved_sym_id);
                let target_arena = target_file_idx
                    .map(|file_idx| self.ctx.get_arena_for_file(file_idx as u32))
                    .unwrap_or(self.ctx.arena);
                let resolved_decl_idx = if resolved_symbol.value_declaration.is_some() {
                    resolved_symbol.value_declaration
                } else {
                    *resolved_symbol
                        .declarations
                        .first()
                        .unwrap_or(&NodeIndex::NONE)
                };
                target_arena
                    .get(resolved_decl_idx)
                    .filter(|decl| {
                        decl.kind == tsz_parser::parser::syntax_kind_ext::CLASS_DECLARATION
                    })
                    .and_then(|decl| target_arena.get_class(decl))
                    .is_some_and(|class| self.has_abstract_modifier(&class.modifiers))
            };
        if resolved_symbol_is_abstract_class {
            self.error_at_node(
                new_idx,
                "Cannot create an instance of an abstract class.",
                diagnostic_codes::CANNOT_CREATE_AN_INSTANCE_OF_AN_ABSTRACT_CLASS,
            );
            return Some(TypeId::ERROR);
        }
        let symbol_type = self.get_type_of_symbol(sym_id);
        if symbol_type != TypeId::ERROR
            && symbol_type != TypeId::UNKNOWN
            && self.type_contains_abstract_class(symbol_type)
        {
            self.error_at_node(
                new_idx,
                "Cannot create an instance of an abstract class.",
                diagnostic_codes::CANNOT_CREATE_AN_INSTANCE_OF_AN_ABSTRACT_CLASS,
            );
            return Some(TypeId::ERROR);
        }
        None
    }

    pub(crate) fn new_target_is_class_symbol(&self, expr_idx: NodeIndex) -> bool {
        use tsz_binder::symbol_flags;
        use tsz_parser::parser::syntax_kind_ext;
        let Some(ident) = self.ctx.arena.get_identifier_at(expr_idx) else {
            // For property access expressions (e.g., `new B.a.C()`), resolve the
            // qualified name to check if the target is a class. Forward-referenced
            // classes accessed through namespace import aliases may transiently lack
            // construct signatures; this suppresses the false TS2351.
            if let Some(sym_id) = self.resolve_qualified_symbol(expr_idx)
                && let Some(symbol) = self.ctx.binder.get_symbol(sym_id)
                && (symbol.flags & symbol_flags::CLASS) != 0
            {
                return true;
            }
            return false;
        };
        let name = &ident.escaped_text;
        let Some(sym_id) = self
            .ctx
            .binder
            .resolve_identifier(self.ctx.arena, expr_idx)
            .or_else(|| self.ctx.binder.get_node_symbol(expr_idx))
            .or_else(|| self.ctx.binder.file_locals.get(name))
            .or_else(|| self.ctx.binder.get_symbols().find_by_name(name))
        else {
            return false;
        };
        if let Some(symbol) = self.ctx.binder.get_symbol(sym_id) {
            if (symbol.flags & symbol_flags::CLASS) != 0 {
                return true;
            }
            // Variables initialized with class expressions (e.g., `let C = class { ... }`)
            // should be treated as class symbols for circular self-reference suppression.
            // The variable has VARIABLE flags, not CLASS, but `new C()` inside the class
            // body is a valid self-referencing construct that tsc accepts.
            if (symbol.flags & symbol_flags::VARIABLE) != 0
                && symbol.value_declaration.is_some()
                && let Some(decl_node) = self.ctx.arena.get(symbol.value_declaration)
                && let Some(var_decl) = self.ctx.arena.get_variable_declaration(decl_node)
                && let Some(init_node) = self.ctx.arena.get(var_decl.initializer)
                && init_node.kind == syntax_kind_ext::CLASS_EXPRESSION
            {
                return true;
            }
        }
        // Cross-file: in multi-file mode, the name may resolve to a namespace in the
        // current file while a class with the same name exists in another file
        // (class+namespace declaration merging across files). Walk up enclosing
        // namespaces and search all binders for a CLASS symbol with the same name.
        if let Some(all_binders) = self.ctx.all_binders.as_ref()
            && !self.ctx.binder.is_external_module()
        {
            let arena = self.ctx.arena;
            let mut current = expr_idx;
            for _ in 0..100 {
                let Some(ext) = arena.get_extended(current) else {
                    break;
                };
                let parent_idx = ext.parent;
                if parent_idx.is_none() {
                    break;
                }
                let Some(parent_node) = arena.get(parent_idx) else {
                    break;
                };
                if parent_node.kind == syntax_kind_ext::MODULE_DECLARATION
                    && let Some(module_data) = arena.get_module(parent_node)
                    && let Some(ns_name_ident) = arena.get_identifier_at(module_data.name)
                {
                    let ns_name = ns_name_ident.escaped_text.as_str();
                    // Search all binders for a CLASS symbol with the target
                    // name exported from a namespace matching ns_name.
                    for binder in all_binders.iter() {
                        for (_, &parent_sym_id) in binder.file_locals.iter() {
                            let Some(parent_sym) = self.ctx.binder.get_symbol(parent_sym_id) else {
                                continue;
                            };
                            if parent_sym.flags
                                & (symbol_flags::NAMESPACE_MODULE | symbol_flags::VALUE_MODULE)
                                == 0
                            {
                                continue;
                            }
                            if let Some(parent_exports) = parent_sym.exports.as_ref()
                                && let Some(nested_ns_id) = parent_exports.get(ns_name)
                                && let Some(nested_ns) = self.ctx.binder.get_symbol(nested_ns_id)
                                && nested_ns.flags
                                    & (symbol_flags::NAMESPACE_MODULE | symbol_flags::VALUE_MODULE)
                                    != 0
                                && let Some(nested_exports) = nested_ns.exports.as_ref()
                                && let Some(member_id) = nested_exports.get(name)
                                && self
                                    .ctx
                                    .binder
                                    .get_symbol(member_id)
                                    .is_some_and(|s| (s.flags & symbol_flags::CLASS) != 0)
                            {
                                return true;
                            }
                        }
                    }
                }
                current = parent_idx;
            }
        }
        false
    }

    /// Check whether a `new` expression target is a class whose `extends` clause
    /// has a TS2314 error (wrong number of type arguments on the base class).
    ///
    /// In tsc, when `class C extends Base` omits required type arguments for a
    /// generic `Base<T>`, `typeof C` has no construct signatures, so `new C()`
    /// produces TS2351. Our constructor builder still generates a default
    /// constructor in this case; this helper detects the condition so the caller
    /// can override to TS2351 + return `any`.
    pub(crate) fn class_has_invalid_base_type_args(&self, expr_idx: NodeIndex) -> bool {
        self.class_has_invalid_base_type_args_inner(expr_idx)
            .unwrap_or(false)
    }

    fn class_has_invalid_base_type_args_inner(&self, expr_idx: NodeIndex) -> Option<bool> {
        use tsz_binder::symbol_flags;
        use tsz_parser::parser::syntax_kind_ext;

        // Resolve to symbol
        let sym_id = self
            .ctx
            .binder
            .resolve_identifier(self.ctx.arena, expr_idx)
            .or_else(|| self.ctx.binder.get_node_symbol(expr_idx))?;
        let symbol = self.ctx.binder.get_symbol(sym_id)?;
        if symbol.flags & symbol_flags::CLASS == 0 {
            return Some(false);
        }

        // Find class declaration
        let decl_idx = symbol
            .declarations
            .iter()
            .copied()
            .find(|&idx| {
                self.ctx
                    .arena
                    .get(idx)
                    .is_some_and(|n| n.kind == syntax_kind_ext::CLASS_DECLARATION)
            })
            .unwrap_or(symbol.value_declaration);

        let class_node = self.ctx.arena.get(decl_idx)?;
        let class = self.ctx.arena.get_class(class_node)?;
        let heritage_clauses = class.heritage_clauses.as_ref()?;

        // Check if any extends clause type reference has a TS2314 diagnostic
        for &clause_idx in &heritage_clauses.nodes {
            let Some(heritage) = self.ctx.arena.get_heritage_clause_at(clause_idx) else {
                continue;
            };
            if heritage.token != tsz_scanner::SyntaxKind::ExtendsKeyword as u16 {
                continue;
            }
            for &type_idx in &heritage.types.nodes {
                if let Some(node) = self.ctx.arena.get(type_idx) {
                    let start = node.pos;
                    let end = node.end;
                    if self.has_diagnostic_code_within_span(
                        start,
                        end,
                        crate::diagnostics::diagnostic_codes::GENERIC_TYPE_REQUIRES_TYPE_ARGUMENT_S,
                    ) {
                        return Some(true);
                    }
                }
            }
        }
        Some(false)
    }
}
