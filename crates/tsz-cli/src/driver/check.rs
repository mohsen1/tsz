                // Map resolved path to file index
                // NOTE: Only mark as resolved if there's NO error. When there's a resolution
                // error (TS2307, etc.), the module should NOT be in resolved_module_specifiers
                // so that the checker will emit the appropriate error.
                let request_resolution_mode = effective_request_resolution_mode(
                    &mut module_resolver,
                    options,
                    file_path,
                    *import_kind,
                    *resolution_mode_override,
                );
                if outcome.error.is_none() {
                    if let Some(ref resolved_path) = outcome.resolved_path {
                        resolved_module_specifiers.insert((file_idx, specifier.clone()));
                        if let Some(target_idx) = resolved_path_target_idx(
                            resolved_path,
                            options,
                            &package_redirects,
                            &canonical_to_file_idx,
                        ) {
                            resolved_module_paths.insert((file_idx, specifier.clone()), target_idx);
                            resolved_module_request_paths.insert(
                                (file_idx, specifier.clone(), request_resolution_mode),
                                target_idx,
                            );
                            if options.rewrite_relative_import_extensions
                                && outcome.resolved_using_ts_extension
                                && !should_rewrite_relative_ts_specifier(specifier)
                                && may_emit_input_source(Path::new(&program.files[target_idx].file_name))
                            {
                                let ts_ext = if specifier.ends_with(".tsx") {
                                    ".tsx"
                                } else if specifier.ends_with(".mts") {
                                    ".mts"
                                } else if specifier.ends_with(".cts") {
                                    ".cts"
                                } else {
                                    ".ts"
                                };
                                let error = tsz::checker::context::ResolutionError {
                                    code: tsz::checker::diagnostics::diagnostic_codes::THIS_IMPORT_USES_A_EXTENSION_TO_RESOLVE_TO_AN_INPUT_TYPESCRIPT_FILE_BUT_WILL_NOT,
                                    message: tsz::checker::diagnostics::format_message(
                                        tsz::checker::diagnostics::diagnostic_messages::THIS_IMPORT_USES_A_EXTENSION_TO_RESOLVE_TO_AN_INPUT_TYPESCRIPT_FILE_BUT_WILL_NOT,
                                        &[ts_ext],
                                    ),
                                };
                                resolved_module_errors.insert((file_idx, specifier.clone()), error.clone());
                                resolved_module_request_errors.insert(
                                    (file_idx, specifier.clone(), request_resolution_mode),
                                    error,
                                );
                            }
                        }
                    } else if outcome.is_resolved {
                        resolved_module_specifiers.insert((file_idx, specifier.clone()));
                    }
                }

                // Record error for the checker
                if let Some(ref error) = outcome.error {
                    resolved_module_errors.insert(
                        (file_idx, specifier.clone()),
                        tsz::checker::context::ResolutionError {
                            code: error.code,
                            message: error.message.clone(),
                        },
                    );
                    resolved_module_request_errors.insert(
                        (file_idx, specifier.clone(), request_resolution_mode),
                        tsz::checker::context::ResolutionError {
                            code: error.code,
                            message: error.message.clone(),
                        },
                    );
                }
            }
        }
    }

    let resolved_module_paths = Arc::new(resolved_module_paths);
    let resolved_module_request_paths = Arc::new(resolved_module_request_paths);
    let resolved_module_specifiers = Arc::new(resolved_module_specifiers);
    let resolved_module_errors = Arc::new(resolved_module_errors);
    let resolved_module_request_errors = Arc::new(resolved_module_request_errors);

    // Pre-compute per-file TS7016 diagnostics for CJS require() calls.
    // The driver's resolution pass detects untyped JS modules (TS7016) but the
    // checker's module-not-found path skips them because the module DID resolve.
    // For CJS require() calls (not import declarations), we emit TS7016 directly.
    let per_file_ts7016_diagnostics: Vec<Vec<Diagnostic>> = {
        let mut result: Vec<Vec<Diagnostic>> = Vec::with_capacity(program.files.len());
        for (file_idx, file) in program.files.iter().enumerate() {
            let mut diags = Vec::new();
            for (specifier, spec_node, import_kind, _) in &cached_module_specifiers[file_idx] {
