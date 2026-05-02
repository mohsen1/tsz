//! Composite type printing (unions, intersections, callables, etc.) for the `TypePrinter`.

use tsz_binder::{SymbolId, symbol_flags};
use tsz_common::interner::Atom;
use tsz_parser::parser::node::{NodeAccess, NodeArena};
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;
use tsz_solver::types::TypeId;
use tsz_solver::visitor;

use super::{
    TypePrinter, needs_property_name_quoting_with_flag, quote_property_name,
    quote_property_name_single,
};

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
        match visitor::literal_value(self.interner, type_id) {
            Some(tsz_solver::types::LiteralValue::String(_)) => TypeId::STRING,
            Some(tsz_solver::types::LiteralValue::Number(_)) => TypeId::NUMBER,
            Some(tsz_solver::types::LiteralValue::Boolean(_)) => TypeId::BOOLEAN,
            Some(tsz_solver::types::LiteralValue::BigInt(_)) => TypeId::BIGINT,
            None => type_id,
        }
    }

    /// Check if a name is a valid JavaScript/TypeScript identifier
    /// (can be used in dot-access notation).
    pub(crate) fn is_valid_identifier(name: &str) -> bool {
        if name.is_empty() {
            return false;
        }
        let mut chars = name.chars();
        let first = chars.next().unwrap();
        if !first.is_ascii_alphabetic() && first != '_' && first != '$' {
            return false;
        }
        chars.all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '$')
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
            members.push(format!(
                "set {printed_name}(arg: {})",
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

    const fn optional_param_display_type(&self, type_id: TypeId) -> TypeId {
        type_id
    }

    pub(crate) fn property_is_accessor(&self, property: &tsz_solver::types::PropertyInfo) -> bool {
        if property.is_class_prototype {
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
            .any(|decl_idx| self.class_declares_accessor(node_arena, decl_idx, property.name))
    }

    pub(crate) fn class_declares_accessor(
        &self,
        node_arena: &NodeArena,
        decl_idx: tsz_parser::NodeIndex,
        property_name: Atom,
    ) -> bool {
        let Some(decl_node) = node_arena.get(decl_idx) else {
            return false;
        };
        let Some(class_data) = node_arena.get_class(decl_node) else {
            return false;
        };

        class_data.members.nodes.iter().copied().any(|member_idx| {
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
        })
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
            let display_type = if param.optional {
                scoped.optional_param_display_type(param.type_id)
            } else {
                param.type_id
            };
            result.push_str(&scoped.print_type(display_type));
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

    pub(crate) fn print_union(&self, type_list_id: tsz_solver::types::TypeListId) -> String {
        let types = self.interner.type_list(type_list_id);
        if types.is_empty() {
            return "never".to_string();
        }
        if let Some(enum_text) = self.try_print_enum_member_union_as_parent(&types) {
            return enum_text;
        }

        // Split members into real types vs nullish tail. tsc's type printer
        // emits the nullish members (`null`, `undefined`, `void`) at the end
        // of the union, in that canonical order — e.g. optional parameter
        // annotations render as `(X | undefined)[]`, not `(undefined | X)[]`.
        // Our solver stores unions in the order they were built, which for
        // `X | undefined` inferred from `a?: X[]` happens to be `undefined`
        // first. Re-ordering here keeps every other call site alone and
        // matches tsc's display without touching solver-level canonicalization.
        let mut real: Vec<TypeId> = Vec::with_capacity(types.len());
        let mut has_null = false;
        let mut has_undefined = false;
        let mut has_void = false;
        for &type_id in types.iter() {
            // When strictNullChecks is off, filter null/undefined/void from unions
            if !self.strict_null_checks
                && matches!(type_id, TypeId::NULL | TypeId::UNDEFINED | TypeId::VOID)
            {
                continue;
            }
            match type_id {
                TypeId::NULL => has_null = true,
                TypeId::UNDEFINED => has_undefined = true,
                TypeId::VOID => has_void = true,
                _ => real.push(type_id),
            }
        }

        // tsc's compareTypes orders union members by `getSortOrderFlags`,
        // which for primitives returns `TypeFlags` directly. Give primitive
        // intrinsics a stable, tsc-matching order (`any` < `unknown` < `string`
        // < `number` < `boolean` < `bigint` < `symbol`/`object`) so an inferred
        // `number | string` prints as `string | number`. Non-primitive members
        // keep their original relative order because a sort comparator that
        // returns "equal" for them is stable.
        const fn primitive_rank(id: TypeId) -> Option<u32> {
            // Mirrors tsc's TypeFlags bit values in ascending order.
            match id {
                TypeId::ANY => Some(1),
                TypeId::UNKNOWN => Some(2),
                TypeId::STRING => Some(4),
                TypeId::NUMBER => Some(8),
                TypeId::BOOLEAN => Some(16),
                TypeId::BIGINT => Some(64),
                TypeId::SYMBOL => Some(4096),
                TypeId::OBJECT => Some(33_554_432),
                _ => None,
            }
        }
        real.sort_by(|a, b| {
            // Keep non-primitive members in their original relative order; only
            // the known primitives get sorted among themselves. For a mixed
            // union like `MyAlias | string | number`, this reorders the
            // primitives into tsc order while leaving `MyAlias` in place.
            match (primitive_rank(*a), primitive_rank(*b)) {
                (Some(ra), Some(rb)) => ra.cmp(&rb),
                _ => std::cmp::Ordering::Equal,
            }
        });

        // tsc's compareTypes orders union members by TypeFlags; for the nullish
        // trio the flag values are Void < Undefined < Null, so the tail prints
        // `void | undefined | null` in that order when members are present.
        let mut ordered = real;
        if has_void {
            ordered.push(TypeId::VOID);
        }
        if has_undefined {
            ordered.push(TypeId::UNDEFINED);
        }
        if has_null {
            ordered.push(TypeId::NULL);
        }

        let mut parts = Vec::with_capacity(ordered.len());
        for type_id in ordered {
            let s = self.composition_member_text(type_id);
            // Parenthesize function/constructor types and conditional types in union position.
            // Conditional types need parens because `extends` binds more tightly than `|`:
            // `A | B extends C ? D : E` parses as `(A | B) extends C ? D : E`.
            if self.type_needs_parentheses_in_composition(type_id)
                || visitor::conditional_type_id(self.interner, type_id).is_some()
            {
                parts.push(format!("({s})"));
            } else {
                parts.push(s);
            }
        }

        // If all members were filtered out, the result is `any` (widened)
        if parts.is_empty() {
            return "any".to_string();
        }

        // Join with " | "
        parts.join(" | ")
    }

    pub(crate) fn try_print_enum_member_union_as_parent(&self, types: &[TypeId]) -> Option<String> {
        let cache = self.type_cache?;
        let symbols = self.symbol_arena?;
        let mut parent: Option<SymbolId> = None;
        for &type_id in types {
            let (def_id, _) = visitor::enum_components(self.interner, type_id)?;
            let member_sym = cache.def_to_symbol.get(&def_id).copied()?;
            let member = symbols.get(member_sym)?;
            if member.flags & symbol_flags::ENUM_MEMBER == 0 || !member.parent.is_some() {
                return None;
            }
            if let Some(existing) = parent {
                if existing != member.parent {
                    return None;
                }
            } else {
                parent = Some(member.parent);
            }
        }
        let parent = parent?;
        let parent_symbol = symbols.get(parent)?;
        let exports = parent_symbol.exports.as_ref()?;
        let enum_member_count = exports
            .iter()
            .filter(|(_, sym_id)| {
                symbols
                    .get(**sym_id)
                    .is_some_and(|symbol| symbol.flags & symbol_flags::ENUM_MEMBER != 0)
            })
            .count();
        if enum_member_count != types.len() {
            return None;
        }

        self.print_named_symbol_reference(parent, false)
    }

    pub(crate) fn print_intersection(&self, type_list_id: tsz_solver::types::TypeListId) -> String {
        let types = self.interner.type_list(type_list_id);
        if types.is_empty() {
            return "unknown".to_string(); // Intersection of 0 types is unknown
        }

        let mut members: Vec<(u8, String)> = Vec::with_capacity(types.len());
        for &type_id in types.iter() {
            let s = self.composition_member_text(type_id);
            // Parenthesize function/constructor types, union types, and conditional types
            // in intersection position.
            // Union types need parens because `&` binds tighter than `|`:
            // `(A | B) & C` is different from `A | B & C`.
            // Conditional types need parens for the same precedence reason.
            let needs_parens = self.type_needs_parentheses_in_composition(type_id)
                || visitor::union_list_id(self.interner, type_id).is_some()
                || visitor::conditional_type_id(self.interner, type_id).is_some();
            if needs_parens {
                members.push((self.intersection_member_priority(type_id), format!("({s})")));
            } else {
                members.push((self.intersection_member_priority(type_id), s));
            }
        }
        members.sort_by_key(|(priority, _)| *priority);

        // Join with " & "
        members
            .into_iter()
            .map(|(_, text)| text)
            .collect::<Vec<_>>()
            .join(" & ")
    }

    pub(crate) fn print_tuple(&self, tuple_id: tsz_solver::types::TupleListId) -> String {
        let elements = self.interner.tuple_list(tuple_id);

        if elements.is_empty() {
            return "[]".to_string();
        }

        let mut parts = Vec::with_capacity(elements.len());
        for elem in elements.iter() {
            let mut part = String::new();

            // Handle labeled tuple members (e.g., [name: string])
            if let Some(name) = elem.name {
                part.push_str(&self.resolve_atom(name));
                // Optional marker comes after the label for labeled tuples
                if elem.optional {
                    part.push('?');
                }
                part.push_str(": ");
            }

            // Rest parameter prefix
            if elem.rest {
                part.push_str("...");
                // For unlabeled rest+optional elements, tsc places ? before the type: [...?T]
                if elem.name.is_none() && elem.optional {
                    part.push('?');
                }
            }

            // Type annotation
            part.push_str(&self.print_type(elem.type_id));

            // Optional marker for unlabeled non-rest tuples (comes after type): [T?]
            if elem.name.is_none() && elem.optional && !elem.rest {
                part.push('?');
            }

            parts.push(part);
        }

        if let Some(indent) = self.indent_level
            && elements.iter().any(|elem| elem.name.is_some())
        {
            let member_indent = "    ".repeat((indent + 1) as usize);
            let closing_indent = "    ".repeat(indent as usize);
            let lines: Vec<String> = parts
                .iter()
                .map(|part| format!("{member_indent}{part}"))
                .collect();
            return format!("[\n{}\n{closing_indent}]", lines.join(",\n"));
        }

        format!("[{}]", parts.join(", "))
    }

    pub(crate) fn print_function_type(
        &self,
        func_id: tsz_solver::types::FunctionShapeId,
    ) -> String {
        let func_shape = self.interner.function_shape(func_id);
        let scoped = self.with_type_param_scope(&func_shape.type_params);
        let type_params_str = if !func_shape.type_params.is_empty() {
            let params: Vec<String> = func_shape
                .type_params
                .iter()
                .map(|tp| scoped.print_type_parameter_decl(tp))
                .collect();
            format!("<{}>", params.join(", "))
        } else {
            String::new()
        };

        // Parameters
        let mut params = Vec::new();
        for param in &func_shape.params {
            let mut param_str = String::new();

            // Rest parameter
            if param.rest {
                param_str.push_str("...");
            }

            // Parameter name (optional in function types)
            if let Some(name) = param.name {
                param_str.push_str(&scoped.resolve_atom(name));
                if param.optional {
                    param_str.push('?');
                }
                param_str.push_str(": ");
            }

            let display_type = if param.optional {
                scoped.optional_param_display_type(param.type_id)
            } else {
                param.type_id
            };
            param_str.push_str(&scoped.print_type(display_type));

            params.push(param_str);
        }

        let return_str = if let Some(ref pred) = func_shape.type_predicate {
            scoped.print_type_predicate(pred)
        } else {
            scoped.print_type(func_shape.return_type)
        };

        format!(
            "{}({}) => {}",
            type_params_str,
            params.join(", "),
            return_str
        )
    }

    pub(crate) fn print_callable(&self, callable_id: tsz_solver::types::CallableShapeId) -> String {
        let callable = self.interner.callable_shape(callable_id);

        // For class constructor types with a visible symbol, use `typeof ClassName` form.
        // This matches tsc's behavior for declaration emit.
        if !callable.construct_signatures.is_empty()
            && let Some(sym_id) = callable.symbol
            && (self.is_symbol_visible(sym_id) || self.symbol_is_nameable(sym_id))
            && let Some(name) = self.resolve_symbol_qualified_name(sym_id)
        {
            return format!("typeof {name}");
        }

        // Simple callable: one call signature, no properties/construct/index sigs
        // → use arrow function syntax: (params) => ReturnType
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
            return self.print_call_signature_arrow(&callable.call_signatures[0]);
        }

        // Simple constructor callable: one construct signature, no other members
        // → use `new (...) => T` (or `abstract new (...) => T`) syntax. This matches
        // tsc's declaration-emit form for `new (...args) => T` written explicitly
        // as a constructor type in source (e.g. `Record<string, new (...) => T>`
        // or in the extends clause of a conditional). For anonymous and named
        // class constructor types (which carry `symbol: Some(_)`), tsc keeps
        // the structural `{ new (): T }` object-literal form, so we leave those
        // to fall through to the multi-line rendering below.
        if callable.symbol.is_none()
            && callable.call_signatures.is_empty()
            && callable.construct_signatures.len() == 1
            && !has_properties
            && callable.string_index.is_none()
            && callable.number_index.is_none()
        {
            return self.print_construct_signature_arrow(
                &callable.construct_signatures[0],
                callable.is_abstract,
            );
        }
        // Abstract constructor callables historically used the arrow form even
        // when they carried a synthetic class symbol; preserve that to avoid
        // regressing `abstract new () => { ... }` cases.
        if callable.is_abstract
            && callable.call_signatures.is_empty()
            && callable.construct_signatures.len() == 1
            && !has_properties
            && callable.string_index.is_none()
            && callable.number_index.is_none()
        {
            return self.print_construct_signature_arrow(
                &callable.construct_signatures[0],
                callable.is_abstract,
            );
        }

        // Collect all signatures (call + construct)
        let mut parts = Vec::new();

        for sig in &callable.call_signatures {
            parts.push(self.print_call_signature(sig, false, false));
        }
        for sig in &callable.construct_signatures {
            parts.push(self.print_call_signature(sig, true, callable.is_abstract));
        }

        // Add index signatures (tsc emits these before properties)
        if let Some(ref idx) = callable.string_index {
            let readonly = if idx.readonly { "readonly " } else { "" };
            let param = idx
                .param_name
                .map(|a| self.resolve_atom(a))
                .unwrap_or_else(|| "x".to_string());
            parts.push(format!(
                "{}[{}: string]: {}",
                readonly,
                param,
                self.print_type(idx.value_type)
            ));
        }
        if let Some(ref idx) = callable.number_index {
            let readonly = if idx.readonly { "readonly " } else { "" };
            let param = idx
                .param_name
                .map(|a| self.resolve_atom(a))
                .unwrap_or_else(|| "x".to_string());
            parts.push(format!(
                "{}[{}: number]: {}",
                readonly,
                param,
                self.print_type(idx.value_type)
            ));
        }

        // Add properties (filter out internal props tsc strips from .d.ts)
        for prop in &callable.properties {
            if self.property_is_hidden_in_declaration_shape(prop) {
                continue;
            }

            // Try to emit as method syntax if the property is a method
            if prop.is_method
                && let Some(method_str) = self.print_property_as_method(prop, callable.symbol)
            {
                parts.push(method_str);
                continue;
            }

            if let Some(accessors) = self.print_property_as_accessors(prop) {
                parts.extend(accessors);
                continue;
            }

            let readonly = if prop.readonly { "readonly " } else { "" };
            let optional = if prop.optional { "?" } else { "" };
            parts.push(format!(
                "{}{}{}: {}",
                readonly,
                self.declaration_property_name_text(prop),
                optional,
                self.print_type(prop.type_id)
            ));
        }

        if parts.is_empty() {
            return "{}".to_string();
        }

        // Multi-line format when indent context is set
        if let Some(indent) = self.indent_level {
            let member_indent = "    ".repeat((indent + 1) as usize);
            let closing_indent = "    ".repeat(indent as usize);
            let lines: Vec<String> = parts
                .iter()
                .map(|p| format!("{member_indent}{p};"))
                .collect();
            format!("{{\n{}\n{}}}", lines.join("\n"), closing_indent)
        } else {
            format!("{{ {} }}", parts.join("; "))
        }
    }

    pub(crate) fn print_call_signature(
        &self,
        sig: &tsz_solver::types::CallSignature,
        is_construct: bool,
        is_abstract: bool,
    ) -> String {
        let prefix = if is_construct && is_abstract {
            "abstract new "
        } else if is_construct {
            "new "
        } else {
            ""
        };

        let scoped = self.with_type_param_scope(&sig.type_params);
        let type_params_str = if !sig.type_params.is_empty() {
            let params: Vec<String> = sig
                .type_params
                .iter()
                .map(|tp| scoped.print_type_parameter_decl(tp))
                .collect();
            format!("<{}>", params.join(", "))
        } else {
            String::new()
        };

        let mut params = Vec::new();
        for param in &sig.params {
            let mut param_str = String::new();
            if param.rest {
                param_str.push_str("...");
            }
            if let Some(name) = param.name {
                param_str.push_str(&scoped.resolve_atom(name));
                if param.optional {
                    param_str.push('?');
                }
                param_str.push_str(": ");
            }
            let display_type = if param.optional {
                scoped.optional_param_display_type(param.type_id)
            } else {
                param.type_id
            };
            param_str.push_str(&scoped.print_type(display_type));
            params.push(param_str);
        }

        let mut nested = scoped.clone();
        if let Some(indent) = nested.indent_level {
            nested.indent_level = Some(indent + 1);
        }
        let return_str = if let Some(ref pred) = sig.type_predicate {
            nested.print_type_predicate(pred)
        } else {
            nested.print_type(sig.return_type)
        };
        format!(
            "{}{}({}): {}",
            prefix,
            type_params_str,
            params.join(", "),
            return_str
        )
    }

    /// Print a call signature in arrow function syntax: (params) => `ReturnType`
    pub(crate) fn print_call_signature_arrow(
        &self,
        sig: &tsz_solver::types::CallSignature,
    ) -> String {
        let scoped = self.with_type_param_scope(&sig.type_params);
        let type_params_str = if !sig.type_params.is_empty() {
            let params: Vec<String> = sig
                .type_params
                .iter()
                .map(|tp| scoped.print_type_parameter_decl(tp))
                .collect();
            format!("<{}>", params.join(", "))
        } else {
            String::new()
        };

        let mut params = Vec::new();
        for param in &sig.params {
            let mut param_str = String::new();
            if param.rest {
                param_str.push_str("...");
            }
            if let Some(name) = param.name {
                param_str.push_str(&scoped.resolve_atom(name));
                if param.optional {
                    param_str.push('?');
                }
                param_str.push_str(": ");
            }
            let display_type = if param.optional {
                scoped.optional_param_display_type(param.type_id)
            } else {
                param.type_id
            };
            param_str.push_str(&scoped.print_type(display_type));
            params.push(param_str);
        }

        let mut nested = scoped.clone();
        if let Some(indent) = nested.indent_level {
            nested.indent_level = Some(indent + 1);
        }
        let return_str = if let Some(ref pred) = sig.type_predicate {
            nested.print_type_predicate(pred)
        } else {
            nested.print_type(sig.return_type)
        };
        format!(
            "{}({}) => {}",
            type_params_str,
            params.join(", "),
            return_str
        )
    }

    pub(crate) fn print_construct_signature_arrow(
        &self,
        sig: &tsz_solver::types::CallSignature,
        is_abstract: bool,
    ) -> String {
        let scoped = self.with_type_param_scope(&sig.type_params);
        let type_params_str = if !sig.type_params.is_empty() {
            let params: Vec<String> = sig
                .type_params
                .iter()
                .map(|tp| scoped.print_type_parameter_decl(tp))
                .collect();
            format!("<{}>", params.join(", "))
        } else {
            String::new()
        };

        let mut params = Vec::new();
        for param in &sig.params {
            let mut param_str = String::new();
            if param.rest {
                param_str.push_str("...");
            }
            if let Some(name) = param.name {
                param_str.push_str(&scoped.resolve_atom(name));
                if param.optional {
                    param_str.push('?');
                }
                param_str.push_str(": ");
            }
            let display_type = if param.optional {
                scoped.optional_param_display_type(param.type_id)
            } else {
                param.type_id
            };
            param_str.push_str(&scoped.print_type(display_type));
            params.push(param_str);
        }

        let mut nested = scoped.clone();
        if let Some(indent) = nested.indent_level {
            nested.indent_level = Some(indent.saturating_sub(2));
        }
        let return_str = if let Some(ref pred) = sig.type_predicate {
            nested.print_type_predicate(pred)
        } else {
            nested.print_type(sig.return_type)
        };

        let prefix = if is_abstract { "abstract new " } else { "new " };
        format!(
            "{prefix}{}({}) => {}",
            type_params_str,
            params.join(", "),
            return_str
        )
    }

    pub(crate) fn type_needs_parentheses_in_composition(&self, type_id: TypeId) -> bool {
        if visitor::function_shape_id(self.interner, type_id).is_some() {
            return true;
        }

        let Some(callable_id) = visitor::callable_shape_id(self.interner, type_id) else {
            return false;
        };
        let callable = self.interner.callable_shape(callable_id);
        let has_properties = callable
            .properties
            .iter()
            .any(|property| !self.property_is_hidden_in_declaration_shape(property));

        callable.symbol.is_none()
            && !has_properties
            && callable.string_index.is_none()
            && callable.number_index.is_none()
            && (callable.call_signatures.len() == 1
                || (callable.call_signatures.is_empty()
                    && callable.construct_signatures.len() == 1))
    }

    pub(crate) fn composition_member_text(&self, type_id: TypeId) -> String {
        let Some(callable_id) = visitor::callable_shape_id(self.interner, type_id) else {
            return self.print_type(type_id);
        };
        let callable = self.interner.callable_shape(callable_id);
        let has_properties = callable
            .properties
            .iter()
            .any(|property| !self.property_is_hidden_in_declaration_shape(property));

        if callable.symbol.is_none()
            && !has_properties
            && callable.string_index.is_none()
            && callable.number_index.is_none()
            && callable.call_signatures.is_empty()
            && callable.construct_signatures.len() == 1
        {
            return self.print_construct_signature_arrow(
                &callable.construct_signatures[0],
                callable.is_abstract,
            );
        }

        self.print_type(type_id)
    }

    /// Print a type predicate (e.g., `x is string`, `asserts x is string`, `this is Foo`)
    pub(crate) fn print_type_predicate(&self, pred: &tsz_solver::types::TypePredicate) -> String {
        let mut result = String::new();
        if pred.asserts {
            result.push_str("asserts ");
        }
        match &pred.target {
            tsz_solver::types::TypePredicateTarget::This => result.push_str("this"),
            tsz_solver::types::TypePredicateTarget::Identifier(atom) => {
                result.push_str(&self.resolve_atom(*atom));
            }
        }
        if let Some(type_id) = pred.type_id {
            result.push_str(" is ");
            result.push_str(&self.print_type(type_id));
        }
        result
    }

    /// Print a type parameter as a type reference (just the name).
    pub(crate) fn print_type_parameter(
        &self,
        param_info: &tsz_solver::types::TypeParamInfo,
    ) -> String {
        self.resolve_type_param_name(param_info.name)
    }

    /// Print a type parameter declaration with constraint and default.
    /// Used in `<T extends Foo = Bar>` positions.
    pub(crate) fn print_type_parameter_decl(
        &self,
        param_info: &tsz_solver::types::TypeParamInfo,
    ) -> String {
        let mut result = String::new();

        if param_info.is_const {
            result.push_str("const ");
        }

        result.push_str(&self.resolve_type_param_name(param_info.name));

        if let Some(constraint) = param_info.constraint {
            result.push_str(" extends ");
            result.push_str(&self.print_type(constraint));
        }

        if let Some(default) = param_info.default {
            result.push_str(" = ");
            result.push_str(&self.print_type(default));
        }

        result
    }

    pub(crate) fn print_lazy_type(&self, def_id: tsz_solver::def::DefId) -> String {
        // Check recursion depth
        if self.current_depth >= self.max_depth {
            return "any".to_string();
        }

        // Try to get the SymbolId for this DefId using TypeCache
        let sym_id = if let Some(cache) = self.type_cache {
            cache.def_to_symbol.get(&def_id).copied()
        } else {
            None
        };

        // If the symbol is a global lib type (e.g. Promise) that is NOT in
        // the current file's symbol arena (multi-file tests: each file has its
        // own binder without lib symbols merged), fall back to the pre-built
        // name map from TypeCache so we can still emit "Promise" instead of "any".
        if let Some(sym_id) = sym_id
            && self.symbol_arena.is_some_and(|a| a.get(sym_id).is_none())
        {
            if let Some(name) = self.type_cache.and_then(|c| c.def_to_name.get(&def_id)) {
                return name.clone();
            }
        }

        // If we have a symbol and it's visible/global, use the name. Otherwise
        // fall back to an import-qualified reference when the emitter can
        // resolve the owning module specifier.
        if let Some(sym_id) = sym_id
            && let Some(arena) = self.symbol_arena
            && let Some(symbol) = arena.get(sym_id)
        {
            // Lazy(DefId) for value-space entities (enums, modules, functions) represents
            // the VALUE side of the symbol. In .d.ts output, these must be prefixed with
            // `typeof` to distinguish from the type-side meaning.
            // E.g., `var x = MyEnum` → `declare var x: typeof MyEnum;`
            // The type-side meaning (e.g., enum member union) uses Enum(DefId, members)
            // and is handled by print_enum, not print_lazy_type.
            let needs_typeof = symbol.has_any_flags(
                symbol_flags::ENUM | symbol_flags::VALUE_MODULE | symbol_flags::FUNCTION,
            );
            if !needs_typeof
                && !self.symbol_is_import_qualifiable(sym_id)
                && !self.is_global_like_symbol(sym_id)
                && let Some(symbol_type) = self
                    .def_type_fallback(def_id)
                    .or_else(|| self.symbol_type_fallback(sym_id))
                && visitor::lazy_def_id(self.interner, symbol_type) != Some(def_id)
                && !self.type_contains_lazy_def(symbol_type, def_id, 0)
            {
                let mut nested = self.clone();
                nested.current_depth += 1;
                return nested.print_type(symbol_type);
            }
            if let Some(name) = self.print_named_symbol_reference(sym_id, needs_typeof) {
                return name;
            }
            // Preserve canonical names for global-like symbols when visibility
            // heuristics fail (e.g. utility aliases like `Extract`, `FlatArray`).
            if !needs_typeof && self.is_global_like_symbol(sym_id) {
                return symbol.escaped_name.clone();
            }
        }

        if let Some(name) = self
            .type_cache
            .and_then(|cache| cache.def_to_name.get(&def_id))
        {
            return name.clone();
        }

        // Symbol is not visible or we don't have symbol info.
        // Fallback to `any` when we cannot legally name the referenced type.
        "any".to_string()
    }

    pub(crate) fn type_contains_lazy_def(
        &self,
        type_id: TypeId,
        target_def: tsz_solver::def::DefId,
        depth: u32,
    ) -> bool {
        if depth > 64 {
            return true;
        }

        if visitor::lazy_def_id(self.interner, type_id) == Some(target_def) {
            return true;
        }

        if let Some(app_id) = visitor::application_id(self.interner, type_id) {
            let app = self.interner.type_application(app_id);
            return self.type_contains_lazy_def(app.base, target_def, depth + 1)
                || app
                    .args
                    .iter()
                    .copied()
                    .any(|arg| self.type_contains_lazy_def(arg, target_def, depth + 1));
        }

        if let Some(list_id) = visitor::union_list_id(self.interner, type_id)
            .or_else(|| visitor::intersection_list_id(self.interner, type_id))
        {
            return self
                .interner
                .type_list(list_id)
                .iter()
                .copied()
                .any(|member| self.type_contains_lazy_def(member, target_def, depth + 1));
        }

        if let Some(elem_id) = visitor::array_element_type(self.interner, type_id) {
            return self.type_contains_lazy_def(elem_id, target_def, depth + 1);
        }

        if let Some(tuple_id) = visitor::tuple_list_id(self.interner, type_id) {
            return self
                .interner
                .tuple_list(tuple_id)
                .iter()
                .any(|elem| self.type_contains_lazy_def(elem.type_id, target_def, depth + 1));
        }

        if let Some(func_id) = visitor::function_shape_id(self.interner, type_id) {
            let func = self.interner.function_shape(func_id);
            return func.type_params.iter().any(|tp| {
                tp.constraint.is_some_and(|constraint| {
                    self.type_contains_lazy_def(constraint, target_def, depth + 1)
                }) || tp.default.is_some_and(|default| {
                    self.type_contains_lazy_def(default, target_def, depth + 1)
                })
            }) || func
                .params
                .iter()
                .any(|param| self.type_contains_lazy_def(param.type_id, target_def, depth + 1))
                || func.type_predicate.as_ref().is_some_and(|pred| {
                    pred.type_id.is_some_and(|type_id| {
                        self.type_contains_lazy_def(type_id, target_def, depth + 1)
                    })
                })
                || self.type_contains_lazy_def(func.return_type, target_def, depth + 1);
        }

        if let Some(callable_id) = visitor::callable_shape_id(self.interner, type_id) {
            let callable = self.interner.callable_shape(callable_id);
            return callable
                .call_signatures
                .iter()
                .chain(callable.construct_signatures.iter())
                .any(|sig| {
                    sig.type_params.iter().any(|tp| {
                        tp.constraint.is_some_and(|constraint| {
                            self.type_contains_lazy_def(constraint, target_def, depth + 1)
                        }) || tp.default.is_some_and(|default| {
                            self.type_contains_lazy_def(default, target_def, depth + 1)
                        })
                    }) || sig.params.iter().any(|param| {
                        self.type_contains_lazy_def(param.type_id, target_def, depth + 1)
                    }) || sig.type_predicate.as_ref().is_some_and(|pred| {
                        pred.type_id.is_some_and(|type_id| {
                            self.type_contains_lazy_def(type_id, target_def, depth + 1)
                        })
                    }) || self.type_contains_lazy_def(sig.return_type, target_def, depth + 1)
                })
                || callable.properties.iter().any(|prop| {
                    self.type_contains_lazy_def(prop.type_id, target_def, depth + 1)
                        || (prop.write_type != TypeId::UNDEFINED
                            && self.type_contains_lazy_def(prop.write_type, target_def, depth + 1))
                })
                || callable.string_index.as_ref().is_some_and(|idx| {
                    self.type_contains_lazy_def(idx.value_type, target_def, depth + 1)
                })
                || callable.number_index.as_ref().is_some_and(|idx| {
                    self.type_contains_lazy_def(idx.value_type, target_def, depth + 1)
                });
        }

        if let Some(shape_id) = visitor::object_shape_id(self.interner, type_id)
            .or_else(|| visitor::object_with_index_shape_id(self.interner, type_id))
        {
            let shape = self.interner.object_shape(shape_id);
            return shape.properties.iter().any(|prop| {
                self.type_contains_lazy_def(prop.type_id, target_def, depth + 1)
                    || (prop.write_type != TypeId::UNDEFINED
                        && self.type_contains_lazy_def(prop.write_type, target_def, depth + 1))
            }) || shape.string_index.as_ref().is_some_and(|idx| {
                self.type_contains_lazy_def(idx.value_type, target_def, depth + 1)
            }) || shape.number_index.as_ref().is_some_and(|idx| {
                self.type_contains_lazy_def(idx.value_type, target_def, depth + 1)
            });
        }

        if let Some(cond_id) = visitor::conditional_type_id(self.interner, type_id) {
            let cond = self.interner.conditional_type(cond_id);
            return self.type_contains_lazy_def(cond.check_type, target_def, depth + 1)
                || self.type_contains_lazy_def(cond.extends_type, target_def, depth + 1)
                || self.type_contains_lazy_def(cond.true_type, target_def, depth + 1)
                || self.type_contains_lazy_def(cond.false_type, target_def, depth + 1);
        }

        if let Some(template_id) = visitor::template_literal_id(self.interner, type_id) {
            return self
                .interner
                .template_list(template_id)
                .iter()
                .any(|span| match span {
                    tsz_solver::types::TemplateSpan::Text(_) => false,
                    tsz_solver::types::TemplateSpan::Type(inner) => {
                        self.type_contains_lazy_def(*inner, target_def, depth + 1)
                    }
                });
        }

        if let Some(mapped_id) = visitor::mapped_type_id(self.interner, type_id) {
            let mapped = self.interner.mapped_type(mapped_id);
            return mapped.type_param.constraint.is_some_and(|constraint| {
                self.type_contains_lazy_def(constraint, target_def, depth + 1)
            }) || mapped.type_param.default.is_some_and(|default| {
                self.type_contains_lazy_def(default, target_def, depth + 1)
            }) || self.type_contains_lazy_def(mapped.constraint, target_def, depth + 1)
                || self.type_contains_lazy_def(mapped.template, target_def, depth + 1)
                || mapped.name_type.is_some_and(|name_type| {
                    self.type_contains_lazy_def(name_type, target_def, depth + 1)
                });
        }

        if let Some((container, index)) = visitor::index_access_parts(self.interner, type_id) {
            return self.type_contains_lazy_def(container, target_def, depth + 1)
                || self.type_contains_lazy_def(index, target_def, depth + 1);
        }

        if let Some(inner) = visitor::keyof_inner_type(self.interner, type_id)
            .or_else(|| visitor::readonly_inner_type(self.interner, type_id))
            .or_else(|| visitor::no_infer_inner_type(self.interner, type_id))
        {
            return self.type_contains_lazy_def(inner, target_def, depth + 1);
        }

        if let Some((_kind, inner)) = visitor::string_intrinsic_components(self.interner, type_id) {
            return self.type_contains_lazy_def(inner, target_def, depth + 1);
        }

        false
    }

    pub(crate) fn type_contains_symbol_reference(
        &self,
        type_id: TypeId,
        target_sym: SymbolId,
        depth: u32,
    ) -> bool {
        if depth > 64 {
            return true;
        }

        if visitor::type_query_symbol(self.interner, type_id)
            .is_some_and(|sym_ref| sym_ref.0 == target_sym.0)
        {
            return true;
        }

        if let Some(def_id) = visitor::lazy_def_id(self.interner, type_id)
            && self
                .type_cache
                .and_then(|cache| cache.def_to_symbol.get(&def_id))
                .is_some_and(|&sym_id| sym_id == target_sym)
        {
            return true;
        }

        if visitor::object_shape_id(self.interner, type_id)
            .or_else(|| visitor::object_with_index_shape_id(self.interner, type_id))
            .and_then(|shape_id| self.interner.object_shape(shape_id).symbol)
            .is_some_and(|sym_id| sym_id == target_sym)
        {
            return true;
        }

        if let Some(app_id) = visitor::application_id(self.interner, type_id) {
            let app = self.interner.type_application(app_id);
            return self.type_contains_symbol_reference(app.base, target_sym, depth + 1)
                || app
                    .args
                    .iter()
                    .copied()
                    .any(|arg| self.type_contains_symbol_reference(arg, target_sym, depth + 1));
        }

        if let Some(shape_id) = visitor::object_shape_id(self.interner, type_id)
            .or_else(|| visitor::object_with_index_shape_id(self.interner, type_id))
        {
            let shape = self.interner.object_shape(shape_id);
            return shape.properties.iter().any(|property| {
                self.type_contains_symbol_reference(property.type_id, target_sym, depth + 1)
            }) || shape.string_index.is_some_and(|index_info| {
                self.type_contains_symbol_reference(index_info.key_type, target_sym, depth + 1)
                    || self.type_contains_symbol_reference(
                        index_info.value_type,
                        target_sym,
                        depth + 1,
                    )
            }) || shape.number_index.is_some_and(|index_info| {
                self.type_contains_symbol_reference(index_info.key_type, target_sym, depth + 1)
                    || self.type_contains_symbol_reference(
                        index_info.value_type,
                        target_sym,
                        depth + 1,
                    )
            });
        }

        if let Some(type_list_id) = visitor::union_list_id(self.interner, type_id)
            .or_else(|| visitor::intersection_list_id(self.interner, type_id))
        {
            return self
                .interner
                .type_list(type_list_id)
                .iter()
                .copied()
                .any(|member| self.type_contains_symbol_reference(member, target_sym, depth + 1));
        }

        if let Some(elem_id) = visitor::array_element_type(self.interner, type_id) {
            return self.type_contains_symbol_reference(elem_id, target_sym, depth + 1);
        }

        if let Some(tuple_id) = visitor::tuple_list_id(self.interner, type_id) {
            return self.interner.tuple_list(tuple_id).iter().any(|member| {
                self.type_contains_symbol_reference(member.type_id, target_sym, depth + 1)
            });
        }

        if let Some(func_id) = visitor::function_shape_id(self.interner, type_id) {
            return self.function_shape_contains_symbol_reference(func_id, target_sym, depth + 1);
        }

        if let Some(callable_id) = visitor::callable_shape_id(self.interner, type_id) {
            let callable = self.interner.callable_shape(callable_id);
            return callable.call_signatures.iter().any(|sig| {
                self.call_signature_contains_symbol_reference(sig, target_sym, depth + 1)
            }) || callable.construct_signatures.iter().any(|sig| {
                self.call_signature_contains_symbol_reference(sig, target_sym, depth + 1)
            }) || callable.properties.iter().any(|property| {
                self.type_contains_symbol_reference(property.type_id, target_sym, depth + 1)
            }) || callable.string_index.is_some_and(|index_info| {
                self.type_contains_symbol_reference(index_info.key_type, target_sym, depth + 1)
                    || self.type_contains_symbol_reference(
                        index_info.value_type,
                        target_sym,
                        depth + 1,
                    )
            }) || callable.number_index.is_some_and(|index_info| {
                self.type_contains_symbol_reference(index_info.key_type, target_sym, depth + 1)
                    || self.type_contains_symbol_reference(
                        index_info.value_type,
                        target_sym,
                        depth + 1,
                    )
            });
        }

        if let Some(cond_id) = visitor::conditional_type_id(self.interner, type_id) {
            let cond = self.interner.conditional_type(cond_id);
            return self.type_contains_symbol_reference(cond.check_type, target_sym, depth + 1)
                || self.type_contains_symbol_reference(cond.extends_type, target_sym, depth + 1)
                || self.type_contains_symbol_reference(cond.true_type, target_sym, depth + 1)
                || self.type_contains_symbol_reference(cond.false_type, target_sym, depth + 1);
        }

        if let Some(template_id) = visitor::template_literal_id(self.interner, type_id) {
            return self
                .interner
                .template_list(template_id)
                .iter()
                .any(|span| matches!(span, tsz_solver::types::TemplateSpan::Type(inner) if self.type_contains_symbol_reference(*inner, target_sym, depth + 1)));
        }

        if let Some(mapped_id) = visitor::mapped_type_id(self.interner, type_id) {
            let mapped = self.interner.mapped_type(mapped_id);
            return self.type_contains_symbol_reference(mapped.constraint, target_sym, depth + 1)
                || self.type_contains_symbol_reference(mapped.template, target_sym, depth + 1)
                || mapped.name_type.is_some_and(|name_type| {
                    self.type_contains_symbol_reference(name_type, target_sym, depth + 1)
                })
                || mapped.type_param.constraint.is_some_and(|constraint| {
                    self.type_contains_symbol_reference(constraint, target_sym, depth + 1)
                })
                || mapped.type_param.default.is_some_and(|default| {
                    self.type_contains_symbol_reference(default, target_sym, depth + 1)
                });
        }

        if let Some((container, index)) = visitor::index_access_parts(self.interner, type_id) {
            return self.type_contains_symbol_reference(container, target_sym, depth + 1)
                || self.type_contains_symbol_reference(index, target_sym, depth + 1);
        }

        if let Some(inner) = visitor::keyof_inner_type(self.interner, type_id)
            .or_else(|| visitor::readonly_inner_type(self.interner, type_id))
            .or_else(|| visitor::no_infer_inner_type(self.interner, type_id))
        {
            return self.type_contains_symbol_reference(inner, target_sym, depth + 1);
        }

        false
    }

    pub(crate) fn function_shape_contains_symbol_reference(
        &self,
        func_id: tsz_solver::types::FunctionShapeId,
        target_sym: SymbolId,
        depth: u32,
    ) -> bool {
        let func = self.interner.function_shape(func_id);
        func.params
            .iter()
            .any(|param| self.type_contains_symbol_reference(param.type_id, target_sym, depth + 1))
            || self.type_contains_symbol_reference(func.return_type, target_sym, depth + 1)
            || func.this_type.is_some_and(|this_type| {
                self.type_contains_symbol_reference(this_type, target_sym, depth + 1)
            })
            || func.type_params.iter().any(|param| {
                param.constraint.is_some_and(|constraint| {
                    self.type_contains_symbol_reference(constraint, target_sym, depth + 1)
                }) || param.default.is_some_and(|default| {
                    self.type_contains_symbol_reference(default, target_sym, depth + 1)
                })
            })
    }

    pub(crate) fn call_signature_contains_symbol_reference(
        &self,
        signature: &tsz_solver::types::CallSignature,
        target_sym: SymbolId,
        depth: u32,
    ) -> bool {
        signature
            .params
            .iter()
            .any(|param| self.type_contains_symbol_reference(param.type_id, target_sym, depth + 1))
            || self.type_contains_symbol_reference(signature.return_type, target_sym, depth + 1)
            || signature.this_type.is_some_and(|this_type| {
                self.type_contains_symbol_reference(this_type, target_sym, depth + 1)
            })
            || signature.type_params.iter().any(|param| {
                param.constraint.is_some_and(|constraint| {
                    self.type_contains_symbol_reference(constraint, target_sym, depth + 1)
                }) || param.default.is_some_and(|default| {
                    self.type_contains_symbol_reference(default, target_sym, depth + 1)
                })
            })
    }

    /// Check if a symbol is a global (ambient) type that's always accessible.
    /// Global types like Object, Array, Function, etc. have no parent symbol
    /// (parent == `SymbolId::NONE`) and are always referenceable in declarations.
    pub(crate) fn is_global_symbol(&self, sym_id: SymbolId) -> bool {
        let Some(arena) = self.symbol_arena else {
            return false;
        };
        let Some(symbol) = arena.get(sym_id) else {
            return false;
        };
        symbol.declarations.is_empty()
            && !symbol.parent.is_some()
            && self.resolve_symbol_module_path(sym_id).is_none()
            && !(symbol.has_any_flags(symbol_flags::ALIAS) && symbol.import_module.is_some())
    }

    pub(crate) fn intersection_member_priority(&self, type_id: TypeId) -> u8 {
        if visitor::type_param_info(self.interner, type_id).is_some() {
            return 2;
        }

        if let Some(sym_ref) = visitor::type_query_symbol(self.interner, type_id) {
            let sym_id = SymbolId(sym_ref.0);
            return u8::from(self.is_symbol_visible(sym_id) || self.symbol_is_nameable(sym_id));
        }

        if let Some(callable_id) = visitor::callable_shape_id(self.interner, type_id) {
            let callable = self.interner.callable_shape(callable_id);
            if let Some(sym_id) = callable.symbol {
                return u8::from(self.is_symbol_visible(sym_id) || self.symbol_is_nameable(sym_id));
            }
            return 0;
        }

        if let Some(shape_id) = visitor::object_shape_id(self.interner, type_id)
            .or_else(|| visitor::object_with_index_shape_id(self.interner, type_id))
        {
            let shape = self.interner.object_shape(shape_id);
            if let Some(sym_id) = shape.symbol {
                return u8::from(self.is_symbol_visible(sym_id) || self.symbol_is_nameable(sym_id));
            }
            return 0;
        }

        1
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
        let base_text = if let Some(sym_ref) = visitor::type_query_symbol(self.interner, app.base) {
            let sym_id = SymbolId(sym_ref.0);
            self.print_named_symbol_reference(sym_id, false)
                .unwrap_or_else(|| self.print_type(app.base))
        } else {
            self.print_type(app.base)
        };

        if app.args.is_empty() {
            base_text
        } else {
            let args: Vec<String> = app
                .args
                .iter()
                .enumerate()
                .map(|(index, &id)| self.print_type_argument(id, index == 0))
                .collect();
            format!("{base_text}<{}>", args.join(", "))
        }
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
                    result.push_str(&self.resolve_atom(*atom));
                }
                tsz_solver::types::TemplateSpan::Type(type_id) => {
                    result.push_str("${");
                    result.push_str(&self.print_type(*type_id));
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
        let constraint = self.print_type(mapped.constraint);

        let mut nested = self.clone();
        if let Some(indent) = nested.indent_level {
            nested.indent_level = Some(indent + 1);
        }
        let template = nested.print_type(mapped.template);

        let as_clause = if let Some(name_type) = mapped.name_type {
            format!(" as {}", self.print_type(name_type))
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
                "{{ {readonly_prefix}[{param_name} in {constraint}{as_clause}]{optional_suffix}: {template} }}"
            )
        }
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
