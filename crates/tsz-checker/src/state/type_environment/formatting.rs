//! Type formatting and globalThis helpers.
//!
//! Extracted from `core.rs` to keep module size manageable.

use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    pub(super) fn error_property_not_exist_on_global_this(
        &mut self,
        name: &str,
        error_node: NodeIndex,
        base_display: &str,
    ) {
        use crate::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};
        self.error_at_node(
            error_node,
            &format_message(
                diagnostic_messages::PROPERTY_DOES_NOT_EXIST_ON_TYPE,
                &[name, base_display],
            ),
            diagnostic_codes::PROPERTY_DOES_NOT_EXIST_ON_TYPE,
        );
    }

    /// Format a type as a human-readable string for error messages and diagnostics.
    ///
    /// This is the main entry point for converting `TypeId` representations into
    /// human-readable type strings. Used throughout the type checker for error
    /// messages, quick info, and IDE features.
    ///
    /// ## Formatting Strategy:
    /// - Delegates to the solver's `TypeFormatter`
    /// - Provides symbol table for resolving symbol names
    /// - Handles all type constructs (primitives, generics, unions, etc.)
    ///
    /// ## Type Formatting Rules:
    /// - Primitives: Display as intrinsic names (string, number, etc.)
    /// - Literals: Display as literal values ("hello", 42, true)
    /// - Arrays: Display as T[] or Array<T>
    /// - Tuples: Display as [T, U, V]
    /// - Unions: Display as T | U | V (with parentheses when needed)
    /// - Intersections: Display as T & U & V (with parentheses when needed)
    /// - Functions: Display as (args) => return
    /// - Objects: Display as { prop: Type; ... }
    /// - Type Parameters: Display as T, U, V (short names)
    /// - Type References: Display as `RefName`<Args>
    ///
    /// ## Use Cases:
    /// - Error messages: "Type X is not assignable to Y"
    /// - Quick info (hover): Type information for IDE
    /// - Completion: Type hints in autocomplete
    /// - Diagnostics: All type-related error messages
    ///
    /// ## TypeScript Examples (Formatted Output):
    /// ```typescript
    /// // Primitives
    /// let x: string;           // format_type → "string"
    /// let y: number;           // format_type → "number"
    ///
    /// // Literals
    /// let a: "hello";          // format_type → "\"hello\""
    /// let b: 42;               // format_type → "42"
    ///
    /// // Composed types
    /// type Pair = [string, number];
    /// // format_type(Pair) → "[string, number]"
    ///
    /// type Union = string | number | boolean;
    /// // format_type(Union) → "string | number | boolean"
    ///
    /// // Generics
    /// type Map<K, V> = Record<K, V>;
    /// // format_type(Map<string, number>) → "Record<string, number>"
    ///
    /// // Functions
    /// type Handler = (data: string) => void;
    /// // format_type(Handler) → "(data: string) => void"
    ///
    /// // Objects
    /// type User = { name: string; age: number };
    /// // format_type(User) → "{ name: string; age: number }"
    ///
    /// // Complex
    /// type Complex = Array<{ id: number } | null>;
    /// // format_type(Complex) → "Array<{ id: number } | null>"
    /// ```
    pub fn format_type(&self, type_id: TypeId) -> String {
        // Use full formatter with DefId context for proper type name display
        let mut formatter = self.ctx.create_type_formatter();
        formatter.format(type_id).into_owned()
    }

    /// Format a type for use in diagnostic error messages.
    /// Unlike `format_type`, this skips union optionalization (synthetic `?: undefined`)
    /// that tsc only uses in hover/quickinfo, not in error messages.
    /// Enables display properties to preserve original literal types from the
    /// freshness model (e.g., `"frizzlebizzle"` not `string`) matching tsc.
    pub fn format_type_diagnostic(&self, type_id: TypeId) -> String {
        if let Some(sym_id) = self.ctx.resolve_type_to_symbol_id(type_id)
            && let Some(symbol) = self.ctx.binder.get_symbol(sym_id)
            && (symbol.flags & tsz_binder::symbol_flags::ENUM_MEMBER) != 0
            && let Some(parent) = self.ctx.binder.get_symbol(symbol.parent)
        {
            return format!("{}.{}", parent.escaped_name, symbol.escaped_name);
        }
        let mut formatter = self
            .ctx
            .create_diagnostic_type_formatter()
            .with_display_properties();
        formatter.format(type_id).into_owned()
    }

    /// Format a type for assignability error messages WITHOUT display properties.
    /// tsc shows widened property types in assignability messages:
    /// `{ two: number }` not `{ two: 1 }`.
    pub fn format_type_diagnostic_widened(&self, type_id: TypeId) -> String {
        let mut formatter = self
            .ctx
            .create_type_formatter()
            .with_diagnostic_mode()
            .with_strict_null_checks(self.ctx.compiler_options.strict_null_checks);
        formatter.format(type_id).into_owned()
    }

    /// Format a type for TS2741 messages, showing the merged object form
    /// instead of following display_alias to intersection types.
    pub fn format_type_diagnostic_flattened(&self, type_id: TypeId) -> String {
        let mut formatter = self
            .ctx
            .create_diagnostic_type_formatter()
            .with_skip_intersection_display_alias();
        formatter.format(type_id).into_owned()
    }

    /// Format a type for diagnostics while suppressing function-type parameter
    /// binders in the displayed surface.
    ///
    /// This matches tsc's iterator-protocol diagnostics, which commonly print
    /// `() => T` instead of `<T>() => T`.
    pub fn format_type_diagnostic_without_function_type_params(
        &mut self,
        type_id: TypeId,
    ) -> String {
        let type_id = self.evaluate_type_with_env(type_id);
        let type_id = self.resolve_type_for_property_access(type_id);
        let type_id = self.resolve_lazy_type(type_id);
        let type_id = self.evaluate_application_type(type_id);

        if let Some(shape) =
            crate::query_boundaries::common::function_shape_for_type(self.ctx.types, type_id)
            && !shape.type_params.is_empty()
        {
            let display_type = self
                .ctx
                .types
                .factory()
                .function(tsz_solver::FunctionShape {
                    type_params: Vec::new(),
                    params: shape.params.clone(),
                    this_type: shape.this_type,
                    return_type: shape.return_type,
                    type_predicate: shape.type_predicate,
                    is_constructor: shape.is_constructor,
                    is_method: shape.is_method,
                });
            return self.format_type_diagnostic(display_type);
        }

        if let Some(shape) =
            crate::query_boundaries::common::callable_shape_for_type(self.ctx.types, type_id)
            && shape.properties.is_empty()
            && shape.string_index.is_none()
            && shape.number_index.is_none()
            && shape.construct_signatures.is_empty()
            && shape.call_signatures.len() == 1
        {
            let sig = &shape.call_signatures[0];
            if !sig.type_params.is_empty() {
                let display_type = self
                    .ctx
                    .types
                    .factory()
                    .function(tsz_solver::FunctionShape {
                        type_params: Vec::new(),
                        params: sig.params.clone(),
                        this_type: sig.this_type,
                        return_type: sig.return_type,
                        type_predicate: sig.type_predicate,
                        is_constructor: false,
                        is_method: sig.is_method,
                    });
                return self.format_type_diagnostic(display_type);
            }
        }

        self.format_type_diagnostic(type_id)
    }

    /// Format a type for diagnostics with display properties enabled.
    /// Uses pre-widened literal types from the freshness model side table.
    pub fn format_type_diagnostic_with_display(&self, type_id: TypeId) -> String {
        let mut formatter = self
            .ctx
            .create_diagnostic_type_formatter()
            .with_display_properties();
        formatter.format(type_id).into_owned()
    }

    /// Format a pair of types for diagnostics that display two types side by side.
    /// Applies cross-type name disambiguation (namespace / `import("<specifier>")`)
    /// when the two types format to the same short name.
    pub fn format_type_pair(&self, type_a: TypeId, type_b: TypeId) -> (String, String) {
        let mut formatter = self.ctx.create_type_formatter();
        formatter.format_pair_disambiguated(type_a, type_b)
    }

    /// Format a type for TS2367 comparison overlap error messages.
    /// tsc shows unique symbols as `typeof varName` in comparison contexts
    /// (distinct from index-type errors where it shows `unique symbol`).
    pub(crate) fn format_type_for_ts2367_display(&self, type_id: TypeId) -> String {
        use crate::query_boundaries::common::unique_symbol_ref;
        if let Some(sym_ref) = unique_symbol_ref(self.ctx.types, type_id) {
            let mut formatter = self.ctx.create_type_formatter();
            if let Some(name) = formatter.resolve_unique_symbol_name(sym_ref) {
                return format!("typeof {name}");
            }
        }
        self.format_type(type_id)
    }

    /// Format a pair of types for diagnostic messages (skips union optionalization).
    /// When the two types format to the same short name, the formatter re-qualifies
    /// them — first via namespace prefix, then `import("<specifier>").Name` — so
    /// the reader can distinguish them.
    pub fn format_type_pair_diagnostic(&self, type_a: TypeId, type_b: TypeId) -> (String, String) {
        let mut formatter = self.ctx.create_diagnostic_type_formatter();
        formatter.format_pair_disambiguated(type_a, type_b)
    }

    /// Restore boolean literal types from display properties onto an
    /// already-widened object type.
    ///
    /// tsc preserves boolean literals (`true`/`false`) in error messages while
    /// widening other literal types (`""` → `string`, `42` → `number`). When
    /// the internal object type has already been widened (properties are
    /// `boolean` not `true`), we look up the display-property side table to
    /// find the original boolean literal and rebuild the object with it.
    ///
    /// Falls back to the `evaluated_type` if no display properties exist or
    /// no boolean restoration is needed.
    pub fn restore_boolean_display_properties(
        &self,
        evaluated_type: TypeId,
        original_type: TypeId,
    ) -> TypeId {
        use crate::query_boundaries::common;

        // Try display properties on both the evaluated and original type IDs.
        let display_props = self
            .ctx
            .types
            .get_display_properties(evaluated_type)
            .or_else(|| self.ctx.types.get_display_properties(original_type));

        let display_props = match display_props {
            Some(props) => props,
            None => return evaluated_type,
        };

        // Check if the evaluated type is an object with properties we can patch.
        let shape = match common::object_shape_for_type(self.ctx.types, evaluated_type) {
            Some(shape) => shape,
            None => return evaluated_type,
        };

        // Build a map of boolean literal display properties keyed by property name.
        let mut bool_overrides = rustc_hash::FxHashMap::default();
        for dp in display_props.iter() {
            if dp.type_id == TypeId::BOOLEAN_TRUE || dp.type_id == TypeId::BOOLEAN_FALSE {
                bool_overrides.insert(dp.name, dp.type_id);
            }
        }

        if bool_overrides.is_empty() {
            return evaluated_type;
        }

        // Rebuild properties with boolean literals restored.
        let mut new_props: Vec<tsz_solver::PropertyInfo> = shape
            .properties
            .iter()
            .map(|prop| {
                if let Some(&bool_type) = bool_overrides.get(&prop.name) {
                    let mut p = prop.clone();
                    p.type_id = bool_type;
                    p
                } else {
                    prop.clone()
                }
            })
            .collect();
        new_props.sort_by_key(|p| p.name);
        self.ctx.types.factory().object(new_props)
    }
}
