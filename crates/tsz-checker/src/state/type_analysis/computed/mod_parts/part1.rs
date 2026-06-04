impl<'a> CheckerState<'a> {
    pub(crate) fn normalize_namespace_export_declaration_order(props: &mut [PropertyInfo]) {
        props.sort_by(
            |a, b| match (a.declaration_order > 0, b.declaration_order > 0) {
                (true, true) => a
                    .declaration_order
                    .cmp(&b.declaration_order)
                    .then_with(|| a.name.cmp(&b.name))
                    .then_with(|| a.type_id.0.cmp(&b.type_id.0)),
                (true, false) => std::cmp::Ordering::Less,
                (false, true) => std::cmp::Ordering::Greater,
                (false, false) => a
                    .name
                    .cmp(&b.name)
                    .then_with(|| a.type_id.0.cmp(&b.type_id.0)),
            },
        );

        for (idx, prop) in props.iter_mut().enumerate() {
            prop.declaration_order = idx as u32 + 1;
        }
    }

    pub(crate) fn ordered_namespace_export_entries<'b>(
        &self,
        exports_table: &'b tsz_binder::SymbolTable,
    ) -> Vec<(&'b str, SymbolId)> {
        let mut entries: Vec<_> = exports_table
            .iter()
            .map(|(name, &sym_id)| {
                let span = self
                    .get_symbol_globally(sym_id)
                    .or_else(|| self.get_cross_file_symbol(sym_id))
                    .and_then(|symbol| {
                        symbol
                            .first_declaration_span
                            .or(symbol.value_declaration_span)
                    });
                (name.as_str(), sym_id, span)
            })
            .collect();

        entries.sort_by_key(|(name, sym_id, span)| {
            let (start, end) = span.unwrap_or((u32::MAX, u32::MAX));
            (span.is_none(), start, end, *name, sym_id.0)
        });

        entries
            .into_iter()
            .map(|(name, sym_id, _)| (name, sym_id))
            .collect()
    }

    /// Resolve the type of a require-style import alias under Node20/NodeNext
    /// CJS-of-ESM `"module.exports"` interop.
    ///
    /// When a `.cjs`/`.cts` file imports a `.mjs`/`.mts` module that exports a
    /// `"module.exports"` binding (`export { X as "module.exports" }`), the
    /// alias resolves to the `"module.exports"` value rather than a synthesized
    /// namespace surface — `import X = require(M)`, `import * as X from M`,
    /// and (separately handled at `new`/identifier sites) `import X from M` all
    /// see `typeof X`, mirroring tsc's CommonJS-of-ESM interop where
    /// `module.exports = X` replaces the entire module value.
    pub(crate) fn module_exports_require_interop_alias_type(
        &mut self,
        sym_id: SymbolId,
    ) -> Option<TypeId> {
        // The interop only applies under Node20/NodeNext; gate on that first so
        // every other build pays one enum compare per call.
        if !matches!(
            self.ctx.compiler_options.module,
            ModuleKind::Node20 | ModuleKind::NodeNext
        ) {
            return None;
        }
        // Local-binder lookup: the alias symbol is owned by the file that
        // wrote the import declaration, not by the target module, so a
        // cross-file fallback would surface an unrelated symbol that happens
        // to share the same raw `SymbolId`.
        let module_exports_sym = {
            let symbol = self.ctx.binder.get_symbol(sym_id)?;
            if symbol.flags & symbol_flags::ALIAS == 0 {
                return None;
            }
            // Only require-style alias shapes: `import X = require(M)`
            // (import_name = None) and `import * as X from M`
            // (import_name = "*"). Named and default imports flow through
            // their own type-resolution paths.
            if !matches!(symbol.import_name.as_deref(), None | Some("*")) {
                return None;
            }
            let module_name = symbol.import_module.as_deref()?;
            if !self.current_file_uses_module_exports_require_interop(module_name) {
                return None;
            }
            let declaring_file_idx = self
                .ctx
                .resolve_symbol_file_index(sym_id)
                .or(Some(self.ctx.current_file_idx));
            self.resolve_effective_module_exports_from_file(module_name, declaring_file_idx)
                .and_then(|exports| exports.get("module.exports"))?
        };
        Some(self.get_type_of_symbol(module_exports_sym))
    }

    pub(crate) fn node_esm_cjs_default_import_namespace_type(
        &mut self,
        module_name: &str,
    ) -> Option<TypeId> {
        if !self.ctx.compiler_options.module.is_node_module()
            || self.ctx.file_is_esm != Some(true)
            || self.module_is_esm(module_name)
        {
            return None;
        }

        if let Some(exports_table) = self.resolve_effective_module_exports_from_file(
            module_name,
            Some(self.ctx.current_file_idx),
        ) {
            // For CJS modules using `export = X`, an ESM `import x from 'cjs'`
            // resolves to the type of X directly — the bare `module.exports`
            // value — not a synthesized namespace wrapper. This matches tsc:
            // CJS without an `__esModule` marker treats the whole
            // `module.exports` as the default, and `export = X` sets
            // `module.exports = X`. See
            // `nodeNextEsmImportsOfPackagesWithExtensionlessMains.ts`.
            if let Some(export_equals_sym_id) = exports_table.get("export=") {
                return Some(self.get_type_of_symbol(export_equals_sym_id));
            }

            let ordered_exports = self.ordered_namespace_export_entries(&exports_table);
            let mut props = Vec::new();
            for &(name, export_sym_id) in &ordered_exports {
                if self.should_skip_namespace_export_name(&exports_table, name, export_sym_id) {
                    continue;
                }
                let prop_type = self.get_type_of_symbol(export_sym_id);
                props.push(PropertyInfo {
                    name: self.ctx.types.intern_string(name),
                    type_id: prop_type,
                    write_type: prop_type,
                    optional: false,
                    readonly: false,
                    is_method: false,
                    is_class_prototype: false,
                    visibility: Visibility::Public,
                    parent_id: None,
                    declaration_order: if name == "default" {
                        1
                    } else {
                        props.len() as u32 + 2
                    },
                    is_string_named: false,
                    is_symbol_named: false,
                    single_quoted_name: false,
                });
            }
            Self::normalize_namespace_export_declaration_order(&mut props);
            let module_type = self.ctx.types.factory().object(props);
            self.ctx.namespace_module_names.insert(
                module_type,
                self.imported_namespace_display_module_name(module_name),
            );
            return Some(module_type);
        }

        let default_sym_id = self.resolve_cross_file_export_from_file(
            module_name,
            "default",
            Some(self.ctx.current_file_idx),
        )?;
        let default_type = self.get_type_of_symbol(default_sym_id);
        let module_type = self.ctx.types.factory().object(vec![PropertyInfo {
            name: self.ctx.types.intern_string("default"),
            type_id: default_type,
            write_type: default_type,
            optional: false,
            readonly: false,
            is_method: false,
            is_class_prototype: false,
            visibility: Visibility::Public,
            parent_id: None,
            declaration_order: 1,
            is_string_named: false,
            is_symbol_named: false,
            single_quoted_name: false,
        }]);
        self.ctx.namespace_module_names.insert(
            module_type,
            self.imported_namespace_display_module_name(module_name),
        );
        Some(module_type)
    }

    pub(crate) fn type_has_unresolved_inference_holes(&self, type_id: TypeId) -> bool {
        contains_type_parameters(self.ctx.types, type_id)
            || contains_infer_types(self.ctx.types, type_id)
    }

    pub(crate) fn should_skip_namespace_export_name(
        &self,
        exports_table: &tsz_binder::SymbolTable,
        export_name: &str,
        export_sym_id: SymbolId,
    ) -> bool {
        if export_name == "export=" {
            return true;
        }
        if export_name == "_default" && exports_table.get("default").is_some() {
            return true;
        }
        if !export_name.starts_with('_') {
            return false;
        }

        let Some(default_sym_id) = exports_table.get("default") else {
            return false;
        };
        if export_name == "_default" {
            return true;
        }
        if default_sym_id == export_sym_id {
            return true;
        }

        let lookup_symbol = |sym_id: SymbolId| {
            self.get_symbol_globally(sym_id)
                .or_else(|| self.get_cross_file_symbol(sym_id))
        };

        let resolve_alias_target = |sym_id: SymbolId| {
            let mut visited = AliasCycleTracker::new();
            self.resolve_alias_symbol(sym_id, &mut visited)
        };
        let default_target = resolve_alias_target(default_sym_id).unwrap_or(default_sym_id);
        let export_target = resolve_alias_target(export_sym_id).unwrap_or(export_sym_id);
        if export_target == default_target {
            return true;
        }

        let Some(export_symbol) = lookup_symbol(export_sym_id) else {
            return false;
        };
        let Some(default_symbol) = lookup_symbol(default_sym_id) else {
            return false;
        };

        (export_symbol.value_declaration.is_some()
            && export_symbol.value_declaration == default_symbol.value_declaration)
            || export_symbol
                .declarations
                .iter()
                .any(|decl| default_symbol.declarations.contains(decl))
    }

    pub(crate) fn namespace_import_export_property_type(
        &mut self,
        module_name: &str,
        export_sym_id: SymbolId,
    ) -> TypeId {
        let symbol_flags_opt = self
            .get_cross_file_symbol(export_sym_id)
            .or_else(|| self.get_symbol_globally(export_sym_id))
            .map(|symbol| symbol.flags);
        let is_pure_namespace = symbol_flags_opt.is_some_and(|flags| {
            (flags & (symbol_flags::NAMESPACE_MODULE | symbol_flags::VALUE_MODULE)) != 0
                && (flags & (symbol_flags::CLASS | symbol_flags::FUNCTION)) == 0
        });
        if is_pure_namespace {
            let prop_type = self.build_namespace_object_type(export_sym_id);
            self.ctx.namespace_module_names.insert(
                prop_type,
                self.imported_namespace_display_module_name(module_name),
            );
            return prop_type;
        }

        let should_delegate = self
            .ctx
            .resolve_symbol_file_index(export_sym_id)
            .is_some_and(|file_idx| file_idx != self.ctx.current_file_idx)
            || self
                .get_cross_file_symbol(export_sym_id)
                .is_some_and(|symbol| {
                    symbol.decl_file_idx != u32::MAX
                        && symbol.decl_file_idx as usize != self.ctx.current_file_idx
                });
        let mut prop_type = if should_delegate {
            self.delegate_cross_arena_symbol_resolution(export_sym_id)
                .map(|(type_id, _)| type_id)
                .unwrap_or_else(|| self.get_type_of_symbol(export_sym_id))
        } else {
            self.get_type_of_symbol(export_sym_id)
        };
        if symbol_flags_opt.is_some_and(|flags| {
            (flags & symbol_flags::ENUM) != 0 && (flags & symbol_flags::ENUM_MEMBER) == 0
        }) {
            prop_type = self.get_enum_namespace_type_for_value(prop_type);
        }
        prop_type
    }

    pub(crate) fn namespace_default_reexport_property_type(
        &mut self,
        module_name: &str,
        declaring_file_idx: Option<usize>,
        export_name: &str,
    ) -> Option<TypeId> {
        let namespace_from = declaring_file_idx.unwrap_or(self.ctx.current_file_idx);
        let namespace_file_idx = self
            .ctx
            .resolve_import_target_from_file(namespace_from, module_name)
            .or_else(|| self.ctx.resolve_import_target(module_name))?;
        let namespace_binder = self.ctx.get_binder_for_file(namespace_file_idx)?;
        let namespace_file_name = self
            .ctx
            .get_arena_for_file(namespace_file_idx as u32)
            .source_files
            .first()
            .map(|sf| sf.file_name.clone())?;
        let reexports = self
            .ctx
            .reexports_for_file(namespace_binder, &namespace_file_name)?;
        let (source_module, original_name) = reexports.get(export_name)?;
        let source_module = source_module.clone();
        let original_name = original_name.clone();
        let imported_name = original_name.as_deref().unwrap_or(export_name);
        if imported_name != "default" {
            return None;
        }

        self.default_import_namespace_object_type_from_file(&source_module, namespace_file_idx)
    }

    fn default_import_namespace_object_type_from_file(
        &mut self,
        module_name: &str,
        source_file_idx: usize,
    ) -> Option<TypeId> {
        let target_file_idx = self
            .ctx
            .resolve_import_target_from_file(source_file_idx, module_name)
            .or_else(|| self.ctx.resolve_import_target(module_name))?;
        let target_is_esm = self.source_file_idx_is_esm_module(target_file_idx);
        let source_is_esm = self.source_file_idx_is_esm_module(source_file_idx);

        let exports_table =
            self.resolve_effective_module_exports_from_file(module_name, Some(source_file_idx))?;
        let is_node_esm_importing_cjs =
            self.ctx.compiler_options.module.is_node_module() && source_is_esm && !target_is_esm;
        let has_export_equals = exports_table.has("export=");
        if !(is_node_esm_importing_cjs
            || has_export_equals && self.ctx.allow_synthetic_default_imports())
        {
            return None;
        }

        let ordered_exports = self.ordered_namespace_export_entries(&exports_table);
        let mut props = Vec::new();
        for &(name, export_sym_id) in &ordered_exports {
            if self.should_skip_namespace_export_name(&exports_table, name, export_sym_id)
                || self.is_type_only_export_symbol(export_sym_id)
                || self.is_export_from_type_only_wildcard(module_name, name)
                || self.export_symbol_has_no_value(export_sym_id)
                || self.is_export_type_only_from_file(module_name, name, Some(source_file_idx))
            {
                continue;
            }

            let mut prop_type = self.get_type_of_symbol(export_sym_id);
            prop_type = self.apply_module_augmentations(module_name, name, prop_type);
            let name_atom = self.ctx.types.intern_string(name);
            props.push(PropertyInfo {
                name: name_atom,
                type_id: prop_type,
                write_type: prop_type,
                optional: false,
                readonly: false,
                is_method: false,
                is_class_prototype: false,
                visibility: Visibility::Public,
                parent_id: None,
                declaration_order: if name == "default" {
                    1
                } else {
                    props.len() as u32 + 2
                },
                is_string_named: false,
                is_symbol_named: false,
                single_quoted_name: false,
            });
        }

        if has_export_equals && let Some(export_equals_sym_id) = exports_table.get("export=") {
            let export_equals_type = self.get_type_of_symbol(export_equals_sym_id);
            let default_atom = self.ctx.types.intern_string("default");
            if let Some(existing_default) = props.iter_mut().find(|p| p.name == default_atom) {
                existing_default.type_id = export_equals_type;
                existing_default.write_type = export_equals_type;
                existing_default.optional = false;
                existing_default.readonly = false;
            } else {
                props.push(PropertyInfo {
                    name: default_atom,
                    type_id: export_equals_type,
                    write_type: export_equals_type,
                    optional: false,
                    readonly: false,
                    is_method: false,
                    is_class_prototype: false,
                    visibility: Visibility::Public,
                    parent_id: None,
                    declaration_order: 1,
                    is_string_named: false,
                    is_symbol_named: false,
                    single_quoted_name: false,
                });
            }
        }

        if props.is_empty() {
            return None;
        }

        Self::normalize_namespace_export_declaration_order(&mut props);
        let module_type = self.ctx.types.factory().object(props);
        self.ctx.namespace_module_names.insert(
            module_type,
            self.imported_namespace_display_module_name(module_name),
        );
        Some(module_type)
    }

    fn source_file_idx_is_esm_module(&self, file_idx: usize) -> bool {
        let arena = self.ctx.get_arena_for_file(file_idx as u32);
        let Some(source_file) = arena.source_files.first() else {
            return false;
        };
        let file_name = source_file.file_name.as_str();
        if file_name.ends_with(".mjs") || file_name.ends_with(".mts") {
            return true;
        }
        if file_name.ends_with(".cjs") || file_name.ends_with(".cts") {
            return false;
        }
        if self.source_file_idx_is_js_with_esm_syntax(file_idx) {
            return true;
        }
        self.lookup_file_is_esm(file_name).unwrap_or(false)
    }

    pub(crate) fn self_namespace_import_object_type(
        &mut self,
        module_name: &str,
        declaring_file_idx: Option<usize>,
    ) -> Option<TypeId> {
        let from_file_idx = declaring_file_idx.unwrap_or(self.ctx.current_file_idx);
        let target_file_idx = self
            .ctx
            .resolve_import_target_from_file(from_file_idx, module_name)
            .or_else(|| self.ctx.resolve_import_target(module_name))?;
        if target_file_idx != self.ctx.current_file_idx {
            return None;
        }
        if !self.namespace_has_default_interop_reexport(module_name, Some(from_file_idx)) {
            return None;
        }

        let exports_table =
            self.resolve_effective_module_exports_from_file(module_name, Some(from_file_idx))?;
        let ordered_exports = self.ordered_namespace_export_entries(&exports_table);
        let mut props = Vec::new();
        for &(name, export_sym_id) in &ordered_exports {
            if self.should_skip_namespace_export_name(&exports_table, name, export_sym_id)
                || self.is_type_only_export_symbol(export_sym_id)
                || self.is_export_from_type_only_wildcard(module_name, name)
                || self.export_symbol_has_no_value(export_sym_id)
                || self.is_export_type_only_from_file(module_name, name, Some(from_file_idx))
            {
                continue;
            }

            let mut prop_type = self
                .namespace_default_reexport_property_type(module_name, Some(from_file_idx), name)
                .unwrap_or_else(|| {
                    self.namespace_import_export_property_type(module_name, export_sym_id)
                });
            prop_type = self.apply_module_augmentations(module_name, name, prop_type);
            props.push(PropertyInfo {
                name: self.ctx.types.intern_string(name),
                type_id: prop_type,
                write_type: prop_type,
                optional: false,
                readonly: false,
                is_method: false,
                is_class_prototype: false,
                visibility: Visibility::Public,
                parent_id: None,
                declaration_order: if name == "default" {
                    1
                } else {
                    props.len() as u32 + 2
                },
                is_string_named: false,
                is_symbol_named: false,
                single_quoted_name: false,
            });
        }

        if props.is_empty() {
            return None;
        }

        Self::normalize_namespace_export_declaration_order(&mut props);
        let namespace_type = self.ctx.types.factory().object(props);
        self.ctx.namespace_module_names.insert(
            namespace_type,
            self.imported_namespace_display_module_name(module_name),
        );
        Some(namespace_type)
    }

    fn namespace_has_default_interop_reexport(
        &mut self,
        module_name: &str,
        declaring_file_idx: Option<usize>,
    ) -> bool {
        let namespace_from = declaring_file_idx.unwrap_or(self.ctx.current_file_idx);
        let Some(namespace_file_idx) = self
            .ctx
            .resolve_import_target_from_file(namespace_from, module_name)
            .or_else(|| self.ctx.resolve_import_target(module_name))
        else {
            return false;
        };
        let Some(namespace_binder) = self.ctx.get_binder_for_file(namespace_file_idx) else {
            return false;
        };
        let Some(namespace_file_name) = self
            .ctx
            .get_arena_for_file(namespace_file_idx as u32)
            .source_files
            .first()
            .map(|sf| sf.file_name.clone())
        else {
            return false;
        };
        let Some(reexports) = self
            .ctx
            .reexports_for_file(namespace_binder, &namespace_file_name)
            .cloned()
        else {
            return false;
        };

        reexports
            .iter()
            .any(|(exported_name, (source_module, original_name))| {
                let imported_name = original_name.as_deref().unwrap_or(exported_name);
                if imported_name != "default" {
                    return false;
                }
                if self.ctx.compiler_options.module.is_node_module()
                    && self.source_file_idx_is_esm_module(namespace_file_idx)
                {
                    return true;
                }
                self.default_import_namespace_object_type_from_file(
                    source_module,
                    namespace_file_idx,
                )
                .is_some()
            })
    }

    pub(crate) fn append_export_equals_import_type_namespace_props(
        &mut self,
        module_name: &str,
        declaring_file_idx: Option<usize>,
        exports_table: &tsz_binder::SymbolTable,
        props: &mut Vec<PropertyInfo>,
    ) -> Option<String> {
        let export_equals_sym_id = exports_table.get("export=")?;
        let mut export_equals_symbol = self
            .get_symbol_globally(export_equals_sym_id)
            .or_else(|| self.get_cross_file_symbol(export_equals_sym_id))?;

        if export_equals_symbol.decl_file_idx == u32::MAX
            && let Some(target_idx) = self.ctx.resolve_symbol_file_index(export_equals_sym_id)
            && let Some(target_binder) = self.ctx.get_binder_for_file(target_idx)
        {
            let target_file_name = self
                .ctx
                .get_arena_for_file(target_idx as u32)
                .source_files
                .first()
                .map(|sf| sf.file_name.clone());
            let target_export_equals_sym = target_file_name
                .as_ref()
                .and_then(|file_name| {
                    target_binder
                        .module_exports
                        .get(file_name)
                        .and_then(|table| table.get("export="))
                })
                .or_else(|| {
                    target_binder
                        .module_exports
                        .get(module_name)
                        .and_then(|table| table.get("export="))
                })
                .or_else(|| {
                    target_binder
                        .module_exports
                        .values()
                        .find_map(|table| table.get("export="))
                });
            if let Some(target_export_equals_sym) = target_export_equals_sym
                && let Some(target_symbol) = target_binder.get_symbol(target_export_equals_sym)
            {
                export_equals_symbol = target_symbol;
            }
        }

        let mut nested_exports = tsz_binder::SymbolTable::new();
        let nested_module_specifier = self.merge_export_equals_import_type_members(
            export_equals_symbol,
            declaring_file_idx,
            &mut nested_exports,
        );

        for (name, export_sym_id) in self.ordered_namespace_export_entries(&nested_exports) {
            if self.should_skip_namespace_export_name(&nested_exports, name, export_sym_id) {
                continue;
            }
            if props
                .iter()
                .any(|p| self.ctx.types.resolve_atom_ref(p.name).as_ref() == name)
            {
                continue;
            }
            let export_is_namespace_module = self
                .get_symbol_globally(export_sym_id)
                .or_else(|| self.get_cross_file_symbol(export_sym_id))
                .is_some_and(|symbol| {
                    (symbol.flags
                        & (tsz_binder::symbol_flags::NAMESPACE_MODULE
                            | tsz_binder::symbol_flags::VALUE_MODULE))
                        != 0
                });
            if self.is_type_only_export_symbol(export_sym_id)
                || self.is_export_from_type_only_wildcard(module_name, name)
                || (self.export_symbol_has_no_value(export_sym_id) && !export_is_namespace_module)
                || self.is_export_type_only_from_file(module_name, name, declaring_file_idx)
            {
                continue;
            }

            self.record_cross_file_symbol_if_needed(export_sym_id, name, module_name);
            let mut prop_type =
                self.namespace_import_export_property_type(module_name, export_sym_id);
            prop_type = self.apply_module_augmentations(module_name, name, prop_type);
            let declaration_order = if name == "default" {
                1
            } else {
                props.len() as u32 + 2
            };
            let name_atom = self.ctx.types.intern_string(name);
            props.push(PropertyInfo {
                name: name_atom,
                type_id: prop_type,
                write_type: prop_type,
                optional: false,
                readonly: false,
                is_method: false,
                is_class_prototype: false,
                visibility: Visibility::Public,
                parent_id: None,
                declaration_order,
                is_string_named: false,
                is_symbol_named: false,
                single_quoted_name: false,
            });
        }
        nested_module_specifier
    }

    /// Merge value-side property exports from a CommonJS module's JS export
    /// surface into a typeof-import namespace `props` list. Supplements the
    /// binder's `module_exports` table for files where the binder records only
    /// an `export=` (or nothing at all) for `module.exports = { … }`-style
    /// object-literal exports, so `typeof import("./mod").foo` can find `foo`
    /// as a value member instead of falsely emitting TS2694.
    ///
    /// Existing props (added from the binder's exports table or
    /// [`append_export_equals_import_type_namespace_props`]) take precedence —
    /// this only fills in names that are not already present.
    pub(crate) fn merge_js_export_surface_into_typeof_import_namespace_props(
        &mut self,
        module_name: &str,
        declaring_file_idx: Option<usize>,
        props: &mut Vec<PropertyInfo>,
    ) {
        let Some(js_surface) =
            self.resolve_js_export_surface_for_module(module_name, declaring_file_idx)
        else {
            return;
        };
        for prop in js_surface.named_exports {
            let prop_name_atom = self.ctx.types.resolve_atom_ref(prop.name);
            let prop_name = prop_name_atom.as_ref();
            if prop_name == "export=" {
                continue;
            }
            if props
                .iter()
                .any(|p| self.ctx.types.resolve_atom_ref(p.name).as_ref() == prop_name)
            {
                continue;
            }
            let mut new_prop = prop.clone();
            new_prop.declaration_order = if prop_name == "default" {
                1
            } else {
                (props.len() as u32) + 2
            };
            props.push(new_prop);
        }
    }
}
