//! Type Printer - Convert `TypeId` to TypeScript syntax
//!
//! This module handles type reification: converting the Solver's internal `TypeId`
//! representation into printable TypeScript syntax for declaration emit (.d.ts files).

use tsz_binder::{SymbolArena, SymbolId, symbol_flags};
use tsz_common::interner::Atom;
use tsz_solver::TypeInterner;
use tsz_solver::types::TypeId;
use tsz_solver::visitor;

use crate::type_cache_view::TypeCacheView;

/// Prints types as TypeScript syntax for declaration emit.
///
/// # Examples
///
/// ```ignore
/// # use tsz_solver::types::TypeId;
/// let printer = TypePrinter::new(&interner);
/// assert_eq!(printer.print_type(TypeId::STRING), "string");
/// assert_eq!(printer.print_type(TypeId::NUMBER), "number");
/// ```
#[derive(Clone)]
pub struct TypePrinter<'a> {
    interner: &'a TypeInterner,
    /// Symbol arena for checking symbol visibility
    symbol_arena: Option<&'a SymbolArena>,
    /// Type cache for resolving Lazy(DefId) types
    type_cache: Option<&'a TypeCacheView>,
    /// Current recursion depth (to prevent infinite loops)
    current_depth: u32,
    /// Maximum recursion depth
    max_depth: u32,
}

impl<'a> TypePrinter<'a> {
    pub const fn new(interner: &'a TypeInterner) -> Self {
        Self {
            interner,
            symbol_arena: None,
            type_cache: None,
            current_depth: 0,
            max_depth: 10,
        }
    }

    /// Set the symbol arena for visibility checking.
    pub const fn with_symbols(mut self, symbol_arena: &'a SymbolArena) -> Self {
        self.symbol_arena = Some(symbol_arena);
        self
    }

    /// Set the type cache for resolving Lazy(DefId) types.
    pub const fn with_type_cache(mut self, type_cache: &'a TypeCacheView) -> Self {
        self.type_cache = Some(type_cache);
        self
    }

    /// Set the maximum recursion depth for type inlining.
    pub const fn with_max_depth(mut self, max_depth: u32) -> Self {
        self.max_depth = max_depth;
        self
    }

    /// Check if a symbol is visible (exported) from the current module.
    ///
    /// A symbol is visible if:
    /// 1. It has the `EXPORT_VALUE` flag or `is_exported` field is true
    /// 2. Its parent is not a Function or Method (not a local type)
    fn is_symbol_visible(&self, sym_id: SymbolId) -> bool {
        let Some(arena) = self.symbol_arena else {
            return false;
        };
        let Some(symbol) = arena.get(sym_id) else {
            return false;
        };

        // Check if it's exported
        if symbol.is_exported || symbol.has_any_flags(symbol_flags::EXPORT_VALUE) {
            // Check parentage - if parent is a function/method, it's local and must be inlined
            if !symbol.parent.is_none()
                && let Some(parent) = arena.get(symbol.parent)
                && parent.has_any_flags(symbol_flags::FUNCTION | symbol_flags::METHOD)
            {
                return false; // Local to function, must inline
            }
            return true;
        }

        false
    }

    /// Resolve an atom to its string representation.
    fn resolve_atom(&self, atom: Atom) -> String {
        self.interner.resolve_atom(atom)
    }

    /// Convert a `TypeId` to TypeScript syntax string.
    pub fn print_type(&self, type_id: TypeId) -> String {
        // Fast path: check built-in intrinsics (TypeId < 100)
        if type_id.is_intrinsic() {
            return self.print_intrinsic_type(type_id);
        }

        if let Some(literal) = visitor::literal_value(self.interner, type_id) {
            return self.print_literal(&literal);
        }
        if let Some(shape_id) = visitor::object_shape_id(self.interner, type_id)
            .or_else(|| visitor::object_with_index_shape_id(self.interner, type_id))
        {
            return self.print_object_type(shape_id);
        }
        if let Some(type_list_id) = visitor::union_list_id(self.interner, type_id) {
            return self.print_union(type_list_id);
        }
        if let Some(type_list_id) = visitor::intersection_list_id(self.interner, type_id) {
            return self.print_intersection(type_list_id);
        }
        if let Some(elem_id) = visitor::array_element_type(self.interner, type_id) {
            return format!("{}[]", self.print_type(elem_id));
        }
        if let Some(tuple_id) = visitor::tuple_list_id(self.interner, type_id) {
            return self.print_tuple(tuple_id);
        }
        if let Some(func_id) = visitor::function_shape_id(self.interner, type_id) {
            return self.print_function_type(func_id);
        }
        if let Some(callable_id) = visitor::callable_shape_id(self.interner, type_id) {
            return self.print_callable(callable_id);
        }
        if let Some(param_info) = visitor::type_param_info(self.interner, type_id) {
            return self.print_type_parameter(&param_info);
        }
        if let Some(def_id) = visitor::lazy_def_id(self.interner, type_id) {
            return self.print_lazy_type(def_id);
        }
        if let Some((def_id, members_id)) = visitor::enum_components(self.interner, type_id) {
            return self.print_enum(def_id, members_id);
        }
        if let Some(app_id) = visitor::application_id(self.interner, type_id) {
            return self.print_type_application(app_id);
        }
        if let Some(cond_id) = visitor::conditional_type_id(self.interner, type_id) {
            return self.print_conditional(cond_id);
        }
        if let Some(template_id) = visitor::template_literal_id(self.interner, type_id) {
            return self.print_template_literal(template_id);
        }
        if let Some(mapped_id) = visitor::mapped_type_id(self.interner, type_id) {
            return self.print_mapped_type(mapped_id);
        }
        if let Some((container, index)) = visitor::index_access_parts(self.interner, type_id) {
            return self.print_index_access(container, index);
        }
        if visitor::type_query_symbol(self.interner, type_id).is_some() {
            return "any".to_string();
        }
        if let Some(type_id) = visitor::keyof_inner_type(self.interner, type_id) {
            return format!("keyof {}", self.print_type(type_id));
        }
        if let Some(type_id) = visitor::readonly_inner_type(self.interner, type_id) {
            return format!("readonly {}", self.print_type(type_id));
        }
        if visitor::unique_symbol_ref(self.interner, type_id).is_some() {
            return "unique symbol".to_string();
        }
        if visitor::is_this_type(self.interner, type_id) {
            return "this".to_string();
        }
        if let Some((kind, type_arg)) = visitor::string_intrinsic_components(self.interner, type_id)
        {
            return self.print_string_intrinsic(kind, type_arg);
        }
        if visitor::module_namespace_symbol_ref(self.interner, type_id).is_some() {
            return "any".to_string();
        }
        if let Some(index) = visitor::recursive_index(self.interner, type_id) {
            return format!("T{index}");
        }
        if let Some(index) = visitor::bound_parameter_index(self.interner, type_id) {
            return format!("P{index}");
        }
        if let Some(inner) = visitor::no_infer_inner_type(self.interner, type_id) {
            // NoInfer<T> evaluates to T, so format the inner type
            return self.print_type(inner);
        }
        if visitor::is_error_type(self.interner, type_id) {
            return "any".to_string();
        }

        "any".to_string()
    }

    fn print_intrinsic_type(&self, type_id: TypeId) -> String {
        if matches!(type_id, TypeId::ERROR | TypeId::ANY) {
            // Errors and `any` emit as `any` in declarations.
            return "any".to_string();
        }
        match type_id {
            TypeId::NEVER => "never".to_string(),
            TypeId::UNKNOWN => "unknown".to_string(),
            TypeId::VOID => "void".to_string(),
            TypeId::UNDEFINED => "undefined".to_string(),
            TypeId::NULL => "null".to_string(),
            TypeId::BOOLEAN => "boolean".to_string(),
            TypeId::NUMBER => "number".to_string(),
            TypeId::STRING => "string".to_string(),
            TypeId::BIGINT => "bigint".to_string(),
            TypeId::SYMBOL => "symbol".to_string(),
            TypeId::OBJECT => "object".to_string(),
            TypeId::FUNCTION => "Function".to_string(),
            TypeId::BOOLEAN_TRUE => "true".to_string(),
            TypeId::BOOLEAN_FALSE => "false".to_string(),
            _ => "any".to_string(),
        }
    }

    fn print_literal(&self, literal: &tsz_solver::types::LiteralValue) -> String {
        match literal {
            tsz_solver::types::LiteralValue::String(atom) => {
                format!("\"{}\"", self.resolve_atom(*atom))
            }
            tsz_solver::types::LiteralValue::Number(n) => n.0.to_string(),
            tsz_solver::types::LiteralValue::Boolean(b) => b.to_string(),
            tsz_solver::types::LiteralValue::BigInt(atom) => {
                format!("{}n", self.resolve_atom(*atom))
            }
        }
    }

    fn print_object_type(&self, shape_id: tsz_solver::types::ObjectShapeId) -> String {
        let shape = self.interner.object_shape(shape_id);

        if shape.properties.is_empty() {
            return "{}".to_string();
        }

        let mut members = Vec::new();
        for property in &shape.properties {
            let mut member = String::new();

            // Property name
            member.push_str(&self.resolve_atom(property.name));

            // Optional marker
            if property.optional {
                member.push('?');
            }

            // Property type
            member.push_str(": ");
            member.push_str(&self.print_type(property.type_id));

            members.push(member);
        }

        format!("{{ {} }}", members.join("; "))
    }

    fn print_union(&self, type_list_id: tsz_solver::types::TypeListId) -> String {
        let types = self.interner.type_list(type_list_id);
        if types.is_empty() {
            return "never".to_string();
        }

        let mut parts = Vec::with_capacity(types.len());
        for &type_id in types.iter() {
            parts.push(self.print_type(type_id));
        }

        // Join with " | "
        parts.join(" | ")
    }

    fn print_intersection(&self, type_list_id: tsz_solver::types::TypeListId) -> String {
        let types = self.interner.type_list(type_list_id);
        if types.is_empty() {
            return "unknown".to_string(); // Intersection of 0 types is unknown
        }

        let mut parts = Vec::with_capacity(types.len());
        for &type_id in types.iter() {
            parts.push(self.print_type(type_id));
        }

        // Join with " & "
        parts.join(" & ")
    }

    fn print_tuple(&self, tuple_id: tsz_solver::types::TupleListId) -> String {
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
            }

            // Type annotation
            part.push_str(&self.print_type(elem.type_id));

            // Optional marker for unlabeled tuples (comes after type)
            if elem.name.is_none() && elem.optional {
                part.push('?');
            }

            parts.push(part);
        }

        format!("[{}]", parts.join(", "))
    }

    fn print_function_type(&self, func_id: tsz_solver::types::FunctionShapeId) -> String {
        let func_shape = self.interner.function_shape(func_id);

        // Type parameters
        let type_params_str = if !func_shape.type_params.is_empty() {
            let params: Vec<String> = func_shape
                .type_params
                .iter()
                .map(|tp| self.print_type_parameter(tp))
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
                param_str.push_str(&self.resolve_atom(name));
                param_str.push_str(": ");
            }

            // Parameter type
            param_str.push_str(&self.print_type(param.type_id));

            // Optional parameter
            if param.optional {
                param_str.push('?');
            }

            params.push(param_str);
        }

        // Return type
        let return_type = self.print_type(func_shape.return_type);

        format!(
            "{}({}) => {}",
            type_params_str,
            params.join(", "),
            return_type
        )
    }

    fn print_callable(&self, callable_id: tsz_solver::types::CallableShapeId) -> String {
        let callable = self.interner.callable_shape(callable_id);

        // Collect all signatures (call + construct)
        let mut parts = Vec::new();

        for sig in &callable.call_signatures {
            parts.push(self.print_call_signature(sig, false));
        }
        for sig in &callable.construct_signatures {
            parts.push(self.print_call_signature(sig, true));
        }

        // Add properties
        for prop in &callable.properties {
            let optional = if prop.optional { "?" } else { "" };
            parts.push(format!(
                "{}{}: {}",
                self.resolve_atom(prop.name),
                optional,
                self.print_type(prop.type_id)
            ));
        }

        if parts.is_empty() {
            return "{}".to_string();
        }

        format!("{{ {} }}", parts.join("; "))
    }

    fn print_call_signature(
        &self,
        sig: &tsz_solver::types::CallSignature,
        is_construct: bool,
    ) -> String {
        let prefix = if is_construct { "new " } else { "" };

        let type_params_str = if !sig.type_params.is_empty() {
            let params: Vec<String> = sig
                .type_params
                .iter()
                .map(|tp| self.print_type_parameter(tp))
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
                param_str.push_str(&self.resolve_atom(name));
                if param.optional {
                    param_str.push('?');
                }
                param_str.push_str(": ");
            }
            param_str.push_str(&self.print_type(param.type_id));
            params.push(param_str);
        }

        let return_type = self.print_type(sig.return_type);
        format!(
            "{}{}({}): {}",
            prefix,
            type_params_str,
            params.join(", "),
            return_type
        )
    }

    fn print_type_parameter(&self, param_info: &tsz_solver::types::TypeParamInfo) -> String {
        // Type parameter names are Atoms
        self.resolve_atom(param_info.name)
    }

    fn print_lazy_type(&self, def_id: tsz_solver::def::DefId) -> String {
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

        // If we have a symbol and it's visible, use the name
        if let Some(sym_id) = sym_id
            && self.is_symbol_visible(sym_id)
        {
            // Get the symbol name
            if let Some(arena) = self.symbol_arena
                && let Some(symbol) = arena.get(sym_id)
            {
                return symbol.escaped_name.clone();
            }
        }

        // Symbol is not visible or we don't have symbol info.
        // Fallback to `any` when we cannot legally name the referenced type.
        "any".to_string()
    }

    fn print_enum(&self, def_id: tsz_solver::def::DefId, _members_id: TypeId) -> String {
        // Try to resolve the enum name via DefId -> SymbolId -> symbol name
        if let Some(cache) = self.type_cache
            && let Some(&sym_id) = cache.def_to_symbol.get(&def_id)
            && let Some(arena) = self.symbol_arena
            && let Some(symbol) = arena.get(sym_id)
        {
            return symbol.escaped_name.clone();
        }
        // Fallback: print the member type structure
        format!("enum({})", def_id.0)
    }

    fn print_type_application(&self, app_id: tsz_solver::types::TypeApplicationId) -> String {
        let app = self.interner.type_application(app_id);

        if app.args.is_empty() {
            self.print_type(app.base)
        } else {
            let args: Vec<String> = app.args.iter().map(|&id| self.print_type(id)).collect();
            format!("{}<{}>", self.print_type(app.base), args.join(", "))
        }
    }

    fn print_conditional(&self, cond_id: tsz_solver::types::ConditionalTypeId) -> String {
        let cond = self.interner.conditional_type(cond_id);
        format!(
            "{} extends {} ? {} : {}",
            self.print_type(cond.check_type),
            self.print_type(cond.extends_type),
            self.print_type(cond.true_type),
            self.print_type(cond.false_type),
        )
    }

    fn print_template_literal(&self, template_id: tsz_solver::types::TemplateLiteralId) -> String {
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

    fn print_mapped_type(&self, mapped_id: tsz_solver::types::MappedTypeId) -> String {
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
        let template = self.print_type(mapped.template);

        let as_clause = if let Some(name_type) = mapped.name_type {
            format!(" as {}", self.print_type(name_type))
        } else {
            String::new()
        };

        format!(
            "{{ {readonly_prefix}[{param_name} in {constraint}{as_clause}]{optional_suffix}: {template} }}"
        )
    }

    fn print_index_access(&self, container: TypeId, index: TypeId) -> String {
        format!("{}[{}]", self.print_type(container), self.print_type(index))
    }

    fn print_string_intrinsic(
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
}

#[cfg(test)]
#[path = "../../tests/type_printer.rs"]
mod tests;
