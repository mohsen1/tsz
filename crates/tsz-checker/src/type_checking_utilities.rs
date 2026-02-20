//! Type Checking Utilities Module
//!
//! This module contains parameter type utilities, type construction, and
//! type resolution methods for `CheckerState`.
//! Split from `type_checking.rs` for maintainability.

use crate::query_boundaries::type_checking_utilities as query;
use crate::state::CheckerState;
use tsz_binder::{SymbolId, symbol_flags};
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    // ============================================================================
    // Section 52: Parameter Type Utilities
    // ============================================================================

    /// Cache parameter types for function parameters.
    ///
    /// This function extracts and caches the types of function parameters,
    /// either from provided type annotations or from explicit type nodes.
    /// For parameters without explicit type annotations, `UNKNOWN` is used
    /// (not `ANY`) to maintain better type safety.
    ///
    /// ## Parameters:
    /// - `params`: Slice of parameter node indices
    /// - `param_types`: Optional pre-computed parameter types (e.g., from contextual typing)
    ///
    /// ## Examples:
    /// ```typescript
    /// // Explicit types: cached from type annotation
    /// function foo(x: string, y: number) {}
    ///
    /// // No types: cached as UNKNOWN
    /// function bar(a, b) {}
    ///
    /// // Contextual types: cached from provided types
    /// const fn = (x: string) => number;
    /// const cb: typeof fn = (x) => x.length;  // x typed from context
    /// ```
    pub(crate) fn cache_parameter_types(
        &mut self,
        params: &[NodeIndex],
        param_types: Option<&[Option<TypeId>]>,
    ) {
        let factory = self.ctx.types.factory();
        for (i, &param_idx) in params.iter().enumerate() {
            let Some(param_node) = self.ctx.arena.get(param_idx) else {
                continue;
            };
            let Some(param) = self.ctx.arena.get_parameter(param_node) else {
                continue;
            };

            let Some(sym_id) = self
                .ctx
                .binder
                .get_node_symbol(param.name)
                .or_else(|| self.ctx.binder.get_node_symbol(param_idx))
            else {
                continue;
            };
            self.push_symbol_dependency(sym_id, true);
            let type_id = if let Some(types) = param_types {
                // param_types already have optional undefined applied
                types.get(i).and_then(|t| *t)
            } else if !param.type_annotation.is_none() {
                let mut t = self.get_type_from_type_node(param.type_annotation);
                // Under strictNullChecks, optional parameters (with `?`) include
                // `undefined` in their type.  Parameters with only a default value
                // (no `?`) do NOT — the default guarantees a value at runtime.
                if param.question_token
                    && self.ctx.strict_null_checks()
                    && t != TypeId::ANY
                    && t != TypeId::UNKNOWN
                    && t != TypeId::ERROR
                {
                    t = factory.union(vec![t, TypeId::UNDEFINED]);
                }
                Some(t)
            } else {
                // Parameters without type annotations get implicit 'any' type.
                // TypeScript uses 'any' (with TS7006 when noImplicitAny is enabled).
                Some(TypeId::ANY)
            };
            self.pop_symbol_dependency();

            if let Some(type_id) = type_id {
                self.cache_symbol_type(sym_id, type_id);
            }
        }
    }

    /// Assign contextual types to destructuring parameters (binding patterns).
    ///
    /// When a function has a contextual type (e.g., from a callback position),
    /// destructuring parameters need to have their bindings inferred from
    /// the contextual parameter type.
    ///
    /// This function only processes parameters without explicit type annotations,
    /// as TypeScript respects explicit annotations over contextual inference.
    ///
    /// ## Examples:
    /// ```typescript
    /// declare function map<T, U>(arr: T[], fn: (item: T) => U): U[];
    ///
    /// // x and y types come from contextual type T
    /// map(arr, ({ x, y }) => x + y);
    ///
    /// // Explicit annotation takes precedence
    /// map(arr, ({ x, y }: { x: string; y: number }) => x + y);
    /// ```
    pub(crate) fn assign_contextual_types_to_destructuring_params(
        &mut self,
        params: &[NodeIndex],
        param_types: &[Option<TypeId>],
    ) {
        for (i, &param_idx) in params.iter().enumerate() {
            let Some(param_node) = self.ctx.arena.get(param_idx) else {
                continue;
            };
            let Some(param) = self.ctx.arena.get_parameter(param_node) else {
                continue;
            };

            // Skip if there's an explicit type annotation
            if !param.type_annotation.is_none() {
                continue;
            }

            let Some(name_node) = self.ctx.arena.get(param.name) else {
                continue;
            };

            // Only process binding patterns (destructuring)
            let is_binding_pattern = name_node.kind == syntax_kind_ext::OBJECT_BINDING_PATTERN
                || name_node.kind == syntax_kind_ext::ARRAY_BINDING_PATTERN;

            if !is_binding_pattern {
                continue;
            }

            // Get the contextual type for this parameter position
            let contextual_type = param_types
                .get(i)
                .and_then(|t| *t)
                .filter(|&t| t != TypeId::UNKNOWN && t != TypeId::ERROR);

            if let Some(ctx_type) = contextual_type {
                // Assign the contextual type to the binding pattern elements
                self.assign_binding_pattern_symbol_types(param.name, ctx_type);
            }
        }
    }

    /// Record destructured parameter binding groups for correlated narrowing.
    ///
    /// This enables cases like:
    /// `function f({ data, isSuccess }: Result) { if (isSuccess) data... }`
    /// where narrowing one binding should narrow sibling bindings from the same source union.
    pub(crate) fn record_destructured_parameter_binding_groups(
        &mut self,
        params: &[NodeIndex],
        param_types: &[Option<TypeId>],
    ) {
        use crate::query_boundaries::state_checking as query;

        for (i, &param_idx) in params.iter().enumerate() {
            let Some(param_node) = self.ctx.arena.get(param_idx) else {
                continue;
            };
            let Some(param) = self.ctx.arena.get_parameter(param_node) else {
                continue;
            };
            let Some(name_node) = self.ctx.arena.get(param.name) else {
                continue;
            };

            let is_binding_pattern = name_node.kind == syntax_kind_ext::OBJECT_BINDING_PATTERN
                || name_node.kind == syntax_kind_ext::ARRAY_BINDING_PATTERN;
            if !is_binding_pattern {
                continue;
            }

            let Some(param_type) = param_types.get(i).and_then(|t| *t) else {
                continue;
            };
            if param_type == TypeId::UNKNOWN || param_type == TypeId::ERROR {
                continue;
            }

            let resolved_for_union = self.evaluate_type_for_assignability(param_type);
            if query::union_members(self.ctx.types, resolved_for_union).is_none() {
                continue;
            }

            // Parameters with binding patterns are treated as stable for correlated
            // narrowing, matching TypeScript's alias-aware flow behavior.
            self.record_destructured_binding_group(
                param.name,
                resolved_for_union,
                true,
                name_node.kind,
            );
        }
    }

    // ============================================================================
    // Section 53: Type and Symbol Utilities
    // ============================================================================

    /// Widen a literal type to its primitive type.
    ///
    /// This function converts literal types to their corresponding primitive types,
    /// which is used for type widening in various contexts:
    /// - Variable declarations without type annotations
    /// - Property assignments
    /// - Return type inference
    ///
    /// ## Examples:
    /// ```typescript
    /// // Literal types are widened to primitives:
    /// let x = "hello";  // Type: string (not "hello")
    /// let y = 42;       // Type: number (not 42)
    /// let z = true;     // Type: boolean (not true)
    /// ```
    pub(crate) fn widen_literal_type(&self, type_id: TypeId) -> TypeId {
        tsz_solver::widening::widen_type(self.ctx.types, type_id)
    }

    /// Widen a mutable binding initializer type (let/var semantics).
    ///
    /// In addition to primitive literal widening, TypeScript widens enum member
    /// initializers (`let x = E.A`) to the parent enum type (`E`), not the
    /// specific member.
    pub(crate) fn widen_initializer_type_for_mutable_binding(&mut self, type_id: TypeId) -> TypeId {
        use tsz_solver::type_queries;

        // Check if this is an enum member type that should widen to parent enum
        if let Some(def_id) = type_queries::get_enum_def_id(self.ctx.types, type_id) {
            // Check if this DefId is an enum member (has a parent enum)
            let parent_def_id = self
                .ctx
                .type_env
                .try_borrow()
                .ok()
                .and_then(|env| env.get_enum_parent(def_id));

            if let Some(parent_def_id) = parent_def_id {
                // This is an enum member - widen to parent enum type
                if let Some(parent_sym_id) = self.ctx.def_to_symbol_id(parent_def_id) {
                    return self.get_type_of_symbol(parent_sym_id);
                }
            }
        }

        // Fallback: check via symbol flags (legacy path)
        if let Some(sym_id) = self.ctx.resolve_type_to_symbol_id(type_id)
            && let Some(symbol) = self.ctx.binder.get_symbol(sym_id)
            && (symbol.flags & tsz_binder::symbol_flags::ENUM_MEMBER) != 0
        {
            return self.get_type_of_symbol(symbol.parent);
        }
        self.widen_literal_type(type_id)
    }

    /// Check if a type is an enum member type (not the parent enum type).
    ///
    /// Enum member types (e.g., `Colors.Red`) should widen to the parent enum type
    /// when assigned to mutable bindings, even if they're not "fresh" literals.
    pub(crate) fn is_enum_member_type_for_widening(&self, type_id: TypeId) -> bool {
        use tsz_solver::type_queries;

        if let Some(def_id) = type_queries::get_enum_def_id(self.ctx.types, type_id) {
            // Check if this DefId has a parent (meaning it's a member, not the enum itself)
            return self
                .ctx
                .type_env
                .try_borrow()
                .ok()
                .is_some_and(|env| env.get_enum_parent(def_id).is_some());
        }
        false
    }

    /// Check if an expression produces a "fresh" literal type that should be widened.
    ///
    /// In TypeScript, literal types created from literal expressions are "fresh" and get
    /// widened when assigned to mutable bindings (let/var). Literal types from other
    /// sources (variable references, type annotations, narrowing) are "non-fresh" and
    /// should NOT be widened.
    ///
    /// ## Examples:
    /// ```typescript
    /// let x = "foo";          // "foo" is fresh → widened to string
    /// let a: "foo" = "foo";
    /// let y = a;              // a's type is non-fresh → y: "foo" (not widened)
    /// let z = a || "bar";     // result from || is non-fresh → z: "foo" (not widened)
    /// ```
    pub(crate) fn is_fresh_literal_expression(&self, idx: NodeIndex) -> bool {
        use tsz_parser::parser::syntax_kind_ext;
        use tsz_scanner::SyntaxKind;

        let Some(node) = self.ctx.arena.get(idx) else {
            return false;
        };

        let kind = node.kind;

        // Direct literal tokens are always fresh
        if kind == SyntaxKind::StringLiteral as u16
            || kind == SyntaxKind::NumericLiteral as u16
            || kind == SyntaxKind::BigIntLiteral as u16
            || kind == SyntaxKind::TrueKeyword as u16
            || kind == SyntaxKind::FalseKeyword as u16
            || kind == SyntaxKind::NullKeyword as u16
            || kind == SyntaxKind::NoSubstitutionTemplateLiteral as u16
        {
            return true;
        }

        // Parenthesized expressions inherit freshness from inner expression
        if kind == syntax_kind_ext::PARENTHESIZED_EXPRESSION
            && let Some(paren) = self.ctx.arena.get_parenthesized(node)
        {
            return self.is_fresh_literal_expression(paren.expression);
        }

        // Prefix unary (+/-) on numeric/bigint literals are fresh
        if kind == syntax_kind_ext::PREFIX_UNARY_EXPRESSION
            && let Some(prefix) = self.ctx.arena.get_unary_expr(node)
        {
            let op = prefix.operator;
            if op == SyntaxKind::PlusToken as u16 || op == SyntaxKind::MinusToken as u16 {
                return self.is_fresh_literal_expression(prefix.operand);
            }
        }

        // Conditional expressions: fresh if both branches produce fresh types
        if kind == syntax_kind_ext::CONDITIONAL_EXPRESSION
            && let Some(cond) = self.ctx.arena.get_conditional_expr(node)
        {
            return self.is_fresh_literal_expression(cond.when_true)
                && self.is_fresh_literal_expression(cond.when_false);
        }

        // Object and array literals need widening (property types get widened)
        if kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
            || kind == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION
        {
            return true;
        }

        // Template expressions (with substitutions) produce string, which doesn't need widening
        // but we mark them fresh for consistency
        if kind == syntax_kind_ext::TEMPLATE_EXPRESSION {
            return true;
        }

        // Everything else (identifiers, call expressions, binary expressions, etc.)
        // produces non-fresh types that should NOT be widened
        false
    }

    /// Map an expanded argument index back to the original argument node index.
    ///
    /// This handles spread arguments that expand to multiple elements.
    /// When a spread argument has a tuple type, it expands to multiple positional
    /// arguments. This function maps from the expanded index back to the original
    /// argument node for error reporting purposes.
    ///
    /// ## Parameters:
    /// - `args`: Slice of argument node indices
    /// - `expanded_index`: Index in the expanded argument list
    ///
    /// ## Returns:
    /// - `Some(NodeIndex)`: The original argument node index
    /// - `None`: If the index doesn't map to a valid argument
    ///
    /// ## Examples:
    /// ```typescript
    /// function foo(a: string, b: number, c: boolean) {}
    /// const tuple = ["hello", 42, true] as const;
    /// // Spread expands to 3 arguments: foo(...tuple)
    /// // expanded_index 0, 1, 2 all map to the spread argument node
    /// ```
    pub(crate) fn map_expanded_arg_index_to_original(
        &self,
        args: &[NodeIndex],
        expanded_index: usize,
    ) -> Option<NodeIndex> {
        let mut current_expanded_index = 0;

        for &arg_idx in args {
            if let Some(arg_node) = self.ctx.arena.get(arg_idx) {
                // Check if this is a spread element
                if arg_node.kind == syntax_kind_ext::SPREAD_ELEMENT
                    && let Some(spread_data) = self.ctx.arena.get_spread(arg_node)
                {
                    // Try to get the cached type, fall back to looking up directly
                    let spread_type = self
                        .ctx
                        .node_types
                        .get(&spread_data.expression.0)
                        .copied()
                        .unwrap_or(TypeId::ANY);
                    let spread_type = self.resolve_type_for_property_access_simple(spread_type);

                    // If it's a tuple type, it expands to multiple elements
                    if let Some(elems_id) = query::tuple_list_id(self.ctx.types, spread_type) {
                        let elems = self.ctx.types.tuple_list(elems_id);
                        let end_index = current_expanded_index + elems.len();
                        if expanded_index >= current_expanded_index && expanded_index < end_index {
                            // The error is within this spread - report at the spread node
                            return Some(arg_idx);
                        }
                        current_expanded_index = end_index;
                        continue;
                    }
                }
            }

            // Non-spread or non-tuple spread: takes one slot
            if expanded_index == current_expanded_index {
                return Some(arg_idx);
            }
            current_expanded_index += 1;
        }

        None
    }

    /// Simple type resolution for property access - doesn't trigger new type computation.
    ///
    /// This function resolves type applications to their base type without
    /// triggering expensive type computation. It's used in contexts where we
    /// just need the base type for inspection, not full type resolution.
    ///
    /// ## Examples:
    /// ```typescript
    /// type Box<T> = { value: T };
    /// // Box<string> resolves to Box for property access inspection
    /// ```
    fn resolve_type_for_property_access_simple(&self, type_id: TypeId) -> TypeId {
        query::application_base(self.ctx.types, type_id).unwrap_or(type_id)
    }

    pub(crate) fn lookup_symbol_with_name(
        &self,
        sym_id: SymbolId,
        name_hint: Option<&str>,
    ) -> Option<(&tsz_binder::Symbol, &tsz_parser::parser::node::NodeArena)> {
        let name_hint = name_hint.map(str::trim).filter(|name| !name.is_empty());

        if let Some(symbol) = self.ctx.binder.symbols.get(sym_id)
            && name_hint.is_none_or(|name| symbol.escaped_name == name)
        {
            let arena = self
                .ctx
                .binder
                .symbol_arenas
                .get(&sym_id)
                .map_or(self.ctx.arena, |arena| arena.as_ref());
            return Some((symbol, arena));
        }

        if let Some(name) = name_hint {
            for lib_ctx in &self.ctx.lib_contexts {
                if let Some(symbol) = lib_ctx.binder.symbols.get(sym_id)
                    && symbol.escaped_name == name
                {
                    return Some((symbol, lib_ctx.arena.as_ref()));
                }
            }
            if let Some(symbol) = self.ctx.binder.get_symbol(sym_id)
                && symbol.escaped_name == name
            {
                let arena = self
                    .ctx
                    .binder
                    .symbol_arenas
                    .get(&sym_id)
                    .map_or(self.ctx.arena, |arena| arena.as_ref());
                return Some((symbol, arena));
            }
            return None;
        }

        if let Some(symbol) = self.ctx.binder.get_symbol(sym_id) {
            let arena = self
                .ctx
                .binder
                .symbol_arenas
                .get(&sym_id)
                .map_or(self.ctx.arena, |arena| arena.as_ref());
            return Some((symbol, arena));
        }

        for lib_ctx in &self.ctx.lib_contexts {
            if let Some(symbol) = lib_ctx.binder.symbols.get(sym_id) {
                return Some((symbol, lib_ctx.arena.as_ref()));
            }
        }

        None
    }

    /// Check if a symbol is value-only (has value but not type).
    ///
    /// This function distinguishes between symbols that can only be used as values
    /// vs. symbols that can be used as types. This is important for:
    /// - Import/export checking
    /// - Type position validation
    /// - Value expression validation
    ///
    /// ## Examples:
    /// ```typescript
    /// // Value-only symbols:
    /// const x = 42;  // x is value-only
    ///
    /// // Not value-only:
    /// type T = string;  // T is type-only
    /// interface Box {}  // Box is both type and value
    /// class Foo {}  // Foo is both type and value
    /// ```
    pub(crate) fn symbol_is_value_only(&self, sym_id: SymbolId, name_hint: Option<&str>) -> bool {
        let (symbol, arena) = match self.lookup_symbol_with_name(sym_id, name_hint) {
            Some(result) => result,
            None => return false,
        };

        // Fast path using symbol flags: if symbol has TYPE flag, it's not value-only
        // This handles classes, interfaces, enums, type aliases, etc.
        // TYPE flag includes: CLASS | INTERFACE | ENUM | ENUM_MEMBER | TYPE_LITERAL | TYPE_PARAMETER | TYPE_ALIAS
        let has_type_flag = (symbol.flags & symbol_flags::TYPE) != 0;
        if has_type_flag {
            return false;
        }

        // Modules/namespaces can be used as types in some contexts, but not if they're
        // merged with functions or other values (e.g., function+namespace declaration merging)
        // In such cases, the function/value takes precedence and TS2749 should be emitted
        let has_module = (symbol.flags & symbol_flags::MODULE) != 0;
        let has_function = (symbol.flags & symbol_flags::FUNCTION) != 0;
        // Exclude both FUNCTION and MODULE flags when checking for "other" value flags.
        // VALUE_MODULE is part of VALUE, but a symbol that only has module flags
        // (VALUE_MODULE | NAMESPACE_MODULE) should be treated as a pure namespace.
        let has_other_value = (symbol.flags
            & (symbol_flags::VALUE & !symbol_flags::FUNCTION & !symbol_flags::MODULE))
            != 0;

        // Pure namespace (MODULE only, no function/value flags) is not value-only
        if has_module && !has_function && !has_other_value {
            return false;
        }

        // Check declarations as a secondary source of truth (for cases where flags might not be set correctly)
        if self.symbol_has_type_declaration(symbol, arena) {
            return false;
        }

        // If the symbol is type-only (from `import type`), it's not value-only
        // In type positions, type-only imports should be allowed
        if symbol.is_type_only {
            return false;
        }

        // Finally, check if this is purely a value symbol (has VALUE but not TYPE)
        let has_value = (symbol.flags & symbol_flags::VALUE) != 0;
        let has_type = (symbol.flags & symbol_flags::TYPE) != 0;
        has_value && !has_type
    }

    /// Check if an alias resolves to a value-only symbol.
    ///
    /// This function follows alias chains to determine if the ultimate target
    /// is a value-only symbol. This is used for validating import/export aliases
    /// and type position checks.
    ///
    /// ## Examples:
    /// ```typescript
    /// // Original declarations
    /// const x = 42;
    /// type T = string;
    ///
    /// // Aliases
    /// import { x as xAlias } from "./mod";  // xAlias resolves to value-only
    /// import { type T as TAlias } from "./mod";  // TAlias is type-only
    /// ```
    pub(crate) fn alias_resolves_to_value_only(
        &self,
        sym_id: SymbolId,
        name_hint: Option<&str>,
    ) -> bool {
        let (symbol, _arena) = match self.lookup_symbol_with_name(sym_id, name_hint) {
            Some(result) => result,
            None => return false,
        };

        if symbol.flags & symbol_flags::ALIAS == 0 {
            return false;
        }

        // If the alias symbol itself is type-only, it doesn't resolve to value-only
        if symbol.is_type_only {
            return false;
        }

        let mut visited = Vec::new();
        let target = match self.resolve_alias_symbol(sym_id, &mut visited) {
            Some(target) => target,
            None => return false,
        };

        // symbol_is_value_only already checks TYPE flags and declarations
        // No need for redundant declaration check here
        let target_name = symbol
            .import_name
            .as_deref()
            .unwrap_or(symbol.escaped_name.as_str());
        self.symbol_is_value_only(target, Some(target_name))
    }

    fn symbol_has_type_declaration(
        &self,
        symbol: &tsz_binder::Symbol,
        arena: &tsz_parser::parser::node::NodeArena,
    ) -> bool {
        use tsz_parser::parser::syntax_kind_ext;

        for &decl in &symbol.declarations {
            if decl.is_none() {
                continue;
            }
            let Some(node) = arena.get(decl) else {
                continue;
            };
            match node.kind {
                k if k == syntax_kind_ext::INTERFACE_DECLARATION => return true,
                k if k == syntax_kind_ext::TYPE_ALIAS_DECLARATION => return true,
                k if k == syntax_kind_ext::CLASS_DECLARATION => return true,
                k if k == syntax_kind_ext::ENUM_DECLARATION => return true,
                _ => {}
            }
        }

        false
    }

    // ============================================================================
    // Section 54: Literal Key and Element Access Utilities
    // ============================================================================

    /// Extract literal keys from a type as string and number atom vectors.
    ///
    /// This function is used for element access type inference when the index
    /// type contains literal types. It extracts string and number literal values
    /// from single literals or unions of literals.
    ///
    /// ## Parameters:
    /// - `index_type`: The type to extract literal keys from
    ///
    /// ## Returns:
    /// - `Some((string_keys, number_keys))`: Tuple of string and number literal keys
    /// - `None`: If the type is not a literal or union of literals
    ///
    /// ## Examples:
    /// ```typescript
    /// // Single literal:
    /// type T1 = "foo";  // Returns: (["foo"], [])
    ///
    /// // Union of literals:
    /// type T2 = "a" | "b" | 1 | 2;  // Returns: (["a", "b"], [1.0, 2.0])
    ///
    /// // Non-literal type:
    /// type T3 = string;  // Returns: None
    /// ```
    pub(crate) fn get_literal_key_union_from_type(
        &self,
        index_type: TypeId,
    ) -> Option<(Vec<tsz_common::interner::Atom>, Vec<f64>)> {
        match query::literal_key_kind(self.ctx.types, index_type) {
            query::LiteralKeyKind::StringLiteral(atom) => Some((vec![atom], Vec::new())),
            query::LiteralKeyKind::NumberLiteral(num) => Some((Vec::new(), vec![num])),
            query::LiteralKeyKind::Union(members) => {
                let mut string_keys = Vec::with_capacity(members.len());
                let mut number_keys = Vec::new();
                for &member in &members {
                    match query::literal_key_kind(self.ctx.types, member) {
                        query::LiteralKeyKind::StringLiteral(atom) => string_keys.push(atom),
                        query::LiteralKeyKind::NumberLiteral(num) => number_keys.push(num),
                        _ => return None,
                    }
                }
                Some((string_keys, number_keys))
            }
            query::LiteralKeyKind::Other => None,
        }
    }

    /// Get element access type for literal string keys.
    ///
    /// This function computes the type of element access when the index is a
    /// string literal or union of string literals. It handles both property
    /// access and numeric array indexing (when strings represent numeric indices).
    ///
    /// ## Parameters:
    /// - `object_type`: The type of the object being accessed
    /// - `keys`: Slice of string literal keys to look up
    ///
    /// ## Returns:
    /// - `Some(TypeId)`: The union of all property/element types
    /// - `None`: If any property is not found or if keys is empty
    ///
    /// ## Examples:
    /// ```typescript
    /// const obj = { a: 1, b: "hello" };
    /// type T = obj["a" | "b"];  // number | string
    ///
    /// const arr = [1, 2, 3];
    /// type U = arr["0" | "1"];  // number (treated as numeric index)
    /// ```
    pub(crate) fn get_element_access_type_for_literal_keys(
        &mut self,
        object_type: TypeId,
        keys: &[tsz_common::interner::Atom],
    ) -> Option<TypeId> {
        use tsz_solver::operations_property::PropertyAccessResult;

        if keys.is_empty() {
            return None;
        }

        // Resolve type references (Ref, TypeQuery, etc.) before property access lookup
        let resolved_type = self.resolve_type_for_property_access(object_type);
        if resolved_type == TypeId::ANY {
            return Some(TypeId::ANY);
        }
        if resolved_type == TypeId::ERROR {
            return None;
        }

        let numeric_as_index = self.is_array_like_type(resolved_type);
        let mut types = Vec::with_capacity(keys.len());

        for &key in keys {
            let name = self.ctx.types.resolve_atom(key);
            if numeric_as_index && let Some(index) = self.get_numeric_index_from_string(&name) {
                let element_type =
                    self.get_element_access_type(resolved_type, TypeId::NUMBER, Some(index));
                types.push(element_type);
                continue;
            }

            match self.ctx.types.property_access_type(resolved_type, &name) {
                PropertyAccessResult::Success { type_id, .. } => types.push(type_id),
                PropertyAccessResult::PossiblyNullOrUndefined { property_type, .. } => {
                    types.push(property_type.unwrap_or(TypeId::UNKNOWN));
                }
                // IsUnknown: Return None to signal that property access on unknown failed
                // The caller has node context and will report TS2571 error
                PropertyAccessResult::IsUnknown | PropertyAccessResult::PropertyNotFound { .. } => {
                    return None;
                }
            }
        }

        Some(tsz_solver::utils::union_or_single(self.ctx.types, types))
    }

    /// Get element access type for literal number keys.
    ///
    /// This function computes the type of element access when the index is a
    /// number literal or union of number literals. It handles array/tuple
    /// indexing with literal numeric values.
    ///
    /// ## Parameters:
    /// - `object_type`: The type of the object being accessed
    /// - `keys`: Slice of numeric literal keys to look up
    ///
    /// ## Returns:
    /// - `Some(TypeId)`: The union of all element types
    /// - `None`: If keys is empty
    ///
    /// ## Examples:
    /// ```typescript
    /// const arr = [1, "hello", true];
    /// type T = arr[0 | 1];  // number | string
    ///
    /// const tuple = [1, 2] as const;
    /// type U = tuple[0 | 1];  // 1 | 2
    /// ```
    pub(crate) fn get_element_access_type_for_literal_number_keys(
        &mut self,
        object_type: TypeId,
        keys: &[f64],
    ) -> Option<TypeId> {
        if keys.is_empty() {
            return None;
        }

        let mut types = Vec::with_capacity(keys.len());
        for &value in keys {
            if let Some(index) = self.get_numeric_index_from_number(value) {
                types.push(self.get_element_access_type(object_type, TypeId::NUMBER, Some(index)));
            } else {
                return Some(self.get_element_access_type(object_type, TypeId::NUMBER, None));
            }
        }

        Some(tsz_solver::utils::union_or_single(self.ctx.types, types))
    }

    /// Check if a type is array-like (supports numeric indexing).
    ///
    /// This function determines if a type supports numeric element access,
    /// including arrays, tuples, and unions/intersections of array-like types.
    ///
    /// ## Array-like Types:
    /// - Array types: `T[]`, `Array<T>`
    /// - Tuple types: `[T1, T2, ...]`
    /// - Readonly arrays: `readonly T[]`, `ReadonlyArray<T>`
    /// - Unions where all members are array-like
    /// - Intersections where any member is array-like
    ///
    /// ## Examples:
    /// ```typescript
    /// // Array-like types:
    /// type A = number[];
    /// type B = [string, number];
    /// type C = readonly boolean[];
    /// type D = A | B;  // Union of array-like types
    ///
    /// // Not array-like:
    /// type E = { [key: string]: number };  // Index signature, not array-like
    /// ```
    pub(crate) fn is_array_like_type(&self, object_type: TypeId) -> bool {
        // Check for array/tuple types directly
        if self.is_array_type(object_type) {
            return true;
        }

        match query::classify_array_like(self.ctx.types, object_type) {
            query::ArrayLikeKind::Array(_) | query::ArrayLikeKind::Tuple => true,
            query::ArrayLikeKind::Readonly(inner) => self.is_array_like_type(inner),
            query::ArrayLikeKind::Union(members) => members
                .iter()
                .all(|&member| self.is_array_like_type(member)),
            query::ArrayLikeKind::Intersection(members) => members
                .iter()
                .any(|&member| self.is_array_like_type(member)),
            query::ArrayLikeKind::Other => false,
        }
    }

    /// Check if an index signature error should be reported for element access.
    ///
    /// This function determines whether a "No index signature" error should be
    /// emitted for element access on an object type. This happens when:
    /// - The object type doesn't have an appropriate index signature
    /// - The index type is a literal or union of literals
    /// - The access is not valid property access
    ///
    /// ## Parameters:
    /// - `object_type`: The type of the object being accessed
    /// - `index_type`: The type of the index expression
    /// - `literal_index`: Optional explicit numeric index
    ///
    /// ## Returns:
    /// - `true`: Report "No index signature" error
    /// - `false`: Don't report (has index signature, or any/unknown type)
    ///
    /// ## Examples:
    /// ```typescript
    /// const obj = { a: 1, b: 2 };
    /// obj["c"];  // Error: No index signature with parameter of type '"c"'
    ///
    /// const obj2: { [key: string]: number } = { a: 1 };
    /// obj2["c"];  // OK: Has string index signature
    /// ```
    pub(crate) fn should_report_no_index_signature(
        &self,
        object_type: TypeId,
        index_type: TypeId,
        literal_index: Option<usize>,
    ) -> bool {
        if object_type == TypeId::ANY
            || object_type == TypeId::UNKNOWN
            || object_type == TypeId::ERROR
        {
            return false;
        }

        if index_type == TypeId::ANY || index_type == TypeId::UNKNOWN {
            return false;
        }

        let index_key_kind = self.get_index_key_kind(index_type);
        let wants_number = literal_index.is_some()
            || index_key_kind
                .as_ref()
                .is_some_and(|(_, wants_number)| *wants_number);
        let wants_string = index_key_kind
            .as_ref()
            .is_some_and(|(wants_string, _)| *wants_string);
        if !wants_number && !wants_string {
            return false;
        }

        let unwrapped_type = query::unwrap_readonly_for_lookup(self.ctx.types, object_type);

        !self.is_element_indexable(unwrapped_type, wants_string, wants_number)
    }

    /// Determine what kind of index key a type represents.
    ///
    /// This function analyzes a type to determine if it can be used for string
    /// or numeric indexing. Returns a tuple of (`wants_string`, `wants_number`).
    ///
    /// ## Returns:
    /// - `Some((true, false))`: String index (e.g., `"foo"`, `string`)
    /// - `Some((false, true))`: Number index (e.g., `42`, `number`)
    /// - `Some((true, true))`: Both string and number (e.g., `"a" | 1 | 2`)
    /// - `None`: Not an index type
    ///
    /// ## Examples:
    /// ```typescript
    /// type A = "foo";        // (true, false) - string literal
    /// type B = 42;           // (false, true) - number literal
    /// type C = string;       // (true, false) - string type
    /// type D = "a" | "b";    // (true, false) - union of strings
    /// type E = "a" | 1;      // (true, true) - mixed literals
    /// ```
    pub(crate) fn get_index_key_kind(&self, index_type: TypeId) -> Option<(bool, bool)> {
        match query::classify_index_key(self.ctx.types, index_type) {
            query::IndexKeyKind::String | query::IndexKeyKind::StringLiteral => Some((true, false)),
            query::IndexKeyKind::Number | query::IndexKeyKind::NumberLiteral => Some((false, true)),
            query::IndexKeyKind::Union(members) => {
                let mut wants_string = false;
                let mut wants_number = false;
                for member in members {
                    let (member_string, member_number) = self.get_index_key_kind(member)?;
                    wants_string |= member_string;
                    wants_number |= member_number;
                }
                Some((wants_string, wants_number))
            }
            query::IndexKeyKind::Other => None,
        }
    }

    /// Check if a type key supports element indexing.
    ///
    /// This function determines if a type supports element access with the
    /// specified index kind (string, number, or both).
    ///
    /// ## Parameters:
    /// - `object_key`: The type key to check
    /// - `wants_string`: Whether string indexing is needed
    /// - `wants_number`: Whether numeric indexing is needed
    ///
    /// ## Returns:
    /// - `true`: The type supports the requested indexing
    /// - `false`: The type does not support the requested indexing
    ///
    /// ## Examples:
    /// ```typescript
    /// // Array supports numeric indexing:
    /// const arr: number[] = [1, 2, 3];
    /// arr[0];  // OK
    ///
    /// // Object with string index supports string indexing:
    /// const obj: { [key: string]: number } = {};
    /// obj["foo"];  // OK
    ///
    /// // Object without index signature doesn't support indexing:
    /// const plain: { a: number } = { a: 1 };
    /// plain["b"];  // Error: No index signature
    /// ```
    fn is_element_indexable(
        &self,
        object_type: TypeId,
        wants_string: bool,
        wants_number: bool,
    ) -> bool {
        match query::classify_element_indexable(self.ctx.types, object_type) {
            query::ElementIndexableKind::Array
            | query::ElementIndexableKind::Tuple
            | query::ElementIndexableKind::StringLike => wants_number,
            query::ElementIndexableKind::ObjectWithIndex {
                has_string,
                has_number,
            } => (wants_string && has_string) || (wants_number && (has_number || has_string)),
            query::ElementIndexableKind::Union(members) => members
                .iter()
                .all(|&member| self.is_element_indexable(member, wants_string, wants_number)),
            query::ElementIndexableKind::Intersection(members) => members
                .iter()
                .any(|&member| self.is_element_indexable(member, wants_string, wants_number)),
            query::ElementIndexableKind::Other => false,
        }
    }

    // ============================================================================
    // Section 55: Return Type Inference Utilities
    // ============================================================================

    /// Check if a function body falls through (doesn't always return).
    ///
    /// This function determines whether a function body might fall through
    /// without an explicit return statement. This is important for return type
    /// inference and validating function return annotations.
    ///
    /// ## Returns:
    /// - `true`: The function might fall through (no guaranteed return)
    /// - `false`: The function always returns (has return in all code paths)
    ///
    /// ## Examples:
    /// ```typescript
    /// // Falls through:
    /// function foo() {  // No return statement
    /// }
    ///
    /// function bar() {
    ///     if (cond) { return 1; }  // Might not return
    /// }
    ///
    /// // Doesn't fall through:
    /// function baz() {
    ///     return 1;
    /// }
    /// ```
    /// Lightweight AST scan: does the function body contain any `throw` statements?
    /// This is used as a pre-check before the more expensive `function_body_falls_through`
    /// to avoid triggering type evaluation in simple function bodies that obviously fall through.
    fn body_contains_throw_or_never_call(&self, body_idx: NodeIndex) -> bool {
        fn scan_stmts(arena: &tsz_parser::parser::NodeArena, stmts: &[NodeIndex]) -> bool {
            use tsz_parser::parser::syntax_kind_ext;
            for &idx in stmts {
                let Some(node) = arena.get(idx) else {
                    continue;
                };
                match node.kind {
                    syntax_kind_ext::THROW_STATEMENT => return true,
                    syntax_kind_ext::BLOCK => {
                        if let Some(block) = arena.get_block(node)
                            && scan_stmts(arena, &block.statements.nodes)
                        {
                            return true;
                        }
                    }
                    syntax_kind_ext::IF_STATEMENT => {
                        if let Some(if_data) = arena.get_if_statement(node) {
                            if scan_stmts(arena, &[if_data.then_statement]) {
                                return true;
                            }
                            if !if_data.else_statement.is_none()
                                && scan_stmts(arena, &[if_data.else_statement])
                            {
                                return true;
                            }
                        }
                    }
                    syntax_kind_ext::TRY_STATEMENT => {
                        if let Some(try_data) = arena.get_try(node)
                            && scan_stmts(arena, &[try_data.try_block])
                        {
                            return true;
                        }
                    }
                    syntax_kind_ext::SWITCH_STATEMENT => {
                        if let Some(switch_data) = arena.get_switch(node)
                            && let Some(cb_node) = arena.get(switch_data.case_block)
                            && let Some(cb) = arena.get_block(cb_node)
                        {
                            for &clause_idx in &cb.statements.nodes {
                                if let Some(cn) = arena.get(clause_idx)
                                    && let Some(clause) = arena.get_case_clause(cn)
                                    && scan_stmts(arena, &clause.statements.nodes)
                                {
                                    return true;
                                }
                            }
                        }
                    }
                    // Expression statements could contain never-returning calls,
                    // but detecting those requires type checking. We conservatively
                    // return false here; the full falls_through check will catch them.
                    _ => {}
                }
            }
            false
        }

        let Some(body_node) = self.ctx.arena.get(body_idx) else {
            return false;
        };
        if body_node.kind == syntax_kind_ext::BLOCK
            && let Some(block) = self.ctx.arena.get_block(body_node)
        {
            return scan_stmts(self.ctx.arena, &block.statements.nodes);
        }
        false
    }

    pub fn function_body_falls_through(&mut self, body_idx: NodeIndex) -> bool {
        let Some(body_node) = self.ctx.arena.get(body_idx) else {
            return true;
        };
        if body_node.kind == syntax_kind_ext::BLOCK
            && let Some(block) = self.ctx.arena.get_block(body_node)
        {
            return self.block_falls_through(&block.statements.nodes);
        }
        false
    }

    /// Infer the return type of a function body by collecting return expressions.
    ///
    /// This function walks through all statements in a function body, collecting
    /// the types of all return expressions. It then infers the return type as:
    /// - `void`: If there are no return expressions
    /// - `union` of all return types: If there are multiple return expressions
    /// - The single return type: If there's only one return expression
    ///
    /// ## Parameters:
    /// - `body_idx`: The function body node index
    /// - `return_context`: Optional contextual type for return expressions
    ///
    /// ## Examples:
    /// ```typescript
    /// // No returns → void
    /// function foo() {}
    ///
    /// // Single return → string
    /// function bar() { return "hello"; }
    ///
    /// // Multiple returns → string | number
    /// function baz() {
    ///     if (cond) return "hello";
    ///     return 42;
    /// }
    ///
    /// // Empty return included → string | number | void
    /// function qux() {
    ///     if (cond) return;
    ///     return "hello";
    /// }
    /// ```
    pub(crate) fn has_only_explicit_any_assertion_returns(&mut self, body_idx: NodeIndex) -> bool {
        if body_idx.is_none() {
            return false;
        }
        let mut saw_value_return = false;
        let mut all_value_returns_explicit_any = true;
        self.collect_explicit_any_assertion_returns(
            body_idx,
            &mut saw_value_return,
            &mut all_value_returns_explicit_any,
        );
        saw_value_return && all_value_returns_explicit_any
    }

    fn collect_explicit_any_assertion_returns(
        &mut self,
        stmt_idx: NodeIndex,
        saw_value_return: &mut bool,
        all_value_returns_explicit_any: &mut bool,
    ) {
        let Some(node) = self.ctx.arena.get(stmt_idx) else {
            return;
        };

        match node.kind {
            syntax_kind_ext::RETURN_STATEMENT => {
                if let Some(return_data) = self.ctx.arena.get_return_statement(node)
                    && !return_data.expression.is_none()
                {
                    *saw_value_return = true;
                    if !self.is_explicit_any_assertion_expression(return_data.expression) {
                        *all_value_returns_explicit_any = false;
                    }
                }
            }
            syntax_kind_ext::BLOCK => {
                if let Some(block) = self.ctx.arena.get_block(node) {
                    for &stmt in &block.statements.nodes {
                        self.collect_explicit_any_assertion_returns(
                            stmt,
                            saw_value_return,
                            all_value_returns_explicit_any,
                        );
                    }
                }
            }
            syntax_kind_ext::IF_STATEMENT => {
                if let Some(if_data) = self.ctx.arena.get_if_statement(node) {
                    self.collect_explicit_any_assertion_returns(
                        if_data.then_statement,
                        saw_value_return,
                        all_value_returns_explicit_any,
                    );
                    if !if_data.else_statement.is_none() {
                        self.collect_explicit_any_assertion_returns(
                            if_data.else_statement,
                            saw_value_return,
                            all_value_returns_explicit_any,
                        );
                    }
                }
            }
            syntax_kind_ext::SWITCH_STATEMENT => {
                if let Some(switch_data) = self.ctx.arena.get_switch(node)
                    && let Some(case_block_node) = self.ctx.arena.get(switch_data.case_block)
                    && let Some(case_block) = self.ctx.arena.get_block(case_block_node)
                {
                    for &clause_idx in &case_block.statements.nodes {
                        if let Some(clause_node) = self.ctx.arena.get(clause_idx)
                            && let Some(clause) = self.ctx.arena.get_case_clause(clause_node)
                        {
                            for &stmt in &clause.statements.nodes {
                                self.collect_explicit_any_assertion_returns(
                                    stmt,
                                    saw_value_return,
                                    all_value_returns_explicit_any,
                                );
                            }
                        }
                    }
                }
            }
            syntax_kind_ext::TRY_STATEMENT => {
                if let Some(try_data) = self.ctx.arena.get_try(node) {
                    self.collect_explicit_any_assertion_returns(
                        try_data.try_block,
                        saw_value_return,
                        all_value_returns_explicit_any,
                    );
                    if !try_data.catch_clause.is_none() {
                        self.collect_explicit_any_assertion_returns(
                            try_data.catch_clause,
                            saw_value_return,
                            all_value_returns_explicit_any,
                        );
                    }
                    if !try_data.finally_block.is_none() {
                        self.collect_explicit_any_assertion_returns(
                            try_data.finally_block,
                            saw_value_return,
                            all_value_returns_explicit_any,
                        );
                    }
                }
            }
            syntax_kind_ext::CATCH_CLAUSE => {
                if let Some(catch_data) = self.ctx.arena.get_catch_clause(node) {
                    self.collect_explicit_any_assertion_returns(
                        catch_data.block,
                        saw_value_return,
                        all_value_returns_explicit_any,
                    );
                }
            }
            syntax_kind_ext::WHILE_STATEMENT
            | syntax_kind_ext::DO_STATEMENT
            | syntax_kind_ext::FOR_STATEMENT => {
                if let Some(loop_data) = self.ctx.arena.get_loop(node) {
                    self.collect_explicit_any_assertion_returns(
                        loop_data.statement,
                        saw_value_return,
                        all_value_returns_explicit_any,
                    );
                }
            }
            syntax_kind_ext::FOR_IN_STATEMENT | syntax_kind_ext::FOR_OF_STATEMENT => {
                if let Some(for_in_of_data) = self.ctx.arena.get_for_in_of(node) {
                    self.collect_explicit_any_assertion_returns(
                        for_in_of_data.statement,
                        saw_value_return,
                        all_value_returns_explicit_any,
                    );
                }
            }
            syntax_kind_ext::LABELED_STATEMENT => {
                if let Some(labeled_data) = self.ctx.arena.get_labeled_statement(node) {
                    self.collect_explicit_any_assertion_returns(
                        labeled_data.statement,
                        saw_value_return,
                        all_value_returns_explicit_any,
                    );
                }
            }
            _ => {}
        }
    }

    fn is_explicit_any_assertion_expression(&mut self, expr_idx: NodeIndex) -> bool {
        let mut current = expr_idx;
        while let Some(node) = self.ctx.arena.get(current) {
            if node.kind == syntax_kind_ext::PARENTHESIZED_EXPRESSION
                && let Some(paren) = self.ctx.arena.get_parenthesized(node)
            {
                current = paren.expression;
                continue;
            }
            if (node.kind == syntax_kind_ext::AS_EXPRESSION
                || node.kind == syntax_kind_ext::TYPE_ASSERTION)
                && let Some(assertion) = self.ctx.arena.get_type_assertion(node)
            {
                return self.get_type_from_type_node(assertion.type_node) == TypeId::ANY;
            }
            return false;
        }
        false
    }

    pub(crate) fn infer_return_type_from_body(
        &mut self,
        _function_idx: NodeIndex,
        body_idx: NodeIndex,
        return_context: Option<TypeId>,
    ) -> TypeId {
        // The inference pass evaluates return expressions WITHOUT narrowing
        // context, which can produce false errors (e.g. TS2339 for discriminated
        // union property accesses) and cache wrong types.  Snapshot diagnostic,
        // node-type, and flow-analysis-cache state, then restore after inference
        // so that the subsequent check_statement pass recomputes everything with
        // proper narrowing context.
        let diag_count = self.ctx.diagnostics.len();
        let emitted_before = self.ctx.emitted_diagnostics.clone();
        let emitted_ts2454_before = self.ctx.emitted_ts2454_errors.clone();
        let modules_ts2307_before = self.ctx.modules_with_ts2307_emitted.clone();
        let cached_before: std::collections::HashSet<u32> =
            self.ctx.node_types.keys().copied().collect();
        let flow_cache_before = self.ctx.flow_analysis_cache.borrow().clone();

        let result = self.infer_return_type_from_body_inner(body_idx, return_context);

        self.ctx.diagnostics.truncate(diag_count);
        self.ctx.emitted_diagnostics = emitted_before;
        self.ctx.emitted_ts2454_errors = emitted_ts2454_before;
        self.ctx.modules_with_ts2307_emitted = modules_ts2307_before;
        self.ctx.node_types.retain(|k, _| cached_before.contains(k));
        *self.ctx.flow_analysis_cache.borrow_mut() = flow_cache_before;

        // Widen inferred return types when there is no contextual return type.
        // `function f() { return "a"; }` → return type `string` (widened).
        // But `const g: () => "a" = () => "a"` → return type `"a"` (preserved
        // by contextual typing).
        if return_context.is_none() {
            self.widen_literal_type(result)
        } else {
            result
        }
    }

    /// Inner implementation of return type inference (no diagnostic/cache cleanup).
    fn infer_return_type_from_body_inner(
        &mut self,
        body_idx: NodeIndex,
        return_context: Option<TypeId>,
    ) -> TypeId {
        let factory = self.ctx.types.factory();
        if body_idx.is_none() {
            return TypeId::VOID; // No body - function returns void
        }

        let Some(node) = self.ctx.arena.get(body_idx) else {
            return TypeId::ERROR; // Missing node - propagate error
        };

        if node.kind != syntax_kind_ext::BLOCK {
            return self.return_expression_type(body_idx, return_context);
        }

        let mut return_types = Vec::new();
        let mut saw_empty = false;

        if let Some(block) = self.ctx.arena.get_block(node) {
            for &stmt_idx in &block.statements.nodes {
                self.collect_return_types_in_statement(
                    stmt_idx,
                    &mut return_types,
                    &mut saw_empty,
                    return_context,
                );
            }
        }

        if return_types.is_empty() {
            // No return statements found. Check if the body falls through:
            // - If it does (normal implicit return), the return type is `void`
            // - If it doesn't (all paths throw or call never), the return type is `never`
            // Only call the (potentially expensive) fallthrough checker when the body
            // could plausibly be non-falling-through, i.e. it contains throw statements.
            // This avoids triggering unnecessary type evaluation in simple function bodies.
            let may_not_fall_through = self.body_contains_throw_or_never_call(body_idx);

            // Check if function has a return type annotation
            let has_return_type_annotation = if let Some(func_node) = self.ctx.arena.get(body_idx)
                && let Some(func) = self.ctx.arena.get_function(func_node)
            {
                !func.type_annotation.is_none()
            } else {
                false
            };

            if has_return_type_annotation
                && may_not_fall_through
                && !self.function_body_falls_through(body_idx)
            {
                use crate::diagnostics::diagnostic_codes;
                self.error_at_node(
                    body_idx,
                    "Function lacks ending return statement and return type does not include undefined",
                    diagnostic_codes::FUNCTION_LACKS_ENDING_RETURN_STATEMENT_AND_RETURN_TYPE_DOES_NOT_INCLUDE_UNDEFINE,
                );
                return TypeId::ERROR; // Return error to avoid further issues
            }

            return if !may_not_fall_through || self.function_body_falls_through(body_idx) {
                TypeId::VOID
            } else {
                TypeId::NEVER
            };
        }

        if saw_empty || self.function_body_falls_through(body_idx) {
            return_types.push(TypeId::VOID);
        }

        factory.union(return_types)
    }

    /// Get the type of a return expression with optional contextual typing.
    ///
    /// This function temporarily sets the contextual type (if provided) before
    /// computing the type of the return expression, then restores the previous
    /// contextual type. This enables contextual typing for return expressions.
    ///
    /// ## Parameters:
    /// - `expr_idx`: The return expression node index
    /// - `return_context`: Optional contextual type for the return
    fn return_expression_type(
        &mut self,
        expr_idx: NodeIndex,
        return_context: Option<TypeId>,
    ) -> TypeId {
        // Expression-bodied arrows returning `void expr` are always `void`.
        // During inference this avoids unnecessary recursive type computation
        // (which can create self-referential cycles and spuriously degrade to `any`).
        if let Some(expr_node) = self.ctx.arena.get(expr_idx)
            && expr_node.kind == syntax_kind_ext::PREFIX_UNARY_EXPRESSION
            && let Some(unary) = self.ctx.arena.get_unary_expr(expr_node)
            && unary.operator == SyntaxKind::VoidKeyword as u16
        {
            return TypeId::VOID;
        }

        let prev_context = self.ctx.contextual_type;
        if let Some(ctx_type) = return_context {
            self.ctx.contextual_type = Some(ctx_type);
        }
        let return_type = self.get_type_of_node(expr_idx);
        self.ctx.contextual_type = prev_context;
        return_type
    }

    /// Collect return types from a statement and its nested statements.
    ///
    /// This function recursively walks through statements, collecting the types
    /// of all return expressions. It handles:
    /// - Direct return statements
    /// - Nested blocks
    /// - If/else statements (both branches)
    /// - Switch statements (all cases)
    /// - Try/catch/finally statements (all blocks)
    /// - Loops (nested statements)
    fn collect_return_types_in_statement(
        &mut self,
        stmt_idx: NodeIndex,
        return_types: &mut Vec<TypeId>,
        saw_empty: &mut bool,
        return_context: Option<TypeId>,
    ) {
        let Some(node) = self.ctx.arena.get(stmt_idx) else {
            return;
        };

        match node.kind {
            syntax_kind_ext::RETURN_STATEMENT => {
                if let Some(return_data) = self.ctx.arena.get_return_statement(node) {
                    if return_data.expression.is_none() {
                        *saw_empty = true;
                    } else {
                        let return_type =
                            self.return_expression_type(return_data.expression, return_context);
                        return_types.push(return_type);
                    }
                }
            }
            syntax_kind_ext::BLOCK => {
                if let Some(block) = self.ctx.arena.get_block(node) {
                    for &stmt in &block.statements.nodes {
                        self.collect_return_types_in_statement(
                            stmt,
                            return_types,
                            saw_empty,
                            return_context,
                        );
                    }
                }
            }
            syntax_kind_ext::IF_STATEMENT => {
                if let Some(if_data) = self.ctx.arena.get_if_statement(node) {
                    self.collect_return_types_in_statement(
                        if_data.then_statement,
                        return_types,
                        saw_empty,
                        return_context,
                    );
                    if !if_data.else_statement.is_none() {
                        self.collect_return_types_in_statement(
                            if_data.else_statement,
                            return_types,
                            saw_empty,
                            return_context,
                        );
                    }
                }
            }
            syntax_kind_ext::SWITCH_STATEMENT => {
                if let Some(switch_data) = self.ctx.arena.get_switch(node)
                    && let Some(case_block_node) = self.ctx.arena.get(switch_data.case_block)
                    && let Some(case_block) = self.ctx.arena.get_block(case_block_node)
                {
                    for &clause_idx in &case_block.statements.nodes {
                        if let Some(clause_node) = self.ctx.arena.get(clause_idx)
                            && let Some(clause) = self.ctx.arena.get_case_clause(clause_node)
                        {
                            for &stmt_idx in &clause.statements.nodes {
                                self.collect_return_types_in_statement(
                                    stmt_idx,
                                    return_types,
                                    saw_empty,
                                    return_context,
                                );
                            }
                        }
                    }
                }
            }
            syntax_kind_ext::TRY_STATEMENT => {
                if let Some(try_data) = self.ctx.arena.get_try(node) {
                    self.collect_return_types_in_statement(
                        try_data.try_block,
                        return_types,
                        saw_empty,
                        return_context,
                    );
                    if !try_data.catch_clause.is_none() {
                        self.collect_return_types_in_statement(
                            try_data.catch_clause,
                            return_types,
                            saw_empty,
                            return_context,
                        );
                    }
                    if !try_data.finally_block.is_none() {
                        self.collect_return_types_in_statement(
                            try_data.finally_block,
                            return_types,
                            saw_empty,
                            return_context,
                        );
                    }
                }
            }
            syntax_kind_ext::CATCH_CLAUSE => {
                if let Some(catch_data) = self.ctx.arena.get_catch_clause(node) {
                    self.collect_return_types_in_statement(
                        catch_data.block,
                        return_types,
                        saw_empty,
                        return_context,
                    );
                }
            }
            syntax_kind_ext::WHILE_STATEMENT
            | syntax_kind_ext::DO_STATEMENT
            | syntax_kind_ext::FOR_STATEMENT => {
                if let Some(loop_data) = self.ctx.arena.get_loop(node) {
                    self.collect_return_types_in_statement(
                        loop_data.statement,
                        return_types,
                        saw_empty,
                        return_context,
                    );
                }
            }
            syntax_kind_ext::FOR_IN_STATEMENT | syntax_kind_ext::FOR_OF_STATEMENT => {
                if let Some(for_in_of_data) = self.ctx.arena.get_for_in_of(node) {
                    self.collect_return_types_in_statement(
                        for_in_of_data.statement,
                        return_types,
                        saw_empty,
                        return_context,
                    );
                }
            }
            syntax_kind_ext::LABELED_STATEMENT => {
                if let Some(labeled_data) = self.ctx.arena.get_labeled_statement(node) {
                    self.collect_return_types_in_statement(
                        labeled_data.statement,
                        return_types,
                        saw_empty,
                        return_context,
                    );
                }
            }
            _ => {}
        }
    }

    /// Check if a function body has at least one return statement with a value.
    ///
    /// This is a simplified check that doesn't do full control flow analysis.
    /// It's used to determine if a function needs an explicit return type
    /// annotation or if implicit any should be inferred.
    ///
    /// ## Returns:
    /// - `true`: At least one return statement with a value exists
    /// - `false`: No return statements or only empty returns
    ///
    /// ## Examples:
    /// ```typescript
    /// // Returns true:
    /// function foo() { return 42; }
    /// function bar() { if (x) return "hello"; else return 42; }
    ///
    /// // Returns false:
    /// function baz() {}  // No returns
    /// function qux() { return; }  // Only empty return
    /// ```
    pub(crate) fn body_has_return_with_value(&self, body_idx: NodeIndex) -> bool {
        let Some(node) = self.ctx.arena.get(body_idx) else {
            return false;
        };

        // For block bodies, check all statements
        if node.kind == syntax_kind_ext::BLOCK
            && let Some(block) = self.ctx.arena.get_block(node)
        {
            return self.statements_have_return_with_value(&block.statements.nodes);
        }

        false
    }

    /// Check if any statement in the list contains a return with a value.
    fn statements_have_return_with_value(&self, statements: &[NodeIndex]) -> bool {
        for &stmt_idx in statements {
            if self.statement_has_return_with_value(stmt_idx) {
                return true;
            }
        }
        false
    }

    /// Check if a statement contains a return with a value.
    ///
    /// This function recursively checks a statement (and its nested statements)
    /// for any return statement with a value. It handles all statement types
    /// including blocks, conditionals, loops, and try/catch.
    fn statement_has_return_with_value(&self, stmt_idx: NodeIndex) -> bool {
        let Some(node) = self.ctx.arena.get(stmt_idx) else {
            return false;
        };

        match node.kind {
            syntax_kind_ext::RETURN_STATEMENT => {
                if let Some(return_data) = self.ctx.arena.get_return_statement(node) {
                    // Return with expression
                    return !return_data.expression.is_none();
                }
                false
            }
            syntax_kind_ext::BLOCK => {
                if let Some(block) = self.ctx.arena.get_block(node) {
                    return self.statements_have_return_with_value(&block.statements.nodes);
                }
                false
            }
            syntax_kind_ext::IF_STATEMENT => {
                if let Some(if_data) = self.ctx.arena.get_if_statement(node) {
                    // Check both then and else branches
                    let then_has = self.statement_has_return_with_value(if_data.then_statement);
                    let else_has = if !if_data.else_statement.is_none() {
                        self.statement_has_return_with_value(if_data.else_statement)
                    } else {
                        false
                    };
                    return then_has || else_has;
                }
                false
            }
            syntax_kind_ext::SWITCH_STATEMENT => {
                if let Some(switch_data) = self.ctx.arena.get_switch(node)
                    && let Some(case_block_node) = self.ctx.arena.get(switch_data.case_block)
                {
                    // Case block is stored as a Block containing case clauses
                    if let Some(case_block) = self.ctx.arena.get_block(case_block_node) {
                        for &clause_idx in &case_block.statements.nodes {
                            if let Some(clause_node) = self.ctx.arena.get(clause_idx)
                                && let Some(clause) = self.ctx.arena.get_case_clause(clause_node)
                                && self.statements_have_return_with_value(&clause.statements.nodes)
                            {
                                return true;
                            }
                        }
                    }
                }
                false
            }
            syntax_kind_ext::TRY_STATEMENT => {
                if let Some(try_data) = self.ctx.arena.get_try(node) {
                    let try_has = self.statement_has_return_with_value(try_data.try_block);
                    let catch_has = if !try_data.catch_clause.is_none() {
                        self.statement_has_return_with_value(try_data.catch_clause)
                    } else {
                        false
                    };
                    let finally_has = if !try_data.finally_block.is_none() {
                        self.statement_has_return_with_value(try_data.finally_block)
                    } else {
                        false
                    };
                    return try_has || catch_has || finally_has;
                }
                false
            }
            syntax_kind_ext::CATCH_CLAUSE => {
                if let Some(catch_data) = self.ctx.arena.get_catch_clause(node) {
                    return self.statement_has_return_with_value(catch_data.block);
                }
                false
            }
            syntax_kind_ext::WHILE_STATEMENT
            | syntax_kind_ext::DO_STATEMENT
            | syntax_kind_ext::FOR_STATEMENT => {
                if let Some(loop_data) = self.ctx.arena.get_loop(node) {
                    return self.statement_has_return_with_value(loop_data.statement);
                }
                false
            }
            syntax_kind_ext::FOR_IN_STATEMENT | syntax_kind_ext::FOR_OF_STATEMENT => {
                if let Some(for_in_of_data) = self.ctx.arena.get_for_in_of(node) {
                    return self.statement_has_return_with_value(for_in_of_data.statement);
                }
                false
            }
            syntax_kind_ext::LABELED_STATEMENT => {
                if let Some(labeled_data) = self.ctx.arena.get_labeled_statement(node) {
                    return self.statement_has_return_with_value(labeled_data.statement);
                }
                false
            }
            _ => false,
        }
    }
}
