//! Type/value mismatch and declaration error reporting (TS2693, TS2749, TS2708, TS2709).

use super::TypeOnlyKind;
use crate::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};
use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    // =========================================================================
    // Variable/Declaration Errors
    // =========================================================================

    /// Report error 2403: Subsequent variable declarations must have the same type.
    pub fn error_subsequent_variable_declaration(
        &mut self,
        name: &str,
        prev_type: TypeId,
        current_type: TypeId,
        idx: NodeIndex,
    ) {
        // Suppress for ERROR types to avoid cascading diagnostics from unresolved types.
        if prev_type == TypeId::ERROR || current_type == TypeId::ERROR {
            return;
        }
        let prev_type_str = self.format_type_diagnostic(prev_type);
        let current_type_str = self.format_type_diagnostic(current_type);
        // Suppress when both types format to the same name. This handles cross-binder
        // scenarios where a lib_checker resolves a type annotation (e.g., `Document`)
        // to a separate DefId from the main checker's version. Interface declaration
        // merging means both annotations semantically refer to the same type, but
        // different internal TypeIds prevent the structural check from recognizing this.
        if prev_type_str == current_type_str {
            return;
        }
        let message = format!(
            "Subsequent variable declarations must have the same type. Variable '{name}' must be of type '{prev_type_str}', but here has type '{current_type_str}'."
        );
        self.error_at_node(idx, &message, diagnostic_codes::SUBSEQUENT_VARIABLE_DECLARATIONS_MUST_HAVE_THE_SAME_TYPE_VARIABLE_MUST_BE_OF_TYP);
    }

    /// Report TS2454: Variable is used before being assigned.
    pub fn error_variable_used_before_assigned_at(&mut self, name: &str, idx: NodeIndex) {
        self.error_at_node_msg(
            idx,
            diagnostic_codes::VARIABLE_IS_USED_BEFORE_BEING_ASSIGNED,
            &[name],
        );
    }

    // =========================================================================
    // Class-Related Errors
    // =========================================================================

    /// Report error 2715: Abstract property 'X' in class 'C' cannot be accessed in the constructor.
    pub fn error_abstract_property_in_constructor(
        &mut self,
        prop_name: &str,
        class_name: &str,
        idx: NodeIndex,
    ) {
        let message = format!(
            "Abstract property '{prop_name}' in class '{class_name}' cannot be accessed in the constructor."
        );
        self.error_at_node(
            idx,
            &message,
            diagnostic_codes::ABSTRACT_PROPERTY_IN_CLASS_CANNOT_BE_ACCESSED_IN_THE_CONSTRUCTOR,
        );
    }

    // =========================================================================
    // Module/Namespace Errors
    // =========================================================================

    /// Report TS2694 or TS2724: Namespace has no exported member.
    ///
    /// If `export_names` is provided and a spelling suggestion is found,
    /// emits TS2724 ("Did you mean?") instead of TS2694.
    pub fn error_namespace_no_export(
        &mut self,
        namespace_name: &str,
        member_name: &str,
        idx: NodeIndex,
    ) {
        self.error_namespace_no_export_with_exports(namespace_name, member_name, idx, &[]);
    }

    /// Report TS2694 or TS2724 with candidate export names for spelling suggestions.
    pub fn error_namespace_no_export_with_exports(
        &mut self,
        namespace_name: &str,
        member_name: &str,
        idx: NodeIndex,
        export_names: &[String],
    ) {
        // Try to find a spelling suggestion among the namespace's exports
        if let Some(suggestion) = Self::find_export_spelling_suggestion(member_name, export_names) {
            let message = format!(
                "'{namespace_name}' has no exported member named '{member_name}'. Did you mean '{suggestion}'?"
            );
            self.error_at_node(
                idx,
                &message,
                diagnostic_codes::HAS_NO_EXPORTED_MEMBER_NAMED_DID_YOU_MEAN,
            );
        } else {
            let message =
                format!("Namespace '{namespace_name}' has no exported member '{member_name}'.");
            self.error_at_node(idx, &message, 2694);
        }
    }

    /// Search export names for a spelling suggestion matching `member_name`.
    pub(crate) fn find_export_spelling_suggestion(
        member_name: &str,
        export_names: &[String],
    ) -> Option<String> {
        if export_names.is_empty() {
            return None;
        }

        let name_len = member_name.len();
        // tsc: bestDistance = (name.length + 2) * 0.34 rounded down, min 2
        let maximum_length_difference = if name_len * 34 / 100 > 2 {
            name_len * 34 / 100
        } else {
            2
        };
        // tsc: initial bestDistance = floor(name.length * 0.4) + 1
        let mut best_distance = (name_len * 4 / 10 + 1) as f64;
        let mut best_candidate: Option<String> = None;

        for candidate in export_names {
            Self::consider_identifier_suggestion(
                member_name,
                candidate,
                name_len,
                maximum_length_difference,
                &mut best_distance,
                &mut best_candidate,
            );
        }

        best_candidate
    }

    // =========================================================================
    // Type/Value Mismatch Errors
    // =========================================================================

    /// Report TS2698: Spread types may only be created from object types.
    pub fn report_spread_not_object_type(&mut self, idx: NodeIndex) {
        self.error_at_node(
            idx,
            diagnostic_messages::SPREAD_TYPES_MAY_ONLY_BE_CREATED_FROM_OBJECT_TYPES,
            diagnostic_codes::SPREAD_TYPES_MAY_ONLY_BE_CREATED_FROM_OBJECT_TYPES,
        );
    }

    /// Report TS2693/TS2585: Symbol only refers to a type, but is used as a value.
    ///
    /// For ES2015+ types (Promise, Map, Set, Symbol, etc.), emits TS2585 with a suggestion
    /// to change the target library. For other types, emits TS2693 without the lib suggestion.
    pub fn error_type_only_value_at(&mut self, name: &str, idx: NodeIndex) {
        use tsz_binder::lib_loader;

        // Don't emit TS2693 for identifiers used as import equals module references.
        // `import r = undefined` already gets TS2503 from check_namespace_import.
        if self.ctx.arena.get_extended(idx).is_some_and(|ext| {
            self.ctx.arena.get(ext.parent).is_some_and(|p| {
                p.kind == tsz_parser::parser::syntax_kind_ext::IMPORT_EQUALS_DECLARATION
            })
        }) {
            return;
        }

        // Only suppress TS1361/TS1362 in type-only heritage contexts
        // (interface extends, class implements, declare class extends).
        // Regular class extends is a value context — TS1361 must fire.
        if self.is_heritage_type_only_context(idx) {
            return;
        }

        // Check if this is an ES2015+ type that requires specific lib support
        let is_es2015_type = lib_loader::is_es2015_plus_type(name);
        let allow_in_parse_recovery = self.has_type_only_value_in_parse_recovery_context(name, idx);

        // In syntax-error files, TS2693 often cascades from parser recovery and
        // diverges from tsc's primary-diagnostic set. Keep TS2585 behavior intact.
        let allow_any_in_parse_recovery = name == "any";
        if self.has_parse_errors()
            && !is_es2015_type
            && !allow_any_in_parse_recovery
            && !allow_in_parse_recovery
        {
            return;
        }

        let (code, message) = if is_es2015_type {
            // TS2585: Type only refers to a type, suggest changing target library
            (
                diagnostic_codes::ONLY_REFERS_TO_A_TYPE_BUT_IS_BEING_USED_AS_A_VALUE_HERE_DO_YOU_NEED_TO_CHANGE_YO,
                format_message(
                    diagnostic_messages::ONLY_REFERS_TO_A_TYPE_BUT_IS_BEING_USED_AS_A_VALUE_HERE_DO_YOU_NEED_TO_CHANGE_YO,
                    &[name],
                ),
            )
        } else if let Some(type_only_kind) = self.get_type_only_import_export_kind(idx) {
            match type_only_kind {
                TypeOnlyKind::Import => (
                    diagnostic_codes::CANNOT_BE_USED_AS_A_VALUE_BECAUSE_IT_WAS_IMPORTED_USING_IMPORT_TYPE,
                    format_message(
                        diagnostic_messages::CANNOT_BE_USED_AS_A_VALUE_BECAUSE_IT_WAS_IMPORTED_USING_IMPORT_TYPE,
                        &[name],
                    ),
                ),
                TypeOnlyKind::Export => (
                    diagnostic_codes::CANNOT_BE_USED_AS_A_VALUE_BECAUSE_IT_WAS_EXPORTED_USING_EXPORT_TYPE,
                    format_message(
                        diagnostic_messages::CANNOT_BE_USED_AS_A_VALUE_BECAUSE_IT_WAS_EXPORTED_USING_EXPORT_TYPE,
                        &[name],
                    ),
                ),
            }
        } else if self.is_computed_property_in_type_member(idx) {
            // TS2690: Type used as computed property key in type literal.
            // Suggest mapped type syntax: "Did you mean to use 'P in K'?"
            let suggested_var = Self::suggest_mapped_type_variable(name);
            (
                diagnostic_codes::ONLY_REFERS_TO_A_TYPE_BUT_IS_BEING_USED_AS_A_VALUE_HERE_DID_YOU_MEAN_TO_USE_IN,
                format_message(
                    diagnostic_messages::ONLY_REFERS_TO_A_TYPE_BUT_IS_BEING_USED_AS_A_VALUE_HERE_DID_YOU_MEAN_TO_USE_IN,
                    &[name, &suggested_var],
                ),
            )
        } else {
            // TS2693: Generic type-only error
            (
                diagnostic_codes::ONLY_REFERS_TO_A_TYPE_BUT_IS_BEING_USED_AS_A_VALUE_HERE,
                format_message(
                    diagnostic_messages::ONLY_REFERS_TO_A_TYPE_BUT_IS_BEING_USED_AS_A_VALUE_HERE,
                    &[name],
                ),
            )
        };

        self.error_at_node(idx, &message, code);
    }

    /// Check if the identifier at `idx` is used as a computed property name
    /// inside a type member (property signature, method signature, etc.) within
    /// a type literal or interface declaration.
    ///
    /// This detects patterns like `type T = { [K]: number }` where `K` is a type
    /// alias being used as a computed property key, which should get TS2690
    /// (with mapped type suggestion) instead of TS2693.
    fn is_computed_property_in_type_member(&self, idx: NodeIndex) -> bool {
        // Walk up: Identifier -> ComputedPropertyName -> TypeMember -> TypeLiteral/Interface
        let Some(ext) = self.ctx.arena.get_extended(idx) else {
            return false;
        };
        let Some(parent) = self.ctx.arena.get(ext.parent) else {
            return false;
        };
        if parent.kind != syntax_kind_ext::COMPUTED_PROPERTY_NAME {
            return false;
        }

        let Some(parent_ext) = self.ctx.arena.get_extended(ext.parent) else {
            return false;
        };
        let Some(grandparent) = self.ctx.arena.get(parent_ext.parent) else {
            return false;
        };

        // The computed property name must be inside a type member
        let is_type_member = matches!(
            grandparent.kind,
            syntax_kind_ext::PROPERTY_SIGNATURE
                | syntax_kind_ext::METHOD_SIGNATURE
                | syntax_kind_ext::INDEX_SIGNATURE
        );
        if !is_type_member {
            return false;
        }

        // The type member must be inside a type literal or interface
        let Some(grandparent_ext) = self.ctx.arena.get_extended(parent_ext.parent) else {
            return false;
        };
        let Some(great_grandparent) = self.ctx.arena.get(grandparent_ext.parent) else {
            return false;
        };
        if !matches!(
            great_grandparent.kind,
            syntax_kind_ext::TYPE_LITERAL | syntax_kind_ext::INTERFACE_DECLARATION
        ) {
            return false;
        }

        // tsc only suggests mapped type syntax when the computed property is the
        // sole member of the type literal. When there are multiple members,
        // it can't simply be converted to a mapped type, so TS2693 is emitted.
        use tsz_parser::parser::node::NodeAccess;
        let children = self.ctx.arena.get_children(grandparent_ext.parent);
        let member_count = children
            .iter()
            .filter(|&&child| {
                self.ctx.arena.get(child).is_some_and(|n| {
                    matches!(
                        n.kind,
                        syntax_kind_ext::PROPERTY_SIGNATURE
                            | syntax_kind_ext::METHOD_SIGNATURE
                            | syntax_kind_ext::INDEX_SIGNATURE
                            | syntax_kind_ext::CALL_SIGNATURE
                            | syntax_kind_ext::CONSTRUCT_SIGNATURE
                    )
                })
            })
            .count();
        member_count <= 1
    }

    /// Generate a suggested variable name for a mapped type suggestion.
    ///
    /// tsc uses the first character of the type name. If the type name is a
    /// single character (so the suggestion would equal the name itself),
    /// it falls back to `"P"`.
    fn suggest_mapped_type_variable(type_name: &str) -> String {
        let first_char = type_name.chars().next().unwrap_or('P');
        let suggested = first_char.to_string();
        if suggested == type_name {
            // Single-char type name - use a fallback to avoid suggesting `[K in K]`
            "P".to_string()
        } else {
            suggested
        }
    }

    /// Determine if the identifier at `idx` resolves to a symbol that was
    /// explicitly imported with `import type` or exported with `export type`.
    /// Returns `Some(TypeOnlyKind::Import)` for TS1361 or
    /// `Some(TypeOnlyKind::Export)` for TS1362 when applicable.
    fn get_type_only_import_export_kind(&self, idx: NodeIndex) -> Option<TypeOnlyKind> {
        use tsz_binder::symbol_flags;

        let sym_id = self.resolve_identifier_symbol(idx)?;
        let mut visited = Vec::new();
        let target = self.resolve_alias_symbol(sym_id, &mut visited);

        let lib_binders = self.get_lib_binders();

        // Walk the alias chain to find the first type-only import or export.
        for &alias_sym_id in &visited {
            let symbol = match self
                .ctx
                .binder
                .get_symbol_with_libs(alias_sym_id, &lib_binders)
            {
                Some(s) => s,
                None => continue,
            };

            // Only applies to alias symbols explicitly marked type-only
            // Check this FIRST — if the local symbol is marked type-only from
            // `import type`, that takes precedence over export-side type-only.
            if (symbol.flags & symbol_flags::ALIAS) != 0 && symbol.is_type_only {
                // Walk up from the symbol's declaration to determine if it came from
                // an import or export statement.
                for &decl in &symbol.declarations {
                    if decl.is_none() {
                        continue;
                    }
                    let mut current = decl;
                    let mut guard = 0;

                    // Get the arena for this declaration if it's from a different file
                    let arena = self
                        .ctx
                        .binder
                        .symbol_arenas
                        .get(&alias_sym_id)
                        .map(|arc| &**arc)
                        .unwrap_or(self.ctx.arena);

                    while guard < 16 {
                        guard += 1;
                        let Some(node) = arena.get(current) else {
                            break;
                        };
                        if node.kind == syntax_kind_ext::IMPORT_DECLARATION
                            || node.kind == syntax_kind_ext::IMPORT_EQUALS_DECLARATION
                        {
                            return Some(TypeOnlyKind::Import);
                        }
                        if node.kind == syntax_kind_ext::EXPORT_DECLARATION {
                            return Some(TypeOnlyKind::Export);
                        }
                        let Some(ext) = arena.get_extended(current) else {
                            break;
                        };
                        if ext.parent.is_none() {
                            break;
                        }
                        current = ext.parent;
                    }
                }
            }

            // If this alias resolves through an import chain that came from a type-only
            // re-export or import, classify accordingly.
            // Skip for namespace imports (import * as ns) and namespace re-exports
            // (export * as ns from). These create value bindings — the namespace
            // object itself is a value even if the module's exports are type-only.
            // Individual type-only members surface as TS2339 via property lookup.
            if let Some(module_specifier) = symbol.import_module.as_deref() {
                let is_namespace_binding =
                    symbol.import_name.is_none() || symbol.import_name.as_deref() == Some("*");
                let export_name = symbol
                    .import_name
                    .as_deref()
                    .unwrap_or(&symbol.escaped_name);
                if !is_namespace_binding
                    && self.is_export_type_only_across_binders(module_specifier, export_name)
                {
                    // Determine whether the type-only came from `import type` or `export type`
                    // in the target module. Resolve the export symbol and walk its declarations.
                    if let Some(kind) =
                        self.classify_cross_file_type_only_kind(module_specifier, export_name)
                    {
                        return Some(kind);
                    }
                    // Default to Export if we can't determine the kind
                    return Some(TypeOnlyKind::Export);
                }
            }
        }

        // If the target symbol itself is marked type-only (e.g. `export type { A }`),
        // it means it was exported as a type, so return Export.
        if let Some(target_id) = target
            && let Some(target_symbol) = self
                .ctx
                .binder
                .get_symbol_with_libs(target_id, &lib_binders)
            && target_symbol.is_type_only
        {
            return Some(TypeOnlyKind::Export);
        }

        // Check for type-only propagation through `export =` chains.
        // When a module does `import type * as ns from './a'; export = ns;`,
        // any import from that module should inherit the type-only status.
        // This handles: `import X from './b'`, `import X = require('./b')`,
        // and `import * as X from './b'` where b has `export = type_only_ns`.
        for &alias_sym_id in &visited {
            let symbol = match self
                .ctx
                .binder
                .get_symbol_with_libs(alias_sym_id, &lib_binders)
            {
                Some(s) => s,
                None => continue,
            };
            if let Some(module_specifier) = symbol.import_module.as_deref()
                && let Some(target_idx) = self.ctx.resolve_import_target(module_specifier)
                && let Some(target_binder) = self.ctx.get_binder_for_file(target_idx)
            {
                let target_arena = self.ctx.get_arena_for_file(target_idx as u32);
                if let Some(file_name) = target_arena.source_files.first().map(|f| &f.file_name)
                    && let Some(exports) = target_binder.module_exports.get(file_name)
                    && let Some(export_eq_sym) = exports.get("export=")
                {
                    // Check if the export= symbol itself is type-only
                    if let Some(eq_sym) = target_binder.get_symbol(export_eq_sym) {
                        if eq_sym.is_type_only {
                            return Some(TypeOnlyKind::Import);
                        }
                        // Also check if the export= symbol is an alias that
                        // resolves to a type-only import
                        if eq_sym.flags & symbol_flags::ALIAS != 0
                            && let Some(ref eq_import_module) = eq_sym.import_module
                        {
                            let eq_name = eq_sym
                                .import_name
                                .as_deref()
                                .unwrap_or(&eq_sym.escaped_name);
                            let is_ns = eq_sym.import_name.is_none()
                                || eq_sym.import_name.as_deref() == Some("*");
                            // For namespace imports, check the main binder's
                            // merged symbol for is_type_only
                            if is_ns {
                                if let Some(main_sym) = self
                                    .ctx
                                    .binder
                                    .get_symbol_with_libs(export_eq_sym, &lib_binders)
                                    && main_sym.is_type_only
                                {
                                    return Some(TypeOnlyKind::Import);
                                }
                            } else if self
                                .is_export_type_only_across_binders(eq_import_module, eq_name)
                            {
                                return Some(TypeOnlyKind::Import);
                            }
                        }
                    }
                }
            }
        }

        None
    }

    /// Determine whether a cross-file type-only export came from `import type`
    /// (TS1361) or `export type` (TS1362) by resolving the target module and
    /// walking the export symbol's declarations.
    fn classify_cross_file_type_only_kind(
        &self,
        module_specifier: &str,
        export_name: &str,
    ) -> Option<TypeOnlyKind> {
        let target_file_idx = self.ctx.resolve_import_target(module_specifier)?;
        let target_binder = self.ctx.get_binder_for_file(target_file_idx)?;
        let target_arena = self.ctx.get_arena_for_file(target_file_idx as u32);
        let target_file_name = target_arena.source_files.first()?.file_name.clone();

        let exports_table = target_binder.module_exports.get(&target_file_name)?;
        let sym_id = exports_table.get(export_name)?;
        let sym = self.ctx.binder.get_symbol(sym_id)?;

        if !sym.is_type_only {
            return None;
        }

        // Walk the symbol's declarations to find the import/export that made it type-only
        let decl_arena = self
            .ctx
            .binder
            .symbol_arenas
            .get(&sym_id)
            .map(|arc| &**arc)
            .unwrap_or(target_arena);

        for &decl in &sym.declarations {
            if decl.is_none() {
                continue;
            }
            let mut current = decl;
            let mut guard = 0;
            while guard < 16 {
                guard += 1;
                let Some(node) = decl_arena.get(current) else {
                    break;
                };
                if node.kind == syntax_kind_ext::IMPORT_DECLARATION {
                    return Some(TypeOnlyKind::Import);
                }
                if node.kind == syntax_kind_ext::EXPORT_DECLARATION {
                    return Some(TypeOnlyKind::Export);
                }
                let Some(ext) = decl_arena.get_extended(current) else {
                    break;
                };
                if ext.parent.is_none() {
                    break;
                }
                current = ext.parent;
            }
        }

        None
    }

    /// Parser-recovery exceptions for TS2693/TS2585.
    ///
    /// Some grammar-recovery scenarios continue checking and should still emit
    /// type/value mismatch diagnostics even with parse errors.
    fn has_type_only_value_in_parse_recovery_context(&self, name: &str, idx: NodeIndex) -> bool {
        // Recovery for async-generator computed members (`async * [yield] ...`) should
        // still report TS2693.
        if name == "yield" {
            let mut guard = 0;
            let mut current = Some(idx);
            let mut seen_computed_property_name = false;

            while let Some(current_idx) = current {
                if guard > 64 {
                    break;
                }
                guard += 1;

                let Some(ext) = self.ctx.arena.get_extended(current_idx) else {
                    break;
                };

                let Some(parent) = self.ctx.arena.get(ext.parent) else {
                    break;
                };

                if parent.kind == syntax_kind_ext::COMPUTED_PROPERTY_NAME {
                    seen_computed_property_name = true;
                } else if parent.kind == syntax_kind_ext::METHOD_DECLARATION
                    && seen_computed_property_name
                {
                    return true;
                }

                current = Some(ext.parent);
            }

            return false;
        }

        // Recovery for malformed type-literal indexers (`[number]: ...`) should still
        // report TS2693 even when unrelated parse errors exist in the file.
        let is_primitive_type_keyword = matches!(
            name,
            "number"
                | "string"
                | "boolean"
                | "symbol"
                | "void"
                | "undefined"
                | "null"
                | "any"
                | "unknown"
                | "never"
                | "object"
                | "bigint",
        );
        if !is_primitive_type_keyword {
            return false;
        }

        let mut guard = 0;
        let mut current = Some(idx);
        let mut seen_computed_property_name = false;

        while let Some(current_idx) = current {
            if guard > 64 {
                break;
            }
            guard += 1;

            let Some(ext) = self.ctx.arena.get_extended(current_idx) else {
                break;
            };

            let Some(parent) = self.ctx.arena.get(ext.parent) else {
                break;
            };

            if parent.kind == syntax_kind_ext::COMPUTED_PROPERTY_NAME {
                seen_computed_property_name = true;
            } else if seen_computed_property_name
                && (parent.kind == syntax_kind_ext::PROPERTY_SIGNATURE
                    || parent.kind == syntax_kind_ext::METHOD_DECLARATION)
            {
                return true;
            }

            current = Some(ext.parent);
        }

        false
    }

    /// Report TS2749: Symbol refers to a value, but is used as a type.
    pub fn error_value_only_type_at(&mut self, name: &str, idx: NodeIndex) {
        self.error_at_node_msg(
            idx,
            diagnostic_codes::REFERS_TO_A_VALUE_BUT_IS_BEING_USED_AS_A_TYPE_HERE_DID_YOU_MEAN_TYPEOF,
            &[name],
        );
    }

    /// Report TS2702: '{0}' only refers to a type, but is being used as a namespace here.
    pub fn error_type_used_as_namespace_at(&mut self, name: &str, idx: NodeIndex) {
        self.error_at_node_msg(
            idx,
            diagnostic_codes::ONLY_REFERS_TO_A_TYPE_BUT_IS_BEING_USED_AS_A_NAMESPACE_HERE,
            &[name],
        );
    }

    /// Report TS2709: Cannot use namespace '{0}' as a type.
    pub fn error_namespace_used_as_type_at(&mut self, name: &str, idx: NodeIndex) {
        self.error_at_node_msg(
            idx,
            diagnostic_codes::CANNOT_USE_NAMESPACE_AS_A_TYPE,
            &[name],
        );
    }

    /// Report TS2708: Cannot use namespace '{0}' as a value.
    pub fn error_namespace_used_as_value_at(&mut self, name: &str, idx: NodeIndex) {
        self.error_at_node_msg(
            idx,
            diagnostic_codes::CANNOT_USE_NAMESPACE_AS_A_VALUE,
            &[name],
        );
    }
}

#[cfg(test)]
mod tests {
    use crate::test_utils::check_source_diagnostics;

    #[test]
    fn emits_ts2690_for_computed_type_keyword_in_type_member() {
        let diagnostics = check_source_diagnostics(
            r#"
namespace m1 {
  export class C2 {
    public get p(arg) {
      return 0;
    }
  }

  export function f4(arg1: {
    [number]: C1;
  }) {}
}

class C1 {}
"#,
        );

        // TS2690 is emitted (with mapped type suggestion) instead of TS2693
        // when a type is used as a computed property key in a type member
        assert!(
            diagnostics.iter().any(|diag| diag.code == 2690),
            "Expected TS2690 for computed type keyword in type member, got: {diagnostics:?}",
        );
    }

    #[test]
    fn suppresses_ts2693_for_new_primitive_array_recovery() {
        let diagnostics = check_source_diagnostics(
            r#"
const x = new number[];
"#,
        );

        let ts2693_count = diagnostics.iter().filter(|diag| diag.code == 2693).count();
        assert_eq!(
            ts2693_count, 0,
            "Expected no TS2693 for `new number[]` parse recovery, got: {diagnostics:?}",
        );
    }

    #[test]
    fn emits_ts2702_for_empty_interface_used_as_namespace() {
        // Empty interface has no property "hello", so TS2702 should fire
        let diagnostics = check_source_diagnostics(
            r#"
interface OhNo {}
declare let y: OhNo.hello;
"#,
        );

        assert!(
            diagnostics.iter().any(|diag| diag.code == 2702),
            "Expected TS2702 for empty interface used as namespace, got: {diagnostics:?}",
        );
        assert!(
            !diagnostics.iter().any(|diag| diag.code == 2713),
            "Should NOT emit TS2713 when property doesn't exist, got: {diagnostics:?}",
        );
    }

    #[test]
    fn emits_ts2713_for_interface_property_as_type() {
        // Interface has property "bar", so TS2713 (with suggestion) should fire
        let diagnostics = check_source_diagnostics(
            r#"
interface Foo { bar: string; }
var x: Foo.bar = "";
"#,
        );

        assert!(
            diagnostics.iter().any(|diag| diag.code == 2713),
            "Expected TS2713 for interface property used as type, got: {diagnostics:?}",
        );
        assert!(
            !diagnostics.iter().any(|diag| diag.code == 2702),
            "Should NOT emit TS2702 when property exists, got: {diagnostics:?}",
        );
    }

    #[test]
    fn emits_ts2702_for_union_with_non_shared_property() {
        // Union where NOT all members have "bar" (Test5 pattern) → TS2702
        let diagnostics = check_source_diagnostics(
            r#"
type Foo = { bar: number } | { wat: string };
var x: Foo.bar = "";
"#,
        );

        assert!(
            diagnostics.iter().any(|diag| diag.code == 2702),
            "Expected TS2702 for union with non-shared property, got: {diagnostics:?}",
        );
    }

    #[test]
    fn emits_ts2713_for_union_with_shared_property() {
        // Union where ALL members have "bar" (Test4 pattern) → TS2713
        let diagnostics = check_source_diagnostics(
            r#"
type Foo = { bar: number } | { bar: string };
var x: Foo.bar = "";
"#,
        );

        assert!(
            diagnostics.iter().any(|diag| diag.code == 2713),
            "Expected TS2713 for union with shared property, got: {diagnostics:?}",
        );
    }

    #[test]
    fn emits_ts2713_for_type_alias_with_property() {
        // Type alias with property "bar" → TS2713
        let diagnostics = check_source_diagnostics(
            r#"
type Foo = { bar: string; };
var x: Foo.bar = "";
"#,
        );

        assert!(
            diagnostics.iter().any(|diag| diag.code == 2713),
            "Expected TS2713 for type alias property used as type, got: {diagnostics:?}",
        );
    }

    #[test]
    fn suppresses_ts1361_for_computed_property_in_interface() {
        // Type-only import used in interface computed property name should NOT
        // emit TS1361 — the expression is never evaluated at runtime.
        let diagnostics = check_source_diagnostics(
            r#"
import type { onInit } from './hooks';
interface Component {
  [onInit]?(): void;
}
"#,
        );

        let ts1361_count = diagnostics.iter().filter(|d| d.code == 1361).count();
        assert_eq!(
            ts1361_count, 0,
            "Should not emit TS1361 for computed property in interface, got: {diagnostics:?}",
        );
    }

    #[test]
    fn suppresses_ts1361_for_computed_property_in_type_literal() {
        let diagnostics = check_source_diagnostics(
            r#"
import type { key } from './keys';
type T = { [key]: any; };
"#,
        );

        let ts1361_count = diagnostics.iter().filter(|d| d.code == 1361).count();
        assert_eq!(
            ts1361_count, 0,
            "Should not emit TS1361 for computed property in type literal, got: {diagnostics:?}",
        );
    }

    #[test]
    fn alias_merges_with_local_value_suppresses_ts1361() {
        // When import type is followed by a local const with the same name,
        // the const should shadow the import type in value position.
        let diagnostics = check_source_diagnostics(
            r#"
import type { A } from './a';
const A: A = "a";
A.toUpperCase();
"#,
        );

        let ts1361_count = diagnostics.iter().filter(|d| d.code == 1361).count();
        assert_eq!(
            ts1361_count, 0,
            "Should not emit TS1361 when local value shadows type-only import, got: {diagnostics:?}",
        );
    }
}
