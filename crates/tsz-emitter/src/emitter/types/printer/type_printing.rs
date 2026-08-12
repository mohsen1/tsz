//! Composite type printing (unions, intersections, callables, etc.) for the `TypePrinter`.

use tsz_binder::SymbolId;
use tsz_common::interner::Atom;
use tsz_parser::parser::node::{NodeAccess, NodeArena};
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::{SyntaxKind, is_ecmascript_identifier_part};
use tsz_solver::computation::{
    TypeSubstitution, free_type_params_named, instantiate_type_cached, substitute_exact_types,
};
pub(crate) use tsz_solver::construction::JsSignatureDisplaySource;
pub(crate) use tsz_solver::type_queries::ts7_sort_order;
pub(crate) use tsz_solver::types::LiteralValue as SolverLiteralValue;
use tsz_solver::types::TypeId;
use tsz_solver::visitor;

use super::{
    TypePrinter, needs_property_name_quoting_with_flag, quote_property_name,
    quote_property_name_single,
};

#[path = "type_printing_composites.rs"]
mod type_printing_composites;
#[path = "type_printing_references.rs"]
mod type_printing_references;

/// Re-escape a cooked template-literal text span so it can be placed back
/// inside backtick delimiters. The solver stores the cooked value (e.g. a
/// real tab/newline for `\t`/`\n`), so this converts the characters that are
/// significant in a backtick-delimited context back into escape sequences,
/// analogous to `escape_string_for_double_quote` for double-quoted strings.
fn escape_text_for_backtick(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 4);
    let mut chars = s.chars().peekable();
    while let Some(ch) = chars.next() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '`' => out.push_str("\\`"),
            // Escape the `$` of a `${` sequence so it is not re-read as the
            // start of a substitution when the text round-trips.
            '$' if chars.peek() == Some(&'{') => out.push_str("\\$"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\0' => out.push_str("\\0"),
            c => out.push(c),
        }
    }
    out
}

impl<'a> TypePrinter<'a> {
    pub(crate) fn property_is_hidden_in_declaration_shape(
        &self,
        property: &tsz_solver::types::PropertyInfo,
    ) -> bool {
        let name = self.resolve_atom(property.name);
        // `prototype` and private brand markers are emitter-internal structural
        // details. Properties like `name` and `length` are only omitted by tsc
        // when they come from ambient function intrinsics, so we must not strip
        // them solely by raw property text here.
        name == "prototype" || name.starts_with("__private_brand_")
    }

    pub(crate) fn declaration_property_name_text(
        &self,
        property: &tsz_solver::types::PropertyInfo,
    ) -> String {
        if let Some(unique_symbol_name) = self.unique_symbol_property_name_text(property) {
            return unique_symbol_name;
        }

        let name = self.resolve_atom(property.name);
        if !property.is_string_named && name.starts_with('-') && name.parse::<f64>().is_ok() {
            return format!("[{name}]");
        }
        if needs_property_name_quoting_with_flag(&name, property.is_string_named) {
            if property.single_quoted_name {
                quote_property_name_single(&name)
            } else {
                quote_property_name(&name)
            }
        } else {
            name
        }
    }

    pub(crate) fn unique_symbol_property_name_text(
        &self,
        property: &tsz_solver::types::PropertyInfo,
    ) -> Option<String> {
        let name = self.resolve_atom(property.name);
        let symbol_id = name.strip_prefix("__unique_")?.parse::<u32>().ok()?;
        let qualified_name = self.resolve_symbol_qualified_name(SymbolId(symbol_id))?;
        Some(format!("[{qualified_name}]"))
    }

    pub(crate) fn synthesized_empty_shape_members(&self, sym_id: SymbolId) -> Option<Vec<String>> {
        let symbol_arena = self.symbol_arena?;
        let node_arena = self.node_arena?;
        let symbol = symbol_arena.get(sym_id)?;

        symbol.declarations.iter().copied().find_map(|decl_idx| {
            let decl_node = node_arena.get(decl_idx)?;
            let class_data = node_arena.get_class(decl_node)?;

            let members: Vec<String> = class_data
                .members
                .nodes
                .iter()
                .copied()
                .filter_map(|member_idx| self.synthesized_class_member_text(sym_id, member_idx))
                .collect();

            (!members.is_empty()).then_some(members)
        })
    }

    pub(crate) fn synthesized_class_member_text(
        &self,
        sym_id: SymbolId,
        member_idx: tsz_parser::NodeIndex,
    ) -> Option<String> {
        let node_arena = self.node_arena?;
        let member_node = node_arena.get(member_idx)?;
        let method = node_arena.get_method_decl(member_node)?;
        let name_idx = method.name;
        let name = self.render_name_node(node_arena, name_idx)?;
        let method_type = self.synthesized_method_type(member_idx, method)?;

        let mut property = tsz_solver::types::PropertyInfo::method(
            self.interner.intern_string(&name),
            method_type,
        );
        property.optional = method.question_token;
        property.parent_id = Some(sym_id);

        if self.computed_method_requires_property_syntax(&property, Some(sym_id)) {
            return Some(format!(
                "{}{}: {}",
                name,
                if property.optional { "?" } else { "" },
                self.print_type(property.type_id)
            ));
        }

        self.print_property_as_method(&property, Some(sym_id))
            .or_else(|| {
                Some(format!(
                    "{}{}: {}",
                    name,
                    if property.optional { "?" } else { "" },
                    self.print_type(property.type_id)
                ))
            })
    }

    pub(crate) fn synthesized_method_type(
        &self,
        member_idx: tsz_parser::NodeIndex,
        method: &tsz_parser::parser::node::MethodDeclData,
    ) -> Option<TypeId> {
        let cache = self.type_cache?;
        let candidate = cache
            .node_types
            .get(&member_idx.0)
            .copied()
            .or_else(|| cache.node_types.get(&method.name.0).copied())
            .unwrap_or(TypeId::ANY);

        if visitor::function_shape_id(self.interner, candidate).is_some()
            || visitor::callable_shape_id(self.interner, candidate).is_some()
        {
            return Some(candidate);
        }

        let return_type = self.widen_synthesized_method_return_type(candidate);
        let params = self.synthesized_method_params(&method.parameters);
        Some(
            self.interner
                .function(tsz_solver::types::FunctionShape::new(params, return_type)),
        )
    }

    pub(crate) fn synthesized_method_params(
        &self,
        params: &tsz_parser::parser::NodeList,
    ) -> Vec<tsz_solver::types::ParamInfo> {
        let Some(node_arena) = self.node_arena else {
            return Vec::new();
        };
        let cache = self.type_cache;

        params
            .nodes
            .iter()
            .copied()
            .filter_map(|param_idx| {
                let param_node = node_arena.get(param_idx)?;
                let param = node_arena.get_parameter(param_node)?;
                let name = node_arena
                    .get_identifier_text(param.name)
                    .map(|text| self.interner.intern_string(text));
                let type_id = cache
                    .and_then(|cache| {
                        cache
                            .node_types
                            .get(&param_idx.0)
                            .copied()
                            .or_else(|| cache.node_types.get(&param.name.0).copied())
                    })
                    .unwrap_or(TypeId::ANY);

                Some(tsz_solver::types::ParamInfo {
                    name,
                    type_id,
                    optional: param.question_token,
                    rest: param.dot_dot_dot_token,
                })
            })
            .collect()
    }

    pub(crate) fn widen_synthesized_method_return_type(&self, type_id: TypeId) -> TypeId {
        self.widen_synthesized_method_return_type_depth(type_id, 0)
    }

    fn widen_synthesized_method_return_type_depth(&self, type_id: TypeId, depth: usize) -> TypeId {
        if depth > 16 {
            return type_id;
        }
        match visitor::literal_value(self.interner, type_id) {
            Some(tsz_solver::types::LiteralValue::String(_)) => return TypeId::STRING,
            Some(tsz_solver::types::LiteralValue::Number(_)) => return TypeId::NUMBER,
            Some(tsz_solver::types::LiteralValue::Boolean(_)) => return TypeId::BOOLEAN,
            Some(tsz_solver::types::LiteralValue::BigInt(_)) => return TypeId::BIGINT,
            None => {}
        }

        if let Some(list_id) = visitor::union_list_id(self.interner, type_id) {
            let members = self.interner.type_list(list_id);
            let widened: Vec<_> = members
                .iter()
                .map(|&member| self.widen_synthesized_method_return_type_depth(member, depth + 1))
                .collect();
            return self.interner.union(widened);
        }

        if let Some(func_id) = visitor::function_shape_id(self.interner, type_id) {
            let mut shape = (*self.interner.function_shape(func_id)).clone();
            shape.return_type =
                self.widen_synthesized_method_return_type_depth(shape.return_type, depth + 1);
            // Re-interning must not drop an arity-only-optional display mask,
            // or the widened shape would regain a spurious `?` (#17238).
            return match self.interner.function_shape_arity_optional_mask(func_id) {
                Some(mask) => self
                    .interner
                    .function_with_arity_optional_mask(shape, &mask),
                None => self.interner.function(shape),
            };
        }

        if let Some(shape_id) = visitor::object_shape_id(self.interner, type_id)
            .or_else(|| visitor::object_with_index_shape_id(self.interner, type_id))
        {
            let mut shape = (*self.interner.object_shape(shape_id)).clone();
            for prop in &mut shape.properties {
                prop.type_id =
                    self.widen_synthesized_method_return_type_depth(prop.type_id, depth + 1);
                prop.write_type =
                    self.widen_synthesized_method_return_type_depth(prop.write_type, depth + 1);
            }
            if let Some(index) = &mut shape.string_index {
                index.value_type =
                    self.widen_synthesized_method_return_type_depth(index.value_type, depth + 1);
            }
            if let Some(index) = &mut shape.number_index {
                index.value_type =
                    self.widen_synthesized_method_return_type_depth(index.value_type, depth + 1);
            }
            return self.interner.object_with_index(shape);
        }

        if let Some(callable_id) = visitor::callable_shape_id(self.interner, type_id) {
            let mut shape = (*self.interner.callable_shape(callable_id)).clone();
            for sig in &mut shape.call_signatures {
                sig.return_type =
                    self.widen_synthesized_method_return_type_depth(sig.return_type, depth + 1);
            }
            for sig in &mut shape.construct_signatures {
                sig.return_type =
                    self.widen_synthesized_method_return_type_depth(sig.return_type, depth + 1);
            }
            for prop in &mut shape.properties {
                prop.type_id =
                    self.widen_synthesized_method_return_type_depth(prop.type_id, depth + 1);
                prop.write_type =
                    self.widen_synthesized_method_return_type_depth(prop.write_type, depth + 1);
            }
            if let Some(index) = &mut shape.string_index {
                index.value_type =
                    self.widen_synthesized_method_return_type_depth(index.value_type, depth + 1);
            }
            if let Some(index) = &mut shape.number_index {
                index.value_type =
                    self.widen_synthesized_method_return_type_depth(index.value_type, depth + 1);
            }
            return self.interner.callable(shape);
        }

        type_id
    }

    pub(crate) fn is_valid_identifier(name: &str) -> bool {
        crate::transforms::emit_utils::is_valid_identifier_name(name)
    }

    pub(crate) fn print_property_as_accessors(
        &self,
        property: &tsz_solver::types::PropertyInfo,
    ) -> Option<Vec<String>> {
        if property.is_method || !self.property_is_accessor(property) {
            return None;
        }

        let printed_name = self.declaration_property_name_text(property);

        let mut members = Vec::new();
        if property.type_id != TypeId::UNDEFINED {
            members.push(format!(
                "get {printed_name}(): {}",
                self.print_type(property.type_id)
            ));
        }
        if !property.readonly && property.write_type != TypeId::UNDEFINED {
            let param_name = self
                .setter_parameter_name_resolver
                .and_then(|resolve| resolve(&printed_name))
                .unwrap_or_else(|| "arg".to_string());
            members.push(format!(
                "set {printed_name}({param_name}: {})",
                self.print_type(property.write_type)
            ));
        }

        if members.is_empty() {
            return None;
        }

        Some(members)
    }

    pub(crate) fn declaration_property_type(
        &self,
        property: &tsz_solver::types::PropertyInfo,
    ) -> TypeId {
        if !property.readonly
            && property.type_id == TypeId::UNDEFINED
            && property.write_type != TypeId::UNDEFINED
        {
            property.write_type
        } else {
            property.type_id
        }
    }

    /// Check if a type is a union that contains `undefined` as a direct member.
    pub(crate) fn type_has_undefined_in_union(&self, type_id: TypeId) -> bool {
        if let Some(list_id) = visitor::union_list_id(self.interner, type_id) {
            self.interner
                .type_list(list_id)
                .contains(&TypeId::UNDEFINED)
        } else {
            type_id == TypeId::UNDEFINED
        }
    }

    fn optional_param_display_type(&self, type_id: TypeId) -> TypeId {
        if visitor::type_param_info(self.interner, type_id).is_some() {
            return self.interner.union2(type_id, TypeId::UNDEFINED);
        }

        let Some(list_id) = visitor::union_list_id(self.interner, type_id) else {
            return type_id;
        };
        let members = self.interner.type_list(list_id);
        if !members.contains(&TypeId::UNDEFINED) {
            return type_id;
        }

        let non_undefined = members
            .iter()
            .copied()
            .filter(|&member| member != TypeId::UNDEFINED)
            .collect::<Vec<_>>();

        if self.interner.get_union_origin(type_id).is_some() {
            return type_id;
        }

        if non_undefined.len() == 1
            && visitor::type_param_info(self.interner, non_undefined[0])
                .is_some_and(|info| info.default.is_some())
        {
            return type_id;
        }

        if non_undefined.len() == 1
            && visitor::function_shape_id(self.interner, non_undefined[0]).is_none()
            && visitor::callable_shape_id(self.interner, non_undefined[0]).is_none()
        {
            return non_undefined[0];
        }

        type_id
    }

    pub(crate) fn print_optional_param_type(&self, type_id: TypeId) -> String {
        let display_type = self.optional_param_display_type(type_id);
        let text = self.print_type(display_type);
        // Re-add `| undefined` only when `optional_param_display_type` left the
        // type unchanged AND the printed text is a bare type-parameter name.
        // When the display type differs from the input (i.e. `| undefined` was
        // already stripped from a synthesized union), we must NOT re-add it.
        if display_type == type_id && self.type_param_scope_contains_name(&text) {
            format!("{text} | undefined")
        } else {
            text
        }
    }

    pub(crate) fn property_is_accessor(&self, property: &tsz_solver::types::PropertyInfo) -> bool {
        if property.is_class_prototype {
            return true;
        }

        // Type-literal and interface accessors have no parent class symbol, so
        // the parent_id check below cannot detect them. Synthetic structural
        // setter-only properties use an undefined read type as an absent-read
        // sentinel; keep those in property-signature form so the write type is
        // used as the declaration surface.
        if property.has_split_accessor() && property.type_id != TypeId::UNDEFINED {
            return true;
        }

        let Some(parent_id) = property.parent_id else {
            return false;
        };
        let Some(symbol_arena) = self.symbol_arena else {
            return false;
        };
        let Some(node_arena) = self.node_arena else {
            return false;
        };
        let Some(parent_symbol) = symbol_arena.get(parent_id) else {
            return false;
        };

        parent_symbol
            .declarations
            .iter()
            .copied()
            .any(|decl_idx| self.declaration_declares_accessor(node_arena, decl_idx, property.name))
    }

    pub(crate) fn declaration_declares_accessor(
        &self,
        node_arena: &NodeArena,
        decl_idx: tsz_parser::NodeIndex,
        property_name: Atom,
    ) -> bool {
        let Some(decl_node) = node_arena.get(decl_idx) else {
            return false;
        };

        if let Some(class_data) = node_arena.get_class(decl_node) {
            return class_data.members.nodes.iter().copied().any(|member_idx| {
                self.member_is_accessor_named(node_arena, member_idx, property_name)
            });
        }

        if let Some(interface_data) = node_arena.get_interface(decl_node) {
            return interface_data
                .members
                .nodes
                .iter()
                .copied()
                .any(|member_idx| {
                    self.member_is_accessor_named(node_arena, member_idx, property_name)
                });
        }

        if let Some(type_alias) = node_arena.get_type_alias(decl_node)
            && let Some(type_node) = node_arena.get(type_alias.type_node)
            && let Some(type_literal) = node_arena.get_type_literal(type_node)
        {
            return type_literal
                .members
                .nodes
                .iter()
                .copied()
                .any(|member_idx| {
                    self.member_is_accessor_named(node_arena, member_idx, property_name)
                });
        }

        false
    }

    fn member_is_accessor_named(
        &self,
        node_arena: &NodeArena,
        member_idx: tsz_parser::NodeIndex,
        property_name: Atom,
    ) -> bool {
        let Some(member_node) = node_arena.get(member_idx) else {
            return false;
        };

        match member_node.kind {
            k if k == syntax_kind_ext::GET_ACCESSOR || k == syntax_kind_ext::SET_ACCESSOR => {
                node_arena
                    .get_accessor(member_node)
                    .is_some_and(|accessor| {
                        self.node_name_matches_atom(node_arena, accessor.name, property_name)
                    })
            }
            k if k == syntax_kind_ext::PROPERTY_DECLARATION => node_arena
                .get_property_decl(member_node)
                .is_some_and(|prop| {
                    node_arena.has_modifier(&prop.modifiers, SyntaxKind::AccessorKeyword)
                        && self.node_name_matches_atom(node_arena, prop.name, property_name)
                }),
            _ => false,
        }
    }

    pub(crate) fn find_member_name_node(
        &self,
        parent_id: Option<SymbolId>,
        property_name: Atom,
    ) -> Option<tsz_parser::NodeIndex> {
        let parent_id = parent_id?;
        let symbol_arena = self.symbol_arena?;
        let node_arena = self.node_arena?;
        let parent_symbol = symbol_arena.get(parent_id)?;

        parent_symbol
            .declarations
            .iter()
            .copied()
            .find_map(|decl_idx| {
                let decl_node = node_arena.get(decl_idx)?;

                if let Some(class_data) = node_arena.get_class(decl_node) {
                    return class_data
                        .members
                        .nodes
                        .iter()
                        .copied()
                        .find_map(|member_idx| {
                            self.member_name_matches_atom(node_arena, member_idx, property_name)
                        });
                }

                if let Some(iface) = node_arena.get_interface(decl_node) {
                    return iface.members.nodes.iter().copied().find_map(|member_idx| {
                        self.member_name_matches_atom(node_arena, member_idx, property_name)
                    });
                }

                if let Some(type_alias) = node_arena.get_type_alias(decl_node)
                    && let Some(type_node) = node_arena.get(type_alias.type_node)
                    && let Some(type_literal) = node_arena.get_type_literal(type_node)
                {
                    return type_literal
                        .members
                        .nodes
                        .iter()
                        .copied()
                        .find_map(|member_idx| {
                            self.member_name_matches_atom(node_arena, member_idx, property_name)
                        });
                }

                None
            })
    }

    pub(crate) fn member_name_matches_atom(
        &self,
        node_arena: &NodeArena,
        member_idx: tsz_parser::NodeIndex,
        property_name: Atom,
    ) -> Option<tsz_parser::NodeIndex> {
        let member_node = node_arena.get(member_idx)?;

        let name_idx = if let Some(method) = node_arena.get_method_decl(member_node) {
            Some(method.name)
        } else if let Some(accessor) = node_arena.get_accessor(member_node) {
            Some(accessor.name)
        } else {
            node_arena
                .get_property_decl(member_node)
                .map(|prop| prop.name)
        }?;

        self.node_name_matches_atom(node_arena, name_idx, property_name)
            .then_some(name_idx)
    }

    pub(crate) fn node_name_matches_atom(
        &self,
        node_arena: &NodeArena,
        name_idx: tsz_parser::NodeIndex,
        property_name: Atom,
    ) -> bool {
        self.render_name_node(node_arena, name_idx)
            .is_some_and(|rendered| rendered == self.resolve_atom(property_name))
    }

    pub(crate) fn render_name_node(
        &self,
        node_arena: &NodeArena,
        name_idx: tsz_parser::NodeIndex,
    ) -> Option<String> {
        let name_node = node_arena.get(name_idx)?;

        if let Some(ident) = node_arena.get_identifier(name_node) {
            return Some(node_arena.resolve_identifier_text(ident).to_string());
        }

        if let Some(computed) = node_arena.get_computed_property(name_node) {
            let expr = Self::render_name_expression(node_arena, computed.expression)?;
            return Some(format!("[{expr}]"));
        }

        if let Some(lit) = node_arena.get_literal(name_node) {
            return Some(lit.text.clone());
        }

        match name_node.kind {
            k if k == SyntaxKind::ThisKeyword as u16 => Some("this".to_string()),
            k if k == SyntaxKind::SuperKeyword as u16 => Some("super".to_string()),
            _ => None,
        }
    }

    pub(crate) fn render_name_expression(
        node_arena: &NodeArena,
        expr_idx: tsz_parser::NodeIndex,
    ) -> Option<String> {
        let expr_node = node_arena.get(expr_idx)?;

        if let Some(ident) = node_arena.get_identifier(expr_node) {
            return Some(node_arena.resolve_identifier_text(ident).to_string());
        }

        if let Some(access) = node_arena.get_access_expr(expr_node) {
            let base = Self::render_name_expression(node_arena, access.expression)?;
            let member = Self::render_name_expression(node_arena, access.name_or_argument)?;
            return Some(format!("{base}.{member}"));
        }

        if let Some(qname) = node_arena.get_qualified_name(expr_node) {
            let left = Self::render_name_expression(node_arena, qname.left)?;
            let right = Self::render_name_expression(node_arena, qname.right)?;
            return Some(format!("{left}.{right}"));
        }

        if let Some(lit) = node_arena.get_literal(expr_node) {
            return Some(lit.text.clone());
        }

        match expr_node.kind {
            k if k == SyntaxKind::ThisKeyword as u16 => Some("this".to_string()),
            k if k == SyntaxKind::SuperKeyword as u16 => Some("super".to_string()),
            _ => None,
        }
    }

    pub(crate) fn print_method_signature(
        &self,
        printed_name: &str,
        optional: bool,
        type_params: &[tsz_solver::types::TypeParamInfo],
        params: &[tsz_solver::types::ParamInfo],
        type_predicate: Option<&tsz_solver::types::TypePredicate>,
        return_type: TypeId,
    ) -> String {
        let mut result = String::new();
        result.push_str(printed_name);
        if optional {
            result.push('?');
        }

        let scoped = self.with_type_param_scope(type_params);
        if !type_params.is_empty() {
            let tps: Vec<String> = type_params
                .iter()
                .map(|tp| scoped.print_type_parameter_decl(tp))
                .collect();
            result.push('<');
            result.push_str(&tps.join(", "));
            result.push('>');
        }

        result.push('(');
        let mut first = true;
        for param in params {
            if !first {
                result.push_str(", ");
            }
            first = false;

            if param.rest {
                result.push_str("...");
            }
            if let Some(name) = param.name {
                result.push_str(&scoped.resolve_atom(name));
                if param.optional {
                    result.push('?');
                }
                result.push_str(": ");
            }
            if param.optional {
                result.push_str(&scoped.print_optional_param_type(param.type_id));
            } else {
                result.push_str(&scoped.print_type(param.type_id));
            }
        }
        result.push(')');

        result.push_str(": ");
        if let Some(pred) = type_predicate {
            result.push_str(&scoped.print_type_predicate(pred));
        } else {
            result.push_str(&scoped.print_type(return_type));
        }

        result
    }

    fn type_reference_base_is_nameable(&self, type_id: TypeId) -> bool {
        if let Some(def_id) = visitor::lazy_def_id(self.interner, type_id)
            && let Some(cache) = self.type_cache
        {
            if let Some(&sym_id) = cache.def_to_symbol.get(&def_id) {
                return self.is_symbol_visible(sym_id) || self.symbol_is_nameable(sym_id);
            }
            return cache.def_to_name.contains_key(&def_id);
        }

        if let Some(sym_ref) = visitor::type_query_symbol(self.interner, type_id) {
            let sym_id = SymbolId(sym_ref.0);
            return self.is_symbol_visible(sym_id) || self.symbol_is_nameable(sym_id);
        }

        if let Some(callable_id) = visitor::callable_shape_id(self.interner, type_id) {
            let callable = self.interner.callable_shape(callable_id);
            return callable.symbol.is_some_and(|sym_id| {
                self.is_symbol_visible(sym_id) || self.symbol_is_nameable(sym_id)
            });
        }

        visitor::object_shape_id(self.interner, type_id)
            .or_else(|| visitor::object_with_index_shape_id(self.interner, type_id))
            .and_then(|shape_id| self.interner.object_shape(shape_id).symbol)
            .is_some_and(|sym_id| self.is_symbol_visible(sym_id) || self.symbol_is_nameable(sym_id))
    }

    pub(crate) fn print_enum(&self, def_id: tsz_solver::def::DefId, _members_id: TypeId) -> String {
        // Try to resolve the enum name via DefId -> SymbolId -> symbol name
        if let Some(cache) = self.type_cache
            && let Some(&sym_id) = cache.def_to_symbol.get(&def_id)
            && let Some(name) = self.print_named_symbol_reference(sym_id, false)
        {
            return name;
        }
        // Fallback: print the member type structure
        format!("enum({})", def_id.0)
    }

    pub(crate) fn print_type_application(
        &self,
        app_id: tsz_solver::types::TypeApplicationId,
    ) -> String {
        let app = self.interner.type_application(app_id);
        if let Some(keyof_text) = self.print_keyof_alias_application(&app) {
            return keyof_text;
        }
        let base_def_id = visitor::lazy_def_id(self.interner, app.base);
        let base_text = if let Some(sym_ref) = visitor::type_query_symbol(self.interner, app.base) {
            let sym_id = SymbolId(sym_ref.0);
            self.print_named_symbol_reference(sym_id, false)
                .unwrap_or_else(|| self.print_type(app.base))
        } else if base_def_id.is_some_and(|def_id| {
            self.elided_local_alias_defs
                .is_some_and(|defs| defs.contains(&def_id))
        }) {
            // Print an elided-alias Lazy base without the bare-reference
            // intercept: the application itself decides between `Name<...>`
            // and the elided form below, based on how the base renders.
            let mut base_printer = self.clone();
            base_printer.elided_local_alias_defs = None;
            base_printer.print_type(app.base)
        } else {
            self.print_type(app.base)
        };
        if Self::is_parameters_utility_name(&base_text)
            && app.args.len() == 1
            && let Some(tuple_text) = self.print_parameters_utility_tuple(app.args[0])
        {
            return tuple_text;
        }

        // A function-local alias can never be referenced by name in a .d.ts;
        // when its application would render as `Name<...>`, print the elided
        // form instead. Keyed on the def, so same-named visible types and
        // literal text containing the name are unaffected.
        if let Some(def_id) = base_def_id
            && self.printed_as_elided_local_alias_name(def_id, &base_text)
        {
            let visible = self.elided_alias_visible_arg_count(app.base, &app.args);
            let mut visible_args = app.args.iter().copied().take(visible);
            return match (visible_args.next(), visible_args.next()) {
                (Some(only_arg), None) => {
                    format!(
                        "{} | {}",
                        self.print_type_argument(only_arg, true),
                        crate::ELIDED_ANY
                    )
                }
                _ => crate::ELIDED_ANY.to_string(),
            };
        }

        // A base that rendered as a bare fallback keyword cannot syntactically
        // carry type arguments, so drop them rather than emit invalid
        // `any<...>`. See `base_text_cannot_carry_type_arguments`.
        if app.args.is_empty() || Self::base_text_cannot_carry_type_arguments(&base_text) {
            base_text
        } else {
            // Print every type argument. `tsc` reconstructs a *computed* type
            // reference (`typeToTypeNodeHelper`) with all its arguments up to
            // arity and never trims a trailing one that happens to equal its
            // default — `const g = mk<number>()` emits `Box<number, number>`,
            // an unannotated `function* g() { yield* src(); }` emits
            // `Generator<unknown, void, any>`. Trailing defaults are only
            // dropped on the node-reuse path, where a *written* annotation is
            // copied verbatim (`const g: Generator<number>` stays
            // `Generator<number>`); that path never reaches this printer, so a
            // computed reference must render its full argument list.
            let args: Vec<String> = app
                .args
                .iter()
                .enumerate()
                .map(|(index, &id)| self.print_type_argument(id, index == 0))
                .collect();
            // `app.args` is non-empty here (the outer guard returns early
            // otherwise) and the map is 1:1, so `args` is always non-empty.
            format!("{base_text}<{}>", args.join(", "))
        }
    }

    /// True when a type-application base rendered as a bare fallback that cannot
    /// head a generic reference. A valid generic base is always an identifier,
    /// qualified name, or `import("...").Name`; when the printer instead
    /// truncates (depth cutoff → `any`), renders a bare intrinsic, or elides a
    /// self-referential alias (`crate::ELIDED_ANY`), the result is a non-generic
    /// keyword and appending `<...>` would emit invalid TypeScript — tsc renders
    /// a truncated or unnameable recursive application as the bare fallback,
    /// never as `<keyword><...>`.
    ///
    /// The keyword set is the *non-generic* subset of the bare renderings
    /// `print_intrinsic_type` produces. It deliberately omits the intrinsic
    /// renderings that are legitimate generic reference heads (`Promise`,
    /// `Function`) and the boolean-literal renderings (`true`/`false`); keep it
    /// in sync if a new non-generic intrinsic keyword is added there.
    ///
    /// (tsc additionally reports TS7056 for the *non-exported* form of this
    /// shape — a separate alias-retention gap tracked on #15983; this guard only
    /// keeps the emitted fallback syntactically valid.)
    fn base_text_cannot_carry_type_arguments(base_text: &str) -> bool {
        let trimmed = base_text.trim();
        trimmed == crate::ELIDED_ANY
            || matches!(
                trimmed,
                "any"
                    | "unknown"
                    | "never"
                    | "void"
                    | "undefined"
                    | "null"
                    | "object"
                    | "string"
                    | "number"
                    | "boolean"
                    | "symbol"
                    | "bigint"
            )
    }

    fn print_keyof_alias_application(
        &self,
        app: &tsz_solver::types::TypeApplication,
    ) -> Option<String> {
        let def_id = visitor::lazy_def_id(self.interner, app.base)?;
        let cache = self.type_cache?;
        let body = cache.def_types.get(&def_id.0).copied()?;
        let type_params = cache.def_type_params.get(&def_id.0)?;
        if type_params.len() != app.args.len() {
            return None;
        }
        let subst = TypeSubstitution::from_args(self.interner, type_params, &app.args);
        let instantiated = instantiate_type_cached(self.interner, None, body, &subst);
        visitor::keyof_inner_type(self.interner, instantiated)?;
        Some(self.print_type(instantiated))
    }

    fn is_parameters_utility_name(type_text: &str) -> bool {
        type_text
            .trim()
            .rsplit(['.', ' '])
            .next()
            .is_some_and(|name| name == "Parameters")
    }

    fn print_parameters_utility_tuple(&self, arg: TypeId) -> Option<String> {
        if let Some(func_id) = visitor::function_shape_id(self.interner, arg) {
            let func = self.interner.function_shape(func_id);
            // Arity-only-optional JS parameters print as required (#17238).
            let display_params = self
                .interner
                .display_params_for_function_shape(func_id, &func.params);
            return Some(self.print_parameters_tuple_elements(&display_params));
        }

        if let Some(callable_id) = visitor::callable_shape_id(self.interner, arg) {
            let callable = self.interner.callable_shape(callable_id);
            if callable.call_signatures.len() == 1 && callable.construct_signatures.is_empty() {
                return Some(
                    self.print_parameters_tuple_elements(&callable.call_signatures[0].params),
                );
            }
        }

        self.print_parameters_tuple_from_function_text(&self.print_type(arg))
    }

    fn print_parameters_tuple_elements(&self, params: &[tsz_solver::types::ParamInfo]) -> String {
        let mut parts = Vec::with_capacity(params.len());
        for param in params {
            let mut part = String::new();
            if let Some(name) = param.name {
                if param.rest {
                    part.push_str("...");
                }
                part.push_str(&self.resolve_atom(name));
                if param.optional {
                    part.push('?');
                }
                part.push_str(": ");
            } else if param.rest {
                part.push_str("...");
            }

            if param.optional {
                part.push_str(&self.print_optional_param_type(param.type_id));
            } else {
                part.push_str(&self.print_type(param.type_id));
            }
            if param.name.is_none() && param.optional && !param.rest {
                part.push('?');
            }
            parts.push(part);
        }

        format!("[{}]", parts.join(", "))
    }

    fn print_parameters_tuple_from_function_text(&self, type_text: &str) -> Option<String> {
        let trimmed = type_text.trim();
        let arrow_idx = Self::find_top_level_arrow_in_type_text(trimmed)?;
        let head = trimmed.get(..arrow_idx)?.trim_end();
        let open_idx = head.rfind('(')?;
        let params_text = head.get(open_idx + 1..)?.strip_suffix(')')?.trim();
        if params_text.is_empty() {
            return Some("[]".to_string());
        }
        let parts = Self::split_top_level_commas_in_type_text(params_text)
            .into_iter()
            .map(str::trim)
            .collect::<Vec<_>>();
        Some(format!("[{}]", parts.join(", ")))
    }

    fn find_top_level_arrow_in_type_text(text: &str) -> Option<usize> {
        let bytes = text.as_bytes();
        let mut paren_depth = 0usize;
        let mut bracket_depth = 0usize;
        let mut brace_depth = 0usize;
        let mut angle_depth = 0usize;
        let mut i = 0usize;
        while i + 1 < bytes.len() {
            match bytes[i] {
                b'(' => paren_depth += 1,
                b')' => paren_depth = paren_depth.saturating_sub(1),
                b'[' => bracket_depth += 1,
                b']' => bracket_depth = bracket_depth.saturating_sub(1),
                b'{' => brace_depth += 1,
                b'}' => brace_depth = brace_depth.saturating_sub(1),
                b'<' => angle_depth += 1,
                b'>' => angle_depth = angle_depth.saturating_sub(1),
                b'=' if bytes[i + 1] == b'>'
                    && paren_depth == 0
                    && bracket_depth == 0
                    && brace_depth == 0
                    && angle_depth == 0 =>
                {
                    return Some(i);
                }
                _ => {}
            }
            i += 1;
        }
        None
    }

    fn split_top_level_commas_in_type_text(text: &str) -> Vec<&str> {
        let mut parts = Vec::new();
        let mut start = 0usize;
        let mut paren_depth = 0usize;
        let mut bracket_depth = 0usize;
        let mut brace_depth = 0usize;
        let mut angle_depth = 0usize;

        for (idx, byte) in text.bytes().enumerate() {
            match byte {
                b'(' => paren_depth += 1,
                b')' => paren_depth = paren_depth.saturating_sub(1),
                b'[' => bracket_depth += 1,
                b']' => bracket_depth = bracket_depth.saturating_sub(1),
                b'{' => brace_depth += 1,
                b'}' => brace_depth = brace_depth.saturating_sub(1),
                b'<' => angle_depth += 1,
                b'>' => angle_depth = angle_depth.saturating_sub(1),
                b',' if paren_depth == 0
                    && bracket_depth == 0
                    && brace_depth == 0
                    && angle_depth == 0 =>
                {
                    if let Some(part) = text.get(start..idx) {
                        parts.push(part);
                    }
                    start = idx + 1;
                }
                _ => {}
            }
        }
        if let Some(part) = text.get(start..) {
            parts.push(part);
        }
        parts
    }

    /// Trailing-default-trimmed argument count, used *only* by the elided
    /// local-alias fallback above to decide between `X | any` and a bare `any`
    /// for an unnameable recursive alias (#16594). This is deliberately **not**
    /// applied to the ordinary `Name<...>` render path: `tsc` never trims a
    /// trailing type argument that equals its default on a computed type
    /// reference, so a nameable application prints its full argument list. Do
    /// not reintroduce this as a general arg-count policy.
    fn elided_alias_visible_arg_count(&self, base: TypeId, args: &[TypeId]) -> usize {
        let Some(type_params) = self.type_application_type_params(base) else {
            return args.len();
        };
        if type_params.len() < args.len() {
            return args.len();
        }

        let mut visible = args.len();
        while visible > 0 {
            let Some(default) = type_params.get(visible - 1).and_then(|param| param.default) else {
                break;
            };
            if args[visible - 1] != default {
                break;
            }
            visible -= 1;
        }
        visible
    }

    fn type_application_type_params(
        &self,
        base: TypeId,
    ) -> Option<&'a [tsz_solver::types::TypeParamInfo]> {
        let cache = self.type_cache?;
        if let Some(def_id) = visitor::lazy_def_id(self.interner, base) {
            return cache
                .def_type_params
                .get(&def_id.0)
                .map(std::vec::Vec::as_slice);
        }
        if let Some(sym_ref) = visitor::type_query_symbol(self.interner, base) {
            let sym_id = SymbolId(sym_ref.0);
            return cache
                .def_to_symbol
                .iter()
                .find_map(|(def_id, &candidate_sym_id)| {
                    (candidate_sym_id == sym_id).then_some(def_id)
                })
                .and_then(|def_id| {
                    cache
                        .def_type_params
                        .get(&def_id.0)
                        .map(std::vec::Vec::as_slice)
                });
        }
        None
    }

    pub(crate) fn print_type_argument(&self, type_id: TypeId, is_first: bool) -> String {
        let printed = self.print_type(type_id);

        if is_first
            && self.type_needs_parentheses_in_composition(type_id)
            && printed.trim_start().starts_with('<')
        {
            format!("({printed})")
        } else {
            printed
        }
    }

    pub(crate) fn print_conditional(
        &self,
        cond_id: tsz_solver::types::ConditionalTypeId,
    ) -> String {
        let cond = self.interner.conditional_type(cond_id);

        // The check type, true branch, and false branch are NOT in the extends
        // clause. Only the extends_type subtree should render `Infer(T)` as
        // `infer T`. Use scoped clones to avoid leaking the flag.
        let bare = self.leaving_extends_clause();
        let extends_scope = self.entering_extends_clause();

        // Check type needs parens when it's a conditional, function, constructor,
        // union, or intersection. Constructor/function types need parens because
        // their return type parsing greedily consumes the `extends` keyword.
        let check_str = bare.print_type(cond.check_type);
        let check_needs_parens = visitor::conditional_type_id(self.interner, cond.check_type)
            .is_some()
            || visitor::function_shape_id(self.interner, cond.check_type).is_some()
            || self.type_renders_as_constructor(cond.check_type)
            || visitor::union_list_id(self.interner, cond.check_type).is_some()
            || visitor::intersection_list_id(self.interner, cond.check_type).is_some();

        // Extends type needs parens when it's a conditional type or when it's
        // a function/constructor type whose return contains `extends` (i.e., a
        // conditional return type). Without parens the inner `extends` would be
        // mis-parsed as the outer conditional's extends clause.
        let extends_str = extends_scope.print_type(cond.extends_type);
        let extends_needs_parens = visitor::conditional_type_id(self.interner, cond.extends_type)
            .is_some()
            || self.function_like_has_conditional_return(cond.extends_type);

        let check = if check_needs_parens {
            format!("({check_str})")
        } else {
            check_str
        };
        let extends = if extends_needs_parens {
            format!("({extends_str})")
        } else {
            extends_str
        };

        format!(
            "{} extends {} ? {} : {}",
            check,
            extends,
            bare.print_type(cond.true_type),
            bare.print_type(cond.false_type),
        )
    }

    pub(crate) fn print_template_literal(
        &self,
        template_id: tsz_solver::types::TemplateLiteralId,
    ) -> String {
        let spans = self.interner.template_list(template_id);
        let mut result = String::from("`");

        for span in spans.iter() {
            match span {
                tsz_solver::types::TemplateSpan::Text(atom) => {
                    // Resolved atoms hold the *cooked* text span, so control
                    // characters and template-significant characters (`` ` ``,
                    // `\`, `${`) must be re-escaped for the backtick-delimited
                    // context — otherwise a cooked `\t`/`\r\n` would be written
                    // as a literal tab/newline.
                    result.push_str(&escape_text_for_backtick(&self.resolve_atom(*atom)));
                }
                tsz_solver::types::TemplateSpan::Type(type_id) => {
                    let printed = self.print_type(*type_id);
                    if printed.len() >= 2
                        && printed.starts_with('"')
                        && printed.ends_with('"')
                        && !printed[1..printed.len() - 1].contains('"')
                    {
                        result.push_str(&printed[1..printed.len() - 1]);
                        continue;
                    }
                    result.push_str("${");
                    result.push_str(&printed);
                    result.push('}');
                }
            }
        }

        result.push('`');
        result
    }

    pub(crate) fn print_mapped_type(&self, mapped_id: tsz_solver::types::MappedTypeId) -> String {
        let mapped = self.interner.mapped_type(mapped_id);

        let readonly_prefix = match mapped.readonly_modifier {
            Some(tsz_solver::types::MappedModifier::Add) => "+readonly ",
            Some(tsz_solver::types::MappedModifier::Remove) => "-readonly ",
            None => "",
        };

        let optional_suffix = match mapped.optional_modifier {
            Some(tsz_solver::types::MappedModifier::Add) => "+?",
            Some(tsz_solver::types::MappedModifier::Remove) => "-?",
            None => "",
        };

        let param_name = self.resolve_atom(mapped.type_param.name);
        let mut constraint = self.print_type(mapped.constraint);
        let recovered_as_clause = if mapped.name_type.is_none() {
            Self::split_recovered_mapped_as_clause(&constraint)
                .map(|(before, after)| (before.to_string(), after.to_string()))
        } else {
            None
        };
        let recovered_as_clause = recovered_as_clause.map(|(before, after)| {
            constraint = before;
            after
        });

        let mut nested = self.clone();
        if let Some(indent) = nested.indent_level {
            nested.indent_level = Some(indent + 1);
        }
        let template = nested.print_type(mapped.template);

        let as_clause = if let Some(name_type) = mapped.name_type {
            let name_type = self.print_type(name_type);
            format!(" as {}", Self::mapped_name_type_text(&name_type))
        } else if let Some(name_type) = recovered_as_clause {
            format!(" as {name_type}")
        } else {
            String::new()
        };

        // Multi-line format when indent context is set (matching tsc's .d.ts output)
        if let Some(indent) = self.indent_level {
            let member_indent = "    ".repeat((indent + 1) as usize);
            let closing_indent = "    ".repeat(indent as usize);
            format!(
                "{{\n{member_indent}{readonly_prefix}[{param_name} in {constraint}{as_clause}]{optional_suffix}: {template};\n{closing_indent}}}"
            )
        } else {
            format!(
                "{{ {readonly_prefix}[{param_name} in {constraint}{as_clause}]{optional_suffix}: {template}; }}"
            )
        }
    }

    fn trim_mapped_constraint_trailing_as(constraint: &str) -> &str {
        let trimmed = constraint.trim_end();
        let Some(before_as) = trimmed.strip_suffix("as") else {
            return trimmed;
        };

        let had_separator = before_as
            .chars()
            .next_back()
            .is_some_and(char::is_whitespace);
        let before_as = before_as.trim_end();
        let has_keyword_boundary = before_as
            .chars()
            .next_back()
            .is_some_and(|ch| !ch.is_ascii_alphanumeric() && ch != '_' && ch != '$');

        if had_separator || has_keyword_boundary {
            before_as
        } else {
            trimmed
        }
    }

    fn mapped_name_type_text(name_type: &str) -> &str {
        let mut name_type = Self::trim_mapped_constraint_trailing_as(name_type).trim_start();
        while let Some(after_as) = name_type.strip_prefix("as") {
            let has_keyword_boundary = after_as.chars().next().is_some_and(|ch| {
                ch.is_whitespace() || !Self::is_identifier_part_for_mapped_as(ch)
            });
            if !has_keyword_boundary {
                break;
            }
            name_type = after_as.trim_start();
        }
        name_type
    }

    fn split_recovered_mapped_as_clause(constraint: &str) -> Option<(&str, &str)> {
        for (idx, _) in constraint.match_indices("as") {
            let before = &constraint[..idx];
            let after = &constraint[idx + 2..];
            let before_boundary = before.chars().next_back().is_some_and(|ch| {
                ch.is_whitespace() || !Self::is_identifier_part_for_mapped_as(ch)
            });
            let after_boundary = after.chars().next().is_some_and(|ch| {
                ch.is_whitespace() || !Self::is_identifier_part_for_mapped_as(ch)
            });
            if before_boundary && after_boundary {
                return Some((before.trim_end(), after.trim_start()));
            }
        }
        None
    }

    fn is_identifier_part_for_mapped_as(ch: char) -> bool {
        is_ecmascript_identifier_part(ch)
    }

    pub(crate) fn print_index_access(&self, container: TypeId, index: TypeId) -> String {
        let container_str = self.print_type(container);
        // Parenthesize union, intersection, function, and conditional types in indexed access position
        // e.g., (A | B)[K], (A & B)[K], ((x: number) => void)[K],
        // (T extends U ? X : Y)[K]
        let needs_parens = visitor::union_list_id(self.interner, container).is_some()
            || visitor::intersection_list_id(self.interner, container).is_some()
            || visitor::function_shape_id(self.interner, container).is_some()
            || visitor::conditional_type_id(self.interner, container).is_some();
        if needs_parens {
            format!("({})[{}]", container_str, self.print_type(index))
        } else {
            format!("{}[{}]", container_str, self.print_type(index))
        }
    }

    pub(crate) fn print_string_intrinsic(
        &self,
        kind: tsz_solver::types::StringIntrinsicKind,
        type_arg: TypeId,
    ) -> String {
        let kind_name = match kind {
            tsz_solver::types::StringIntrinsicKind::Uppercase => "Uppercase",
            tsz_solver::types::StringIntrinsicKind::Lowercase => "Lowercase",
            tsz_solver::types::StringIntrinsicKind::Capitalize => "Capitalize",
            tsz_solver::types::StringIntrinsicKind::Uncapitalize => "Uncapitalize",
        };
        format!("{}<{}>", kind_name, self.print_type(type_arg))
    }

    /// Check if a type renders as a constructor type (`new (...) => T`).
    /// These need special parenthesization in conditional check/extends positions.
    pub(crate) fn type_renders_as_constructor(&self, type_id: TypeId) -> bool {
        let Some(callable_id) = visitor::callable_shape_id(self.interner, type_id) else {
            return false;
        };
        let callable = self.interner.callable_shape(callable_id);
        let has_properties = callable
            .properties
            .iter()
            .any(|property| !self.property_is_hidden_in_declaration_shape(property));
        // A callable renders as `new (...) => T` when it has a single construct
        // signature and no call signatures or extra members.
        callable.call_signatures.is_empty()
            && callable.construct_signatures.len() == 1
            && !has_properties
            && callable.string_index.is_none()
            && callable.number_index.is_none()
    }

    /// Check if a type is a function-like (`FunctionShape` or single-call-sig `Callable`)
    /// whose return type is a conditional type. Used to decide whether the type needs
    /// parentheses in the extends position of a conditional type.
    pub(crate) fn function_like_has_conditional_return(&self, type_id: TypeId) -> bool {
        if let Some(func_id) = visitor::function_shape_id(self.interner, type_id) {
            let func = self.interner.function_shape(func_id);
            return visitor::conditional_type_id(self.interner, func.return_type).is_some();
        }
        if let Some(callable_id) = visitor::callable_shape_id(self.interner, type_id) {
            let callable = self.interner.callable_shape(callable_id);
            // Only arrow-form callables (single call or single construct sig with
            // no extra members) would produce `extends` in the printed output.
            let has_properties = callable
                .properties
                .iter()
                .any(|property| !self.property_is_hidden_in_declaration_shape(property));
            if callable.call_signatures.len() == 1
                && callable.construct_signatures.is_empty()
                && !has_properties
                && callable.string_index.is_none()
                && callable.number_index.is_none()
            {
                return visitor::conditional_type_id(
                    self.interner,
                    callable.call_signatures[0].return_type,
                )
                .is_some();
            }
            if callable.call_signatures.is_empty()
                && callable.construct_signatures.len() == 1
                && !has_properties
                && callable.string_index.is_none()
                && callable.number_index.is_none()
            {
                return visitor::conditional_type_id(
                    self.interner,
                    callable.construct_signatures[0].return_type,
                )
                .is_some();
            }
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use tsz_solver::DefId;
    use tsz_solver::construction::TypeInterner;
    use tsz_solver::types::{IntrinsicKind, TupleElement, TypeId, TypeParamInfo};

    use super::TypePrinter;

    #[test]
    fn unscoped_type_parameter_prints_constraint_or_unknown() {
        let interner = TypeInterner::new();
        let s = interner.intern_string("S");

        let unconstrained = interner.type_param(TypeParamInfo {
            name: s,
            constraint: None,
            default: None,
            is_const: false,
            origin: tsz_solver::types::TypeParamOrigin::User,
        });
        assert_eq!(
            TypePrinter::new(&interner).print_type(unconstrained),
            "unknown"
        );

        let constrained = interner.type_param(TypeParamInfo {
            name: s,
            constraint: Some(TypeId::NUMBER),
            default: None,
            is_const: false,
            origin: tsz_solver::types::TypeParamOrigin::User,
        });
        assert_eq!(
            TypePrinter::new(&interner).print_type(constrained),
            "number"
        );

        assert_eq!(
            TypePrinter::replace_type_param_name_with_any("S[]", "S"),
            "any[]"
        );
    }

    #[test]
    fn type_param_intersection_with_empty_object_prints_as_non_nullable() {
        // Regression: tsc's truthy-narrowing of a type-parameter-typed
        // value yields `T & {}` structurally and renders it as the
        // alias `NonNullable<T>`. tsz constructs the same intersection
        // in narrowing without storing the alias on every code path,
        // so the printer must recover the spelling from the structural
        // shape (mirroring the diagnostic compound formatter).
        let interner = TypeInterner::new();
        let t_atom = interner.intern_string("T");
        let t = interner.type_param(TypeParamInfo {
            name: t_atom,
            constraint: None,
            default: None,
            is_const: false,
            origin: tsz_solver::types::TypeParamOrigin::User,
        });
        let empty = interner.object(Vec::new());

        // Mark `T` as visible in the printer scope so it renders as `T`
        // rather than its `unknown` fallback for unscoped type parameters.
        let printer = TypePrinter::new(&interner).with_outer_type_params(vec![t_atom]);
        let intersection = interner.intersection2(t, empty);
        assert_eq!(printer.print_type(intersection), "NonNullable<T>");

        let printer = TypePrinter::new(&interner).with_outer_type_params(vec![t_atom]);
        let intersection_swapped = interner.intersection2(empty, t);
        assert_eq!(printer.print_type(intersection_swapped), "NonNullable<T>");
    }

    #[test]
    fn boxed_object_intersection_prints_primitive_surface() {
        let interner = TypeInterner::new();
        let object_def = DefId(1);
        let object_type = interner.lazy(object_def);
        interner.set_boxed_type(IntrinsicKind::Object, object_type);
        interner.register_boxed_def_id(IntrinsicKind::Object, object_def);

        let text = TypePrinter::new(&interner)
            .print_type(interner.intersection(vec![object_type, TypeId::STRING]));
        assert_eq!(text, "string");

        let literal = interner.literal_string("def");
        let text = TypePrinter::new(&interner)
            .print_type(interner.intersection(vec![object_type, literal]));
        assert_eq!(text, "\"def\"");
    }

    #[test]
    fn degraded_application_base_drops_type_arguments() {
        // A type application whose base cannot be resolved to a nameable
        // reference renders the base as the bare `any` fallback. Appending the
        // type arguments would emit `any<number, string>`, which is not valid
        // TypeScript (`any` is not generic) — the printer must drop them and
        // keep just the fallback, matching tsc's rendering of a truncated or
        // unnameable recursive application.
        let interner = TypeInterner::new();
        let base = interner.lazy(DefId(9999));
        let app = interner.application(base, vec![TypeId::NUMBER, TypeId::STRING]);
        let printed = TypePrinter::new(&interner).print_type(app);
        assert!(
            !printed.contains('<'),
            "degraded base must not carry type arguments, got: {printed}"
        );
        assert_eq!(printed, "any");
    }

    #[test]
    fn base_text_cannot_carry_type_arguments_classifies_degraded_fallbacks() {
        assert!(TypePrinter::base_text_cannot_carry_type_arguments("any"));
        assert!(TypePrinter::base_text_cannot_carry_type_arguments(
            "unknown"
        ));
        assert!(TypePrinter::base_text_cannot_carry_type_arguments("never"));
        assert!(TypePrinter::base_text_cannot_carry_type_arguments(
            crate::ELIDED_ANY
        ));
        // Real, nameable reference heads must keep their type arguments.
        assert!(!TypePrinter::base_text_cannot_carry_type_arguments("Foo"));
        assert!(!TypePrinter::base_text_cannot_carry_type_arguments(
            "Promise"
        ));
        assert!(!TypePrinter::base_text_cannot_carry_type_arguments(
            "import(\"./m\").Bar"
        ));
        // A member named after a keyword (e.g. `ns.any`) is still a reference.
        assert!(!TypePrinter::base_text_cannot_carry_type_arguments(
            "ns.any"
        ));
    }

    #[test]
    fn mapped_constraint_trims_parser_recovered_as_keyword() {
        assert_eq!(
            TypePrinter::trim_mapped_constraint_trailing_as("T[number]as"),
            "T[number]"
        );
        assert_eq!(
            TypePrinter::trim_mapped_constraint_trailing_as("T[number] as"),
            "T[number]"
        );
        assert_eq!(
            TypePrinter::trim_mapped_constraint_trailing_as("Alias"),
            "Alias"
        );
        assert_eq!(
            TypePrinter::split_recovered_mapped_as_clause("T[number]as Item[Attr]"),
            Some(("T[number]", "Item[Attr]"))
        );
        assert_eq!(
            TypePrinter::mapped_name_type_text("as `get${Capitalize<string & K>}`"),
            "`get${Capitalize<string & K>}`"
        );
        assert_eq!(
            TypePrinter::mapped_name_type_text("as as `get${Capitalize<string & K>}`"),
            "`get${Capitalize<string & K>}`"
        );
        assert_eq!(TypePrinter::mapped_name_type_text("asserts T"), "asserts T");
    }

    #[test]
    fn optional_param_display_omits_synthesized_primitive_undefined() {
        let interner = TypeInterner::new();
        let separator = interner.intern_string("separator");
        let ty = interner.union2(TypeId::STRING, TypeId::UNDEFINED);
        let printed = TypePrinter::new(&interner).print_method_signature(
            "join",
            false,
            &[],
            &[tsz_solver::ParamInfo::optional(separator, ty)],
            None,
            TypeId::STRING,
        );
        assert_eq!(printed, "join(separator?: string): string");
    }

    #[test]
    fn optional_param_display_preserves_callback_undefined() {
        let interner = TypeInterner::new();
        let compare_fn = interner.intern_string("compareFn");
        let callback = interner.function(tsz_solver::FunctionShape::new(
            vec![
                tsz_solver::ParamInfo::required(interner.intern_string("a"), TypeId::NUMBER),
                tsz_solver::ParamInfo::required(interner.intern_string("b"), TypeId::NUMBER),
            ],
            TypeId::NUMBER,
        ));
        let ty = interner.union2(callback, TypeId::UNDEFINED);
        let printed = TypePrinter::new(&interner).print_method_signature(
            "sort",
            false,
            &[],
            &[tsz_solver::ParamInfo::optional(compare_fn, ty)],
            None,
            TypeId::VOID,
        );
        assert_eq!(
            printed,
            "sort(compareFn?: ((a: number, b: number) => number) | undefined): void"
        );
    }

    #[test]
    fn optional_param_display_preserves_explicit_union_origin() {
        let interner = TypeInterner::new();
        let value = interner.intern_string("value");
        let ty = interner.union2(TypeId::STRING, TypeId::UNDEFINED);
        interner.replace_union_origin_for_display(ty, vec![TypeId::STRING, TypeId::UNDEFINED]);
        let printed = TypePrinter::new(&interner).print_method_signature(
            "set",
            false,
            &[],
            &[tsz_solver::ParamInfo::optional(value, ty)],
            None,
            TypeId::VOID,
        );
        assert_eq!(printed, "set(value?: string | undefined): void");
    }

    #[test]
    fn optional_param_display_strips_plain_type_param_undefined() {
        let interner = TypeInterner::new();
        let t_name = interner.intern_string("T");
        let value = interner.intern_string("value");
        let t_param = tsz_solver::types::TypeParamInfo {
            name: t_name,
            constraint: None,
            default: None,
            is_const: false,
            origin: tsz_solver::types::TypeParamOrigin::User,
        };
        let t_type = interner.type_param(t_param);
        let ty = interner.union2(t_type, TypeId::UNDEFINED);
        let printed = TypePrinter::new(&interner).print_method_signature(
            "set",
            false,
            &[t_param],
            &[tsz_solver::ParamInfo::optional(value, ty)],
            None,
            TypeId::VOID,
        );
        assert_eq!(printed, "set<T>(value?: T): void");
    }

    #[test]
    fn optional_param_display_preserves_defaulted_type_param_undefined() {
        let interner = TypeInterner::new();
        let this_name = interner.intern_string("This");
        let this_arg = interner.intern_string("thisArg");
        let this_param = tsz_solver::types::TypeParamInfo {
            name: this_name,
            constraint: None,
            default: Some(TypeId::UNDEFINED),
            is_const: false,
            origin: tsz_solver::types::TypeParamOrigin::User,
        };
        let this_type = interner.type_param(this_param);
        let ty = interner.union2(this_type, TypeId::UNDEFINED);
        let printed = TypePrinter::new(&interner).print_method_signature(
            "flatMap",
            false,
            &[this_param],
            &[tsz_solver::ParamInfo::optional(this_arg, ty)],
            None,
            TypeId::VOID,
        );
        assert_eq!(
            printed,
            "flatMap<This = undefined>(thisArg?: This | undefined): void"
        );
    }

    #[test]
    fn labeled_tuple_typeids_print_compact_even_with_indent() {
        // Declaration AST tuple nodes own source trivia such as member JSDoc and
        // choose multiline output there. Solver tuple `TypeId`s only carry the
        // public tuple shape, so labels alone should not force multiline text.
        let interner = TypeInterner::new();
        let elem = interner.intern_string("elem");
        let index = interner.intern_string("index");
        let tuple = interner.tuple(vec![
            TupleElement {
                type_id: TypeId::OBJECT,
                name: Some(elem),
                optional: false,
                rest: false,
            },
            TupleElement {
                type_id: TypeId::NUMBER,
                name: Some(index),
                optional: false,
                rest: false,
            },
        ]);

        let printed = TypePrinter::new(&interner)
            .with_indent_level(1)
            .print_type(tuple);
        assert_eq!(printed, "[elem: object, index: number]");

        let nested = interner.tuple(vec![TupleElement {
            type_id: tuple,
            name: None,
            optional: false,
            rest: false,
        }]);
        let printed = TypePrinter::new(&interner)
            .with_indent_level(1)
            .print_type(nested);
        assert_eq!(printed, "[[elem: object, index: number]]");
    }
}
