impl<'a> DeclarationEmitter<'a> {
    pub(crate) fn emit_non_portable_import_type_text_diagnostics(
        &mut self,
        printed_type_text: &str,
        decl_name: &str,
        file: &str,
        pos: u32,
        length: u32,
    ) -> bool {
        if let Some((module_specifier, _)) = self.parse_import_type_text(printed_type_text)
            && !module_specifier.starts_with('.')
            && !module_specifier.starts_with('/')
            && let Some(binder) = self.binder
            && self
                .matching_module_export_paths(binder, file, &module_specifier)
                .into_iter()
                .any(|module_path| !module_path.contains("node_modules"))
        {
            return false;
        }

        let Some(sym_id) = self.find_symbol_for_import_type_text(printed_type_text) else {
            if let Some((from_path, type_name)) =
                self.private_import_type_package_root_reference(printed_type_text)
            {
                self.emit_non_portable_named_reference_diagnostic(
                    decl_name, file, pos, length, &from_path, &type_name,
                );
                return true;
            }
            return false;
        };
        if let Some(binder) = self.binder
            && let Some(source_path) = self.get_symbol_source_path(sym_id, binder)
            && self.source_path_is_root_file(&source_path)
        {
            return false;
        }
        let mut references = self.collect_non_portable_references_in_symbol_declaration(sym_id);
        if references.is_empty()
            && let Some(binder) = self.binder
            && let Some(symbol) = binder.symbols.get(sym_id)
            && let Some(source_path) = self.get_symbol_source_path(sym_id, binder)
            && source_path.contains("node_modules")
            && self
                .package_root_export_reference_path(
                    sym_id,
                    symbol.escaped_name.as_str(),
                    binder,
                    file,
                )
                .is_none()
            // Skip if the source file is a direct-dependency entry point
            // (accessible via bare specifier). Root export resolution only
            // accepts same-file entries when the package root is reachable
            // from the current file, so we check explicit subpaths here.
            && self
                .package_specifier_for_node_modules_path(file, &source_path)
                .is_none_or(|spec| spec.contains('/'))
        {
            references.push((
                self.strip_ts_extensions(&self.calculate_relative_path(file, &source_path)),
                symbol.escaped_name.clone(),
            ));
        }
        if self.import_type_uses_private_package_subpath(printed_type_text)
            && let Some(parsed_reference) = self.parse_import_type_text(printed_type_text)
            && !references.contains(&parsed_reference)
        {
            references.insert(0, parsed_reference);
        }
        if let Some(root_reference) =
            self.private_import_type_package_root_reference(printed_type_text)
            && !references.contains(&root_reference)
        {
            references.push(root_reference);
        }
        if references.is_empty() {
            return false;
        }
        for (from_path, type_name) in references {
            self.emit_non_portable_named_reference_diagnostic(
                decl_name, file, pos, length, &from_path, &type_name,
            );
        }
        true
    }

    pub(in crate::declaration_emitter) fn emit_non_portable_function_return_diagnostics(
        &mut self,
        printed_type_text: &str,
        func_body: NodeIndex,
        func_name: NodeIndex,
    ) -> bool {
        let Some(name_text) = self.get_identifier_text(func_name) else {
            return false;
        };
        let Some(name_node) = self.arena.get(func_name) else {
            return false;
        };
        let Some(file_path) = self.current_file_path.clone() else {
            return false;
        };

        if func_body.is_some() {
            if let Some(returned_identifier) =
                self.function_body_unique_return_identifier(func_body)
                && self.emit_non_portable_initializer_declaration_diagnostics(
                    returned_identifier,
                    &name_text,
                    &file_path,
                    name_node.pos,
                    name_node.end - name_node.pos,
                )
            {
                return true;
            }

            if let Some(returned_identifier) =
                self.function_body_unique_return_identifier(func_body)
                && self.emit_returned_identifier_import_type_provenance_diagnostic(
                    returned_identifier,
                    printed_type_text,
                    &name_text,
                    &file_path,
                    name_node.pos,
                    name_node.end - name_node.pos,
                )
            {
                return true;
            }

            if let Some(return_expression) = self.function_body_single_return_expression(func_body)
                && self.emit_non_portable_initializer_declaration_diagnostics(
                    return_expression,
                    &name_text,
                    &file_path,
                    name_node.pos,
                    name_node.end - name_node.pos,
                )
            {
                return true;
            }
        }

        self.emit_non_portable_import_type_text_diagnostics(
            printed_type_text,
            &name_text,
            &file_path,
            name_node.pos,
            name_node.end - name_node.pos,
        )
    }

    pub(in crate::declaration_emitter) fn emit_non_serializable_property_diagnostic(
        &mut self,
        printed_type_text: &str,
        file: &str,
        pos: u32,
        length: u32,
    ) -> bool {
        use tsz_common::diagnostics::Diagnostic;

        let Some(property_name) =
            self.find_non_serializable_property_name_in_printed_type(printed_type_text)
        else {
            return false;
        };

        self.diagnostics.push(Diagnostic::from_code(
            4118,
            file,
            pos,
            length,
            &[&property_name],
        ));
        true
    }

    pub(crate) fn emit_non_serializable_import_type_diagnostic(
        &mut self,
        printed_type_text: &str,
        file: &str,
        pos: u32,
        length: u32,
    ) -> bool {
        use tsz_common::diagnostics::Diagnostic;

        // When isolated declarations is enabled, the checker will emit more
        // specific errors (TS9010, TS9038, etc.). Skip TS7056 to avoid masking them.
        if self.isolated_declarations {
            return false;
        }

        if self
            .find_unexported_import_type_reference_in_printed_type(printed_type_text)
            .is_none()
        {
            return false;
        }

        self.diagnostics
            .push(Diagnostic::from_code(7056, file, pos, length, &[]));
        true
    }

    pub(in crate::declaration_emitter) fn emit_truncation_diagnostic_if_needed(
        &mut self,
        expr_idx: NodeIndex,
        file: &str,
        pos: u32,
        length: u32,
    ) -> bool {
        // When isolated declarations is enabled, the checker will emit more
        // specific errors (TS9010, TS9038, etc.). Skip TS7056 to avoid masking them.
        if self.isolated_declarations {
            return false;
        }

        // Skip truncation check for property access expressions (e.g., Foo.m1).
        // These are not truncation candidates - their types are typically short
        // function type references like () => void, not complex literal types.
        if let Some(node) = self.arena.get(expr_idx)
            && node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
        {
            return false;
        }

        const NO_TRUNCATION_MAXIMUM_TRUNCATION_LENGTH: usize = 1_000_000;

        if let Some(estimated_length) = self.estimated_truncation_candidate_length(expr_idx)
            && estimated_length > NO_TRUNCATION_MAXIMUM_TRUNCATION_LENGTH
        {
            self.diagnostics
                .push(tsz_common::diagnostics::Diagnostic::from_code(
                    7056,
                    file,
                    pos,
                    length,
                    &[],
                ));
            return true;
        }

        let Some(type_text) = self.truncation_candidate_type_text(expr_idx) else {
            return false;
        };

        if type_text.chars().count() <= NO_TRUNCATION_MAXIMUM_TRUNCATION_LENGTH {
            return false;
        }

        self.diagnostics
            .push(tsz_common::diagnostics::Diagnostic::from_code(
                7056,
                file,
                pos,
                length,
                &[],
            ));
        true
    }

    pub(crate) fn emit_serialized_type_text_truncation_diagnostic_if_needed(
        &mut self,
        type_text: &str,
        file: &str,
        pos: u32,
        length: u32,
    ) -> bool {
        // When isolated declarations is enabled, the checker will emit more
        // specific errors (TS9010, TS9038, etc.). Skip TS7056 to avoid masking them.
        if self.isolated_declarations {
            return false;
        }

        const NO_TRUNCATION_MAXIMUM_TRUNCATION_LENGTH: usize = 1_000_000;

        if type_text.chars().count() <= NO_TRUNCATION_MAXIMUM_TRUNCATION_LENGTH {
            return false;
        }

        self.diagnostics
            .push(tsz_common::diagnostics::Diagnostic::from_code(
                7056,
                file,
                pos,
                length,
                &[],
            ));
        true
    }

    pub(crate) fn emit_non_serializable_local_alias_diagnostic(
        &mut self,
        printed_type_text: &str,
        file: &str,
        pos: u32,
        length: u32,
    ) -> bool {
        use tsz_common::diagnostics::Diagnostic;

        // When isolated declarations is enabled, the checker will emit more
        // specific errors (TS9010, TS9038, etc.). Skip TS7056 to avoid masking them.
        if self.isolated_declarations {
            return false;
        }

        if !self.printed_type_uses_non_emittable_local_alias_root(printed_type_text) {
            return false;
        }

        self.diagnostics
            .push(Diagnostic::from_code(7056, file, pos, length, &[]));
        true
    }
}
