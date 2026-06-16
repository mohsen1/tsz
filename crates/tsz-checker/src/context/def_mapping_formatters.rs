//! `TypeFormatter` construction factories for `CheckerContext`.
//!
//! Split out of `def_mapping.rs` to keep that file under the 2000-line cap.
//! These are the single construction sites for the display, diagnostic, and
//! assignability type formatters.

use crate::context::CheckerContext;

impl<'a> CheckerContext<'a> {
    /// Create a `TypeFormatter` with full context for displaying types (Phase 4.2.1).
    ///
    /// This includes symbol arena and definition store, which allows the formatter
    /// to display type names for Lazy(DefId) types instead of the internal "`Lazy(def_id)`"
    /// representation.
    ///
    /// # Example
    /// ```text
    /// let formatter = self.create_type_formatter();
    /// let type_str = formatter.format(type_id);  // Shows "List<number>" not "Lazy(1)<number>"
    /// ```
    pub fn create_type_formatter(&self) -> crate::query_boundaries::common::TypeFormatter<'_> {
        use crate::query_boundaries::common::TypeFormatter;

        TypeFormatter::with_symbols(self.types, &self.binder.symbols)
            .with_def_store(&self.definition_store)
            .with_namespace_module_names(&self.namespace_module_names)
            .with_module_specifiers(&self.module_specifiers)
            .with_module_path_specifiers(&self.module_path_specifiers)
            .with_current_file_id(self.current_file_idx as u32)
    }

    /// Create a type formatter configured for diagnostic error messages.
    /// Skips union optionalization (synthetic `?: undefined` members) that
    /// tsc only uses in hover/quickinfo, not in error messages.
    pub fn create_diagnostic_type_formatter(
        &self,
    ) -> crate::query_boundaries::common::TypeFormatter<'_> {
        self.create_type_formatter()
            .with_diagnostic_mode()
            .with_strict_null_checks(self.compiler_options.strict_null_checks)
            .with_builtin_iterator_return_type(
                if self.compiler_options.strict_builtin_iterator_return {
                    tsz_solver::TypeId::UNDEFINED
                } else {
                    tsz_solver::TypeId::ANY
                },
            )
            .with_exact_optional_property_types(self.compiler_options.exact_optional_property_types)
    }

    /// Create a `TypeFormatter` configured for assignability (`TS2322`/`TS2345`)
    /// diagnostic surfaces.
    ///
    /// Unlike [`Self::create_diagnostic_type_formatter`], this factory deliberately
    /// omits the module-name / current-file qualification context: assignability
    /// messages render type names structurally and must not pick up
    /// `import("<specifier>")` or namespace prefixes that the hover-style factory
    /// adds. It does enable diagnostic mode, optional-parameter surface syntax
    /// (`(a?: T)`), the `BuiltinIteratorReturn` substitution, and the strict-null /
    /// exact-optional flags so optional members render exactly as tsc prints them.
    ///
    /// This is the single construction site for the assignability formatter base;
    /// callers that need an extra policy flag (e.g.
    /// `with_skip_application_display_alias_chase`) chain it onto the returned
    /// builder.
    pub fn create_assignability_type_formatter(
        &self,
    ) -> crate::query_boundaries::common::TypeFormatter<'_> {
        use crate::query_boundaries::common::TypeFormatter;

        TypeFormatter::with_symbols(self.types, &self.binder.symbols)
            .with_def_store(&self.definition_store)
            .with_diagnostic_mode()
            // Match tsc: optional parameters display as `(a?: T)`.
            .with_preserve_optional_parameter_surface_syntax(true)
            .with_strict_null_checks(self.compiler_options.strict_null_checks)
            .with_builtin_iterator_return_type(
                if self.compiler_options.strict_builtin_iterator_return {
                    tsz_solver::TypeId::UNDEFINED
                } else {
                    tsz_solver::TypeId::ANY
                },
            )
            .with_exact_optional_property_types(self.compiler_options.exact_optional_property_types)
    }

    /// Create a `TypeFormatter` for the failed-instantiation (`TS2635`) display
    /// surface (`format_type_diagnostic_for_instantiation_expression`).
    ///
    /// Structural base shared by every instantiation-display formatter: diagnostic
    /// mode, the strict-null / exact-optional flags, and `with_display_properties`
    /// so object/callable shapes render members inline. Unlike
    /// [`Self::create_diagnostic_type_formatter`] it omits the
    /// `BuiltinIteratorReturn` substitution so the raw instantiated shape renders.
    /// Structural call sites use the builder as-is; the named-callable entry point
    /// chains the module-name / current-file qualification context onto it.
    pub fn create_instantiation_display_formatter(
        &self,
    ) -> crate::query_boundaries::common::TypeFormatter<'_> {
        use crate::query_boundaries::common::TypeFormatter;

        TypeFormatter::with_symbols(self.types, &self.binder.symbols)
            .with_diagnostic_mode()
            .with_strict_null_checks(self.compiler_options.strict_null_checks)
            .with_exact_optional_property_types(self.compiler_options.exact_optional_property_types)
            .with_display_properties()
    }
}
