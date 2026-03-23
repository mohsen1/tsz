//! Member completion logic: dot-access, namespace exports, object literals,
//! and contextual type resolution.

use super::*;

#[derive(Clone, Copy, Debug)]
pub(super) struct PropertyCompletion {
    pub type_id: TypeId,
    pub is_method: bool,
    pub is_optional: bool,
}

impl<'a> Completions<'a> {
    /// Create a `CheckerState` from the current provider state, re-using a
    /// cached instance when available.
    pub(super) fn make_checker(
        &self,
        cache_ref: Option<&mut Option<TypeCache>>,
    ) -> Option<CheckerState<'a>> {
        let interner = self.interner?;
        let file_name = self.file_name.as_ref()?;
        let compiler_options = self.checker_options();
        let checker = if let Some(cache) = cache_ref {
            if let Some(cache_value) = cache.take() {
                CheckerState::with_cache(
                    self.arena,
                    self.binder,
                    interner,
                    file_name.clone(),
                    cache_value,
                    compiler_options,
                )
            } else {
                CheckerState::new(
                    self.arena,
                    self.binder,
                    interner,
                    file_name.clone(),
                    compiler_options,
                )
            }
        } else {
            CheckerState::new(
                self.arena,
                self.binder,
                interner,
                file_name.clone(),
                compiler_options,
            )
        };
        Some(checker)
    }

    /// Build the `CheckerOptions` corresponding to this provider's strict-mode
    /// configuration.
    fn checker_options(&self) -> tsz_checker::context::CheckerOptions {
        tsz_checker::context::CheckerOptions {
            strict: self.strict,
            no_implicit_any: self.strict,
            no_implicit_returns: false,
            no_implicit_this: self.strict,
            strict_null_checks: self.strict,
            strict_function_types: self.strict,
            strict_property_initialization: self.strict,
            use_unknown_in_catch_variables: self.strict,
            isolated_modules: false,
            ..Default::default()
        }
    }

    pub(super) fn get_member_completions(
        &self,
        expr_idx: NodeIndex,
        type_cache: Option<&mut Option<TypeCache>>,
    ) -> Option<Vec<CompletionItem>> {
        let interner = self.interner?;

        let mut cache_ref = type_cache;
        let mut checker = self.make_checker(cache_ref.as_deref_mut())?;

        let mut items = Vec::new();
        let mut seen_names = FxHashSet::default();
        let mut annotation_target_symbol = None;

        // Type-qualified member access (`A.B`) should prefer namespace/module exports
        // instead of instance/member shape properties.
        let qualified_name_target = self.is_qualified_name_member_target(expr_idx);
        // When the expression is `this` inside a class body, all members
        // (public, private, protected) should be visible in completions.
        let is_this_in_class = self
            .arena
            .get(expr_idx)
            .is_some_and(|n| n.kind == SyntaxKind::ThisKeyword as u16)
            && self.find_enclosing_class_declaration(expr_idx).is_some();
        // When the expression is `super`, resolve base class and show its members.
        let is_super = self
            .arena
            .get(expr_idx)
            .is_some_and(|n| n.kind == SyntaxKind::SuperKeyword as u16);

        if is_super {
            if let Some(base_sym_id) = self.resolve_super_base_class_symbol(expr_idx) {
                self.append_symbol_members_as_completions(
                    base_sym_id,
                    &mut checker,
                    &mut seen_names,
                    &mut items,
                );
            }
        } else if !qualified_name_target {
            let type_id = checker.get_type_of_node(expr_idx);
            let mut visited = FxHashSet::default();
            let mut props: FxHashMap<String, PropertyCompletion> = FxHashMap::default();
            if is_this_in_class {
                self.collect_all_properties_for_type(
                    type_id,
                    interner,
                    &mut checker,
                    &mut visited,
                    &mut props,
                );
            } else {
                self.collect_properties_for_type(
                    type_id,
                    interner,
                    &mut checker,
                    &mut visited,
                    &mut props,
                );
            }

            for (name, info) in props {
                let kind = if info.is_method {
                    CompletionItemKind::Method
                } else {
                    CompletionItemKind::Property
                };
                let mut item = CompletionItem::new(name.clone(), kind);
                item = item.with_detail(checker.format_type(info.type_id));
                if name == "substr" {
                    item.sort_text =
                        Some(sort_priority::deprecated(sort_priority::LOCATION_PRIORITY));
                    item.kind_modifiers = Some("deprecated".to_string());
                } else {
                    item.sort_text = Some(sort_priority::MEMBER.to_string());
                }

                // Add snippet insert text for method completions
                if info.is_method {
                    item.insert_text = Some(format!("{name}($1)"));
                    item.is_snippet = true;
                }

                seen_names.insert(name);
                items.push(item);
            }
        }

        if items.is_empty()
            && let Some(sym_id) = self.resolve_member_target_symbol(expr_idx)
            && let Some(type_annotation) = self.symbol_type_annotation_node(sym_id)
        {
            let mut visited = FxHashSet::default();
            let mut props: FxHashMap<String, PropertyCompletion> = FxHashMap::default();
            let declared_type = checker.get_type_of_node(type_annotation);
            self.collect_properties_for_type(
                declared_type,
                interner,
                &mut checker,
                &mut visited,
                &mut props,
            );
            if props.is_empty()
                && let Some(type_annotation_node) = self.arena.get(type_annotation)
                && type_annotation_node.kind == syntax_kind_ext::TYPE_REFERENCE
                && let Some(type_ref) = self.arena.get_type_ref(type_annotation_node)
                && let Some(type_symbol_id) = self.resolve_member_target_symbol(type_ref.type_name)
            {
                annotation_target_symbol = Some(type_symbol_id);
                let annotation_symbol_type = checker.get_type_of_symbol(type_symbol_id);
                self.collect_properties_for_type(
                    annotation_symbol_type,
                    interner,
                    &mut checker,
                    &mut visited,
                    &mut props,
                );
            } else if props.is_empty()
                && let Some(type_annotation_node) = self.arena.get(type_annotation)
                && type_annotation_node.kind == syntax_kind_ext::TYPE_QUERY
                && let Some(type_query) = self.arena.get_type_query(type_annotation_node)
            {
                let query_expr_type = checker.get_type_of_node(type_query.expr_name);
                self.collect_properties_for_type(
                    query_expr_type,
                    interner,
                    &mut checker,
                    &mut visited,
                    &mut props,
                );
                annotation_target_symbol = self.resolve_member_target_symbol(type_query.expr_name);
                if let Some(type_symbol_id) = annotation_target_symbol {
                    let annotation_symbol_type = checker.get_type_of_symbol(type_symbol_id);
                    self.collect_properties_for_type(
                        annotation_symbol_type,
                        interner,
                        &mut checker,
                        &mut visited,
                        &mut props,
                    );
                }
            }
            for (name, info) in props {
                let kind = if info.is_method {
                    CompletionItemKind::Method
                } else {
                    CompletionItemKind::Property
                };
                let mut item = CompletionItem::new(name.clone(), kind);
                item = item.with_detail(checker.format_type(info.type_id));
                if name == "substr" {
                    item.sort_text =
                        Some(sort_priority::deprecated(sort_priority::LOCATION_PRIORITY));
                    item.kind_modifiers = Some("deprecated".to_string());
                } else {
                    item.sort_text = Some(sort_priority::MEMBER.to_string());
                }
                if info.is_method {
                    item.insert_text = Some(format!("{name}($1)"));
                    item.is_snippet = true;
                }
                seen_names.insert(name);
                items.push(item);
            }
        }

        if let Some(target_symbol_id) = self.resolve_member_target_symbol(expr_idx) {
            self.append_namespace_export_member_completions(
                target_symbol_id,
                &mut checker,
                !qualified_name_target,
                &mut seen_names,
                &mut items,
            );
        }
        if let Some(annotation_symbol_id) = annotation_target_symbol
            && Some(annotation_symbol_id) != self.resolve_member_target_symbol(expr_idx)
        {
            self.append_namespace_export_member_completions(
                annotation_symbol_id,
                &mut checker,
                !qualified_name_target,
                &mut seen_names,
                &mut items,
            );
        }

        if items.is_empty() {
            self.append_syntactic_member_fallback(expr_idx, &mut seen_names, &mut items);
        }

        items.sort_by(|a, b| a.label.cmp(&b.label));
        if let Some(cache) = cache_ref {
            *cache = Some(checker.extract_cache());
        }
        Some(items)
    }

    pub(super) fn collect_properties_for_type(
        &self,
        type_id: TypeId,
        interner: &TypeInterner,
        checker: &mut CheckerState,
        visited: &mut FxHashSet<TypeId>,
        props: &mut FxHashMap<String, PropertyCompletion>,
    ) {
        self.collect_properties_for_type_inner(type_id, interner, checker, visited, props, false);
    }

    /// Collect properties including private/protected members.
    /// Used for `this.` access inside a class body where all members are accessible.
    pub(super) fn collect_all_properties_for_type(
        &self,
        type_id: TypeId,
        interner: &TypeInterner,
        checker: &mut CheckerState,
        visited: &mut FxHashSet<TypeId>,
        props: &mut FxHashMap<String, PropertyCompletion>,
    ) {
        self.collect_properties_for_type_inner(type_id, interner, checker, visited, props, true);
    }

    fn collect_properties_for_type_inner(
        &self,
        type_id: TypeId,
        interner: &TypeInterner,
        checker: &mut CheckerState,
        visited: &mut FxHashSet<TypeId>,
        props: &mut FxHashMap<String, PropertyCompletion>,
        include_private: bool,
    ) {
        if !visited.insert(type_id) {
            return;
        }

        let resolved = checker.resolve_lazy_type(type_id);
        let evaluated = tsz_solver::evaluate_type(interner, resolved);
        if evaluated != type_id {
            self.collect_properties_for_type_inner(
                evaluated,
                interner,
                checker,
                visited,
                props,
                include_private,
            );
            return;
        }

        if let Some(shape_id) = visitor::object_shape_id(interner, evaluated)
            .or_else(|| visitor::object_with_index_shape_id(interner, evaluated))
        {
            let shape = interner.object_shape(shape_id);
            for prop in &shape.properties {
                if !include_private && prop.visibility != Visibility::Public {
                    continue;
                }
                let name = interner.resolve_atom(prop.name);
                // Skip synthetic brand properties used for nominal class typing.
                // These are internal type system markers (e.g. `__private_brand_42`)
                // and must never appear in user-facing completions.
                if name.starts_with("__private_brand_") {
                    continue;
                }
                self.add_property_completion_ex(
                    props,
                    interner,
                    name,
                    prop.type_id,
                    prop.is_method,
                    prop.optional,
                );
            }
            return;
        }

        if let Some(members_id) = visitor::union_list_id(interner, evaluated) {
            // For unions, only include properties that exist on ALL members (intersection).
            // E.g., `string | number` should only show `toString`, `valueOf`, `toLocaleString`
            // — not all string methods or all number methods.
            let members = interner.type_list(members_id);
            if members.len() > 1 {
                let mut per_member_props: Vec<FxHashMap<String, PropertyCompletion>> = Vec::new();
                for &member in members.iter() {
                    let mut member_props = FxHashMap::default();
                    let mut member_visited = visited.clone();
                    self.collect_properties_for_type_inner(
                        member,
                        interner,
                        checker,
                        &mut member_visited,
                        &mut member_props,
                        include_private,
                    );
                    per_member_props.push(member_props);
                }
                // Intersect: keep only properties present in ALL members,
                // and create a union of each member's property type.
                if let Some(first) = per_member_props.first() {
                    for (name, info) in first {
                        if per_member_props[1..].iter().all(|m| m.contains_key(name)) {
                            let mut member_types = vec![info.type_id];
                            for member_props in &per_member_props[1..] {
                                if let Some(mi) = member_props.get(name) {
                                    member_types.push(mi.type_id);
                                }
                            }
                            let union_type = if member_types.len() > 1 {
                                interner.union(member_types)
                            } else {
                                info.type_id
                            };
                            self.add_property_completion(
                                props,
                                interner,
                                name.clone(),
                                union_type,
                                info.is_method,
                            );
                        }
                    }
                }
            } else {
                for &member in members.iter() {
                    self.collect_properties_for_type_inner(
                        member,
                        interner,
                        checker,
                        visited,
                        props,
                        include_private,
                    );
                }
            }
            return;
        }

        if let Some(members_id) = visitor::intersection_list_id(interner, evaluated) {
            // For intersections, include properties from ALL members (union of properties).
            let members = interner.type_list(members_id);
            for &member in members.iter() {
                self.collect_properties_for_type_inner(
                    member,
                    interner,
                    checker,
                    visited,
                    props,
                    include_private,
                );
            }
            return;
        }

        if let Some(app) = visitor::application_id(interner, evaluated) {
            let app = interner.type_application(app);
            self.collect_properties_for_type_inner(
                app.base,
                interner,
                checker,
                visited,
                props,
                include_private,
            );
            return;
        }

        if let Some(literal) = visitor::literal_value(interner, evaluated) {
            if let Some(kind) = self.literal_intrinsic_kind(&literal) {
                self.collect_intrinsic_members(kind, interner, props);
            }
            return;
        }

        if visitor::template_literal_id(interner, evaluated).is_some() {
            self.collect_intrinsic_members(IntrinsicKind::String, interner, props);
            return;
        }

        if let Some(kind) = visitor::intrinsic_kind(interner, evaluated) {
            self.collect_intrinsic_members(kind, interner, props);
        }
    }

    fn symbol_type_annotation_node(&self, sym_id: tsz_binder::SymbolId) -> Option<NodeIndex> {
        let symbol = self.binder.symbols.get(sym_id)?;
        let decl = if symbol.value_declaration.is_some() {
            symbol.value_declaration
        } else {
            *symbol.declarations.first()?
        };
        let node = self.arena.get(decl)?;
        if node.kind == syntax_kind_ext::VARIABLE_DECLARATION {
            let var_decl = self.arena.get_variable_declaration(node)?;
            return var_decl
                .type_annotation
                .is_some()
                .then_some(var_decl.type_annotation);
        }
        if node.kind == syntax_kind_ext::PARAMETER {
            let param = self.arena.get_parameter(node)?;
            return param
                .type_annotation
                .is_some()
                .then_some(param.type_annotation);
        }
        if node.kind == syntax_kind_ext::PROPERTY_DECLARATION {
            let property = self.arena.get_property_decl(node)?;
            return property
                .type_annotation
                .is_some()
                .then_some(property.type_annotation);
        }
        if node.kind == syntax_kind_ext::PROPERTY_SIGNATURE
            || node.kind == syntax_kind_ext::METHOD_SIGNATURE
        {
            let signature = self.arena.get_signature(node)?;
            return signature
                .type_annotation
                .is_some()
                .then_some(signature.type_annotation);
        }
        None
    }

    fn is_qualified_name_member_target(&self, expr_idx: NodeIndex) -> bool {
        let Some(ext) = self.arena.get_extended(expr_idx) else {
            return false;
        };
        let Some(parent) = self.arena.get(ext.parent) else {
            return false;
        };
        if parent.kind != syntax_kind_ext::QUALIFIED_NAME {
            return false;
        }
        self.arena
            .get_qualified_name(parent)
            .is_some_and(|qualified| qualified.left == expr_idx)
    }

    pub(super) fn resolve_member_target_symbol(
        &self,
        expr_idx: NodeIndex,
    ) -> Option<tsz_binder::SymbolId> {
        if let Some(sym_id) = self.binder.node_symbols.get(&expr_idx.0).copied() {
            return Some(sym_id);
        }

        let node = self.arena.get(expr_idx)?;
        match node.kind {
            k if k == SyntaxKind::Identifier as u16 => {
                self.binder.resolve_identifier(self.arena, expr_idx)
            }
            k if k == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION => {
                let access = self.arena.get_access_expr(node)?;
                let left = self.resolve_member_target_symbol(access.expression)?;
                let name = self.arena.get_identifier_text(access.name_or_argument)?;
                self.resolve_exported_member_symbol(left, name)
            }
            k if k == syntax_kind_ext::QUALIFIED_NAME => {
                let qualified = self.arena.get_qualified_name(node)?;
                let left = self.resolve_member_target_symbol(qualified.left)?;
                let name = self.arena.get_identifier_text(qualified.right)?;
                self.resolve_exported_member_symbol(left, name)
            }
            k if k == SyntaxKind::SuperKeyword as u16 => {
                self.resolve_super_base_class_symbol(expr_idx)
            }
            _ => self.binder.resolve_identifier(self.arena, expr_idx),
        }
    }

    /// Resolve `super` to the base class symbol for completions.
    fn resolve_super_base_class_symbol(
        &self,
        super_idx: NodeIndex,
    ) -> Option<tsz_binder::SymbolId> {
        let mut current = super_idx;
        loop {
            let ext = self.arena.get_extended(current)?;
            if ext.parent.is_none() {
                return None;
            }
            let parent = self.arena.get(ext.parent)?;
            if parent.kind == syntax_kind_ext::CLASS_DECLARATION
                || parent.kind == syntax_kind_ext::CLASS_EXPRESSION
            {
                let class = self.arena.get_class(parent)?;
                let heritage = class.heritage_clauses.as_ref()?;
                for &clause_idx in &heritage.nodes {
                    let clause_node = self.arena.get(clause_idx)?;
                    let hd = self.arena.get_heritage(clause_node)?;
                    if hd.token != SyntaxKind::ExtendsKeyword as u16 {
                        continue;
                    }
                    if let Some(&type_idx) = hd.types.nodes.first() {
                        let type_node = self.arena.get(type_idx)?;
                        let expr_idx =
                            if type_node.kind == syntax_kind_ext::EXPRESSION_WITH_TYPE_ARGUMENTS {
                                self.arena
                                    .get_expr_type_args(type_node)
                                    .map(|e| e.expression)?
                            } else {
                                type_idx
                            };
                        return self.binder.resolve_identifier(self.arena, expr_idx);
                    }
                }
                return None;
            }
            current = ext.parent;
        }
    }

    fn resolve_exported_member_symbol(
        &self,
        container: tsz_binder::SymbolId,
        member_name: &str,
    ) -> Option<tsz_binder::SymbolId> {
        let container_symbol = self.binder.symbols.get(container)?;
        if let Some(exports) = container_symbol.exports.as_ref()
            && let Some(member) = exports.get(member_name)
        {
            return Some(member);
        }
        if let Some(members) = container_symbol.members.as_ref()
            && let Some(member) = members.get(member_name)
        {
            return Some(member);
        }
        None
    }

    fn append_syntactic_member_fallback(
        &self,
        expr_idx: NodeIndex,
        seen_names: &mut FxHashSet<String>,
        items: &mut Vec<CompletionItem>,
    ) {
        let Some(expr_node) = self.arena.get(expr_idx) else {
            return;
        };

        if expr_node.kind == SyntaxKind::RegularExpressionLiteral as u16 {
            for (name, kind, deprecated) in [
                ("exec", CompletionItemKind::Method, false),
                ("test", CompletionItemKind::Method, false),
                ("source", CompletionItemKind::Property, false),
                ("global", CompletionItemKind::Property, false),
                ("ignoreCase", CompletionItemKind::Property, false),
                ("multiline", CompletionItemKind::Property, false),
                ("lastIndex", CompletionItemKind::Property, false),
                ("compile", CompletionItemKind::Method, true),
            ] {
                if !seen_names.insert(name.to_string()) {
                    continue;
                }
                let mut item = CompletionItem::new(name.to_string(), kind);
                item.sort_text = Some(if deprecated {
                    sort_priority::deprecated(sort_priority::LOCATION_PRIORITY)
                } else {
                    sort_priority::MEMBER.to_string()
                });
                if deprecated {
                    item.kind_modifiers = Some("deprecated".to_string());
                }
                if kind == CompletionItemKind::Method {
                    item.insert_text = Some(format!("{name}($1)"));
                    item.is_snippet = true;
                }
                items.push(item);
            }
            return;
        }

        if expr_node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
            && let Some(access) = self.arena.get_access_expr(expr_node)
        {
            if let Some(sym_id) = self.resolve_member_target_symbol(access.name_or_argument) {
                self.append_type_literal_annotation_members(sym_id, seen_names, items);
                return;
            }
            if let Some(prop_name) = self.arena.get_identifier_text(access.name_or_argument)
                && let Some(container_sym) = self.resolve_member_target_symbol(access.expression)
            {
                self.append_named_property_annotation_members(
                    container_sym,
                    prop_name,
                    seen_names,
                    items,
                );
            }
            return;
        }

        if expr_node.kind == SyntaxKind::ThisKeyword as u16
            || (expr_node.kind == SyntaxKind::Identifier as u16
                && self.arena.get_identifier_text(expr_idx) == Some("this"))
        {
            let Some(class_idx) = self.find_enclosing_class_declaration(expr_idx) else {
                return;
            };
            let Some(class_node) = self.arena.get(class_idx) else {
                return;
            };
            let Some(class_data) = self.arena.get_class(class_node) else {
                return;
            };

            for &member_idx in &class_data.members.nodes {
                let Some(member_node) = self.arena.get(member_idx) else {
                    continue;
                };

                // Extract the member name and kind based on AST node type.
                // `this.` inside a class body should show all instance members
                // (properties and methods, both public and private).
                let (name, kind) = if member_node.kind == syntax_kind_ext::PROPERTY_DECLARATION {
                    let Some(prop) = self.arena.get_property_decl(member_node) else {
                        continue;
                    };
                    if self.has_static_modifier_node(member_idx) {
                        continue;
                    }
                    let Some(name) = self.arena.get_identifier_text(prop.name) else {
                        continue;
                    };
                    (name, CompletionItemKind::Property)
                } else if member_node.kind == syntax_kind_ext::METHOD_DECLARATION {
                    let Some(method) = self.arena.get_method_decl(member_node) else {
                        continue;
                    };
                    if self.has_static_modifier_node(member_idx) {
                        continue;
                    }
                    let Some(name) = self.arena.get_identifier_text(method.name) else {
                        continue;
                    };
                    (name, CompletionItemKind::Method)
                } else {
                    continue;
                };

                if name.starts_with('#') {
                    continue;
                }
                if !seen_names.insert(name.to_string()) {
                    continue;
                }
                let mut item = CompletionItem::new(name.to_string(), kind);
                item.sort_text = Some(sort_priority::MEMBER.to_string());
                if kind == CompletionItemKind::Method {
                    item.insert_text = Some(format!("{name}($1)"));
                    item.is_snippet = true;
                    // Extract method signature from source text for display.
                    item.detail = self.extract_method_signature_from_source(member_idx);
                } else if kind == CompletionItemKind::Property {
                    // Extract property type from source for display.
                    item.detail = self.extract_property_type_from_source(member_idx);
                }
                items.push(item);
            }
        }
    }

    fn append_type_literal_annotation_members(
        &self,
        sym_id: tsz_binder::SymbolId,
        seen_names: &mut FxHashSet<String>,
        items: &mut Vec<CompletionItem>,
    ) {
        let Some(type_node_idx) = self.symbol_type_annotation_node(sym_id) else {
            return;
        };
        self.append_members_from_type_node(type_node_idx, seen_names, items);
    }

    fn append_named_property_annotation_members(
        &self,
        container_sym_id: tsz_binder::SymbolId,
        property_name: &str,
        seen_names: &mut FxHashSet<String>,
        items: &mut Vec<CompletionItem>,
    ) {
        let Some(type_node_idx) = self.symbol_type_annotation_node(container_sym_id) else {
            return;
        };
        let Some(type_node) = self.arena.get(type_node_idx) else {
            return;
        };
        if type_node.kind != syntax_kind_ext::TYPE_REFERENCE {
            return;
        }
        let Some(type_ref) = self.arena.get_type_ref(type_node) else {
            return;
        };
        let Some(type_name) = self.arena.get_identifier_text(type_ref.type_name) else {
            return;
        };
        let type_sym = self
            .binder
            .file_locals
            .get(type_name)
            .or_else(|| self.resolve_member_target_symbol(type_ref.type_name));
        let Some(type_sym) = type_sym else {
            return;
        };
        let Some(symbol) = self.binder.symbols.get(type_sym) else {
            return;
        };
        for &decl_idx in &symbol.declarations {
            let Some(decl_node) = self.arena.get(decl_idx) else {
                continue;
            };
            match decl_node.kind {
                k if k == syntax_kind_ext::INTERFACE_DECLARATION => {
                    let Some(iface) = self.arena.get_interface(decl_node) else {
                        continue;
                    };
                    for &member_idx in &iface.members.nodes {
                        let Some(member_node) = self.arena.get(member_idx) else {
                            continue;
                        };
                        if member_node.kind != syntax_kind_ext::PROPERTY_SIGNATURE
                            && member_node.kind != syntax_kind_ext::METHOD_SIGNATURE
                        {
                            continue;
                        }
                        let Some(signature) = self.arena.get_signature(member_node) else {
                            continue;
                        };
                        let Some(name) = self.arena.get_identifier_text(signature.name) else {
                            continue;
                        };
                        if name != property_name || !signature.type_annotation.is_some() {
                            continue;
                        }
                        self.append_members_from_type_node(
                            signature.type_annotation,
                            seen_names,
                            items,
                        );
                    }
                }
                k if k == syntax_kind_ext::CLASS_DECLARATION
                    || k == syntax_kind_ext::CLASS_EXPRESSION =>
                {
                    let Some(class_data) = self.arena.get_class(decl_node) else {
                        continue;
                    };
                    for &member_idx in &class_data.members.nodes {
                        let Some(member_node) = self.arena.get(member_idx) else {
                            continue;
                        };
                        if member_node.kind != syntax_kind_ext::PROPERTY_DECLARATION {
                            continue;
                        }
                        let Some(prop) = self.arena.get_property_decl(member_node) else {
                            continue;
                        };
                        let Some(name) = self.arena.get_identifier_text(prop.name) else {
                            continue;
                        };
                        if name != property_name || !prop.type_annotation.is_some() {
                            continue;
                        }
                        self.append_members_from_type_node(prop.type_annotation, seen_names, items);
                    }
                }
                _ => {}
            }
        }
    }

    fn append_members_from_type_node(
        &self,
        type_node_idx: NodeIndex,
        seen_names: &mut FxHashSet<String>,
        items: &mut Vec<CompletionItem>,
    ) {
        let Some(type_node) = self.arena.get(type_node_idx) else {
            return;
        };
        if type_node.kind == syntax_kind_ext::TYPE_REFERENCE {
            let Some(type_ref) = self.arena.get_type_ref(type_node) else {
                return;
            };
            let Some(type_name) = self.arena.get_identifier_text(type_ref.type_name) else {
                return;
            };
            let sym_id = self
                .binder
                .file_locals
                .get(type_name)
                .or_else(|| self.resolve_member_target_symbol(type_ref.type_name));
            let Some(sym_id) = sym_id else {
                return;
            };
            let Some(symbol) = self.binder.symbols.get(sym_id) else {
                return;
            };
            for &decl_idx in &symbol.declarations {
                let Some(decl_node) = self.arena.get(decl_idx) else {
                    continue;
                };
                if decl_node.kind != syntax_kind_ext::CLASS_DECLARATION
                    && decl_node.kind != syntax_kind_ext::CLASS_EXPRESSION
                {
                    continue;
                }
                let Some(class_data) = self.arena.get_class(decl_node) else {
                    continue;
                };
                for &member_idx in &class_data.members.nodes {
                    let Some(member_node) = self.arena.get(member_idx) else {
                        continue;
                    };
                    if member_node.kind != syntax_kind_ext::PROPERTY_DECLARATION {
                        continue;
                    }
                    let Some(prop) = self.arena.get_property_decl(member_node) else {
                        continue;
                    };
                    let Some(name) = self.arena.get_identifier_text(prop.name) else {
                        continue;
                    };
                    if name.starts_with('#') || !seen_names.insert(name.to_string()) {
                        continue;
                    }
                    let mut item =
                        CompletionItem::new(name.to_string(), CompletionItemKind::Property);
                    item.sort_text = Some(sort_priority::MEMBER.to_string());
                    items.push(item);
                }
            }
            return;
        }
        if type_node.kind != syntax_kind_ext::TYPE_LITERAL {
            return;
        }
        let Some(type_literal) = self.arena.get_type_literal(type_node) else {
            return;
        };

        for &member_idx in &type_literal.members.nodes {
            let Some(member_node) = self.arena.get(member_idx) else {
                continue;
            };
            let is_method = member_node.kind == syntax_kind_ext::METHOD_SIGNATURE;
            let is_property = member_node.kind == syntax_kind_ext::PROPERTY_SIGNATURE;
            if !(is_method || is_property) {
                continue;
            }
            let Some(signature) = self.arena.get_signature(member_node) else {
                continue;
            };
            let Some(name_node) = self.arena.get(signature.name) else {
                continue;
            };
            let start = name_node.pos as usize;
            let end = (name_node.end as usize).min(self.source_text.len());
            if start >= end {
                continue;
            }
            let raw_name = self.source_text[start..end]
                .trim()
                .trim_end_matches(':')
                .trim_end()
                .trim_end_matches('?')
                .trim_end();
            let (label, needs_quoted_insert) = if (raw_name.starts_with('"')
                && raw_name.ends_with('"'))
                || (raw_name.starts_with('\'') && raw_name.ends_with('\''))
            {
                (raw_name[1..raw_name.len() - 1].to_string(), true)
            } else {
                (raw_name.to_string(), false)
            };
            if label.is_empty() || !seen_names.insert(label.clone()) {
                continue;
            }
            let kind = if is_method {
                CompletionItemKind::Method
            } else {
                CompletionItemKind::Property
            };
            let mut item = CompletionItem::new(label.clone(), kind);
            item.sort_text = Some(sort_priority::MEMBER.to_string());
            if signature.type_annotation.is_some()
                && let Some(type_node) = self.arena.get(signature.type_annotation)
            {
                let type_start = type_node.pos as usize;
                let type_end = (type_node.end as usize).min(self.source_text.len());
                if type_start < type_end {
                    let type_text = self.source_text[type_start..type_end]
                        .trim()
                        .trim_end_matches(';')
                        .trim_end();
                    if !type_text.is_empty() {
                        item = item.with_detail(type_text.to_string());
                    }
                }
            }
            if needs_quoted_insert {
                item.insert_text = Some(format!("?.[\"{label}\"]"));
            } else if is_method {
                item.insert_text = Some(format!("{label}($1)"));
                item.is_snippet = true;
            }
            items.push(item);
        }
    }

    fn append_namespace_export_member_completions(
        &self,
        symbol_id: tsz_binder::SymbolId,
        checker: &mut CheckerState,
        allow_class_prototype: bool,
        seen_names: &mut FxHashSet<String>,
        items: &mut Vec<CompletionItem>,
    ) {
        use tsz_binder::symbol_flags;

        let Some(symbol) = self.binder.symbols.get(symbol_id) else {
            return;
        };

        let symbol_name = symbol.escaped_name.clone();
        let is_class = (symbol.flags & symbol_flags::CLASS) != 0;

        let export_entries: Vec<(String, tsz_binder::SymbolId)> = symbol
            .exports
            .as_ref()
            .map(|exports| {
                exports
                    .iter()
                    .map(|(name, id)| (name.clone(), *id))
                    .collect()
            })
            .unwrap_or_default();

        for (name, export_id) in export_entries {
            if seen_names.contains(&name) {
                continue;
            }
            let Some(export_symbol) = self.binder.symbols.get(export_id) else {
                continue;
            };

            let kind = self.determine_completion_kind(export_symbol);
            let mut item = CompletionItem::new(name.clone(), kind);
            item.sort_text = Some(sort_priority::LOCATION_PRIORITY.to_string());

            let export_type = checker.get_type_of_symbol(export_id);
            let detail = checker.format_type(export_type);
            if !detail.is_empty() {
                item = item.with_detail(detail);
            } else if let Some(detail) = self.get_symbol_detail(export_symbol) {
                item = item.with_detail(detail);
            }

            if let Some(modifiers) = self.build_kind_modifiers(export_symbol) {
                item.kind_modifiers = Some(modifiers);
            }

            if kind == CompletionItemKind::Function || kind == CompletionItemKind::Method {
                item.insert_text = Some(format!("{name}($1)"));
                item.is_snippet = true;
            }

            seen_names.insert(name);
            items.push(item);
        }

        if is_class {
            let static_member_entries: Vec<(String, tsz_binder::SymbolId)> = symbol
                .members
                .as_ref()
                .map(|members| {
                    members
                        .iter()
                        .map(|(name, id)| (name.clone(), *id))
                        .collect()
                })
                .unwrap_or_default();

            for (name, member_id) in static_member_entries {
                if seen_names.contains(&name) {
                    continue;
                }
                let Some(member_symbol) = self.binder.symbols.get(member_id) else {
                    continue;
                };
                if (member_symbol.flags & symbol_flags::STATIC) == 0 {
                    continue;
                }
                if (member_symbol.flags & (symbol_flags::PRIVATE | symbol_flags::PROTECTED)) != 0 {
                    continue;
                }

                let kind = self.determine_completion_kind(member_symbol);
                let mut item = CompletionItem::new(name.clone(), kind);
                item.sort_text = Some(sort_priority::LOCAL_DECLARATION.to_string());
                let member_type = checker.get_type_of_symbol(member_id);
                let detail = checker.format_type(member_type);
                if !detail.is_empty() {
                    item = item.with_detail(detail);
                } else if let Some(detail) = self.get_symbol_detail(member_symbol) {
                    item = item.with_detail(detail);
                }
                if let Some(modifiers) = self.build_kind_modifiers(member_symbol) {
                    item.kind_modifiers = Some(modifiers);
                }
                if kind == CompletionItemKind::Function || kind == CompletionItemKind::Method {
                    item.insert_text = Some(format!("{name}($1)"));
                    item.is_snippet = true;
                }

                seen_names.insert(name);
                items.push(item);
            }
        }

        if allow_class_prototype && is_class && !seen_names.contains("prototype") {
            let mut item =
                CompletionItem::new("prototype".to_string(), CompletionItemKind::Property);
            item.sort_text = Some(sort_priority::MEMBER.to_string());
            item = item.with_detail(symbol_name);
            seen_names.insert("prototype".to_string());
            items.push(item);
        }
    }

    /// Add instance members of a class symbol as completions (for `super.` access).
    fn append_symbol_members_as_completions(
        &self,
        symbol_id: tsz_binder::SymbolId,
        checker: &mut CheckerState,
        seen_names: &mut FxHashSet<String>,
        items: &mut Vec<CompletionItem>,
    ) {
        let Some(symbol) = self.binder.symbols.get(symbol_id) else {
            return;
        };
        let member_entries: Vec<(String, tsz_binder::SymbolId)> = symbol
            .members
            .as_ref()
            .map(|members| {
                members
                    .iter()
                    .map(|(name, id)| (name.clone(), *id))
                    .collect()
            })
            .unwrap_or_default();

        for (name, member_id) in member_entries {
            if seen_names.contains(&name) {
                continue;
            }
            let Some(member_symbol) = self.binder.symbols.get(member_id) else {
                continue;
            };
            let kind = self.determine_completion_kind(member_symbol);
            let mut item = CompletionItem::new(name.clone(), kind);
            item.sort_text = Some(sort_priority::MEMBER.to_string());
            let member_type = checker.get_type_of_symbol(member_id);
            let detail = checker.format_type(member_type);
            if !detail.is_empty() {
                item = item.with_detail(detail);
            }
            if kind == CompletionItemKind::Function || kind == CompletionItemKind::Method {
                item.insert_text = Some(format!("{name}($1)"));
                item.is_snippet = true;
            }
            seen_names.insert(name);
            items.push(item);
        }
    }

    pub fn get_member_completion_parent_type_name(
        &self,
        root: NodeIndex,
        position: Position,
    ) -> Option<String> {
        let offset = self
            .line_map
            .position_to_offset(position, self.source_text)?;
        if let Some(parent) = self.meta_property_parent_type_name(offset) {
            return Some(parent);
        }
        let node_idx = self.find_completions_node(root, offset);
        let expr_idx = self.member_completion_target(node_idx, offset)?;

        let mut checker = self.make_checker(None)?;
        let type_id = checker.get_type_of_node(expr_idx);
        let type_text = checker.format_type(type_id);
        if let Some(parent) = Self::normalize_member_parent_type_name(&type_text) {
            return Some(parent);
        }
        if let Some(parent) = self
            .resolve_member_target_symbol(expr_idx)
            .and_then(|sym_id| self.binder.symbols.get(sym_id))
            .and_then(|symbol| {
                use tsz_binder::symbol_flags;
                ((symbol.flags & (symbol_flags::CLASS | symbol_flags::FUNCTION)) != 0)
                    .then(|| symbol.escaped_name.clone())
            })
        {
            return Some(parent);
        }

        // For `this.` inside a class body, use the enclosing class name.
        let expr_node = self.arena.get(expr_idx)?;
        if expr_node.kind == SyntaxKind::ThisKeyword as u16 {
            let class_idx = self.find_enclosing_class_declaration(expr_idx)?;
            let class_node = self.arena.get(class_idx)?;
            let class_data = self.arena.get_class(class_node)?;
            return self
                .arena
                .get_identifier_text(class_data.name)
                .map(|s| s.to_string());
        }

        None
    }

    fn meta_property_parent_type_name(&self, offset: u32) -> Option<String> {
        let end = (offset as usize).min(self.source_text.len());
        let prefix = Self::strip_trailing_fourslash_marker(&self.source_text[..end]).trim_end();
        let before_dot = prefix.strip_suffix('.')?;
        let expr = before_dot.trim_end();
        if expr.ends_with("import.meta") {
            return Some("ImportMeta".to_string());
        }
        let token_start = expr
            .rfind(|c: char| !(c == '_' || c == '$' || c.is_ascii_alphanumeric()))
            .map_or(0, |idx| idx + 1);
        let token = &expr[token_start..];
        let before_token = &expr[..token_start];
        let has_ident_before = before_token
            .chars()
            .next_back()
            .is_some_and(|ch| ch == '_' || ch == '$' || ch.is_ascii_alphanumeric());
        if has_ident_before {
            return None;
        }
        if token == "import" {
            return Some("ImportMetaExpression".to_string());
        }
        if token == "new" && self.is_inside_function(offset) {
            return Some("NewTargetExpression".to_string());
        }
        None
    }

    fn normalize_member_parent_type_name(type_text: &str) -> Option<String> {
        let mut normalized = type_text.trim();
        if let Some(stripped) = normalized.strip_prefix("typeof ") {
            normalized = stripped.trim();
        }
        if normalized.is_empty() {
            return None;
        }
        if normalized == "any" {
            return None;
        }
        let mut chars = normalized.chars();
        let first = chars.next()?;
        if !(first == '_' || first == '$' || first.is_ascii_alphabetic()) {
            return None;
        }
        if chars.all(|ch| ch == '_' || ch == '$' || ch.is_ascii_alphanumeric()) {
            Some(normalized.to_string())
        } else {
            None
        }
    }

    fn collect_intrinsic_members(
        &self,
        kind: IntrinsicKind,
        interner: &TypeInterner,
        props: &mut FxHashMap<String, PropertyCompletion>,
    ) {
        let members = apparent_primitive_members(interner, kind);
        for member in members {
            if kind == IntrinsicKind::String
                && !Self::is_baseline_string_completion_member(member.name)
            {
                continue;
            }
            let type_id = match member.kind {
                ApparentMemberKind::Value(type_id) | ApparentMemberKind::Method(type_id) => type_id,
            };
            let is_method = matches!(member.kind, ApparentMemberKind::Method(_));
            self.add_property_completion(
                props,
                interner,
                member.name.to_string(),
                type_id,
                is_method,
            );
        }
    }

    fn is_baseline_string_completion_member(name: &str) -> bool {
        matches!(
            name,
            "toString"
                | "charAt"
                | "charCodeAt"
                | "concat"
                | "indexOf"
                | "lastIndexOf"
                | "localeCompare"
                | "match"
                | "replace"
                | "search"
                | "slice"
                | "split"
                | "substring"
                | "toLowerCase"
                | "toLocaleLowerCase"
                | "toUpperCase"
                | "toLocaleUpperCase"
                | "toLocaleString"
                | "trim"
                | "length"
                | "substr"
                | "valueOf"
        )
    }

    pub(super) const fn literal_intrinsic_kind(
        &self,
        literal: &tsz_solver::LiteralValue,
    ) -> Option<IntrinsicKind> {
        match literal {
            tsz_solver::LiteralValue::String(_) => Some(IntrinsicKind::String),
            tsz_solver::LiteralValue::Number(_) => Some(IntrinsicKind::Number),
            tsz_solver::LiteralValue::Boolean(_) => Some(IntrinsicKind::Boolean),
            tsz_solver::LiteralValue::BigInt(_) => Some(IntrinsicKind::Bigint),
        }
    }

    pub(super) fn add_property_completion(
        &self,
        props: &mut FxHashMap<String, PropertyCompletion>,
        interner: &TypeInterner,
        name: String,
        type_id: TypeId,
        is_method: bool,
    ) {
        self.add_property_completion_ex(props, interner, name, type_id, is_method, false);
    }

    fn add_property_completion_ex(
        &self,
        props: &mut FxHashMap<String, PropertyCompletion>,
        interner: &TypeInterner,
        name: String,
        type_id: TypeId,
        is_method: bool,
        is_optional: bool,
    ) {
        if let Some(existing) = props.get_mut(&name) {
            if existing.type_id != type_id {
                existing.type_id = interner.union(vec![existing.type_id, type_id]);
            }
            existing.is_method |= is_method;
        } else {
            props.insert(
                name,
                PropertyCompletion {
                    type_id,
                    is_method,
                    is_optional,
                },
            );
        }
    }

    /// Suggest properties for object literals based on contextual type.
    /// When typing inside `{ | }`, suggests properties from the expected type.
    pub(super) fn get_object_literal_completions(
        &self,
        node_idx: NodeIndex,
        offset: u32,
        type_cache: Option<&mut Option<TypeCache>>,
    ) -> Option<Vec<CompletionItem>> {
        let interner = self.interner?;

        // 1. Find the enclosing object literal
        let object_literal_idx = self.find_enclosing_object_literal(node_idx, offset)?;

        // 2. Determine the contextual type (expected type)
        let mut cache_ref = type_cache;
        let mut checker = self.make_checker(cache_ref.as_deref_mut())?;

        let context_type = self.get_contextual_type(object_literal_idx, &mut checker)?;

        // 3. Find properties already defined in this literal
        let existing_props = self.get_defined_properties(object_literal_idx);

        // 4. Collect properties from the expected type
        let mut items = Vec::new();
        let mut props: FxHashMap<String, PropertyCompletion> = FxHashMap::default();
        let mut visited = FxHashSet::default();

        self.collect_properties_for_type(
            context_type,
            interner,
            &mut checker,
            &mut visited,
            &mut props,
        );
        let in_string_property_name_context =
            self.is_string_property_name_completion_context(node_idx);

        for (name, info) in props {
            if !in_string_property_name_context && existing_props.contains(&name) {
                continue;
            }

            let kind = if info.is_method {
                CompletionItemKind::Method
            } else {
                CompletionItemKind::Property
            };

            let needs_quoted_label =
                !in_string_property_name_context && !Self::is_valid_unquoted_property_name(&name);
            let label = if needs_quoted_label {
                format!("\"{name}\"")
            } else {
                name.clone()
            };

            let mut item = CompletionItem::new(label, kind);
            item = item.with_detail(checker.format_type(info.type_id));
            if info.is_optional {
                item.sort_text = Some(sort_priority::OPTIONAL_MEMBER.to_string());
                item.kind_modifiers = Some("optional".to_string());
            } else {
                item.sort_text = Some(sort_priority::MEMBER.to_string());
            }

            // Add snippet insert text for method completions in object literals
            if info.is_method {
                item.insert_text = Some(format!("{name}($1)"));
                item.is_snippet = true;
            }

            items.push(item);
        }

        if let Some(cache) = cache_ref {
            *cache = Some(checker.extract_cache());
        }

        if items.is_empty() {
            None
        } else {
            items.sort_by(|a, b| a.label.cmp(&b.label));
            Some(items)
        }
    }

    pub(super) fn is_string_property_name_completion_context(&self, node_idx: NodeIndex) -> bool {
        let mut current = node_idx;
        let mut depth = 0usize;
        while current.is_some() && depth < 8 {
            let Some(node) = self.arena.get(current) else {
                break;
            };
            if node.kind == SyntaxKind::StringLiteral as u16
                || node.kind == SyntaxKind::NoSubstitutionTemplateLiteral as u16
            {
                let Some(ext) = self.arena.get_extended(current) else {
                    break;
                };
                let Some(parent) = self.arena.get(ext.parent) else {
                    break;
                };
                if parent.kind == syntax_kind_ext::PROPERTY_ASSIGNMENT
                    && let Some(prop) = self.arena.get_property_assignment(parent)
                    && prop.name == current
                {
                    return true;
                }
            }
            let Some(ext) = self.arena.get_extended(current) else {
                break;
            };
            if ext.parent == current {
                break;
            }
            current = ext.parent;
            depth += 1;
        }
        false
    }

    fn is_valid_unquoted_property_name(name: &str) -> bool {
        let mut chars = name.chars();
        let Some(first) = chars.next() else {
            return false;
        };
        if !(first == '_' || first == '$' || first.is_ascii_alphabetic()) {
            return false;
        }
        chars.all(|ch| ch == '_' || ch == '$' || ch.is_ascii_alphanumeric())
    }

    /// Find the enclosing object literal expression for a given node.
    fn find_enclosing_object_literal(&self, node_idx: NodeIndex, offset: u32) -> Option<NodeIndex> {
        let node = self.arena.get(node_idx)?;

        // Cursor is directly on the literal (e.g. empty {})
        if node.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION {
            return Some(node_idx);
        }

        // Cursor is on a child (identifier, property, etc.)
        let ext = self.arena.get_extended(node_idx)?;
        let parent = self.arena.get(ext.parent)?;

        if parent.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION {
            return Some(ext.parent);
        }

        // Cursor is deep (e.g. inside a property assignment value)
        // Handle { prop: | } or { prop }
        if parent.kind == syntax_kind_ext::PROPERTY_ASSIGNMENT {
            let grand_ext = self.arena.get_extended(ext.parent)?;
            let grand_parent = self.arena.get(grand_ext.parent)?;
            if grand_parent.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION {
                return Some(grand_ext.parent);
            }
        }

        // Also check for shorthand property assignment
        if parent.kind == syntax_kind_ext::SHORTHAND_PROPERTY_ASSIGNMENT {
            let grand_ext = self.arena.get_extended(ext.parent)?;
            let grand_parent = self.arena.get(grand_ext.parent)?;
            if grand_parent.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION {
                return Some(grand_ext.parent);
            }
        }

        // General fallback: walk ancestors and pick the nearest object literal.
        let mut current = node_idx;
        let mut depth = 0usize;
        while current.is_some() && depth < 64 {
            let Some(current_node) = self.arena.get(current) else {
                break;
            };
            if current_node.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION {
                return Some(current);
            }
            let Some(current_ext) = self.arena.get_extended(current) else {
                break;
            };
            if current_ext.parent == current {
                break;
            }
            current = current_ext.parent;
            depth += 1;
        }

        // Fallback: choose smallest object literal containing the cursor offset.
        let mut best = None;
        let mut best_len = u32::MAX;
        for (i, n) in self.arena.nodes.iter().enumerate() {
            if n.kind != syntax_kind_ext::OBJECT_LITERAL_EXPRESSION {
                continue;
            }
            if n.pos <= offset && offset <= n.end {
                let len = n.end.saturating_sub(n.pos);
                if len < best_len {
                    best_len = len;
                    best = Some(NodeIndex(i as u32));
                }
            }
        }
        if best.is_some() {
            return best;
        }

        None
    }

    pub(super) fn find_enclosing_class_declaration(
        &self,
        node_idx: NodeIndex,
    ) -> Option<NodeIndex> {
        let mut current = node_idx;
        while current.is_some() {
            let node = self.arena.get(current)?;
            if node.kind == syntax_kind_ext::CLASS_DECLARATION
                || node.kind == syntax_kind_ext::CLASS_EXPRESSION
            {
                return Some(current);
            }
            // Stop at regular function boundaries — `function() {}` resets
            // `this` binding, so `this.` inside a function expression/declaration
            // doesn't refer to the enclosing class.
            // Arrow functions do NOT reset `this`, so we continue past them.
            if node.kind == syntax_kind_ext::FUNCTION_EXPRESSION
                || node.kind == syntax_kind_ext::FUNCTION_DECLARATION
            {
                return None;
            }
            let ext = self.arena.get_extended(current)?;
            current = ext.parent;
        }
        None
    }

    /// Check if a class member node has the `static` modifier.
    fn has_static_modifier_node(&self, member_idx: NodeIndex) -> bool {
        let Some(node) = self.arena.get(member_idx) else {
            return false;
        };
        let modifiers = if node.kind == syntax_kind_ext::PROPERTY_DECLARATION {
            self.arena
                .get_property_decl(node)
                .and_then(|d| d.modifiers.as_ref())
        } else if node.kind == syntax_kind_ext::METHOD_DECLARATION {
            self.arena
                .get_method_decl(node)
                .and_then(|d| d.modifiers.as_ref())
        } else {
            None
        };
        if let Some(mods) = modifiers {
            for &mod_idx in &mods.nodes {
                if let Some(mod_node) = self.arena.get(mod_idx)
                    && mod_node.kind == SyntaxKind::StaticKeyword as u16
                {
                    return true;
                }
            }
        }
        false
    }

    /// Extract a method signature string (e.g. `(): void`) from the source text
    /// of a method declaration node. Used by the `this.` AST fallback to provide
    /// type detail when the checker cannot resolve the type.
    fn extract_method_signature_from_source(&self, method_idx: NodeIndex) -> Option<String> {
        let node = self.arena.get(method_idx)?;
        let start = node.pos as usize;
        let end = node.end.min(self.source_text.len() as u32) as usize;
        if start >= end {
            return None;
        }
        let text = &self.source_text[start..end];
        // Find the opening paren of the parameter list
        let open = text.find('(')?;
        // Find the matching close paren
        let mut depth = 0i32;
        let mut close = None;
        for (i, ch) in text[open..].char_indices() {
            match ch {
                '(' => depth += 1,
                ')' => {
                    depth -= 1;
                    if depth == 0 {
                        close = Some(open + i);
                        break;
                    }
                }
                _ => {}
            }
        }
        let close = close?;
        // Check for return type annotation after the close paren
        let after_params = text[close + 1..].trim_start();
        if let Some(rest) = after_params.strip_prefix(':') {
            let return_type = rest.trim_start();
            // Take until `{` or end of line
            let end_pos = return_type
                .find('{')
                .or_else(|| return_type.find('\n'))
                .unwrap_or(return_type.len());
            let return_type = return_type[..end_pos].trim();
            Some(format!("({}): {}", &text[open + 1..close], return_type))
        } else {
            Some(format!("({}): void", &text[open + 1..close]))
        }
    }

    /// Extract a property type string (e.g. `number`) from the source text of a
    /// property declaration node. Used by the `this.` AST fallback.
    fn extract_property_type_from_source(&self, prop_idx: NodeIndex) -> Option<String> {
        let node = self.arena.get(prop_idx)?;
        let prop = self.arena.get_property_decl(node)?;
        if prop.type_annotation.is_some() {
            let type_node = self.arena.get(prop.type_annotation)?;
            let start = type_node.pos as usize;
            let end = type_node.end.min(self.source_text.len() as u32) as usize;
            if start < end {
                return Some(self.source_text[start..end].trim().to_string());
            }
        }
        // If there's an initializer, try to infer the type name from it
        if prop.initializer.is_some() {
            let init_node = self.arena.get(prop.initializer)?;
            let start = init_node.pos as usize;
            let end = init_node.end.min(self.source_text.len() as u32) as usize;
            if start < end {
                let text = self.source_text[start..end].trim();
                // Simple literal type inference
                if text.parse::<f64>().is_ok() {
                    return Some("number".to_string());
                }
                if text == "true" || text == "false" {
                    return Some("boolean".to_string());
                }
                if text.starts_with('"') || text.starts_with('\'') || text.starts_with('`') {
                    return Some("string".to_string());
                }
            }
        }
        None
    }

    pub(super) fn class_extends_expression(&self, class_idx: NodeIndex) -> Option<NodeIndex> {
        let class_node = self.arena.get(class_idx)?;
        let class_data = self.arena.get_class(class_node)?;
        let clauses = class_data.heritage_clauses.as_ref()?;
        for &clause_idx in &clauses.nodes {
            let clause_node = self.arena.get(clause_idx)?;
            let heritage = self.arena.get_heritage_clause(clause_node)?;
            if heritage.token != SyntaxKind::ExtendsKeyword as u16 {
                continue;
            }
            for &type_idx in &heritage.types.nodes {
                let type_node = self.arena.get(type_idx)?;
                if let Some(expr_with_type_args) = self.arena.get_expr_type_args(type_node) {
                    return Some(expr_with_type_args.expression);
                }
            }
        }
        None
    }

    pub(super) fn class_declared_member_names(&self, class_idx: NodeIndex) -> FxHashSet<String> {
        let mut names = FxHashSet::default();
        let Some(class_node) = self.arena.get(class_idx) else {
            return names;
        };
        let Some(class_data) = self.arena.get_class(class_node) else {
            return names;
        };

        for &member_idx in &class_data.members.nodes {
            let Some(member_node) = self.arena.get(member_idx) else {
                continue;
            };
            let name_idx = match member_node.kind {
                k if k == syntax_kind_ext::METHOD_DECLARATION => {
                    self.arena.get_method_decl(member_node).map(|m| m.name)
                }
                k if k == syntax_kind_ext::PROPERTY_DECLARATION => {
                    self.arena.get_property_decl(member_node).map(|m| m.name)
                }
                k if k == syntax_kind_ext::GET_ACCESSOR || k == syntax_kind_ext::SET_ACCESSOR => {
                    self.arena.get_accessor(member_node).map(|m| m.name)
                }
                _ => None,
            };
            if let Some(name_idx) = name_idx
                && let Some(name) = self.arena.get_identifier_text(name_idx)
            {
                names.insert(name.to_string());
            }
        }

        names
    }

    /// Get the set of property names already defined in an object literal.
    fn get_defined_properties(&self, object_literal_idx: NodeIndex) -> FxHashSet<String> {
        let mut names = FxHashSet::default();
        let node = self
            .arena
            .get(object_literal_idx)
            .expect("object_literal_idx must be valid in arena");

        if let Some(lit) = self.arena.get_literal_expr(node) {
            for &prop_idx in &lit.elements.nodes {
                if let Some(name) = self.get_property_name(prop_idx) {
                    names.insert(name);
                }
            }
        }
        names
    }

    /// Extract the property name from a property assignment or shorthand.
    fn get_property_name(&self, prop_idx: NodeIndex) -> Option<String> {
        let node = self.arena.get(prop_idx)?;
        match node.kind {
            k if k == syntax_kind_ext::PROPERTY_ASSIGNMENT => {
                let prop = self.arena.get_property_assignment(node)?;
                self.arena
                    .get_identifier_text(prop.name)
                    .map(std::string::ToString::to_string)
            }
            k if k == syntax_kind_ext::SHORTHAND_PROPERTY_ASSIGNMENT => {
                let prop = self.arena.get_shorthand_property(node)?;
                self.arena
                    .get_identifier_text(prop.name)
                    .map(std::string::ToString::to_string)
            }
            k if k == syntax_kind_ext::METHOD_DECLARATION => {
                let method = self.arena.get_method_decl(node)?;
                self.arena
                    .get_identifier_text(method.name)
                    .map(std::string::ToString::to_string)
            }
            _ => None,
        }
    }

    /// Walk up the AST to find the expected/contextual type for a node.
    pub(super) fn get_contextual_type(
        &self,
        node_idx: NodeIndex,
        checker: &mut CheckerState,
    ) -> Option<TypeId> {
        let ext = self.arena.get_extended(node_idx)?;
        let parent_idx = ext.parent;
        let parent = self.arena.get(parent_idx)?;

        match parent.kind {
            // const x: Type = { ... }
            k if k == syntax_kind_ext::VARIABLE_DECLARATION => {
                let decl = self.arena.get_variable_declaration(parent)?;
                if decl.initializer == node_idx && decl.type_annotation.is_some() {
                    return Some(checker.get_type_of_node(decl.type_annotation));
                }
            }
            // { prop: { ... } } -> Recurse to parent object
            k if k == syntax_kind_ext::PROPERTY_ASSIGNMENT => {
                let prop = self.arena.get_property_assignment(parent)?;
                if prop.initializer == node_idx {
                    let grand_parent_ext = self.arena.get_extended(parent_idx)?;
                    let grand_parent_idx = grand_parent_ext.parent;

                    // Get context of the parent object
                    let parent_context = self.get_contextual_type(grand_parent_idx, checker)?;

                    // Look up this property in the parent context
                    let prop_name = self.arena.get_identifier_text(prop.name)?;
                    return self.lookup_property_type(parent_context, prop_name, checker);
                }
            }
            // return { ... }
            k if k == syntax_kind_ext::RETURN_STATEMENT => {
                let func_idx = self.find_enclosing_function(parent_idx)?;
                let func_node = self.arena.get(func_idx)?;

                // Check return type annotation
                if let Some(func) = self.arena.get_function(func_node)
                    && func.type_annotation.is_some()
                {
                    return Some(checker.get_type_of_node(func.type_annotation));
                }
            }
            // function call argument: foo({ ... })
            k if k == syntax_kind_ext::CALL_EXPRESSION => {
                let call = self.arena.get_call_expr(parent)?;
                // Find which argument position this node is at
                let arg_index = call
                    .arguments
                    .as_ref()
                    .and_then(|args| args.nodes.iter().position(|&arg| arg == node_idx));

                if let Some(arg_idx) = arg_index {
                    // Get the function signature type
                    let func_type = checker.get_type_of_node(call.expression);
                    if let Some(param_type) =
                        self.get_parameter_type_at(func_type, arg_idx, checker)
                    {
                        return Some(param_type);
                    }

                    if let Some(sym_id) = self.resolve_member_target_symbol(call.expression) {
                        let symbol_type = checker.get_type_of_symbol(sym_id);
                        if let Some(param_type) =
                            self.get_parameter_type_at(symbol_type, arg_idx, checker)
                        {
                            return Some(param_type);
                        }
                    }
                }
            }
            // assignment expression: target = { ... }
            k if k == syntax_kind_ext::BINARY_EXPRESSION => {
                let binary = self.arena.get_binary_expr(parent)?;
                if binary.right == node_idx
                    && binary.operator_token == SyntaxKind::EqualsToken as u16
                {
                    return Some(checker.get_type_of_node(binary.left));
                }
            }
            _ => {}
        }
        None
    }

    /// Find the type of a property from an object type.
    fn lookup_property_type(
        &self,
        type_id: TypeId,
        name: &str,
        checker: &mut CheckerState,
    ) -> Option<TypeId> {
        let mut props = FxHashMap::default();
        let mut visited = FxHashSet::default();
        let interner = self.interner?;

        self.collect_properties_for_type(type_id, interner, checker, &mut visited, &mut props);
        props.get(name).map(|p| p.type_id)
    }

    /// Find the enclosing function for a node (for return type lookup).
    fn find_enclosing_function(&self, start_idx: NodeIndex) -> Option<NodeIndex> {
        let mut current = start_idx;
        while current.is_some() {
            let node = self.arena.get(current)?;
            if node.kind == syntax_kind_ext::FUNCTION_DECLARATION
                || node.kind == syntax_kind_ext::ARROW_FUNCTION
                || node.kind == syntax_kind_ext::FUNCTION_EXPRESSION
            {
                return Some(current);
            }
            let ext = self.arena.get_extended(current)?;
            current = ext.parent;
        }
        None
    }

    /// Get the type of the Nth parameter of a function type.
    fn get_parameter_type_at(
        &self,
        func_type: TypeId,
        param_index: usize,
        _checker: &mut CheckerState,
    ) -> Option<TypeId> {
        let interner = self.interner?;

        // Look up the callable signature
        if let Some(callable_id) = visitor::callable_shape_id(interner, func_type) {
            let callable = interner.callable_shape(callable_id);
            // Use the first call signature
            if let Some(first_sig) = callable.call_signatures.first()
                && param_index < first_sig.params.len()
            {
                return Some(first_sig.params[param_index].type_id);
            }
        }
        None
    }
}
