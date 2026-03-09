//! Type Printer - Convert `TypeId` to TypeScript syntax
//!
//! This module handles type reification: converting the Solver's internal `TypeId`
//! representation into printable TypeScript syntax for declaration emit (.d.ts files).

use tsz_binder::{SymbolArena, SymbolId, symbol_flags};
use tsz_common::interner::Atom;
use tsz_parser::parser::node::NodeArena;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;
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
    /// Indentation level for multi-line type formatting (e.g., object types in .d.ts).
    /// `Some(n)` enables multi-line formatting at indent level `n`;
    /// `None` keeps flat single-line format.
    indent_level: Option<u32>,
    /// The enclosing symbol (namespace/class) whose qualified name prefix should
    /// be stripped from type references to produce context-relative names.
    enclosing_symbol: Option<SymbolId>,
    /// AST access for deciding whether a symbol is nameable from declaration output.
    node_arena: Option<&'a NodeArena>,
    /// Optional resolver for turning foreign symbols into import module specifiers.
    module_path_resolver: Option<&'a dyn Fn(SymbolId) -> Option<String>>,
    /// Optional resolver for reusing in-scope namespace import aliases.
    namespace_alias_resolver: Option<&'a dyn Fn(SymbolId) -> Option<String>>,
    /// Optional resolver for deciding whether a local import alias survives in emitted output.
    local_import_alias_name_resolver: Option<&'a dyn Fn(SymbolId) -> bool>,
}

impl<'a> TypePrinter<'a> {
    pub const fn new(interner: &'a TypeInterner) -> Self {
        Self {
            interner,
            symbol_arena: None,
            type_cache: None,
            current_depth: 0,
            max_depth: 10,
            indent_level: None,
            enclosing_symbol: None,
            node_arena: None,
            module_path_resolver: None,
            namespace_alias_resolver: None,
            local_import_alias_name_resolver: None,
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

    /// Enable multi-line type formatting at the given indentation level.
    /// Object types with members will be formatted across multiple lines
    /// using 4-space indentation. Without this, object types use flat format.
    pub const fn with_indent_level(mut self, indent_level: u32) -> Self {
        self.indent_level = Some(indent_level);
        self
    }

    /// Set the enclosing symbol (namespace/class) for context-relative name resolution.
    /// Qualified names that share a prefix with this symbol's path will have the
    /// shared prefix stripped (e.g., inside namespace `m1.m2`, type `m1.m2.c` becomes `c`).
    pub const fn with_enclosing_symbol(mut self, sym_id: SymbolId) -> Self {
        self.enclosing_symbol = Some(sym_id);
        self
    }

    /// Set the AST arena for declaration-reachability checks.
    pub const fn with_node_arena(mut self, node_arena: &'a NodeArena) -> Self {
        self.node_arena = Some(node_arena);
        self
    }

    /// Set a resolver for import-qualified foreign symbol references.
    pub fn with_module_path_resolver(
        mut self,
        resolver: &'a dyn Fn(SymbolId) -> Option<String>,
    ) -> Self {
        self.module_path_resolver = Some(resolver);
        self
    }

    /// Set a resolver for reusing namespace import aliases already in scope.
    pub fn with_namespace_alias_resolver(
        mut self,
        resolver: &'a dyn Fn(SymbolId) -> Option<String>,
    ) -> Self {
        self.namespace_alias_resolver = Some(resolver);
        self
    }

    /// Set a resolver for deciding whether local import aliases can be named directly.
    pub fn with_local_import_alias_name_resolver(
        mut self,
        resolver: &'a dyn Fn(SymbolId) -> bool,
    ) -> Self {
        self.local_import_alias_name_resolver = Some(resolver);
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
            if symbol.parent.is_some()
                && let Some(parent) = arena.get(symbol.parent)
                && parent.has_any_flags(symbol_flags::FUNCTION | symbol_flags::METHOD)
            {
                return false; // Local to function, must inline
            }
            return true;
        }

        false
    }

    fn symbol_is_nameable(&self, sym_id: SymbolId) -> bool {
        let Some(arena) = self.symbol_arena else {
            return false;
        };
        let Some(symbol) = arena.get(sym_id) else {
            return false;
        };

        if symbol.declarations.is_empty() {
            return !symbol.parent.is_some();
        }

        symbol
            .declarations
            .iter()
            .copied()
            .any(|decl| self.declaration_is_nameable(decl))
    }

    fn declaration_is_nameable(&self, decl_idx: tsz_parser::NodeIndex) -> bool {
        let Some(node_arena) = self.node_arena else {
            return false;
        };
        let Some(decl_node) = node_arena.get(decl_idx) else {
            return false;
        };

        if decl_node.kind == syntax_kind_ext::CLASS_EXPRESSION
            || decl_node.kind == syntax_kind_ext::FUNCTION_EXPRESSION
            || decl_node.kind == syntax_kind_ext::ARROW_FUNCTION
        {
            return false;
        }

        let mut current = node_arena.get_extended(decl_idx).map(|ext| ext.parent);
        while let Some(parent_idx) = current {
            let Some(parent_node) = node_arena.get(parent_idx) else {
                break;
            };

            match parent_node.kind {
                k if k == syntax_kind_ext::FUNCTION_DECLARATION => return false,
                k if k == syntax_kind_ext::FUNCTION_EXPRESSION => return false,
                k if k == syntax_kind_ext::ARROW_FUNCTION => return false,
                k if k == syntax_kind_ext::METHOD_DECLARATION => return false,
                k if k == syntax_kind_ext::GET_ACCESSOR => return false,
                k if k == syntax_kind_ext::SET_ACCESSOR => return false,
                k if k == syntax_kind_ext::CONSTRUCTOR => return false,
                k if k == syntax_kind_ext::BLOCK => return false,
                k if k == syntax_kind_ext::CASE_BLOCK => return false,
                k if k == syntax_kind_ext::SOURCE_FILE => return true,
                k if k == syntax_kind_ext::MODULE_BLOCK => return true,
                _ => {
                    current = node_arena.get_extended(parent_idx).map(|ext| ext.parent);
                }
            }
        }

        true
    }

    fn symbol_type_fallback(&self, sym_id: SymbolId) -> Option<TypeId> {
        let cache = self.type_cache?;
        let type_id = cache.symbol_types.get(&sym_id).copied()?;
        if visitor::type_query_symbol(self.interner, type_id)
            .is_some_and(|sym_ref| sym_ref.0 == sym_id.0)
        {
            return None;
        }
        Some(type_id)
    }

    fn symbol_needs_inline_type_query(&self, sym_id: SymbolId) -> bool {
        if self.is_symbol_visible(sym_id) || self.symbol_is_nameable(sym_id) {
            return false;
        }

        let Some(arena) = self.symbol_arena else {
            return false;
        };
        let Some(symbol) = arena.get(sym_id) else {
            return false;
        };

        symbol.declarations.iter().copied().any(|decl_idx| {
            let Some(node_arena) = self.node_arena else {
                return false;
            };
            let Some(decl_node) = node_arena.get(decl_idx) else {
                return false;
            };
            matches!(
                decl_node.kind,
                k if k == syntax_kind_ext::CLASS_DECLARATION
                    || k == syntax_kind_ext::CLASS_EXPRESSION
                    || k == syntax_kind_ext::FUNCTION_DECLARATION
                    || k == syntax_kind_ext::FUNCTION_EXPRESSION
                    || k == syntax_kind_ext::ARROW_FUNCTION
            )
        })
    }

    /// Resolve a SymbolRef/SymbolId to its qualified name (e.g., "M.c" for `typeof M.c`).
    /// If an enclosing symbol is set, the qualified name is relative to it
    /// (e.g., inside namespace `m1.m2`, type `m1.m2.c` becomes `c`).
    fn resolve_symbol_qualified_name(&self, sym_id: SymbolId) -> Option<String> {
        let arena = self.symbol_arena?;
        let sym = arena.get(sym_id)?;
        let mut qualified_name = sym.escaped_name.clone();
        let mut current_parent = sym.parent;

        // Build the enclosing symbol's ancestor set for stripping
        let enclosing_ancestors = if let Some(enc_id) = self.enclosing_symbol {
            let mut ancestors = rustc_hash::FxHashSet::default();
            ancestors.insert(enc_id);
            let mut p = enc_id;
            while let Some(ps) = arena.get(p) {
                if ps.parent == SymbolId::NONE {
                    break;
                }
                ancestors.insert(ps.parent);
                p = ps.parent;
            }
            ancestors
        } else {
            rustc_hash::FxHashSet::default()
        };

        while current_parent != SymbolId::NONE {
            // Stop qualifying when we reach the enclosing scope
            if enclosing_ancestors.contains(&current_parent) {
                break;
            }
            if let Some(parent_sym) = arena.get(current_parent) {
                // Don't prepend for source files and blocks
                if !parent_sym.escaped_name.starts_with('"')
                    && !parent_sym.escaped_name.starts_with("__")
                {
                    qualified_name = format!("{}.{}", parent_sym.escaped_name, qualified_name);
                }
                current_parent = parent_sym.parent;
            } else {
                break;
            }
        }

        Some(qualified_name)
    }

    /// Resolve an atom to its string representation.
    fn resolve_atom(&self, atom: Atom) -> String {
        self.interner.resolve_atom(atom)
    }

    fn resolve_symbol_module_path(&self, sym_id: SymbolId) -> Option<String> {
        self.module_path_resolver
            .and_then(|resolver| resolver(sym_id))
    }

    fn resolve_namespace_import_alias(&self, sym_id: SymbolId) -> Option<String> {
        self.namespace_alias_resolver
            .and_then(|resolver| resolver(sym_id))
    }

    fn import_qualified_symbol_name(&self, sym_id: SymbolId) -> Option<String> {
        let module_path = self.resolve_symbol_module_path(sym_id)?;
        let name = self.resolve_symbol_qualified_name(sym_id)?;
        Some(format!("import(\"{module_path}\").{name}"))
    }

    fn is_local_import_alias(&self, sym_id: SymbolId) -> bool {
        self.symbol_arena
            .and_then(|arena| arena.get(sym_id))
            .is_some_and(|symbol| {
                symbol.has_any_flags(symbol_flags::ALIAS) && symbol.import_module.is_some()
            })
    }

    fn can_reference_symbol_by_name(&self, sym_id: SymbolId) -> bool {
        if self.is_local_import_alias(sym_id) {
            return self
                .local_import_alias_name_resolver
                .is_none_or(|resolver| resolver(sym_id));
        }

        if self.resolve_symbol_module_path(sym_id).is_some() {
            return false;
        }

        self.is_symbol_visible(sym_id) || self.symbol_is_nameable(sym_id)
    }

    fn print_named_symbol_reference(&self, sym_id: SymbolId, needs_typeof: bool) -> Option<String> {
        if let Some(name) = self.resolve_symbol_qualified_name(sym_id)
            && (self.can_reference_symbol_by_name(sym_id) || self.is_global_symbol(sym_id))
        {
            return Some(if needs_typeof {
                format!("typeof {name}")
            } else {
                name
            });
        }

        if let Some(name) = self.import_qualified_symbol_name(sym_id) {
            return Some(if needs_typeof {
                format!("typeof {name}")
            } else {
                name
            });
        }

        None
    }

    fn print_namespace_reference(&self, sym_id: SymbolId) -> Option<String> {
        if let Some(alias) = self.resolve_namespace_import_alias(sym_id) {
            return Some(format!("typeof {alias}"));
        }
        if let Some(name) = self.resolve_symbol_qualified_name(sym_id)
            && (self.can_reference_symbol_by_name(sym_id) || self.is_global_symbol(sym_id))
        {
            return Some(format!("typeof {name}"));
        }
        self.resolve_symbol_module_path(sym_id)
            .map(|module_path| format!("typeof import(\"{module_path}\")"))
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
        if let Some(app_id) = visitor::application_id(self.interner, type_id) {
            let app = self.interner.type_application(app_id);
            let base_has_name = visitor::lazy_def_id(self.interner, app.base).is_some()
                || visitor::type_query_symbol(self.interner, app.base).is_some()
                || visitor::enum_components(self.interner, app.base).is_some()
                || visitor::object_shape_id(self.interner, app.base)
                    .or_else(|| visitor::object_with_index_shape_id(self.interner, app.base))
                    .and_then(|shape_id| self.interner.object_shape(shape_id).symbol)
                    .is_some();
            if base_has_name {
                return self.print_type_application(app_id);
            }
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
            let elem_str = self.print_type(elem_id);
            // Parenthesize complex element types (union, intersection, function, conditional, keyof, readonly)
            let needs_parens = visitor::union_list_id(self.interner, elem_id).is_some()
                || visitor::intersection_list_id(self.interner, elem_id).is_some()
                || visitor::function_shape_id(self.interner, elem_id).is_some()
                || visitor::conditional_type_id(self.interner, elem_id).is_some()
                || visitor::keyof_inner_type(self.interner, elem_id).is_some()
                || visitor::readonly_inner_type(self.interner, elem_id).is_some();
            if needs_parens {
                return format!("({elem_str})[]");
            }
            return format!("{elem_str}[]");
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
        if let Some(sym_ref) = visitor::type_query_symbol(self.interner, type_id) {
            let sym_id = SymbolId(sym_ref.0);
            if let Some(arena) = self.symbol_arena
                && let Some(symbol) = arena.get(sym_id)
                && symbol.has_any_flags(symbol_flags::CLASS | symbol_flags::INTERFACE)
                && let Some(name) =
                    if self.can_reference_symbol_by_name(sym_id) || self.is_global_symbol(sym_id) {
                        self.resolve_symbol_qualified_name(sym_id)
                    } else {
                        self.import_qualified_symbol_name(sym_id)
                    }
            {
                return name;
            }
            if self.symbol_needs_inline_type_query(sym_id)
                && let Some(symbol_type) = self.symbol_type_fallback(sym_id)
            {
                return self.print_type(symbol_type);
            }
            if let Some(name) = self.resolve_symbol_qualified_name(sym_id)
                && (self.can_reference_symbol_by_name(sym_id) || self.is_global_symbol(sym_id))
            {
                return format!("typeof {name}");
            }
            if let Some(name) = self.print_named_symbol_reference(sym_id, true) {
                return name;
            }
            return "any".to_string();
        }
        if let Some(inner_id) = visitor::keyof_inner_type(self.interner, type_id) {
            let inner_str = self.print_type(inner_id);
            // Parenthesize union/intersection operand of keyof
            let needs_parens = visitor::union_list_id(self.interner, inner_id).is_some()
                || visitor::intersection_list_id(self.interner, inner_id).is_some();
            if needs_parens {
                return format!("keyof ({inner_str})");
            }
            return format!("keyof {inner_str}");
        }
        if let Some(inner_id) = visitor::readonly_inner_type(self.interner, type_id) {
            let inner_str = self.print_type(inner_id);
            // Parenthesize union/intersection operand of readonly
            let needs_parens = visitor::union_list_id(self.interner, inner_id).is_some()
                || visitor::intersection_list_id(self.interner, inner_id).is_some();
            if needs_parens {
                return format!("readonly ({inner_str})");
            }
            return format!("readonly {inner_str}");
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
        if let Some(sym_ref) = visitor::module_namespace_symbol_ref(self.interner, type_id) {
            if let Some(name) = self.print_namespace_reference(SymbolId(sym_ref.0)) {
                return name;
            }
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
            tsz_solver::types::LiteralValue::Number(n) => {
                let v = n.0;
                if v.is_infinite() {
                    if v.is_sign_positive() {
                        "Infinity".to_string()
                    } else {
                        "-Infinity".to_string()
                    }
                } else if v.is_nan() {
                    "NaN".to_string()
                } else {
                    v.to_string()
                }
            }
            tsz_solver::types::LiteralValue::Boolean(b) => b.to_string(),
            tsz_solver::types::LiteralValue::BigInt(atom) => {
                format!("{}n", self.resolve_atom(*atom))
            }
        }
    }

    fn print_object_type(&self, shape_id: tsz_solver::types::ObjectShapeId) -> String {
        let shape = self.interner.object_shape(shape_id);

        // If this object has a nominal symbol (class/interface instance), print the name.
        // Use the name when the symbol is visible (exported) or reachable (module-level).
        if let Some(sym_id) = shape.symbol
            && self.can_reference_symbol_by_name(sym_id)
            && let Some(name) = self.resolve_symbol_qualified_name(sym_id)
        {
            return name;
        }

        if let Some(sym_id) = shape.symbol
            && let Some(arena) = self.symbol_arena
            && let Some(symbol) = arena.get(sym_id)
            && !symbol.has_any_flags(symbol_flags::MODULE)
            && let Some(name) = self.print_named_symbol_reference(sym_id, false)
        {
            return name;
        }

        if let Some(sym_id) = shape.symbol
            && let Some(arena) = self.symbol_arena
            && let Some(symbol) = arena.get(sym_id)
            && symbol.has_any_flags(symbol_flags::MODULE)
            && let Some(name) = self.print_namespace_reference(sym_id)
        {
            return name;
        }

        let has_index = shape.string_index.is_some() || shape.number_index.is_some();

        if shape.properties.is_empty() && !has_index {
            return "{}".to_string();
        }

        // Filter out internal properties that tsc strips from .d.ts output:
        // - `prototype`: class constructor prototype property
        // - `__private_brand_*`: internal private member brand fields
        let should_skip_property = |prop: &tsz_solver::types::PropertyInfo| {
            let name = self.resolve_atom(prop.name);
            name == "prototype" || name.starts_with("__private_brand_")
        };

        // When indent context is set, format as multi-line (matching tsc's .d.ts output)
        if let Some(indent) = self.indent_level {
            let member_indent = "    ".repeat((indent + 1) as usize);
            let closing_indent = "    ".repeat(indent as usize);

            // Create a nested printer with incremented indent for property types
            let mut nested = self.clone();
            nested.indent_level = Some(indent + 1);

            let mut lines = Vec::new();

            // Emit index signatures first
            if let Some(ref idx) = shape.string_index {
                let mut line = String::new();
                line.push_str(&member_indent);
                if idx.readonly {
                    line.push_str("readonly ");
                }
                let param = idx
                    .param_name
                    .map(|a| self.resolve_atom(a))
                    .unwrap_or_else(|| "x".to_string());
                line.push_str(&format!(
                    "[{}: string]: {};",
                    param,
                    nested.print_type(idx.value_type)
                ));
                lines.push(line);
            }
            if let Some(ref idx) = shape.number_index {
                let mut line = String::new();
                line.push_str(&member_indent);
                if idx.readonly {
                    line.push_str("readonly ");
                }
                let param = idx
                    .param_name
                    .map(|a| self.resolve_atom(a))
                    .unwrap_or_else(|| "x".to_string());
                line.push_str(&format!(
                    "[{}: number]: {};",
                    param,
                    nested.print_type(idx.value_type)
                ));
                lines.push(line);
            }

            // Sort properties by declaration order when any have non-zero order,
            // otherwise fall back to the interning order (sorted by name).
            let has_decl_order = shape.properties.iter().any(|p| p.declaration_order > 0);
            let mut sorted_props;
            let props: &[tsz_solver::types::PropertyInfo] = if has_decl_order {
                sorted_props = shape.properties.clone();
                sorted_props.sort_by_key(|p| p.declaration_order);
                &sorted_props
            } else {
                &shape.properties
            };

            for property in props {
                if should_skip_property(property) {
                    continue;
                }
                let mut line = String::new();
                line.push_str(&member_indent);

                if property.is_method
                    && let Some(method_str) = nested.print_property_as_method(property)
                {
                    line.push_str(&method_str);
                    line.push(';');
                    lines.push(line);
                    continue;
                }

                if let Some(accessors) = nested.print_property_as_accessors(property) {
                    for accessor in accessors {
                        let mut accessor_line = String::new();
                        accessor_line.push_str(&member_indent);
                        accessor_line.push_str(&accessor);
                        accessor_line.push(';');
                        lines.push(accessor_line);
                    }
                    continue;
                }

                // Readonly marker
                if property.readonly {
                    line.push_str("readonly ");
                }

                // Property name (quote if needed)
                let name = self.resolve_atom(property.name);
                if needs_property_name_quoting(&name) {
                    line.push_str(&quote_property_name(&name));
                } else {
                    line.push_str(&name);
                }

                // Optional marker
                if property.optional {
                    line.push('?');
                }

                // Property type
                line.push_str(": ");
                line.push_str(&nested.print_type(property.type_id));

                line.push(';');
                lines.push(line);
            }

            format!("{{\n{}\n{}}}", lines.join("\n"), closing_indent)
        } else {
            // Flat format when no indent context (non-DTS usage)
            let mut members = Vec::new();

            // Emit index signatures first
            if let Some(ref idx) = shape.string_index {
                let mut member = String::new();
                if idx.readonly {
                    member.push_str("readonly ");
                }
                let param = idx
                    .param_name
                    .map(|a| self.resolve_atom(a))
                    .unwrap_or_else(|| "x".to_string());
                member.push_str(&format!(
                    "[{}: string]: {}",
                    param,
                    self.print_type(idx.value_type)
                ));
                members.push(member);
            }
            if let Some(ref idx) = shape.number_index {
                let mut member = String::new();
                if idx.readonly {
                    member.push_str("readonly ");
                }
                let param = idx
                    .param_name
                    .map(|a| self.resolve_atom(a))
                    .unwrap_or_else(|| "x".to_string());
                member.push_str(&format!(
                    "[{}: number]: {}",
                    param,
                    self.print_type(idx.value_type)
                ));
                members.push(member);
            }

            // Sort properties by declaration order when available
            let has_decl_order = shape.properties.iter().any(|p| p.declaration_order > 0);
            let mut sorted_props_flat;
            let props_flat: &[tsz_solver::types::PropertyInfo] = if has_decl_order {
                sorted_props_flat = shape.properties.clone();
                sorted_props_flat.sort_by_key(|p| p.declaration_order);
                &sorted_props_flat
            } else {
                &shape.properties
            };

            for property in props_flat {
                if should_skip_property(property) {
                    continue;
                }
                let mut member = String::new();

                // Try to emit as method syntax if the property is a method
                if property.is_method
                    && let Some(method_str) = self.print_property_as_method(property)
                {
                    member.push_str(&method_str);
                    members.push(member);
                    continue;
                }

                if let Some(accessors) = self.print_property_as_accessors(property) {
                    members.extend(accessors);
                    continue;
                }

                // Readonly modifier
                if property.readonly {
                    member.push_str("readonly ");
                }

                // Property name (quote if needed)
                let name = self.resolve_atom(property.name);
                if needs_property_name_quoting(&name) {
                    member.push('"');
                    member.push_str(&name);
                    member.push('"');
                } else {
                    member.push_str(&name);
                }

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
    }

    /// Print a property as method syntax: `name(params): ret` instead of `name: (params) => ret`.
    /// Returns `None` if the property's type is not a function shape.
    fn print_property_as_method(
        &self,
        property: &tsz_solver::types::PropertyInfo,
    ) -> Option<String> {
        let name = self.resolve_atom(property.name);
        let printed_name = if needs_property_name_quoting(&name) {
            quote_property_name(&name)
        } else {
            name
        };

        if let Some(func_id) = visitor::function_shape_id(self.interner, property.type_id) {
            let func_shape = self.interner.function_shape(func_id);
            return Some(self.print_method_signature(
                &printed_name,
                property.optional,
                &func_shape.type_params,
                &func_shape.params,
                func_shape.type_predicate.as_ref(),
                func_shape.return_type,
            ));
        }

        let callable_id = visitor::callable_shape_id(self.interner, property.type_id)?;
        let callable = self.interner.callable_shape(callable_id);
        if callable.call_signatures.len() != 1
            || !callable.construct_signatures.is_empty()
            || callable.string_index.is_some()
            || callable.number_index.is_some()
            || callable.properties.iter().any(|prop| {
                let prop_name = self.resolve_atom(prop.name);
                prop_name != "prototype" && !prop_name.starts_with("__private_brand_")
            })
        {
            return None;
        }

        let sig = &callable.call_signatures[0];
        Some(self.print_method_signature(
            &printed_name,
            property.optional,
            &sig.type_params,
            &sig.params,
            sig.type_predicate.as_ref(),
            sig.return_type,
        ))
    }

    fn print_property_as_accessors(
        &self,
        property: &tsz_solver::types::PropertyInfo,
    ) -> Option<Vec<String>> {
        if property.is_method || !self.property_is_accessor(property) {
            return None;
        }

        let name = self.resolve_atom(property.name);
        let printed_name = if needs_property_name_quoting(&name) {
            quote_property_name(&name)
        } else {
            name
        };

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

    fn property_is_accessor(&self, property: &tsz_solver::types::PropertyInfo) -> bool {
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

    fn class_declares_accessor(
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

    fn node_name_matches_atom(
        &self,
        node_arena: &NodeArena,
        name_idx: tsz_parser::NodeIndex,
        property_name: Atom,
    ) -> bool {
        node_arena.get_identifier_at(name_idx).is_some_and(|ident| {
            node_arena.resolve_identifier_text(ident) == self.resolve_atom(property_name)
        })
    }

    fn print_method_signature(
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

        if !type_params.is_empty() {
            let params: Vec<String> = type_params
                .iter()
                .map(|tp| self.print_type_parameter_decl(tp))
                .collect();
            result.push('<');
            result.push_str(&params.join(", "));
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
                result.push_str(&self.resolve_atom(name));
                if param.optional {
                    result.push('?');
                }
                result.push_str(": ");
            }
            result.push_str(&self.print_type(param.type_id));
        }
        result.push(')');

        result.push_str(": ");
        if let Some(pred) = type_predicate {
            result.push_str(&self.print_type_predicate(pred));
        } else {
            result.push_str(&self.print_type(return_type));
        }

        result
    }

    fn print_union(&self, type_list_id: tsz_solver::types::TypeListId) -> String {
        let types = self.interner.type_list(type_list_id);
        if types.is_empty() {
            return "never".to_string();
        }

        let mut parts = Vec::with_capacity(types.len());
        for &type_id in types.iter() {
            let s = self.print_type(type_id);
            // Parenthesize function/constructor types in union position
            if visitor::function_shape_id(self.interner, type_id).is_some() {
                parts.push(format!("({s})"));
            } else {
                parts.push(s);
            }
        }

        // Join with " | "
        parts.join(" | ")
    }

    fn print_intersection(&self, type_list_id: tsz_solver::types::TypeListId) -> String {
        let types = self.interner.type_list(type_list_id);
        if types.is_empty() {
            return "unknown".to_string(); // Intersection of 0 types is unknown
        }

        let mut members: Vec<(u8, String)> = Vec::with_capacity(types.len());
        for &type_id in types.iter() {
            let s = self.print_type(type_id);
            // Parenthesize function/constructor types in intersection position
            if visitor::function_shape_id(self.interner, type_id).is_some() {
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
                .map(|tp| self.print_type_parameter_decl(tp))
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
                if param.optional {
                    param_str.push('?');
                }
                param_str.push_str(": ");
            }

            // Parameter type
            param_str.push_str(&self.print_type(param.type_id));

            params.push(param_str);
        }

        // Return type (with type predicate if present)
        let return_str = if let Some(ref pred) = func_shape.type_predicate {
            self.print_type_predicate(pred)
        } else {
            self.print_type(func_shape.return_type)
        };

        format!(
            "{}({}) => {}",
            type_params_str,
            params.join(", "),
            return_str
        )
    }

    fn print_callable(&self, callable_id: tsz_solver::types::CallableShapeId) -> String {
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
        let has_properties = callable.properties.iter().any(|p| {
            let name = self.resolve_atom(p.name);
            name != "prototype" && !name.starts_with("__private_brand_")
        });
        if callable.call_signatures.len() == 1
            && callable.construct_signatures.is_empty()
            && !has_properties
            && callable.string_index.is_none()
            && callable.number_index.is_none()
        {
            return self.print_call_signature_arrow(&callable.call_signatures[0]);
        }

        if callable.is_abstract
            && callable.call_signatures.is_empty()
            && callable.construct_signatures.len() == 1
            && !has_properties
            && callable.string_index.is_none()
            && callable.number_index.is_none()
        {
            return self.print_construct_signature_arrow(&callable.construct_signatures[0]);
        }

        // Collect all signatures (call + construct)
        let mut parts = Vec::new();

        for sig in &callable.call_signatures {
            parts.push(self.print_call_signature(sig, false, false));
        }
        for sig in &callable.construct_signatures {
            parts.push(self.print_call_signature(sig, true, callable.is_abstract));
        }

        // Add properties (filter out internal props tsc strips from .d.ts)
        for prop in &callable.properties {
            let name = self.resolve_atom(prop.name);
            if name == "prototype" || name.starts_with("__private_brand_") {
                continue;
            }

            // Try to emit as method syntax if the property is a method
            if prop.is_method
                && let Some(method_str) = self.print_property_as_method(prop)
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
            let quoted_name = if needs_property_name_quoting(&name) {
                quote_property_name(&name)
            } else {
                name
            };
            parts.push(format!(
                "{}{}{}: {}",
                readonly,
                quoted_name,
                optional,
                self.print_type(prop.type_id)
            ));
        }

        // Add index signatures
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

    fn print_call_signature(
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

        let type_params_str = if !sig.type_params.is_empty() {
            let params: Vec<String> = sig
                .type_params
                .iter()
                .map(|tp| self.print_type_parameter_decl(tp))
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

        // Use incremented indent for the return type so nested objects/callables
        // are properly indented relative to the signature line.
        let mut nested = self.clone();
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
    fn print_call_signature_arrow(&self, sig: &tsz_solver::types::CallSignature) -> String {
        let type_params_str = if !sig.type_params.is_empty() {
            let params: Vec<String> = sig
                .type_params
                .iter()
                .map(|tp| self.print_type_parameter_decl(tp))
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

        let mut nested = self.clone();
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

    fn print_construct_signature_arrow(&self, sig: &tsz_solver::types::CallSignature) -> String {
        let type_params_str = if !sig.type_params.is_empty() {
            let params: Vec<String> = sig
                .type_params
                .iter()
                .map(|tp| self.print_type_parameter_decl(tp))
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

        let mut nested = self.clone();
        if let Some(indent) = nested.indent_level {
            nested.indent_level = Some(indent.saturating_sub(2));
        }
        let return_str = if let Some(ref pred) = sig.type_predicate {
            nested.print_type_predicate(pred)
        } else {
            nested.print_type(sig.return_type)
        };

        format!(
            "abstract new {}({}) => {}",
            type_params_str,
            params.join(", "),
            return_str
        )
    }

    /// Print a type predicate (e.g., `x is string`, `asserts x is string`, `this is Foo`)
    fn print_type_predicate(&self, pred: &tsz_solver::types::TypePredicate) -> String {
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
    fn print_type_parameter(&self, param_info: &tsz_solver::types::TypeParamInfo) -> String {
        self.resolve_atom(param_info.name)
    }

    /// Print a type parameter declaration with constraint and default.
    /// Used in `<T extends Foo = Bar>` positions.
    fn print_type_parameter_decl(&self, param_info: &tsz_solver::types::TypeParamInfo) -> String {
        let mut result = String::new();

        if param_info.is_const {
            result.push_str("const ");
        }

        result.push_str(&self.resolve_atom(param_info.name));

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
            if let Some(name) = self.print_named_symbol_reference(sym_id, needs_typeof) {
                return name;
            }
        }

        // Symbol is not visible or we don't have symbol info.
        // Fallback to `any` when we cannot legally name the referenced type.
        "any".to_string()
    }

    /// Check if a symbol is a global (ambient) type that's always accessible.
    /// Global types like Object, Array, Function, etc. have no parent symbol
    /// (parent == `SymbolId::NONE`) and are always referenceable in declarations.
    fn is_global_symbol(&self, sym_id: SymbolId) -> bool {
        let Some(arena) = self.symbol_arena else {
            return false;
        };
        let Some(symbol) = arena.get(sym_id) else {
            return false;
        };
        !symbol.parent.is_some()
            && self.resolve_symbol_module_path(sym_id).is_none()
            && !(symbol.has_any_flags(symbol_flags::ALIAS) && symbol.import_module.is_some())
    }

    fn intersection_member_priority(&self, type_id: TypeId) -> u8 {
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

    fn print_enum(&self, def_id: tsz_solver::def::DefId, _members_id: TypeId) -> String {
        // Try to resolve the enum name via DefId -> SymbolId -> symbol name
        if let Some(cache) = self.type_cache
            && let Some(&sym_id) = cache.def_to_symbol.get(&def_id)
            && let Some(name) = self.print_named_symbol_reference(sym_id, false) {
                return name;
            }
        // Fallback: print the member type structure
        format!("enum({})", def_id.0)
    }

    fn print_type_application(&self, app_id: tsz_solver::types::TypeApplicationId) -> String {
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
            let args: Vec<String> = app.args.iter().map(|&id| self.print_type(id)).collect();
            format!("{base_text}<{}>", args.join(", "))
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

/// Quote a property name with the appropriate quote style.
/// tsc uses double quotes for numeric-like strings (e.g. "-1", "0")
/// and for other non-identifier names.
fn quote_property_name(name: &str) -> String {
    format!("\"{name}\"")
}

/// Check if a property name needs quoting (contains spaces, hyphens, etc.)
/// Does NOT quote: valid identifiers, numeric literals, computed names `[...]`
fn needs_property_name_quoting(name: &str) -> bool {
    if name.is_empty() {
        return true;
    }
    // Computed property names like [Symbol.dispose] are emitted as-is
    if name.starts_with('[') && name.ends_with(']') {
        return false;
    }
    // Pure numeric names don't need quoting (e.g. 0, 1, 404)
    if name.chars().all(|ch| ch.is_ascii_digit()) {
        return false;
    }
    // Reserved keywords need quoting when used as property/method names
    if is_reserved_keyword(name) {
        return true;
    }
    let mut chars = name.chars();
    let first = chars.next().unwrap();
    if !(first == '_' || first == '$' || first.is_alphabetic()) {
        return true;
    }
    !chars.all(|ch| ch == '_' || ch == '$' || ch.is_alphanumeric())
}

/// Returns true if the name is a JS/TS reserved keyword that needs quoting in property position.
fn is_reserved_keyword(name: &str) -> bool {
    matches!(
        name,
        "break"
            | "case"
            | "catch"
            | "class"
            | "const"
            | "continue"
            | "debugger"
            | "default"
            | "delete"
            | "do"
            | "else"
            | "enum"
            | "export"
            | "extends"
            | "false"
            | "finally"
            | "for"
            | "function"
            | "if"
            | "import"
            | "in"
            | "instanceof"
            | "new"
            | "null"
            | "return"
            | "super"
            | "switch"
            | "this"
            | "throw"
            | "true"
            | "try"
            | "typeof"
            | "var"
            | "void"
            | "while"
            | "with"
    )
}

#[cfg(test)]
#[path = "../../../tests/type_printer.rs"]
mod tests;
