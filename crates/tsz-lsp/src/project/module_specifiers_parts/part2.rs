impl Project {
    fn package_specifier_from_package_exports_value(
        &self,
        normalized_target: &str,
        package_specifier: &str,
        package_dir: &str,
        exports_value: &serde_json::Value,
        exports_mode: ExportsResolutionMode,
    ) -> Option<String> {
        let package_dir_prefix = format!("{package_dir}/");
        let target_relative_with_ext = normalized_target.strip_prefix(&package_dir_prefix)?;
        let target_runtime_extension = runtime_extension_for_source_path(target_relative_with_ext);
        let target_relative =
            path_to_string(&strip_js_ts_extension(Path::new(target_relative_with_ext)))
                .replace('\\', "/");

        if let Some(exports_target) = exports_value.as_str() {
            let target_pattern = path_to_string(&strip_js_ts_extension(Path::new(exports_target)))
                .replace('\\', "/");
            let target_pattern = target_pattern.strip_prefix("./").unwrap_or(&target_pattern);
            if wildcard_capture_case_insensitive(target_pattern, &target_relative).is_some() {
                return Some(package_specifier.to_string());
            }
            return None;
        }
        let exports_object = exports_value.as_object()?;

        // When no key starts with "./" and no key is exactly ".", the whole
        // object is treated as a top-level conditions map for the "." export.
        let has_subpath_entry = exports_object
            .keys()
            .any(|key| key == "." || key.starts_with("./"));
        if !has_subpath_entry {
            let (type_targets, default_targets) =
                collect_exports_targets(exports_value, exports_mode);
            for target_pattern in type_targets.iter().chain(default_targets.iter()) {
                let target_pattern = target_pattern.replace('\\', "/");
                let target_pattern = target_pattern.strip_prefix("./").unwrap_or(&target_pattern);
                let target_pattern =
                    path_to_string(&strip_js_ts_extension(Path::new(target_pattern)))
                        .replace('\\', "/");
                if wildcard_capture_case_insensitive(&target_pattern, &target_relative).is_some() {
                    return Some(package_specifier.to_string());
                }
            }
            return None;
        }

        for (export_key, export_target) in exports_object {
            let key_pattern = if export_key == "." {
                ""
            } else if let Some(rest) = export_key.strip_prefix("./") {
                rest
            } else {
                continue;
            };

            let (type_targets, default_targets) =
                collect_exports_targets(export_target, exports_mode);
            let should_append_js = key_pattern.contains('*')
                && !has_source_extension(key_pattern)
                && default_targets
                    .iter()
                    .any(|target| !has_source_extension(target));
            // If the exports key explicitly spells an extension (e.g.
            // `./b/*.js`), only files whose runtime extension matches that
            // extension should resolve through this entry. This prevents
            // `.mts`/`.cts` source files from being routed through a `.js`-
            // only wildcard, matching Node's resolution semantics.
            let required_runtime_ext = if key_pattern.ends_with(".js") {
                Some(".js")
            } else if key_pattern.ends_with(".mjs") {
                Some(".mjs")
            } else if key_pattern.ends_with(".cjs") {
                Some(".cjs")
            } else {
                None
            };

            for target_pattern in type_targets.iter().chain(default_targets.iter()) {
                let target_pattern = target_pattern.replace('\\', "/");
                let target_pattern = target_pattern.strip_prefix("./").unwrap_or(&target_pattern);
                let target_pattern =
                    path_to_string(&strip_js_ts_extension(Path::new(target_pattern)))
                        .replace('\\', "/");

                let Some(capture) =
                    wildcard_capture_case_insensitive(&target_pattern, &target_relative)
                else {
                    continue;
                };

                if let Some(required_ext) = required_runtime_ext
                    && target_runtime_extension != required_ext
                {
                    continue;
                }

                if export_key == "." {
                    return Some(package_specifier.to_string());
                }

                let mut subpath = apply_wildcard_capture(key_pattern, &capture)?;
                if should_append_js && !has_source_extension(&subpath) {
                    subpath.push_str(target_runtime_extension);
                }
                if subpath.is_empty() {
                    return Some(package_specifier.to_string());
                }
                return Some(format!("{package_specifier}/{subpath}"));
            }
        }

        None
    }
}
