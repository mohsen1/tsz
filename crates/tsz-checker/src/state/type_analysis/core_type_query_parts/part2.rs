impl<'a> CheckerState<'a> {
    pub(crate) fn resolve_typeof_import_query(&mut self, expr_name: NodeIndex) -> Option<TypeId> {
        let (call_idx, segments) = self.decompose_typeof_import_query(expr_name)?;
        let (module_name, specifier_node) = self.get_import_type_module_specifier(call_idx)?;
        let resolution_mode_override = self.get_import_type_resolution_mode_override(call_idx);
        self.maybe_emit_import_type_cjs_esm_resolution_mode_missing(
            &module_name,
            specifier_node,
            resolution_mode_override,
        );

        let Some(mut current) =
            self.build_typeof_import_namespace_type(&module_name, resolution_mode_override)
        else {
            // Match the bare `import("./missing")` type-position behavior in
            // `import_type.rs`: emit TS2307 at the module specifier when the
            // module cannot be resolved through any of the binder's exports
            // tables. Without this branch, `typeof import("./missing")`
            // silently resolves to `any` and tsc-parity diagnostics go
            // missing. Gated on the same `report_unresolved_imports` flag the
            // bare `import_type` path uses so cross-file fixture pipelines
            // that suppress unresolved-import noise stay quiet.
            if self.ctx.report_unresolved_imports
                && !self.ctx.binder.module_exports.contains_key(&module_name)
            {
                let (message, code) = self.module_not_found_diagnostic_for_site(
                    &module_name,
                    crate::import::core::ModuleNotFoundSite::ImportType,
                );
                self.error_at_node(specifier_node, &message, code);
            }
            return None;
        };
        let mut resolved_segments: Vec<String> = Vec::new();
        let mut segments_iter = segments.into_iter().peekable();
        while let Some((segment_idx, segment)) = segments_iter.next() {
            let access = if self.is_namespace_value_type(current) {
                self.resolve_namespace_value_member(current, &segment)
                    .or_else(|| self.resolve_namespace_typeof_member(current, &segment))
                    .map(
                        |type_id| crate::query_boundaries::common::PropertyAccessResult::Success {
                            type_id,
                            write_type: None,
                            from_index_signature: false,
                        },
                    )
                    .unwrap_or(
                        crate::query_boundaries::common::PropertyAccessResult::PropertyNotFound {
                            type_id: current,
                            property_name: self.ctx.types.intern_string(&segment),
                        },
                    )
            } else {
                self.resolve_property_access_with_env(current, &segment)
            };
            current = match access {
                crate::query_boundaries::common::PropertyAccessResult::Success {
                    type_id, ..
                } => {
                    resolved_segments.push(segment.clone());
                    if self.is_namespace_value_type(type_id)
                        || self.ctx.namespace_module_names.contains_key(&type_id)
                    {
                        type_id
                    } else {
                        self.resolve_type_query_type(type_id)
                    }
                }
                crate::query_boundaries::common::PropertyAccessResult::PropertyNotFound {
                    ..
                }
                | crate::query_boundaries::common::PropertyAccessResult::IsUnknown => {
                    if let Some(type_id) = self.try_resolve_typeof_import_segment_via_export_equals(
                        &module_name,
                        &resolved_segments,
                        &segment,
                        resolution_mode_override,
                    ) {
                        resolved_segments.push(segment.clone());
                        current = self.resolve_type_query_type(type_id);
                        continue;
                    }
                    let namespace_name = self
                        .ctx
                        .namespace_module_names
                        .get(&current)
                        .map(|name| {
                            format!("\"{}\".export=", name.strip_prefix("./").unwrap_or(name))
                        })
                        .or_else(|| {
                            self.is_namespace_value_type(current).then(|| {
                                format!(
                                    "\"{}\".export=",
                                    self.imported_namespace_display_module_name(&module_name)
                                )
                            })
                        });
                    if let Some(mut namespace_name) = namespace_name {
                        // For `typeof import("./m").bar.missing` on export= modules,
                        // preserve the nested qualifier path when the first segment
                        // comes from the export= target surface.
                        if resolved_segments.is_empty()
                            && namespace_name.ends_with(".export=")
                            && let Some((next_idx, next_segment)) = segments_iter.next()
                        {
                            let base = namespace_name.trim_end_matches(".export=");
                            namespace_name = format!("{base}.{segment}.export=");
                            self.error_namespace_no_export(
                                &namespace_name,
                                &next_segment,
                                next_idx,
                            );
                            return Some(TypeId::ERROR);
                        }
                        if namespace_name.ends_with(".export=") && !resolved_segments.is_empty() {
                            let base = namespace_name.trim_end_matches(".export=");
                            namespace_name =
                                format!("{base}.{}.export=", resolved_segments.join("."));
                        }
                        self.error_namespace_no_export(&namespace_name, &segment, segment_idx);
                    } else {
                        self.error_property_not_exist_at(&segment, current, segment_idx);
                    }
                    return Some(TypeId::ERROR);
                }
                _ => return Some(TypeId::ERROR),
            };
        }
        Some(current)
    }

    fn try_resolve_typeof_import_segment_via_export_equals(
        &mut self,
        module_name: &str,
        resolved_segments: &[String],
        next_segment: &str,
        _resolution_mode_override: Option<crate::context::ResolutionModeOverride>,
    ) -> Option<TypeId> {
        if !resolved_segments.is_empty() {
            return None;
        }

        let sym_id = self.resolve_named_export_via_export_equals(module_name, next_segment)?;
        Some(self.get_type_of_symbol(sym_id))
    }
}
