//! Type Checking Utilities Module
//!
//! This module contains parameter type utilities, type construction, and
//! type resolution methods for CheckerState.
//! Split from type_checking.rs for maintainability.

use crate::state::{CheckerState, EnumKind, MemberAccessLevel};
use tsz_binder::{SymbolId, symbol_flags};
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;
use tsz_solver::TypeId;

#[allow(dead_code)]
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
                types.get(i).and_then(|t| *t)
            } else if !param.type_annotation.is_none() {
                Some(self.get_type_from_type_node(param.type_annotation))
            } else {
                // Return UNKNOWN instead of ANY for parameter without type annotation
                Some(TypeId::UNKNOWN)
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
        use tsz_solver::type_queries;

        type_queries::get_widened_literal_type(self.ctx.types, type_id).unwrap_or(type_id)
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
        use tsz_solver::type_queries;

        let mut current_expanded_index = 0;

        for &arg_idx in args.iter() {
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
                    if let Some(elems_id) =
                        type_queries::get_tuple_list_id(self.ctx.types, spread_type)
                    {
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
        use tsz_solver::type_queries;

        type_queries::get_application_base(self.ctx.types, type_id).unwrap_or(type_id)
    }

    pub(crate) fn lookup_symbol_with_name(
        &self,
        sym_id: SymbolId,
        name_hint: Option<&str>,
    ) -> Option<(&tsz_binder::Symbol, &tsz_parser::parser::node::NodeArena)> {
        let name_hint = name_hint.map(str::trim).filter(|name| !name.is_empty());

        if let Some(symbol) = self.ctx.binder.symbols.get(sym_id) {
            if name_hint.is_none_or(|name| symbol.escaped_name == name) {
                let arena = self
                    .ctx
                    .binder
                    .symbol_arenas
                    .get(&sym_id)
                    .map(|arena| arena.as_ref())
                    .unwrap_or(self.ctx.arena);
                return Some((symbol, arena));
            }
        }

        if let Some(name) = name_hint {
            for lib_ctx in &self.ctx.lib_contexts {
                if let Some(symbol) = lib_ctx.binder.symbols.get(sym_id) {
                    if symbol.escaped_name == name {
                        return Some((symbol, lib_ctx.arena.as_ref()));
                    }
                }
            }
            if let Some(symbol) = self.ctx.binder.get_symbol(sym_id) {
                if symbol.escaped_name == name {
                    let arena = self
                        .ctx
                        .binder
                        .symbol_arenas
                        .get(&sym_id)
                        .map(|arena| arena.as_ref())
                        .unwrap_or(self.ctx.arena);
                    return Some((symbol, arena));
                }
            }
            return None;
        }

        if let Some(symbol) = self.ctx.binder.get_symbol(sym_id) {
            let arena = self
                .ctx
                .binder
                .symbol_arenas
                .get(&sym_id)
                .map(|arena| arena.as_ref())
                .unwrap_or(self.ctx.arena);
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
        let has_other_value = (symbol.flags & (symbol_flags::VALUE & !symbol_flags::FUNCTION)) != 0;

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
        use tsz_solver::type_queries::{LiteralKeyKind, classify_literal_key};

        match classify_literal_key(self.ctx.types, index_type) {
            LiteralKeyKind::StringLiteral(atom) => Some((vec![atom], Vec::new())),
            LiteralKeyKind::NumberLiteral(num) => Some((Vec::new(), vec![num])),
            LiteralKeyKind::Union(members) => {
                let mut string_keys = Vec::with_capacity(members.len());
                let mut number_keys = Vec::new();
                for &member in members.iter() {
                    match classify_literal_key(self.ctx.types, member) {
                        LiteralKeyKind::StringLiteral(atom) => string_keys.push(atom),
                        LiteralKeyKind::NumberLiteral(num) => number_keys.push(num),
                        _ => return None,
                    }
                }
                Some((string_keys, number_keys))
            }
            LiteralKeyKind::Other => None,
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
                PropertyAccessResult::IsUnknown => return None,
                PropertyAccessResult::PropertyNotFound { .. } => return None,
            }
        }

        if types.len() == 1 {
            Some(types[0])
        } else {
            Some(self.ctx.types.union(types))
        }
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

        if types.len() == 1 {
            Some(types[0])
        } else {
            Some(self.ctx.types.union(types))
        }
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
    fn is_array_like_type(&self, object_type: TypeId) -> bool {
        use tsz_solver::type_queries::{ArrayLikeKind, classify_array_like};

        // Check for array/tuple types directly
        if self.is_mutable_array_type(object_type) {
            return true;
        }

        match classify_array_like(self.ctx.types, object_type) {
            ArrayLikeKind::Array(_) => true,
            ArrayLikeKind::Tuple => true,
            ArrayLikeKind::Readonly(inner) => self.is_array_like_type(inner),
            ArrayLikeKind::Union(members) => members
                .iter()
                .all(|&member| self.is_array_like_type(member)),
            ArrayLikeKind::Intersection(members) => members
                .iter()
                .any(|&member| self.is_array_like_type(member)),
            ArrayLikeKind::Other => false,
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
        use tsz_solver::type_queries;

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

        let unwrapped_type = type_queries::unwrap_readonly_for_lookup(self.ctx.types, object_type);

        !self.is_element_indexable(unwrapped_type, wants_string, wants_number)
    }

    /// Determine what kind of index key a type represents.
    ///
    /// This function analyzes a type to determine if it can be used for string
    /// or numeric indexing. Returns a tuple of (wants_string, wants_number).
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
        use tsz_solver::type_queries::{IndexKeyKind, classify_index_key};

        match classify_index_key(self.ctx.types, index_type) {
            IndexKeyKind::String | IndexKeyKind::StringLiteral => Some((true, false)),
            IndexKeyKind::Number | IndexKeyKind::NumberLiteral => Some((false, true)),
            IndexKeyKind::Union(members) => {
                let mut wants_string = false;
                let mut wants_number = false;
                for member in members {
                    let (member_string, member_number) = self.get_index_key_kind(member)?;
                    wants_string |= member_string;
                    wants_number |= member_number;
                }
                Some((wants_string, wants_number))
            }
            IndexKeyKind::Other => None,
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
        use tsz_solver::type_queries::{ElementIndexableKind, classify_element_indexable};

        match classify_element_indexable(self.ctx.types, object_type) {
            ElementIndexableKind::Array | ElementIndexableKind::Tuple => wants_number,
            ElementIndexableKind::ObjectWithIndex {
                has_string,
                has_number,
            } => (wants_string && has_string) || (wants_number && (has_number || has_string)),
            ElementIndexableKind::Union(members) => members
                .iter()
                .all(|&member| self.is_element_indexable(member, wants_string, wants_number)),
            ElementIndexableKind::Intersection(members) => members
                .iter()
                .any(|&member| self.is_element_indexable(member, wants_string, wants_number)),
            ElementIndexableKind::StringLike => wants_number,
            ElementIndexableKind::Other => false,
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
    pub(crate) fn infer_return_type_from_body(
        &mut self,
        body_idx: NodeIndex,
        return_context: Option<TypeId>,
    ) -> TypeId {
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
            return TypeId::VOID;
        }

        if saw_empty {
            return_types.push(TypeId::VOID);
        }

        self.ctx.types.union(return_types)
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
            | syntax_kind_ext::FOR_STATEMENT
            | syntax_kind_ext::FOR_IN_STATEMENT
            | syntax_kind_ext::FOR_OF_STATEMENT => {
                if let Some(loop_data) = self.ctx.arena.get_loop(node) {
                    self.collect_return_types_in_statement(
                        loop_data.statement,
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
            | syntax_kind_ext::FOR_STATEMENT
            | syntax_kind_ext::FOR_IN_STATEMENT
            | syntax_kind_ext::FOR_OF_STATEMENT => {
                if let Some(loop_data) = self.ctx.arena.get_loop(node) {
                    return self.statement_has_return_with_value(loop_data.statement);
                }
                false
            }
            _ => false,
        }
    }

    // ============================================================================
    // Section 57: JSDoc Type Annotation Utilities
    // ============================================================================

    /// Resolve a typeof type reference to its actual type.
    ///
    /// This function resolves `typeof X` type queries to the type of symbol X.
    /// It handles both direct typeof queries and typeof queries applied to
    /// type applications (generics).
    ///
    /// ## Parameters:
    /// - `type_id`: The type to resolve (may be a TypeQuery or Application)
    ///
    /// ## Returns:
    /// - The resolved type if `type_id` is a typeof query
    /// - The original `type_id` if it's not a typeof query
    ///
    /// ## Examples:
    /// ```typescript
    /// class C {}
    /// type T1 = typeof C;  // C (the class type)
    /// type T2 = typeof<C>;  // Same as above
    /// ```
    pub(crate) fn resolve_type_query_type(&mut self, type_id: TypeId) -> TypeId {
        use tsz_binder::SymbolId;
        use tsz_solver::SymbolRef;
        use tsz_solver::type_queries::{TypeQueryKind, classify_type_query};

        match classify_type_query(self.ctx.types, type_id) {
            TypeQueryKind::TypeQuery(SymbolRef(sym_id)) => {
                // Check for cycle in typeof resolution (scoped borrow)
                let is_cycle = { self.ctx.typeof_resolution_stack.borrow().contains(&sym_id) };
                if is_cycle {
                    // Cycle detected - return ERROR to prevent infinite loop
                    return TypeId::ERROR;
                }

                // Mark as visiting (use try_borrow_mut to avoid panic on nested borrow)
                if let Ok(mut stack) = self.ctx.typeof_resolution_stack.try_borrow_mut() {
                    stack.insert(sym_id);
                }

                // Resolve the symbol type
                let result = self.get_type_of_symbol(SymbolId(sym_id));

                // Unmark after resolution
                if let Ok(mut stack) = self.ctx.typeof_resolution_stack.try_borrow_mut() {
                    stack.remove(&sym_id);
                }

                result
            }
            TypeQueryKind::ApplicationWithTypeQuery {
                base_sym_ref: SymbolRef(sym_id),
                args,
            } => {
                // Check for cycle in typeof resolution (scoped borrow)
                let is_cycle = { self.ctx.typeof_resolution_stack.borrow().contains(&sym_id) };
                if is_cycle {
                    return TypeId::ERROR;
                }

                // Mark as visiting (use try_borrow_mut to avoid panic on nested borrow)
                if let Ok(mut stack) = self.ctx.typeof_resolution_stack.try_borrow_mut() {
                    stack.insert(sym_id);
                }

                // Resolve the base type
                let base = self.get_type_of_symbol(SymbolId(sym_id));

                // Unmark after resolution
                if let Ok(mut stack) = self.ctx.typeof_resolution_stack.try_borrow_mut() {
                    stack.remove(&sym_id);
                }

                self.ctx.types.application(base, args)
            }
            TypeQueryKind::Application { .. } | TypeQueryKind::Other => type_id,
        }
    }

    /// Get JSDoc type annotation for a node.
    ///
    /// This function extracts and parses JSDoc `@type` annotations for a given node.
    /// It searches for the enclosing source file, extracts JSDoc comments,
    /// and parses the type annotation.
    ///
    /// ## Parameters:
    /// - `idx`: The node to get JSDoc type annotation for
    ///
    /// ## Returns:
    /// - `Some(TypeId)`: The parsed type from JSDoc
    /// - `None`: If no JSDoc type annotation exists
    ///
    /// ## Example:
    /// ```typescript
    /// /**
    ///  * @type {string} x - The parameter type
    ///  */
    /// function foo(x) {}
    /// // The JSDoc annotation can be used for type inference
    /// ```
    pub(crate) fn jsdoc_type_annotation_for_node(&mut self, _idx: NodeIndex) -> Option<TypeId> {
        // TODO: jsdoc_for_node lives in the LSP module; stub until LSP is extracted
        None
    }

    /// Extract type text from JSDoc comment.
    ///
    /// This function parses JSDoc comments to find `@type` tags and
    /// extracts the type annotation from within curly braces.
    ///
    /// ## Parameters:
    /// - `doc`: The JSDoc comment text
    ///
    /// ## Returns:
    /// - `Some(String)`: The extracted type text
    /// - `None`: If no `@type` tag found or type is empty
    ///
    /// ## Example:
    /// ```javascript
    /// /**
    ///  * @type {string | number} The parameter type
    ///  * @returns {boolean} The result
    ///  */
    /// // extract_jsdoc_type returns: "string | number"
    /// ```
    fn extract_jsdoc_type(&self, doc: &str) -> Option<String> {
        let tag_pos = doc.find("@type")?;
        let rest = &doc[tag_pos + "@type".len()..];
        let open = rest.find('{')?;
        let after_open = &rest[open + 1..];
        let close = after_open.find('}')?;
        let type_text = after_open[..close].trim();
        if type_text.is_empty() {
            None
        } else {
            Some(type_text.to_string())
        }
    }

    /// Parse JSDoc type annotation text into a TypeId.
    ///
    /// This function parses simple type expressions from JSDoc comments.
    /// It supports:
    /// - Primitive types: string, number, boolean, void, any, unknown
    /// - Function types: function(paramType, ...): returnType
    ///
    /// ## Parameters:
    /// - `text`: The type annotation text to parse
    ///
    /// ## Returns:
    /// - `Some(TypeId)`: The parsed type
    /// - `None`: If parsing fails
    ///
    /// ## Examples:
    /// ```javascript
    /// /**
    ///  * @type {string}
    ///  */
    /// // Parses to TypeId::STRING
    ///
    /// /**
    ///  * @type {function(string, number): boolean}
    ///  */
    /// // Parses to a function type
    /// ```
    fn parse_jsdoc_type(&mut self, text: &str) -> Option<TypeId> {
        use tsz_solver::{FunctionShape, ParamInfo};

        fn skip_ws(text: &str, pos: &mut usize) {
            while *pos < text.len() && text.as_bytes()[*pos].is_ascii_whitespace() {
                *pos += 1;
            }
        }

        fn parse_ident<'a>(text: &'a str, pos: &mut usize) -> Option<&'a str> {
            let start = *pos;
            while *pos < text.len() {
                let ch = text.as_bytes()[*pos] as char;
                if ch.is_ascii_alphanumeric() || ch == '_' {
                    *pos += 1;
                } else {
                    break;
                }
            }
            if *pos > start {
                Some(&text[start..*pos])
            } else {
                None
            }
        }

        fn parse_type(
            checker: &mut crate::state::CheckerState,
            text: &str,
            pos: &mut usize,
        ) -> Option<TypeId> {
            skip_ws(text, pos);
            if text[*pos..].starts_with("function") {
                return parse_function_type(checker, text, pos);
            }

            let ident = parse_ident(text, pos)?;
            let type_id = match ident {
                "string" => TypeId::STRING,
                "number" => TypeId::NUMBER,
                "boolean" => TypeId::BOOLEAN,
                "void" => TypeId::VOID,
                "any" => TypeId::ANY,
                "unknown" => TypeId::UNKNOWN,
                _ => TypeId::ANY,
            };
            Some(type_id)
        }

        fn parse_function_type(
            checker: &mut crate::state::CheckerState,
            text: &str,
            pos: &mut usize,
        ) -> Option<TypeId> {
            if !text[*pos..].starts_with("function") {
                return None;
            }
            *pos += "function".len();
            skip_ws(text, pos);
            if *pos >= text.len() || text.as_bytes()[*pos] != b'(' {
                return None;
            }
            *pos += 1;
            let mut params = Vec::new();
            loop {
                skip_ws(text, pos);
                if *pos >= text.len() {
                    return None;
                }
                if text.as_bytes()[*pos] == b')' {
                    *pos += 1;
                    break;
                }
                let param_type = parse_type(checker, text, pos)?;
                params.push(ParamInfo {
                    name: None,
                    type_id: param_type,
                    optional: false,
                    rest: false,
                });
                skip_ws(text, pos);
                if *pos < text.len() && text.as_bytes()[*pos] == b',' {
                    *pos += 1;
                    continue;
                }
                if *pos < text.len() && text.as_bytes()[*pos] == b')' {
                    *pos += 1;
                    break;
                }
            }
            skip_ws(text, pos);
            if *pos >= text.len() || text.as_bytes()[*pos] != b':' {
                return None;
            }
            *pos += 1;
            let return_type = parse_type(checker, text, pos)?;
            let shape = FunctionShape {
                type_params: Vec::new(),
                params,
                this_type: None,
                return_type,
                type_predicate: None,
                is_constructor: false,
                is_method: false,
            };
            Some(checker.ctx.types.function(shape))
        }

        let mut pos = 0;
        let type_id = parse_type(self, text, &mut pos)?;
        Some(type_id)
    }

    // =========================================================================
    // Class Helper Methods
    // =========================================================================

    /// Check if a class has a base class (extends clause).
    ///
    /// Returns true if the class has any heritage clause with `extends` keyword.
    pub(crate) fn class_has_base(&self, class: &tsz_parser::parser::node::ClassData) -> bool {
        use tsz_scanner::SyntaxKind;

        let Some(ref heritage_clauses) = class.heritage_clauses else {
            return false;
        };

        for &clause_idx in &heritage_clauses.nodes {
            let Some(clause_node) = self.ctx.arena.get(clause_idx) else {
                continue;
            };
            let Some(heritage) = self.ctx.arena.get_heritage_clause(clause_node) else {
                continue;
            };
            if heritage.token == SyntaxKind::ExtendsKeyword as u16 {
                return true;
            }
        }

        false
    }

    /// Check if a type includes undefined (directly or in a union).
    ///
    /// Returns true if type_id is UNDEFINED or a union containing UNDEFINED.
    pub(crate) fn type_includes_undefined(&self, type_id: TypeId) -> bool {
        if type_id == TypeId::UNDEFINED {
            return true;
        }

        // Check if the type is a union containing undefined
        self.union_contains(type_id, TypeId::UNDEFINED)
    }

    /// Check if a union type contains a specific type.
    ///
    /// Returns true if type_id is a union and contains target_type.
    pub(crate) fn union_contains(&self, type_id: TypeId, target_type: TypeId) -> bool {
        use tsz_solver::type_queries;

        if let Some(members) = type_queries::get_union_members(self.ctx.types, type_id) {
            members.contains(&target_type)
        } else {
            false
        }
    }

    /// Find the constructor body in a class member list.
    ///
    /// Returns the body node of the first constructor member that has a body.
    pub(crate) fn find_constructor_body(
        &self,
        members: &tsz_parser::parser::NodeList,
    ) -> Option<NodeIndex> {
        for &member_idx in &members.nodes {
            let Some(node) = self.ctx.arena.get(member_idx) else {
                continue;
            };
            if node.kind != syntax_kind_ext::CONSTRUCTOR {
                continue;
            }
            let Some(ctor) = self.ctx.arena.get_constructor(node) else {
                continue;
            };
            if !ctor.body.is_none() {
                return Some(ctor.body);
            }
        }
        None
    }

    // =========================================================================
    // Enum Helper Functions
    // =========================================================================

    /// Get the enum symbol from a type reference.
    ///
    /// Returns the symbol ID if the type refers to an enum, None otherwise.
    pub(crate) fn enum_symbol_from_type(&self, type_id: TypeId) -> Option<SymbolId> {
        // Phase 4.2: Use resolve_type_to_symbol_id instead of get_ref_symbol
        let sym_id = self.ctx.resolve_type_to_symbol_id(type_id)?;
        let symbol = self.ctx.binder.get_symbol(sym_id)?;
        if symbol.flags & symbol_flags::ENUM == 0 {
            return None;
        }
        Some(sym_id)
    }

    /// Get the enum symbol from a value type.
    ///
    /// Handles both direct references and type queries (typeof).
    pub(crate) fn enum_symbol_from_value_type(&self, type_id: TypeId) -> Option<SymbolId> {
        use tsz_solver::type_queries::{SymbolRefKind, classify_symbol_ref};

        let sym_id = match classify_symbol_ref(self.ctx.types, type_id) {
            SymbolRefKind::Lazy(def_id) => {
                // Phase 4.2: Use DefId -> SymbolId bridge
                self.ctx.def_to_symbol_id(def_id)?
            }
            #[allow(deprecated)]
            SymbolRefKind::Ref(sym_ref) | SymbolRefKind::TypeQuery(sym_ref) => {
                // Fallback for legacy SymbolRef (shouldn't happen anymore)
                SymbolId(sym_ref.0)
            }
            SymbolRefKind::Other => return None,
        };

        let symbol = self.ctx.binder.get_symbol(sym_id)?;
        if symbol.flags & symbol_flags::ENUM == 0 {
            return None;
        }
        Some(sym_id)
    }

    /// Determine the kind of enum (string, numeric, or mixed).
    ///
    /// Returns None if the symbol is not an enum or has no members.
    pub(crate) fn enum_kind(&self, sym_id: SymbolId) -> Option<EnumKind> {
        let symbol = self.ctx.binder.get_symbol(sym_id)?;
        if symbol.flags & symbol_flags::ENUM == 0 {
            return None;
        }

        let decl_idx = if !symbol.value_declaration.is_none() {
            symbol.value_declaration
        } else {
            *symbol.declarations.first()?
        };
        let node = self.ctx.arena.get(decl_idx)?;
        let enum_decl = self.ctx.arena.get_enum(node)?;

        let mut saw_string = false;
        let mut saw_numeric = false;

        for &member_idx in &enum_decl.members.nodes {
            let Some(member_node) = self.ctx.arena.get(member_idx) else {
                continue;
            };
            let Some(member) = self.ctx.arena.get_enum_member(member_node) else {
                continue;
            };

            if !member.initializer.is_none() {
                let Some(init_node) = self.ctx.arena.get(member.initializer) else {
                    continue;
                };
                match init_node.kind {
                    k if k == SyntaxKind::StringLiteral as u16 => saw_string = true,
                    k if k == SyntaxKind::NumericLiteral as u16 => saw_numeric = true,
                    _ => {}
                }
            } else {
                saw_numeric = true;
            }
        }

        if saw_string && saw_numeric {
            Some(EnumKind::Mixed)
        } else if saw_string {
            Some(EnumKind::String)
        } else if saw_numeric {
            Some(EnumKind::Numeric)
        } else {
            Some(EnumKind::Numeric)
        }
    }

    /// Get the literal type of an enum member from its initializer.
    ///
    /// Returns the literal type (e.g., Literal(0), Literal("a")) of the enum member.
    /// This is used to create TypeKey::Enum(member_def_id, literal_type) for nominal typing.
    pub(crate) fn enum_member_type_from_decl(&self, member_decl: NodeIndex) -> TypeId {
        // Get the member node
        let Some(member_node) = self.ctx.arena.get(member_decl) else {
            return TypeId::ERROR;
        };
        let Some(member) = self.ctx.arena.get_enum_member(member_node) else {
            return TypeId::ERROR;
        };

        // Check if member has an explicit initializer
        if !member.initializer.is_none() {
            let Some(init_node) = self.ctx.arena.get(member.initializer) else {
                return TypeId::ERROR;
            };

            match init_node.kind {
                k if k == SyntaxKind::StringLiteral as u16 => {
                    // Get the string literal value
                    if let Some(lit) = self.ctx.arena.get_literal(init_node) {
                        return self.ctx.types.literal_string(&lit.text);
                    }
                }
                k if k == SyntaxKind::NumericLiteral as u16 => {
                    // Get the numeric literal value
                    if let Some(lit) = self.ctx.arena.get_literal(init_node) {
                        // lit.value is Option<f64>, use it if available
                        if let Some(value) = lit.value {
                            return self.ctx.types.literal_number(value);
                        }
                        // Fallback: parse from text
                        if let Ok(value) = lit.text.parse::<f64>() {
                            return self.ctx.types.literal_number(value);
                        }
                    }
                }
                _ => {
                    // Computed value - fall back to NUMBER for numeric enums
                    // TODO: Evaluate constant expression to get literal value
                }
            }
        }

        // No explicit initializer or computed value
        // This could be an auto-incremented numeric member
        // Fall back to NUMBER type (not a specific literal)
        TypeId::NUMBER
    }

    // =========================================================================
    // Class Helper Functions
    // =========================================================================

    /// Get the class symbol from an expression node.
    ///
    /// Returns the symbol ID if the expression refers to a class, None otherwise.
    pub(crate) fn class_symbol_from_expression(&self, expr_idx: NodeIndex) -> Option<SymbolId> {
        let Some(node) = self.ctx.arena.get(expr_idx) else {
            return None;
        };
        if node.kind == SyntaxKind::Identifier as u16 {
            let sym_id = self.resolve_identifier_symbol(expr_idx)?;
            let symbol = self.ctx.binder.get_symbol(sym_id)?;
            if symbol.flags & symbol_flags::CLASS != 0 {
                return Some(sym_id);
            }
        }
        None
    }

    /// Get the class symbol from a type annotation node.
    ///
    /// Handles type queries like `typeof MyClass`.
    pub(crate) fn class_symbol_from_type_annotation(
        &self,
        type_idx: NodeIndex,
    ) -> Option<SymbolId> {
        let Some(node) = self.ctx.arena.get(type_idx) else {
            return None;
        };
        if node.kind != syntax_kind_ext::TYPE_QUERY {
            return None;
        }
        let query = self.ctx.arena.get_type_query(node)?;
        self.class_symbol_from_expression(query.expr_name)
    }

    /// Get the class symbol from an assignment target.
    ///
    /// Handles cases where the target is a variable with a class type annotation
    /// or initialized with a class expression.
    pub(crate) fn assignment_target_class_symbol(&self, left_idx: NodeIndex) -> Option<SymbolId> {
        let Some(node) = self.ctx.arena.get(left_idx) else {
            return None;
        };
        if node.kind != SyntaxKind::Identifier as u16 {
            return None;
        }
        let sym_id = self.resolve_identifier_symbol(left_idx)?;
        let symbol = self.ctx.binder.get_symbol(sym_id)?;
        if symbol.flags & symbol_flags::CLASS != 0 {
            return Some(sym_id);
        }
        if symbol.flags
            & (symbol_flags::FUNCTION_SCOPED_VARIABLE | symbol_flags::BLOCK_SCOPED_VARIABLE)
            == 0
        {
            return None;
        }
        if symbol.value_declaration.is_none() {
            return None;
        }
        let decl_node = self.ctx.arena.get(symbol.value_declaration)?;
        let var_decl = self.ctx.arena.get_variable_declaration(decl_node)?;
        if !var_decl.type_annotation.is_none()
            && let Some(class_sym) =
                self.class_symbol_from_type_annotation(var_decl.type_annotation)
        {
            return Some(class_sym);
        }
        if !var_decl.initializer.is_none()
            && let Some(class_sym) = self.class_symbol_from_expression(var_decl.initializer)
        {
            return Some(class_sym);
        }
        None
    }

    /// Get the access level of a class constructor.
    ///
    /// Returns Some(MemberAccessLevel::Private) or Some(MemberAccessLevel::Protected) if restricted.
    /// Returns None if public (the default) or if the symbol is not a class.
    ///
    /// Note: If a class has no explicit constructor, it inherits the access level
    /// from its base class's constructor.
    pub(crate) fn class_constructor_access_level(
        &self,
        sym_id: SymbolId,
    ) -> Option<MemberAccessLevel> {
        let symbol = self.ctx.binder.get_symbol(sym_id)?;
        if symbol.flags & symbol_flags::CLASS == 0 {
            return None;
        }
        let decl_idx = if !symbol.value_declaration.is_none() {
            symbol.value_declaration
        } else {
            *symbol.declarations.first()?
        };
        let node = self.ctx.arena.get(decl_idx)?;
        let class = self.ctx.arena.get_class(node)?;

        // First, check if this class has an explicit constructor
        for &member_idx in &class.members.nodes {
            let Some(member_node) = self.ctx.arena.get(member_idx) else {
                continue;
            };
            if member_node.kind != syntax_kind_ext::CONSTRUCTOR {
                continue;
            }
            let Some(ctor) = self.ctx.arena.get_constructor(member_node) else {
                continue;
            };
            // Check modifiers for access level
            if self.has_private_modifier(&ctor.modifiers) {
                return Some(MemberAccessLevel::Private);
            }
            if self.has_protected_modifier(&ctor.modifiers) {
                return Some(MemberAccessLevel::Protected);
            }
            // Explicit public constructor - public default
            return None;
        }

        // No explicit constructor found - check base class if extends clause exists
        let Some(ref heritage_clauses) = class.heritage_clauses else {
            // No extends clause - public default
            return None;
        };

        // Find the extends clause and get the base class
        for &clause_idx in &heritage_clauses.nodes {
            let Some(clause_node) = self.ctx.arena.get(clause_idx) else {
                continue;
            };

            let Some(heritage) = self.ctx.arena.get_heritage_clause(clause_node) else {
                continue;
            };

            // Only check extends clauses (not implements)
            if heritage.token != tsz_scanner::SyntaxKind::ExtendsKeyword as u16 {
                continue;
            }

            // Get the first type in the extends clause
            let Some(&first_type_idx) = heritage.types.nodes.first() else {
                continue;
            };

            // Get the expression from ExpressionWithTypeArguments
            let expr_idx = if let Some(type_node) = self.ctx.arena.get(first_type_idx)
                && let Some(expr_type_args) = self.ctx.arena.get_expr_type_args(type_node)
            {
                expr_type_args.expression
            } else {
                first_type_idx
            };

            // Resolve the base class symbol
            let Some(base_sym) = self.resolve_heritage_symbol(expr_idx) else {
                continue;
            };

            // Recursively check the base class's constructor access level
            // This handles inherited private/protected constructors
            return self.class_constructor_access_level(base_sym);
        }

        // No extends clause or couldn't resolve base class - public default
        None
    }

    // =========================================================================
    // Type Query Helper Functions
    // =========================================================================
    // TODO(solver-visitor): Consider migrating to type_queries pattern.
    // The functions below use manual TypeKey matching but could potentially
    // use a generic contains_type_matching pattern like in type_queries.rs.
    // This would require adding `contains_any_type` to type_queries.rs.
    // =========================================================================

    /// Check if a type contains 'any' (recursively).
    ///
    /// Returns true if the type is ANY or contains ANY in its structure.
    // TODO(solver-visitor): Could use type_queries::contains_type_matching_impl
    // with predicate checking for TypeId::ANY
    pub(crate) fn type_contains_any(&self, type_id: TypeId) -> bool {
        let mut visited = Vec::new();
        self.type_contains_any_inner(type_id, &mut visited)
    }

    /// Inner recursive implementation of type_contains_any.
    fn type_contains_any_inner(&self, type_id: TypeId, visited: &mut Vec<TypeId>) -> bool {
        use tsz_solver::TemplateSpan;
        use tsz_solver::type_queries::{TypeContainsKind, classify_for_contains_traversal};

        if type_id == TypeId::ANY {
            return true;
        }
        if visited.contains(&type_id) {
            return false;
        }
        visited.push(type_id);

        match classify_for_contains_traversal(self.ctx.types, type_id) {
            TypeContainsKind::Array(elem) => self.type_contains_any_inner(elem, visited),
            TypeContainsKind::Tuple(list_id) => self
                .ctx
                .types
                .tuple_list(list_id)
                .iter()
                .any(|elem| self.type_contains_any_inner(elem.type_id, visited)),
            TypeContainsKind::Members(members) => members
                .iter()
                .any(|&member| self.type_contains_any_inner(member, visited)),
            TypeContainsKind::Object(shape_id) => {
                let shape = self.ctx.types.object_shape(shape_id);
                if shape
                    .properties
                    .iter()
                    .any(|prop| self.type_contains_any_inner(prop.type_id, visited))
                {
                    return true;
                }
                if let Some(ref index) = shape.string_index
                    && self.type_contains_any_inner(index.value_type, visited)
                {
                    return true;
                }
                if let Some(ref index) = shape.number_index
                    && self.type_contains_any_inner(index.value_type, visited)
                {
                    return true;
                }
                false
            }
            TypeContainsKind::Function(shape_id) => {
                let shape = self.ctx.types.function_shape(shape_id);
                self.type_contains_any_inner(shape.return_type, visited)
            }
            TypeContainsKind::Callable(shape_id) => {
                let shape = self.ctx.types.callable_shape(shape_id);
                if shape
                    .call_signatures
                    .iter()
                    .any(|sig| self.type_contains_any_inner(sig.return_type, visited))
                {
                    return true;
                }
                if shape
                    .construct_signatures
                    .iter()
                    .any(|sig| self.type_contains_any_inner(sig.return_type, visited))
                {
                    return true;
                }
                shape
                    .properties
                    .iter()
                    .any(|prop| self.type_contains_any_inner(prop.type_id, visited))
            }
            TypeContainsKind::Application(app_id) => {
                let app = self.ctx.types.type_application(app_id);
                if self.type_contains_any_inner(app.base, visited) {
                    return true;
                }
                app.args
                    .iter()
                    .any(|&arg| self.type_contains_any_inner(arg, visited))
            }
            TypeContainsKind::Conditional(cond_id) => {
                let cond = self.ctx.types.conditional_type(cond_id);
                self.type_contains_any_inner(cond.check_type, visited)
                    || self.type_contains_any_inner(cond.extends_type, visited)
                    || self.type_contains_any_inner(cond.true_type, visited)
                    || self.type_contains_any_inner(cond.false_type, visited)
            }
            TypeContainsKind::Mapped(mapped_id) => {
                let mapped = self.ctx.types.mapped_type(mapped_id);
                if self.type_contains_any_inner(mapped.constraint, visited) {
                    return true;
                }
                if let Some(name_type) = mapped.name_type
                    && self.type_contains_any_inner(name_type, visited)
                {
                    return true;
                }
                self.type_contains_any_inner(mapped.template, visited)
            }
            TypeContainsKind::IndexAccess { base, index } => {
                self.type_contains_any_inner(base, visited)
                    || self.type_contains_any_inner(index, visited)
            }
            TypeContainsKind::TemplateLiteral(template_id) => self
                .ctx
                .types
                .template_list(template_id)
                .iter()
                .any(|span| match span {
                    TemplateSpan::Type(span_type) => {
                        self.type_contains_any_inner(*span_type, visited)
                    }
                    _ => false,
                }),
            TypeContainsKind::Inner(inner) => self.type_contains_any_inner(inner, visited),
            TypeContainsKind::TypeParam {
                constraint,
                default,
            } => {
                if let Some(constraint) = constraint
                    && self.type_contains_any_inner(constraint, visited)
                {
                    return true;
                }
                if let Some(default) = default
                    && self.type_contains_any_inner(default, visited)
                {
                    return true;
                }
                false
            }
            TypeContainsKind::Terminal => false,
        }
    }

    /// Get display string for implicit any return type.
    ///
    /// Returns "any" for null/undefined only types, otherwise formats the type.
    pub(crate) fn implicit_any_return_display(&self, return_type: TypeId) -> String {
        if self.is_null_or_undefined_only(return_type) {
            return "any".to_string();
        }
        self.format_type(return_type)
    }

    /// Check if we should report implicit any return type.
    ///
    /// Only reports when return type is exactly 'any', not when it contains 'any' somewhere.
    /// For example, Promise<void> should not trigger TS7010 even if Promise's definition
    /// contains 'any' in its type structure.
    pub(crate) fn should_report_implicit_any_return(&self, return_type: TypeId) -> bool {
        // void is a valid inferred return type (functions with no return statements),
        // it should NOT trigger TS7010 "Function lacks ending return statement"
        if return_type == TypeId::VOID {
            return false;
        }
        return_type == TypeId::ANY || self.is_null_or_undefined_only(return_type)
    }

    /// Check if a property in a derived class is redeclaring a base class property.
    #[allow(dead_code)] // Infrastructure for class inheritance checking
    pub(crate) fn is_derived_property_redeclaration(
        &self,
        member_idx: NodeIndex,
        _property_name: &str,
    ) -> bool {
        // Find the containing class for this member
        if let Some(class_idx) = self.find_containing_class(member_idx)
            && let Some(class_node) = self.ctx.arena.get(class_idx)
            && let Some(class_data) = self.ctx.arena.get_class(class_node)
        {
            // Check if this class has a base class (extends clause)
            if self.class_has_base(class_data) {
                // In derived classes, properties need definite assignment
                // unless they have explicit initializers or definite assignment assertion
                // This catches cases like: class B extends A { property: any; }
                return true;
            }
        }
        false
    }

    /// Find the containing class for a member node by walking up the parent chain.
    #[allow(dead_code)] // Infrastructure for class member resolution
    pub(crate) fn find_containing_class(&self, _member_idx: NodeIndex) -> Option<NodeIndex> {
        // Check if this member is directly in a class
        // Since we don't have parent pointers, we need to search through classes
        // This is a simplified approach - in a full implementation we'd maintain parent links

        // For now, assume the member is in a class context if we're checking properties
        // The actual class detection would require traversing the full AST
        // This is sufficient for the TS2524 definite assignment checking we need
        None // Simplified implementation - could be enhanced with full parent tracking
    }

    // =========================================================================
    // Type Refinement Helper Functions
    // =========================================================================

    /// Refine variable declaration type based on assignment.
    ///
    /// Returns the more specific type when prev_type is ANY and current_type is concrete.
    /// This implements type refinement for multiple assignments.
    pub(crate) fn refine_var_decl_type(&self, prev_type: TypeId, current_type: TypeId) -> TypeId {
        if matches!(prev_type, TypeId::ANY | TypeId::ERROR)
            && !matches!(current_type, TypeId::ANY | TypeId::ERROR)
        {
            return current_type;
        }
        prev_type
    }

    /// Check if two symbol flags can be merged.
    ///
    /// Returns true if the symbols are compatible for merging.
    /// Used in symbol table updates for ambient contexts.
    pub(crate) fn can_merge_symbols(&self, existing_flags: u32, new_flags: u32) -> bool {
        // Interface can merge with interface
        if (existing_flags & symbol_flags::INTERFACE) != 0
            && (new_flags & symbol_flags::INTERFACE) != 0
        {
            return true;
        }

        // Class can merge with interface
        if ((existing_flags & symbol_flags::CLASS) != 0
            && (new_flags & symbol_flags::INTERFACE) != 0)
            || ((existing_flags & symbol_flags::INTERFACE) != 0
                && (new_flags & symbol_flags::CLASS) != 0)
        {
            return true;
        }

        // Namespace/module can merge with namespace/module
        if (existing_flags & symbol_flags::MODULE) != 0 && (new_flags & symbol_flags::MODULE) != 0 {
            return true;
        }

        // Namespace can merge with class, function, or enum
        if (existing_flags & symbol_flags::MODULE) != 0
            && (new_flags & (symbol_flags::CLASS | symbol_flags::FUNCTION | symbol_flags::ENUM))
                != 0
        {
            return true;
        }
        if (new_flags & symbol_flags::MODULE) != 0
            && (existing_flags
                & (symbol_flags::CLASS | symbol_flags::FUNCTION | symbol_flags::ENUM))
                != 0
        {
            return true;
        }

        // Function overloads
        if (existing_flags & symbol_flags::FUNCTION) != 0
            && (new_flags & symbol_flags::FUNCTION) != 0
        {
            return true;
        }

        false
    }

    // =========================================================================
    // Property Readonly Helper Functions
    // =========================================================================

    /// Check if a class property is readonly.
    ///
    /// Returns true if the property has a readonly modifier.
    /// Note: This is a stub - full implementation requires symbol lookup by name.
    pub(crate) fn is_class_property_readonly(&self, _class_name: &str, _prop_name: &str) -> bool {
        // TODO: Implement when get_symbol_by_name is available
        false
    }

    /// Check if a property of a type is readonly.
    ///
    /// Delegates to the solver's comprehensive implementation which handles:
    /// - ReadonlyType wrappers (readonly arrays/tuples)
    /// - Object types with readonly properties
    /// - ObjectWithIndex types (readonly index signatures)
    /// - Union types (readonly if ANY member has readonly property)
    /// - Intersection types (readonly ONLY if ALL members have readonly property)
    pub(crate) fn is_property_readonly(&self, type_id: TypeId, prop_name: &str) -> bool {
        self.ctx.types.is_property_readonly(type_id, prop_name)
    }

    /// Check if property existence errors should be emitted for destructuring.
    ///
    /// Check if we should emit a "property does not exist" error for the given type in destructuring.
    /// Returns false for any, unknown, or types that don't have concrete shapes.
    pub(crate) fn should_emit_property_not_exist_for_destructuring(&self, type_id: TypeId) -> bool {
        use tsz_solver::type_queries;

        if type_id == TypeId::ANY || type_id == TypeId::UNKNOWN || type_id == TypeId::ERROR {
            return false;
        }

        // Object types are concrete - emit errors for them
        if type_queries::is_object_type(self.ctx.types, type_id) {
            return true;
        }

        // For unions, emit error if any member is a concrete object
        if let Some(members) = type_queries::get_union_members(self.ctx.types, type_id) {
            return members
                .iter()
                .any(|&t| self.should_emit_property_not_exist_for_destructuring(t));
        }

        // For intersections, all members should be concrete objects
        if let Some(members) = type_queries::get_intersection_members(self.ctx.types, type_id) {
            return members
                .iter()
                .all(|&t| self.should_emit_property_not_exist_for_destructuring(t));
        }

        false
    }

    /// Get the class name from a variable declaration.
    ///
    /// Returns the class name if the variable is initialized with a class expression.
    pub(crate) fn get_class_name_from_var_decl(&self, decl_idx: NodeIndex) -> Option<String> {
        let node = self.ctx.arena.get(decl_idx)?;
        let var_decl = self.ctx.arena.get_variable_declaration(node)?;

        if var_decl.initializer.is_none() {
            return None;
        }

        let init_node = self.ctx.arena.get(var_decl.initializer)?;
        if init_node.kind != syntax_kind_ext::CLASS_EXPRESSION {
            return None;
        }

        let class = self.ctx.arena.get_class(init_node)?;
        if class.name.is_none() {
            return None;
        }

        let name_node = self.ctx.arena.get(class.name)?;
        let ident = self.ctx.arena.get_identifier(name_node)?;
        Some(ident.escaped_text.clone())
    }

    // =========================================================================
    // AST Navigation Helper Functions
    // =========================================================================

    /// Get class expression returned from a function body.
    ///
    /// Searches for return statements that return class expressions.
    pub(crate) fn returned_class_expression(&self, body_idx: NodeIndex) -> Option<NodeIndex> {
        if body_idx.is_none() {
            return None;
        }
        let node = self.ctx.arena.get(body_idx)?;
        if node.kind != syntax_kind_ext::BLOCK {
            return self.class_expression_from_expr(body_idx);
        }
        let block = self.ctx.arena.get_block(node)?;
        for &stmt_idx in &block.statements.nodes {
            let stmt = self.ctx.arena.get(stmt_idx)?;
            if stmt.kind != syntax_kind_ext::RETURN_STATEMENT {
                continue;
            }
            let ret = self.ctx.arena.get_return_statement(stmt)?;
            if ret.expression.is_none() {
                continue;
            }
            if let Some(expr_idx) = self.class_expression_from_expr(ret.expression) {
                return Some(expr_idx);
            }
            let expr_node = self.ctx.arena.get(ret.expression)?;
            if let Some(ident) = self.ctx.arena.get_identifier(expr_node)
                && let Some(class_idx) =
                    self.class_declaration_from_identifier_in_block(block, &ident.escaped_text)
            {
                return Some(class_idx);
            }
        }
        None
    }

    /// Find class declaration by identifier name in a block.
    ///
    /// Searches for class declarations with the given name.
    pub(crate) fn class_declaration_from_identifier_in_block(
        &self,
        block: &tsz_parser::parser::node::BlockData,
        name: &str,
    ) -> Option<NodeIndex> {
        for &stmt_idx in &block.statements.nodes {
            let stmt = self.ctx.arena.get(stmt_idx)?;
            if stmt.kind != syntax_kind_ext::CLASS_DECLARATION {
                continue;
            }
            let class = self.ctx.arena.get_class(stmt)?;
            if class.name.is_none() {
                continue;
            }
            let name_node = self.ctx.arena.get(class.name)?;
            let ident = self.ctx.arena.get_identifier(name_node)?;
            if ident.escaped_text == name {
                return Some(stmt_idx);
            }
        }
        None
    }

    /// Get class expression from any expression node.
    ///
    /// Unwraps parenthesized expressions and returns the class expression if found.
    pub(crate) fn class_expression_from_expr(&self, expr_idx: NodeIndex) -> Option<NodeIndex> {
        const MAX_TREE_WALK_ITERATIONS: usize = 1000;

        let mut current = expr_idx;
        let mut iterations = 0;
        loop {
            iterations += 1;
            if iterations > MAX_TREE_WALK_ITERATIONS {
                return None;
            }
            let node = self.ctx.arena.get(current)?;
            if node.kind == syntax_kind_ext::PARENTHESIZED_EXPRESSION {
                let paren = self.ctx.arena.get_parenthesized(node)?;
                current = paren.expression;
                continue;
            }
            if node.kind == syntax_kind_ext::CLASS_EXPRESSION {
                return Some(current);
            }
            return None;
        }
    }

    /// Get function declaration from callee expression.
    ///
    /// Returns the function declaration if the callee is a function with a body.
    pub(crate) fn function_decl_from_callee(&self, callee_idx: NodeIndex) -> Option<NodeIndex> {
        let node = self.ctx.arena.get(callee_idx)?;
        if node.kind != SyntaxKind::Identifier as u16 {
            return None;
        }
        let sym_id = self.resolve_identifier_symbol(callee_idx)?;
        let symbol = self.ctx.binder.get_symbol(sym_id)?;

        for &decl_idx in &symbol.declarations {
            let node = self.ctx.arena.get(decl_idx)?;
            let func = self.ctx.arena.get_function(node)?;
            if !func.body.is_none() {
                return Some(decl_idx);
            }
        }

        if !symbol.value_declaration.is_none() {
            let decl_idx = symbol.value_declaration;
            let node = self.ctx.arena.get(decl_idx)?;
            let func = self.ctx.arena.get_function(node)?;
            if !func.body.is_none() {
                return Some(decl_idx);
            }
        }

        None
    }

    // ============================================================================
    // Section 58: Enum Type Utilities
    // ============================================================================

    /// Get enum member type by property name.
    ///
    /// This function resolves the type of an enum member accessed by name.
    /// It searches through all enum declarations for the symbol to find
    /// a matching member name and returns the enum type (not the primitive).
    ///
    /// ## Parameters:
    /// - `sym_id`: The enum symbol ID
    /// - `property_name`: The member property name to search for
    ///
    /// ## Returns:
    /// - `Some(TypeId)`: The enum type (as a Ref to the enum symbol)
    /// - `None`: If the symbol is not an enum or member not found
    ///
    /// ## Examples:
    /// ```typescript
    /// enum Color {
    ///   Red,
    ///   Green,
    ///   Blue
    /// }
    /// type T = Color["Red"];  // Returns the enum type Color
    /// ```
    ///
    /// Note: This returns the enum type itself, not STRING or NUMBER,
    /// which allows proper enum assignability checking.
    pub(crate) fn enum_member_type_for_name(
        &mut self,
        sym_id: SymbolId,
        property_name: &str,
    ) -> Option<TypeId> {
        let symbol = self.ctx.binder.get_symbol(sym_id)?;
        if symbol.flags & symbol_flags::ENUM == 0 {
            return None;
        }

        // Check if the property exists in this enum
        for &decl_idx in &symbol.declarations {
            let Some(node) = self.ctx.arena.get(decl_idx) else {
                continue;
            };
            let Some(enum_decl) = self.ctx.arena.get_enum(node) else {
                continue;
            };
            for &member_idx in &enum_decl.members.nodes {
                let Some(member_node) = self.ctx.arena.get(member_idx) else {
                    continue;
                };
                let Some(member) = self.ctx.arena.get_enum_member(member_node) else {
                    continue;
                };
                if let Some(name) = self.get_property_name(member.name)
                    && name == property_name
                {
                    // Return the enum type itself by getting the computed type of the symbol
                    // This returns TypeKey::Enum(def_id, structural_type) which allows proper
                    // enum assignability checking with nominal identity
                    return Some(self.get_type_of_symbol(sym_id));
                }
            }
        }

        None
    }
}
