//! Type/value mismatch and declaration error reporting (TS2693, TS2749, TS2708, TS2709).

use super::TypeOnlyKind;
use crate::diagnostics::{
    Diagnostic, DiagnosticCategory, diagnostic_codes, diagnostic_messages, format_message,
};
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
        // Suppress for ERROR and UNKNOWN types.
        // ERROR: avoids cascading diagnostics from unresolved types.
        // UNKNOWN: when a lib global (console, Math, etc.) can't be properly
        // typed, it resolves to `unknown`. Comparing against `unknown` always
        // fails and produces false positives. TSC's libs properly type all
        // globals, so this situation only arises from incomplete lib coverage.
        if prev_type == TypeId::ERROR
            || current_type == TypeId::ERROR
            || prev_type == TypeId::UNKNOWN
            || current_type == TypeId::UNKNOWN
        {
            return;
        }
        if let Some(loc) = self.get_source_location(idx) {
            let prev_type_str = self.format_type(prev_type);
            let current_type_str = self.format_type(current_type);
            let message = format!(
                "Subsequent variable declarations must have the same type. Variable '{name}' must be of type '{prev_type_str}', but here has type '{current_type_str}'."
            );
            self.ctx.diagnostics.push(Diagnostic::error(self.ctx.file_name.clone(), loc.start, loc.length(), message, diagnostic_codes::SUBSEQUENT_VARIABLE_DECLARATIONS_MUST_HAVE_THE_SAME_TYPE_VARIABLE_MUST_BE_OF_TYP));
        }
    }

    /// Report TS2454: Variable is used before being assigned.
    pub fn error_variable_used_before_assigned_at(&mut self, name: &str, idx: NodeIndex) {
        if let Some(loc) = self.get_source_location(idx) {
            let message = format_message(
                diagnostic_messages::VARIABLE_IS_USED_BEFORE_BEING_ASSIGNED,
                &[name],
            );
            self.ctx.diagnostics.push(Diagnostic::error(
                self.ctx.file_name.clone(),
                loc.start,
                loc.length(),
                message,
                diagnostic_codes::VARIABLE_IS_USED_BEFORE_BEING_ASSIGNED,
            ));
        }
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
        if let Some(loc) = self.get_source_location(idx) {
            let message = format!(
                "Abstract property '{prop_name}' in class '{class_name}' cannot be accessed in the constructor."
            );
            self.ctx.diagnostics.push(Diagnostic::error(
                self.ctx.file_name.clone(),
                loc.start,
                loc.length(),
                message,
                diagnostic_codes::ABSTRACT_PROPERTY_IN_CLASS_CANNOT_BE_ACCESSED_IN_THE_CONSTRUCTOR,
            ));
        }
    }

    // =========================================================================
    // Module/Namespace Errors
    // =========================================================================

    /// Report TS2694: Namespace has no exported member.
    pub fn error_namespace_no_export(
        &mut self,
        namespace_name: &str,
        member_name: &str,
        idx: NodeIndex,
    ) {
        if let Some(loc) = self.get_source_location(idx) {
            let message =
                format!("Namespace '{namespace_name}' has no exported member '{member_name}'.");
            self.ctx.diagnostics.push(Diagnostic::error(
                self.ctx.file_name.clone(),
                loc.start,
                loc.length(),
                message,
                2694,
            ));
        }
    }

    // =========================================================================
    // Type/Value Mismatch Errors
    // =========================================================================

    /// Report TS2698: Spread types may only be created from object types.
    pub fn report_spread_not_object_type(&mut self, idx: NodeIndex) {
        if let Some(loc) = self.get_source_location(idx) {
            self.ctx.diagnostics.push(Diagnostic {
                code: diagnostic_codes::SPREAD_TYPES_MAY_ONLY_BE_CREATED_FROM_OBJECT_TYPES,
                category: DiagnosticCategory::Error,
                message_text:
                    diagnostic_messages::SPREAD_TYPES_MAY_ONLY_BE_CREATED_FROM_OBJECT_TYPES
                        .to_string(),
                start: loc.start,
                length: loc.length(),
                file: self.ctx.file_name.clone(),
                related_information: Vec::new(),
            });
        }
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

        if self.is_direct_heritage_type_reference(idx) {
            return;
        }

        if let Some(loc) = self.get_source_location(idx) {
            // Check if this is an ES2015+ type that requires specific lib support
            let is_es2015_type = lib_loader::is_es2015_plus_type(name);
            let allow_in_parse_recovery =
                self.has_type_only_value_in_parse_recovery_context(name, idx);

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

            self.ctx.diagnostics.push(Diagnostic::error(
                self.ctx.file_name.clone(),
                loc.start,
                loc.length(),
                message,
                code,
            ));
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

            // If this alias resolves through an import chain that came from a type-only
            // re-export (`export type * from ...` or `export { ... } from ...` with type-only),
            // classify it as a type-only export usage (TS1362).
            if let Some(module_specifier) = symbol.import_module.as_deref() {
                let export_name = symbol
                    .import_name
                    .as_deref()
                    .unwrap_or(&symbol.escaped_name);
                if self
                    .ctx
                    .binder
                    .resolve_import_with_reexports_type_only(module_specifier, export_name)
                    .is_some_and(|(_, is_type_only)| is_type_only)
                {
                    return Some(TypeOnlyKind::Export);
                }
            }

            // Only applies to alias symbols explicitly marked type-only
            if (symbol.flags & symbol_flags::ALIAS) == 0 || !symbol.is_type_only {
                continue;
            }

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
                    if node.kind == syntax_kind_ext::IMPORT_DECLARATION {
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
        if let Some(loc) = self.get_source_location(idx) {
            let message = format_message(
                diagnostic_messages::REFERS_TO_A_VALUE_BUT_IS_BEING_USED_AS_A_TYPE_HERE_DID_YOU_MEAN_TYPEOF,
                &[name],
            );
            self.ctx.diagnostics.push(Diagnostic::error(self.ctx.file_name.clone(), loc.start, loc.length(), message, diagnostic_codes::REFERS_TO_A_VALUE_BUT_IS_BEING_USED_AS_A_TYPE_HERE_DID_YOU_MEAN_TYPEOF));
        }
    }

    /// Report TS2702: '{0}' only refers to a type, but is being used as a namespace here.
    pub fn error_type_used_as_namespace_at(&mut self, name: &str, idx: NodeIndex) {
        if let Some(loc) = self.get_source_location(idx) {
            let message = format_message(
                diagnostic_messages::ONLY_REFERS_TO_A_TYPE_BUT_IS_BEING_USED_AS_A_NAMESPACE_HERE,
                &[name],
            );
            self.ctx.diagnostics.push(Diagnostic::error(
                self.ctx.file_name.clone(),
                loc.start,
                loc.length(),
                message,
                diagnostic_codes::ONLY_REFERS_TO_A_TYPE_BUT_IS_BEING_USED_AS_A_NAMESPACE_HERE,
            ));
        }
    }

    /// Report TS2709: Cannot use namespace '{0}' as a type.
    pub fn error_namespace_used_as_type_at(&mut self, name: &str, idx: NodeIndex) {
        if let Some(loc) = self.get_source_location(idx) {
            let message =
                format_message(diagnostic_messages::CANNOT_USE_NAMESPACE_AS_A_TYPE, &[name]);
            self.ctx.diagnostics.push(Diagnostic::error(
                self.ctx.file_name.clone(),
                loc.start,
                loc.length(),
                message,
                diagnostic_codes::CANNOT_USE_NAMESPACE_AS_A_TYPE,
            ));
        }
    }

    /// Report TS2708: Cannot use namespace '{0}' as a value.
    pub fn error_namespace_used_as_value_at(&mut self, name: &str, idx: NodeIndex) {
        tracing::debug!("error_namespace_used_as_value_at: {name}");

        if let Some(loc) = self.get_source_location(idx) {
            let message = format_message(
                diagnostic_messages::CANNOT_USE_NAMESPACE_AS_A_VALUE,
                &[name],
            );
            self.ctx.diagnostics.push(Diagnostic::error(
                self.ctx.file_name.clone(),
                loc.start,
                loc.length(),
                message,
                diagnostic_codes::CANNOT_USE_NAMESPACE_AS_A_VALUE,
            ));
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::test_utils::check_source_diagnostics;

    #[test]
    fn emits_ts2693_for_recovered_computed_type_keyword() {
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

        assert!(
            diagnostics.iter().any(|diag| diag.code == 2693),
            "Expected TS2693 for recovered computed type keyword, got: {diagnostics:?}",
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
}
