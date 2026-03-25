//! Dynamic import validation: specifier type, options type, attributes, module resolution.

use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::NodeAccess;

impl<'a> CheckerState<'a> {
    // =========================================================================
    // Dynamic Import Validation
    // =========================================================================

    /// TS7036: Check that the dynamic import specifier is assignable to `string`.
    ///
    /// tsc requires that `import(expr)` specifiers have type `string`.
    /// If the specifier type is not assignable to `string`, emit TS7036.
    /// String literals trivially satisfy this; the check matters for
    /// variable/expression specifiers whose type may be `boolean`, `number`,
    /// `string | undefined` (under strictNullChecks), arrays, functions, etc.
    pub(crate) fn check_dynamic_import_specifier_type(
        &mut self,
        call: &tsz_parser::parser::node::CallExprData,
    ) {
        use crate::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};
        use tsz_solver::TypeId;

        let args = match call.arguments.as_ref() {
            Some(a) => a.nodes.as_slice(),
            None => &[],
        };

        if args.is_empty() {
            return;
        }

        let arg_idx = args[0];
        let arg_type = self.get_type_of_node(arg_idx);

        // String and any/error types pass trivially
        if arg_type == TypeId::STRING
            || arg_type == TypeId::ANY
            || arg_type == TypeId::ERROR
            || arg_type == TypeId::NEVER
        {
            return;
        }

        // Check if the specifier type is assignable to `string`
        if !self.is_assignable_to(arg_type, TypeId::STRING) {
            let type_str = self.format_type(arg_type);
            let message = format_message(
                diagnostic_messages::DYNAMIC_IMPORTS_SPECIFIER_MUST_BE_OF_TYPE_STRING_BUT_HERE_HAS_TYPE,
                &[&type_str],
            );
            if let Some(arg_node) = self.ctx.arena.get(arg_idx) {
                let start = arg_node.pos;
                let length = arg_node.end.saturating_sub(arg_node.pos);
                self.error_at_position(
                    start,
                    length,
                    &message,
                    diagnostic_codes::DYNAMIC_IMPORTS_SPECIFIER_MUST_BE_OF_TYPE_STRING_BUT_HERE_HAS_TYPE,
                );
            }
        }
    }

    /// TS2322: Check that dynamic import options are assignable to `ImportCallOptions`.
    ///
    /// For `import(specifier, options)`, validates that the second argument (options)
    /// is assignable to the global `ImportCallOptions` interface. This catches cases like:
    /// ```ts
    /// declare global { interface ImportAttributes { type: "json" } }
    /// import("./a", { with: { type: "not-json" } }); // TS2322
    /// ```
    ///
    /// Builds the options type manually from the AST using string literal types
    /// (not widened) to match TSC's behavior. This avoids dependence on contextual
    /// typing which may not narrow deeply nested object literals.
    pub(crate) fn check_dynamic_import_options_type(
        &mut self,
        call: &tsz_parser::parser::node::CallExprData,
    ) {
        use tsz_parser::parser::syntax_kind_ext;
        use tsz_solver::TypeId;

        let args = match call.arguments.as_ref() {
            Some(a) => a.nodes.as_slice(),
            None => &[],
        };

        // Only check if there's a second argument (the options object)
        if args.len() < 2 {
            return;
        }

        let options_idx = args[1];

        // TS2880: Check for deprecated `assert` keyword in options object
        self.check_import_options_deprecated_assert(options_idx);

        // Resolve ImportAttributes (augmented version including user's `declare global`).
        let Some(import_attributes_type) = self.resolve_lib_type_by_name("ImportAttributes") else {
            return;
        };
        // Build target type manually: { with?: ImportAttributes; assert?: ImportAssertions; }
        // We can't use resolve_lib_type_by_name("ImportCallOptions") because its `with`
        // property references the base ImportAttributes (without user augmentations).
        let with_atom = self.ctx.types.intern_string("with");
        let assert_atom = self.ctx.types.intern_string("assert");
        let import_call_options_type = self.ctx.types.factory().object(vec![
            tsz_solver::PropertyInfo::opt(with_atom, import_attributes_type),
            tsz_solver::PropertyInfo::opt(assert_atom, import_attributes_type),
        ]);

        // Build the options type manually from the AST with string literal types.
        let Some(options_node) = self.ctx.arena.get(options_idx) else {
            return;
        };

        if options_node.kind != syntax_kind_ext::OBJECT_LITERAL_EXPRESSION {
            let options_type = self.get_type_of_node(options_idx);
            if options_type == TypeId::ANY
                || options_type == TypeId::ERROR
                || options_type == TypeId::NEVER
            {
                return;
            }
            // ImportCallOptions is a weak type (all optional properties).
            // When the source is a primitive/literal, emit TS2559 directly with
            // the correct type names matching tsc's format.
            if tsz_solver::is_primitive_type(self.ctx.types, options_type) {
                use crate::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};
                // Use the literal text from the AST for string/numeric literals;
                // for other primitives, fall back to the type formatter.
                let source_str = self
                    .ctx
                    .arena
                    .get(options_idx)
                    .and_then(|n| self.ctx.arena.get_literal(n))
                    .map(|lit| format!("\"{}\"", lit.text))
                    .unwrap_or_else(|| self.format_type(options_type));
                let message = format_message(
                    diagnostic_messages::TYPE_HAS_NO_PROPERTIES_IN_COMMON_WITH_TYPE,
                    &[&source_str, "ImportCallOptions"],
                );
                self.error_at_node(
                    options_idx,
                    &message,
                    diagnostic_codes::TYPE_HAS_NO_PROPERTIES_IN_COMMON_WITH_TYPE,
                );
                return;
            }
            self.check_assignable_or_report_at_exact_anchor(
                options_type,
                import_call_options_type,
                options_idx,
                options_idx,
            );
            return;
        }

        // Build options object type from AST with literal types for nested attributes
        let options_type = self.build_import_options_type(options_idx);

        if options_type == TypeId::ANY
            || options_type == TypeId::ERROR
            || options_type == TypeId::NEVER
        {
            return;
        }

        // Check assignability — emit TS2322/TS2559 if not assignable
        self.check_assignable_or_report_at_exact_anchor(
            options_type,
            import_call_options_type,
            options_idx,
            options_idx,
        );
    }

    /// Build an object type from a dynamic import options literal, using string literal
    /// types for nested `with`/`assert` attribute values.
    fn build_import_options_type(&mut self, obj_idx: NodeIndex) -> tsz_solver::TypeId {
        use tsz_parser::parser::syntax_kind_ext;

        let children = self.ctx.arena.get_children(obj_idx);
        let mut properties = Vec::new();

        for child_idx in children {
            let Some(child_node) = self.ctx.arena.get(child_idx) else {
                continue;
            };

            if child_node.kind == syntax_kind_ext::PROPERTY_ASSIGNMENT {
                let Some(prop) = self.ctx.arena.get_property_assignment(child_node) else {
                    continue;
                };
                let Some(name) = self.get_property_name(prop.name) else {
                    continue;
                };

                // For `with` and `assert` properties, build nested type from attributes
                let value_type = if (name == "with" || name == "assert")
                    && let Some(val_node) = self.ctx.arena.get(prop.initializer)
                    && val_node.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
                {
                    self.build_literal_object_type(prop.initializer)
                } else {
                    self.get_type_of_node(prop.initializer)
                };

                let name_atom = self.ctx.types.intern_string(&name);
                properties.push(tsz_solver::PropertyInfo::new(name_atom, value_type));
            }
        }

        if properties.is_empty() {
            return self.get_type_of_node(obj_idx);
        }

        self.ctx.types.factory().object(properties)
    }

    /// Build an object type from an object literal using string literal types for
    /// all string literal property values (not widened).
    fn build_literal_object_type(&mut self, obj_idx: NodeIndex) -> tsz_solver::TypeId {
        use tsz_parser::parser::syntax_kind_ext;

        let children = self.ctx.arena.get_children(obj_idx);
        let mut properties = Vec::new();

        for child_idx in children {
            let Some(child_node) = self.ctx.arena.get(child_idx) else {
                continue;
            };

            if child_node.kind == syntax_kind_ext::PROPERTY_ASSIGNMENT {
                let Some(prop) = self.ctx.arena.get_property_assignment(child_node) else {
                    continue;
                };
                let Some(name) = self.get_property_name(prop.name) else {
                    continue;
                };

                // Use string literal type for string literal values
                let value_type = if let Some(val_node) = self.ctx.arena.get(prop.initializer)
                    && let Some(lit) = self.ctx.arena.get_literal(val_node)
                {
                    self.ctx.types.factory().literal_string(&lit.text)
                } else {
                    self.get_type_of_node(prop.initializer)
                };

                let name_atom = self.ctx.types.intern_string(&name);
                properties.push(tsz_solver::PropertyInfo::new(name_atom, value_type));
            }
        }

        if properties.is_empty() {
            return self.get_type_of_node(obj_idx);
        }

        self.ctx.types.factory().object(properties)
    }

    /// TS2880: Check for deprecated `assert` property in an import options object literal.
    ///
    /// When `import()` or `import(...)` type expressions use `{ assert: { ... } }` instead
    /// of `{ with: { ... } }`, emit TS2880 at each `assert` property name position.
    /// This applies to both dynamic import calls and import type expressions.
    pub(crate) fn check_import_options_deprecated_assert(&mut self, options_idx: NodeIndex) {
        use tsz_parser::parser::syntax_kind_ext;

        // Only emit if deprecation is not suppressed
        if self
            .ctx
            .capabilities
            .check_import_assert_deprecated()
            .is_none()
        {
            return;
        }

        let Some(options_node) = self.ctx.arena.get(options_idx) else {
            return;
        };

        if options_node.kind != syntax_kind_ext::OBJECT_LITERAL_EXPRESSION {
            return;
        }

        let children = self.ctx.arena.get_children(options_idx);
        for child_idx in children {
            let Some(child_node) = self.ctx.arena.get(child_idx) else {
                continue;
            };
            if child_node.kind != syntax_kind_ext::PROPERTY_ASSIGNMENT {
                continue;
            }
            let Some(prop) = self.ctx.arena.get_property_assignment(child_node) else {
                continue;
            };
            let Some(name) = self.get_property_name(prop.name) else {
                continue;
            };
            if name == "assert" {
                use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
                // Error spans the `assert` property name
                let Some(name_node) = self.ctx.arena.get(prop.name) else {
                    continue;
                };
                self.error_at_position(
                    name_node.pos,
                    name_node.end.saturating_sub(name_node.pos),
                    diagnostic_messages::IMPORT_ASSERTIONS_HAVE_BEEN_REPLACED_BY_IMPORT_ATTRIBUTES_USE_WITH_INSTEAD_OF_AS,
                    diagnostic_codes::IMPORT_ASSERTIONS_HAVE_BEEN_REPLACED_BY_IMPORT_ATTRIBUTES_USE_WITH_INSTEAD_OF_AS,
                );
            }
        }
    }

    /// Check dynamic import module specifier for unresolved modules.
    ///
    /// Validates that the module specifier in a dynamic `import()` call
    /// can be resolved. Emits TS2307 if the module cannot be found.
    ///
    /// ## Parameters:
    /// - `call`: The call expression node for the `import()` call
    ///
    /// ## Validation:
    /// - Only checks string literal specifiers (dynamic specifiers cannot be statically checked)
    /// - Checks if module exists in `resolved_modules`, `module_exports`, `shorthand_ambient_modules`, or `declared_modules`
    /// - Emits TS2307 for unresolved module specifiers
    /// - Validates `CommonJS` vs ESM import compatibility
    pub(crate) fn check_dynamic_import_module_specifier(
        &mut self,
        call: &tsz_parser::parser::node::CallExprData,
    ) {
        if !self.ctx.report_unresolved_imports {
            return;
        }

        // Get the first argument (module specifier)
        let args = match call.arguments.as_ref() {
            Some(a) => a.nodes.as_slice(),
            None => &[],
        };

        if args.is_empty() {
            return; // No argument - will be caught by argument count check
        }

        let arg_idx = args[0];
        let Some(arg_node) = self.ctx.arena.get(arg_idx) else {
            return;
        };

        // Only check string literal module specifiers
        // Dynamic specifiers (variables, template literals) cannot be statically checked
        let Some(literal) = self.ctx.arena.get_literal(arg_node) else {
            return;
        };

        let module_name = &literal.text;

        // TS2846: Check for .d.ts/.d.mts/.d.cts extensions in dynamic imports.
        // Dynamic import() calls are always value-level, so a .d.ts import
        // should always trigger TS2846 (unlike static `import type` which is OK).
        let dts_ext = if module_name.ends_with(".d.ts") {
            Some((".d.ts", ".ts", ".js"))
        } else if module_name.ends_with(".d.mts") {
            Some((".d.mts", ".mts", ".mjs"))
        } else if module_name.ends_with(".d.cts") {
            Some((".d.cts", ".cts", ".cjs"))
        } else {
            None
        };
        if let Some((dts_suffix, ts_ext, js_ext)) = dts_ext {
            use crate::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};
            let base = module_name.trim_end_matches(dts_suffix);
            let suggested = if self.ctx.compiler_options.allow_importing_ts_extensions {
                format!("{base}{ts_ext}")
            } else {
                use tsz_common::common::ModuleKind;
                match self.ctx.compiler_options.module {
                    ModuleKind::CommonJS
                    | ModuleKind::AMD
                    | ModuleKind::UMD
                    | ModuleKind::System
                    | ModuleKind::None => base.to_string(),
                    _ => format!("{base}{js_ext}"),
                }
            };
            let message = format_message(
                diagnostic_messages::A_DECLARATION_FILE_CANNOT_BE_IMPORTED_WITHOUT_IMPORT_TYPE_DID_YOU_MEAN_TO_IMPORT,
                &[&suggested],
            );
            let arg_start = arg_node.pos;
            let arg_length = arg_node.end.saturating_sub(arg_node.pos);
            self.error_at_position(
                arg_start,
                arg_length,
                &message,
                diagnostic_codes::A_DECLARATION_FILE_CANNOT_BE_IMPORTED_WITHOUT_IMPORT_TYPE_DID_YOU_MEAN_TO_IMPORT,
            );
        }

        // TS5097: Check for .ts/.tsx/.mts/.cts extensions in dynamic imports
        if !self.ctx.compiler_options.allow_importing_ts_extensions
            && !self.ctx.compiler_options.rewrite_relative_import_extensions
            && let Some(ext) = super::import::declaration::ts_extension_suffix(module_name)
        {
            use crate::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};
            let message = format_message(
                    diagnostic_messages::AN_IMPORT_PATH_CAN_ONLY_END_WITH_A_EXTENSION_WHEN_ALLOWIMPORTINGTSEXTENSIONS_IS,
                    &[ext],
                );
            let arg_start = arg_node.pos;
            let arg_length = arg_node.end.saturating_sub(arg_node.pos);
            self.error_at_position(
                    arg_start,
                    arg_length,
                    &message,
                    diagnostic_codes::AN_IMPORT_PATH_CAN_ONLY_END_WITH_A_EXTENSION_WHEN_ALLOWIMPORTINGTSEXTENSIONS_IS,
                );
        }

        // Check if the module was resolved by the CLI driver (multi-file mode)
        if let Some(ref resolved) = self.ctx.resolved_modules
            && resolved.contains(module_name)
        {
            return; // Module exists
        }

        // Check if the module exists in the module_exports map (cross-file module resolution)
        if self.ctx.binder.module_exports.contains_key(module_name) {
            return; // Module exists
        }

        // Check if this is a shorthand ambient module (declare module "foo")
        if self
            .ctx
            .binder
            .shorthand_ambient_modules
            .contains(module_name)
        {
            return; // Ambient module exists
        }

        // Check declared modules (regular ambient modules with body)
        if self.ctx.binder.declared_modules.contains(module_name) {
            return; // Declared module exists
        }

        if self.ctx.resolve_import_target(module_name).is_some() {
            return; // Module exists via driver/module resolution candidate matching
        }

        // Check for specific resolution error from driver (TS2834, TS2835, TS2792, etc.)
        let module_key = module_name.to_string();
        if let Some(error) = self.ctx.get_resolution_error(module_name) {
            // Extract error values before mutable borrow
            let error_code = error.code;
            let error_message = error.message.clone();
            if !self.ctx.modules_with_ts2307_emitted.contains(&module_key) {
                self.ctx.modules_with_ts2307_emitted.insert(module_key);
                self.error_at_node(arg_idx, &error_message, error_code);
            }
            return;
        }

        // Fallback: Module not found - emit TS2307 or TS2792 (Classic resolution)
        // Check if we've already emitted for this module (prevents duplicate emissions)
        let module_key = module_name.to_string();
        if !self.ctx.modules_with_ts2307_emitted.contains(&module_key) {
            self.ctx.modules_with_ts2307_emitted.insert(module_key);
            let (message, code) = self.module_not_found_diagnostic(module_name);
            self.error_at_node(arg_idx, &message, code);
        }
    }
}
