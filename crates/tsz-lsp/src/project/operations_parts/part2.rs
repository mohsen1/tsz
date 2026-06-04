impl Project {
    /// Rename a symbol across files in the project.
    pub fn get_rename_edits(
        &mut self,
        file_name: &str,
        position: Position,
        new_name: String,
    ) -> Result<WorkspaceEdit, String> {
        self.touch_file(file_name);
        let start = Instant::now();
        let mut scope_stats = ScopeCacheStats::default();

        // Step 1: Normalize the new name
        let normalized_name = {
            let file = self
                .files
                .get(file_name)
                .ok_or_else(|| "You cannot rename this element.".to_string())?;
            let provider = RenameProvider::from_context(file.provider_context());
            provider.normalize_rename_at_position(position, &new_name)?
        };

        // Step 2: Resolve the symbol at the cursor position
        let (symbol_id, local_name) = {
            let file = self
                .files
                .get_mut(file_name)
                .ok_or_else(|| "You cannot rename this element.".to_string())?;
            let offset = file
                .line_map
                .position_to_offset(position, file.source_text())
                .ok_or_else(|| "Could not find symbol to rename".to_string())?;
            let node_idx = find_node_at_offset(file.arena(), offset);
            if node_idx.is_none() {
                return Err("Could not find symbol to rename".to_string());
            }

            let finder = FindReferences::new(
                file.parser.get_arena(),
                &file.binder,
                &file.line_map,
                file.file_name.clone(),
                file.parser.get_source_text(),
            );
            let symbol_id = finder
                .resolve_symbol_for_node_with_scope_cache(
                    file.root(),
                    node_idx,
                    &mut file.scope_cache,
                    Some(&mut scope_stats),
                )
                .ok_or_else(|| "Could not find symbol to rename".to_string())?;
            let symbol = file
                .binder()
                .symbols
                .get(symbol_id)
                .ok_or_else(|| "Could not find symbol to rename".to_string())?;
            let local_name = symbol.escaped_name.clone();

            (symbol_id, local_name)
        };

        // Step 3: Check if this is a heritage member (class/interface member)
        let is_heritage_member = {
            let file = self
                .files
                .get(file_name)
                .ok_or_else(|| "Could not find file".to_string())?;
            let symbol = file.binder().symbols.get(symbol_id);
            symbol.is_some_and(|s| Self::is_heritage_member_symbol(file, s))
        };

        // Step 4: If heritage member, use heritage-aware rename logic
        if is_heritage_member {
            return self.get_heritage_rename_edits(
                file_name,
                symbol_id,
                &local_name,
                normalized_name,
                start,
                scope_stats,
            );
        }

        // Step 5: Otherwise, use standard rename logic (imports/exports)
        let result = (|| {
            let (import_targets, export_names, source_file_name) = {
                let file = self
                    .files
                    .get_mut(file_name)
                    .ok_or_else(|| "You cannot rename this element.".to_string())?;
                let import_targets = file.import_targets_for_local(&local_name);
                let export_names = file.exported_names_for_symbol(symbol_id);
                let source_file_name = file.file_name().to_string();
                (import_targets, export_names, source_file_name)
            };

            let mut workspace_edit = {
                let file = self
                    .files
                    .get_mut(file_name)
                    .ok_or_else(|| "You cannot rename this element.".to_string())?;
                let root = file.root();
                let provider = RenameProvider::from_context(file.provider_context());
                provider.provide_rename_edits_for_symbol(
                    root,
                    symbol_id,
                    normalized_name.clone(),
                )?
            };

            let mut cross_targets = Vec::new();

            if !import_targets.is_empty() {
                for target in import_targets {
                    let Some(resolved) =
                        self.resolve_module_specifier(&source_file_name, &target.module_specifier)
                    else {
                        continue;
                    };

                    match target.kind {
                        ImportKind::Named(name) => {
                            if name == local_name {
                                cross_targets.push((resolved, name));
                            }
                        }
                        ImportKind::Default => {
                            cross_targets.push((resolved, "default".to_string()));
                        }
                        ImportKind::Namespace => {}
                    }
                }
            }

            let mut export_names: Vec<String> = export_names
                .into_iter()
                .filter(|name| name == &local_name)
                .collect();
            export_names.sort();
            export_names.dedup();

            for export_name in export_names {
                cross_targets.push((source_file_name.clone(), export_name));
            }

            if cross_targets.is_empty() {
                Self::dedup_workspace_edit(&mut workspace_edit);
                return Ok(workspace_edit);
            }

            let mut pending = cross_targets;
            let mut seen_targets: FxHashSet<(String, String)> = FxHashSet::default();
            let mut namespace_targets = Vec::new();

            while let Some((def_file, export_name)) = pending.pop() {
                if !seen_targets.insert((def_file.clone(), export_name.clone())) {
                    continue;
                }

                if def_file != file_name {
                    let export_nodes = {
                        let target_file = self.files.get(&def_file);
                        target_file
                            .map(|file| file.export_nodes(&export_name))
                            .unwrap_or_default()
                    };
                    if !export_nodes.is_empty()
                        && let Some(target_file) = self.files.get_mut(&def_file)
                    {
                        for node in export_nodes {
                            Self::collect_file_rename_edits(
                                target_file,
                                node,
                                &normalized_name,
                                &mut workspace_edit,
                            );
                        }
                    }
                }

                let mut reexport_refs = Vec::new();
                let (reexports, reexport_namespaces) =
                    self.reexport_targets_for(&def_file, &export_name, &mut reexport_refs);
                for location in reexport_refs {
                    workspace_edit.add_edit(
                        location.file_path,
                        TextEdit::new(location.range, normalized_name.clone()),
                    );
                }

                for (reexport_file, reexport_name) in reexports {
                    if reexport_name == export_name {
                        pending.push((reexport_file, reexport_name));
                    }
                }

                namespace_targets.extend(reexport_namespaces);

                // Pool Scan Optimization: Use SymbolIndex for O(M) candidate filtering
                // Instead of O(N) where N = all files, we get O(M) where M = files containing the symbol
                let candidate_files = self.get_candidate_files_for_symbol(&export_name);

                for other_name in &candidate_files {
                    if other_name == &def_file {
                        continue;
                    }

                    let import_targets = {
                        let other_file = self.files.get(other_name);
                        other_file
                            .map(|file| {
                                self.import_specifier_targets_for_export(
                                    file,
                                    &def_file,
                                    &export_name,
                                )
                            })
                            .unwrap_or_default()
                    };
                    if !import_targets.is_empty()
                        && let Some(other_file) = self.files.get_mut(other_name)
                    {
                        for target in import_targets {
                            if let Some(property_name) = target.property_name {
                                if let Some(location) = other_file.node_location(property_name) {
                                    workspace_edit.add_edit(
                                        location.file_path,
                                        TextEdit::new(location.range, normalized_name.clone()),
                                    );
                                }
                            } else {
                                if other_name == file_name {
                                    continue;
                                }
                                Self::collect_file_rename_edits(
                                    other_file,
                                    target.local_ident,
                                    &normalized_name,
                                    &mut workspace_edit,
                                );
                            }
                        }
                    }

                    let namespace_names = {
                        let other_file = self.files.get(other_name);
                        other_file
                            .map(|file| self.namespace_import_names(file, &def_file))
                            .unwrap_or_default()
                    };
                    if !namespace_names.is_empty()
                        && let Some(other_file) = self.files.get(other_name)
                    {
                        let mut locations = Vec::new();
                        for namespace_name in namespace_names {
                            self.collect_namespace_member_locations(
                                other_file,
                                &namespace_name,
                                &export_name,
                                &mut locations,
                            );
                        }
                        for location in locations {
                            workspace_edit.add_edit(
                                location.file_path,
                                TextEdit::new(location.range, normalized_name.clone()),
                            );
                        }
                    }
                }
            }

            let mut seen_namespace_targets: FxHashSet<(String, String, String)> =
                FxHashSet::default();
            for target in namespace_targets {
                if !seen_namespace_targets.insert((
                    target.file.clone(),
                    target.namespace.clone(),
                    target.member.clone(),
                )) {
                    continue;
                }

                // Pool Scan Optimization: Use SymbolIndex for O(M) candidate filtering
                let candidate_files = self.get_candidate_files_for_symbol(&target.member);

                for other_name in &candidate_files {
                    if other_name == &target.file {
                        continue;
                    }

                    let local_names = {
                        let other_file = self.files.get(other_name);
                        other_file
                            .map(|file| {
                                self.named_import_local_names(file, &target.file, &target.namespace)
                            })
                            .unwrap_or_default()
                    };
                    if local_names.is_empty() {
                        continue;
                    }

                    if let Some(other_file) = self.files.get(other_name) {
                        let mut locations = Vec::new();
                        for local_name in local_names {
                            self.collect_namespace_member_locations(
                                other_file,
                                &local_name,
                                &target.member,
                                &mut locations,
                            );
                        }
                        for location in locations {
                            workspace_edit.add_edit(
                                location.file_path,
                                TextEdit::new(location.range, normalized_name.clone()),
                            );
                        }
                    }
                }
            }

            Self::dedup_workspace_edit(&mut workspace_edit);
            Ok(workspace_edit)
        })();

        self.performance
            .record(ProjectRequestKind::Rename, start.elapsed(), scope_stats);

        result
    }

    /// Heritage-aware rename: Renames a class/interface member across the entire
    /// inheritance hierarchy.
    ///
    /// This handles renaming members that are overridden in derived classes or
    /// override base class members. For example, renaming `Base.foo()` should
    /// also rename `Derived.foo()` when `Derived extends Base`.
    ///
    /// # Arguments
    /// * `file_name` - The file containing the symbol being renamed
    /// * `symbol_id` - The `SymbolId` of the member being renamed
    /// * `local_name` - The current name of the member
    /// * `new_name` - The new name for the member
    /// * `start` - Instant for performance tracking
    ///
    /// # Returns
    /// * `Ok(WorkspaceEdit)` - The workspace edit with all rename changes
    /// * `Err(String)` - Error message if rename failed
    fn get_heritage_rename_edits(
        &mut self,
        file_name: &str,
        symbol_id: tsz_binder::SymbolId,
        local_name: &str,
        new_name: String,
        start: Instant,
        scope_stats: ScopeCacheStats,
    ) -> Result<WorkspaceEdit, String> {
        let mut workspace_edit = WorkspaceEdit::default();

        // Get the file containing the symbol
        let file = self
            .files
            .get(file_name)
            .ok_or_else(|| "Could not find file".to_string())?;

        // Find ALL related symbols in the inheritance hierarchy
        let heritage_symbols = self.find_all_heritage_members(file, symbol_id, local_name);

        // For each heritage symbol, find all its references and generate rename edits
        for (_heritage_file_path, heritage_symbol_id) in heritage_symbols {
            // Use pool scan optimization: get candidate files that contain this symbol name
            let candidate_files = self.get_candidate_files_for_symbol(local_name);

            for target_file_path in candidate_files {
                let target_file = match self.files.get_mut(&target_file_path) {
                    Some(f) => f,
                    None => continue,
                };

                // Create a RenameProvider for this file
                let target_root = target_file.root();
                let provider = RenameProvider::from_context(target_file.provider_context());

                // Get rename edits for this specific heritage symbol in this file
                // Note: We must use the heritage_symbol_id, not the original symbol_id,
                // because Base.foo and Derived.foo are different SymbolIds
                match provider.provide_rename_edits_for_symbol(
                    target_root,
                    heritage_symbol_id,
                    new_name.clone(),
                ) {
                    Ok(edits) => {
                        // Merge the edits into the workspace edit
                        for (file_path, text_edits) in edits.changes {
                            for edit in text_edits {
                                workspace_edit.add_edit(file_path.clone(), edit);
                            }
                        }
                    }
                    Err(_) => {
                        // If we can't find references in this file, continue silently
                        // This can happen if the file doesn't actually reference this symbol
                        continue;
                    }
                }
            }
        }

        // Deduplicate the workspace edit in case multiple symbols produced edits for the same location
        Self::dedup_workspace_edit(&mut workspace_edit);

        self.performance
            .record(ProjectRequestKind::Rename, start.elapsed(), scope_stats);

        Ok(workspace_edit)
    }
}
