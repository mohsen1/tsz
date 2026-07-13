use anyhow::{Context, Result};

use crate::checker::diagnostics::Diagnostic;
use tsz_common::diagnostics::data::{diagnostic_codes, diagnostic_messages};
use tsz_common::diagnostics::format_message;

use super::*;

pub fn parse_tsconfig(source: &str) -> Result<TsConfig> {
    let normalized = normalize_jsonc(source);
    let config = serde_json::from_str(&normalized).context("failed to parse tsconfig JSON")?;
    Ok(config)
}

/// Result of parsing a tsconfig.json with diagnostic collection.
pub struct ParsedTsConfig {
    pub config: TsConfig,
    pub diagnostics: Vec<Diagnostic>,
}

/// Parse tsconfig.json source and collect diagnostics for unknown compiler options.
///
/// Unlike `parse_tsconfig`, this function:
/// 1. Detects unknown/miscased compiler option keys in the JSON
/// 2. Normalizes them to canonical casing so serde can deserialize them
/// 3. Returns TS5025 when a spelling suggestion exists, otherwise TS5023
pub fn parse_tsconfig_with_diagnostics(source: &str, file_path: &str) -> Result<ParsedTsConfig> {
    let stripped = strip_jsonc(source);
    let normalized = remove_trailing_commas(&stripped);
    let mut raw: serde_json::Value =
        serde_json::from_str(&normalized).context("failed to parse tsconfig JSON")?;

    let mut diagnostics = Vec::new();
    // Track options that had TS5024 type errors — defaults should not be applied for these.
    let mut ts5024_keys_outer: Vec<String> = Vec::new();

    // Check compiler options for unknown/miscased keys
    if let Some(obj) = raw.as_object_mut()
        && let Some(serde_json::Value::Object(compiler_opts)) = obj.get_mut("compilerOptions")
    {
        let keys: Vec<String> = compiler_opts.keys().cloned().collect();
        let mut renames: Vec<(String, String)> = Vec::new();
        let mut unknown_keys: Vec<String> = Vec::new();

        for key in &keys {
            let key_lower = key.to_lowercase();
            if let Some(canonical) = known_compiler_option(&key_lower) {
                if key.as_str() != canonical {
                    // Miscased option — emit TS5025 and schedule rename
                    let msg = format_message(
                        diagnostic_messages::UNKNOWN_COMPILER_OPTION_DID_YOU_MEAN,
                        &[key, canonical],
                    );
                    push_key_diagnostic(
                        &mut diagnostics,
                        file_path,
                        &stripped,
                        key,
                        msg,
                        diagnostic_codes::UNKNOWN_COMPILER_OPTION_DID_YOU_MEAN,
                    );
                    renames.push((key.clone(), canonical.to_string()));
                }
                // else: exact match, no diagnostic needed
            } else {
                if compiler_opts
                    .get(key)
                    .is_some_and(serde_json::Value::is_null)
                {
                    unknown_keys.push(key.clone());
                    continue;
                }
                // Truly unknown option — emit TS5023
                let suggestion = if TS7_DROPPED_COMPILER_OPTIONS.contains(&key_lower.as_str()) {
                    None
                } else {
                    unknown_compiler_option_suggestion(&key_lower)
                };
                if let Some(suggestion) = suggestion {
                    let msg = format_message(
                        diagnostic_messages::UNKNOWN_COMPILER_OPTION_DID_YOU_MEAN,
                        &[key, suggestion],
                    );
                    push_key_diagnostic(
                        &mut diagnostics,
                        file_path,
                        &stripped,
                        key,
                        msg,
                        diagnostic_codes::UNKNOWN_COMPILER_OPTION_DID_YOU_MEAN,
                    );
                } else {
                    let msg = format_message(diagnostic_messages::UNKNOWN_COMPILER_OPTION, &[key]);
                    push_key_diagnostic(
                        &mut diagnostics,
                        file_path,
                        &stripped,
                        key,
                        msg,
                        diagnostic_codes::UNKNOWN_COMPILER_OPTION,
                    );
                }
                unknown_keys.push(key.clone());
            }
        }

        // Remove unknown keys before serde deserialization so tsz-only struct
        // fields cannot take effect from a tsc-incompatible tsconfig option.
        for key in unknown_keys {
            compiler_opts.remove(&key);
        }

        // Rename miscased keys to canonical casing so serde can deserialize them
        for (old_key, new_key) in renames {
            if let Some(value) = compiler_opts.remove(&old_key) {
                compiler_opts.insert(new_key, value);
            }
        }

        // Check for command-line-only options (TS6266)
        // These options are only valid when passed via the CLI, not in tsconfig.json.
        let cli_only_options: &[&str] = &["listFilesOnly"];
        let mut cli_only_keys: Vec<String> = Vec::new();
        for key in compiler_opts.keys().cloned().collect::<Vec<_>>() {
            if cli_only_options.contains(&key.as_str()) {
                let msg = format_message(
                    diagnostic_messages::OPTION_CAN_ONLY_BE_SPECIFIED_ON_COMMAND_LINE,
                    &[&key],
                );
                push_key_diagnostic(
                    &mut diagnostics,
                    file_path,
                    &stripped,
                    &key,
                    msg,
                    diagnostic_codes::OPTION_CAN_ONLY_BE_SPECIFIED_ON_COMMAND_LINE,
                );
                cli_only_keys.push(key);
            }
        }
        for key in &cli_only_keys {
            compiler_opts.remove(key);
        }

        // Check compiler option value types (TS5024)
        // Collect keys that have type mismatches so we can remove them after iteration.
        // Track TS5024 keys so invalid values stay unavailable to later
        // option-combination and removal validation.
        let keys_after_rename: Vec<String> = compiler_opts.keys().cloned().collect();
        let mut bad_keys: Vec<String> = Vec::new();
        let mut ts5024_keys: Vec<String> = Vec::new();
        for key in &keys_after_rename {
            let expected_type = compiler_option_expected_type(key);
            if expected_type.is_empty() {
                continue; // Unknown option or no type constraint
            }
            let Some(value) = compiler_opts.get(key) else {
                continue;
            };
            let type_ok = value.is_null()
                || match expected_type {
                    "boolean" => value.is_boolean(),
                    "string" => value.is_string(),
                    "number" => value.is_number(),
                    "Array" => value.is_array(),
                    "string or Array" => value.is_string() || value.is_array(),
                    "object" => value.is_object(),
                    _ => true,
                };
            if !type_ok {
                let start = find_value_offset_in_source(&stripped, key);
                let value_len = estimate_json_value_len(value);
                let msg = format_message(
                    diagnostic_messages::COMPILER_OPTION_REQUIRES_A_VALUE_OF_TYPE,
                    &[key, expected_type],
                );
                diagnostics.push(Diagnostic::error(
                    file_path,
                    start,
                    value_len,
                    msg,
                    diagnostic_codes::COMPILER_OPTION_REQUIRES_A_VALUE_OF_TYPE,
                ));
                // Track invalidated keys for the later validation phases.
                ts5024_keys.push(key.clone());
                // tsc emits TS5024 and does NOT apply the value (convertJsonOption
                // returns undefined for type mismatches), so remove invalidly-typed
                // values from the config object before deserialization.
                bad_keys.push(key.clone());
            }
        }
        // Remove invalid values so serde defaults them to None
        for key in &bad_keys {
            compiler_opts.remove(key);
        }

        // Check ignoreDeprecations value (TS5103)
        // TypeScript 7 still accepts the historical "5.0" and "6.0"
        // literals, but neither suppresses TS7 removal diagnostics.
        if let Some(serde_json::Value::String(id_value)) = compiler_opts.get("ignoreDeprecations")
            && id_value != "5.0"
            && id_value != "6.0"
        {
            let start = find_value_offset_in_source(&stripped, "ignoreDeprecations");
            let value_len = id_value.len() as u32 + 2; // include quotes
            diagnostics.push(Diagnostic::error(
                file_path,
                start,
                value_len,
                diagnostic_messages::INVALID_VALUE_FOR_IGNOREDEPRECATIONS.to_string(),
                diagnostic_codes::INVALID_VALUE_FOR_IGNOREDEPRECATIONS,
            ));
        }

        // TypeScript 7 turns the complete 6.0 deprecation wave into
        // unsuppressible removals. Keep the policy in deprecation_helpers so
        // config and direct CLI validation share aliases and display values.
        for notice in deprecation_helpers::removed_option_notices_from_json(compiler_opts) {
            let key = notice.key();
            let start = if notice.is_value() {
                find_value_offset_in_source(&stripped, key)
            } else {
                find_key_offset_in_source(&stripped, key)
            };
            let length = if notice.is_value() {
                compiler_opts.get(key).map_or(0, estimate_json_value_len)
            } else {
                key.len() as u32 + 2
            };
            diagnostics.push(Diagnostic::error(
                file_path,
                start,
                length,
                notice.message(),
                notice.code(),
            ));
        }

        // Check command-line-only options in tsconfig (TS6266)
        // Some options like `listFilesOnly` can only be specified on the command line,
        // not in tsconfig.json. tsc emits TS6266 for these.
        let command_line_only_options = ["listFilesOnly"];
        for key in &command_line_only_options {
            if compiler_opts.contains_key(*key) {
                let msg = format_message(
                    diagnostic_messages::OPTION_CAN_ONLY_BE_SPECIFIED_ON_COMMAND_LINE,
                    &[key],
                );
                push_key_diagnostic(
                    &mut diagnostics,
                    file_path,
                    &stripped,
                    key,
                    msg,
                    diagnostic_codes::OPTION_CAN_ONLY_BE_SPECIFIED_ON_COMMAND_LINE,
                );
                // Remove the option so it doesn't affect compilation
                compiler_opts.remove(*key);
            }
        }

        // Validate `module` before compatibility checks. TypeScript 7 rejects
        // removed values such as `none` with TS6046 and does not derive
        // follow-on diagnostics from that invalid value.
        validate_option_value(
            compiler_opts,
            "module",
            &stripped,
            file_path,
            VALID_MODULE_VALUES,
            "--module",
            VALID_MODULE_DISPLAY,
            &mut diagnostics,
        );

        // Check moduleResolution/module compatibility (TS5095)
        // `moduleResolution: "bundler"` requires `module` to be "preserve" or ES2015+.
        // In TS7 `bundler` is also the default resolution, so the check applies
        // when the option is absent; tsc then anchors the diagnostic at the
        // "compilerOptions" key instead of an explicit option value.
        {
            let mr_explicit = if let Some(serde_json::Value::String(mr_value)) =
                compiler_opts.get("moduleResolution")
            {
                Some((
                    normalize_enum_option_value(mr_value.split(',').next().unwrap_or(mr_value)),
                    mr_value.len() as u32,
                ))
            } else {
                None
            };
            let bundler_effective = match &mr_explicit {
                Some((mr_normalized, _)) => mr_normalized == "bundler",
                // moduleResolution not set — TS7 defaults to bundler. Module
                // kinds that imply their own resolution (node16/nodenext/...)
                // are all in the compatible list below, so treating the
                // default as bundler cannot misfire for them.
                None => true,
            };
            if bundler_effective {
                let module_ok = if let Some(serde_json::Value::String(mod_value)) =
                    compiler_opts.get("module")
                {
                    let mod_normalized = normalize_enum_option_value(
                        mod_value.split(',').next().unwrap_or(mod_value),
                    );
                    // tsc message: "can only be used when 'module' is set to 'preserve',
                    // 'commonjs', or 'es2015' or later" — commonjs IS valid.
                    // AMD, UMD, System, None are the invalid values.
                    matches!(
                        mod_normalized.as_str(),
                        "preserve"
                            | "commonjs"
                            | "es2015"
                            | "es6"
                            | "es2020"
                            | "es2022"
                            | "esnext"
                            | "node16"
                            | "node18"
                            | "node20"
                            | "nodenext"
                    )
                } else {
                    // module not set — default depends on target.
                    // ES2015+ targets default to es2015 (compatible); lower targets
                    // default to commonjs, which is also compatible with bundler.
                    true
                };
                if !module_ok {
                    // Explicit option → point at its value; defaulted option →
                    // point at the "compilerOptions" key (matching tsc).
                    let (start, value_len) = if let Some((_, raw_len)) = mr_explicit {
                        (
                            find_value_offset_in_source(&stripped, "moduleResolution"),
                            raw_len + 2, // include quotes
                        )
                    } else {
                        let search = "\"compilerOptions\"";
                        let s = stripped.find(search).map_or(0, |p| p as u32);
                        (s, search.len() as u32)
                    };
                    let msg = "Option 'bundler' can only be used when 'module' is set to 'preserve', 'commonjs', or 'es2015' or later.".to_string();
                    diagnostics.push(Diagnostic::error(
                        file_path,
                        start,
                        value_len,
                        msg,
                        diagnostic_codes::OPTION_CAN_ONLY_BE_USED_WHEN_MODULE_IS_SET_TO_PRESERVE_COMMONJS_OR_ES2015_OR_LAT,
                    ));
                }
            }
        }

        // Check moduleResolution/module compatibility (TS5110)
        // When moduleResolution is node16/nodenext, module must also be node16/nodenext.
        if let Some(serde_json::Value::String(mr_value)) = compiler_opts.get("moduleResolution") {
            let mr_normalized =
                normalize_enum_option_value(mr_value.split(',').next().unwrap_or(mr_value));
            let is_node_mr = matches!(
                mr_normalized.as_str(),
                "node16" | "node18" | "node20" | "nodenext"
            );
            if is_node_mr {
                let module_ok = if let Some(serde_json::Value::String(mod_value)) =
                    compiler_opts.get("module")
                {
                    let mod_normalized = normalize_enum_option_value(
                        mod_value.split(',').next().unwrap_or(mod_value),
                    );
                    matches!(
                        mod_normalized.as_str(),
                        "node16" | "node18" | "node20" | "nodenext"
                    )
                } else {
                    false // module not explicitly set → tsc requires it to be set explicitly
                };
                if !module_ok {
                    // When module is explicitly set to a wrong value, point at
                    // its value; when module is not set at all, point at
                    // "compilerOptions" key (matching tsc behavior).
                    let (start, value_len) = if compiler_opts.contains_key("module") {
                        let s = find_value_offset_in_source(&stripped, "module");
                        let vl = compiler_opts
                            .get("module")
                            .and_then(|v| v.as_str())
                            .map_or(0, |sv| sv.len() as u32 + 2);
                        (s, vl)
                    } else {
                        // Point at "compilerOptions" key — search from start
                        let search = "\"compilerOptions\"";
                        let s = stripped.find(search).map_or(0, |p| p as u32);
                        let vl = search.len() as u32;
                        (s, vl)
                    };
                    // tsc uses PascalCase for the option values in the message
                    let mr_display = match mr_normalized.as_str() {
                        "node16" => "Node16",
                        "node18" => "Node18",
                        "node20" => "Node20",
                        "nodenext" => "NodeNext",
                        _ => &mr_normalized,
                    };
                    let msg = format_message(
                        diagnostic_messages::OPTION_MODULE_MUST_BE_SET_TO_WHEN_OPTION_MODULERESOLUTION_IS_SET_TO,
                        &[mr_display, mr_display],
                    );
                    diagnostics.push(Diagnostic::error(
                        file_path,
                        start,
                        value_len,
                        msg,
                        diagnostic_codes::OPTION_MODULE_MUST_BE_SET_TO_WHEN_OPTION_MODULERESOLUTION_IS_SET_TO,
                    ));
                }
            }
        }

        // TS5109: moduleResolution must match module for node16/nodenext modules.
        // When module is node16/nodenext, moduleResolution must be the same (or left unspecified).
        if let Some(serde_json::Value::String(mod_value)) = compiler_opts.get("module") {
            let mod_normalized =
                normalize_enum_option_value(mod_value.split(',').next().unwrap_or(mod_value));
            let is_node_module = matches!(
                mod_normalized.as_str(),
                "node16" | "node18" | "node20" | "nodenext"
            );
            if is_node_module
                && let Some(serde_json::Value::String(mr_value)) =
                    compiler_opts.get("moduleResolution")
            {
                let mr_normalized =
                    normalize_enum_option_value(mr_value.split(',').next().unwrap_or(mr_value));
                let mr_ok = matches!(
                    mr_normalized.as_str(),
                    "node16" | "node18" | "node20" | "nodenext"
                );
                if !mr_ok {
                    let start = find_value_offset_in_source(&stripped, "moduleResolution");
                    let value_len = mr_value.len() as u32 + 2;
                    let mod_display = match mod_normalized.as_str() {
                        "node16" => "Node16",
                        "node18" => "Node18",
                        "node20" => "Node20",
                        "nodenext" => "NodeNext",
                        _ => &mod_normalized,
                    };
                    // There is no node18/node20 moduleResolution: the required
                    // resolution arg is 'Node16' for every node1x module kind
                    // and 'NodeNext' for nodenext.
                    let required_resolution = match mod_normalized.as_str() {
                        "nodenext" => "NodeNext",
                        _ => "Node16",
                    };
                    let msg = format_message(
                        diagnostic_messages::OPTION_MODULERESOLUTION_MUST_BE_SET_TO_OR_LEFT_UNSPECIFIED_WHEN_OPTION_MODULE_IS,
                        &[required_resolution, mod_display],
                    );
                    diagnostics.push(Diagnostic::error(
                        file_path,
                        start,
                        value_len,
                        msg,
                        diagnostic_codes::OPTION_MODULERESOLUTION_MUST_BE_SET_TO_OR_LEFT_UNSPECIFIED_WHEN_OPTION_MODULE_IS,
                    ));
                }
            }
        }

        // TS5095: moduleResolution: bundler can only be used when module is
        // preserve, commonjs, or es2015+.
        if let Some(serde_json::Value::String(mr_value)) = compiler_opts.get("moduleResolution") {
            let mr_normalized =
                normalize_enum_option_value(mr_value.split(',').next().unwrap_or(mr_value));
            if mr_normalized == "bundler" {
                let module_ok = if let Some(serde_json::Value::String(mod_value)) =
                    compiler_opts.get("module")
                {
                    let mod_normalized = normalize_enum_option_value(
                        mod_value.split(',').next().unwrap_or(mod_value),
                    );
                    // bundler is incompatible with node16/nodenext module kinds
                    !matches!(
                        mod_normalized.as_str(),
                        "node16" | "node18" | "node20" | "nodenext"
                    )
                } else {
                    true // module not set → tsc defaults it, which is valid
                };
                if !module_ok {
                    let start = find_value_offset_in_source(&stripped, "moduleResolution");
                    let value_len = mr_value.len() as u32 + 2;
                    let msg = format_message(
                        diagnostic_messages::OPTION_CAN_ONLY_BE_USED_WHEN_MODULE_IS_SET_TO_PRESERVE_COMMONJS_OR_ES2015_OR_LAT,
                        &["bundler"],
                    );
                    diagnostics.push(Diagnostic::error(
                        file_path,
                        start,
                        value_len,
                        msg,
                        diagnostic_codes::OPTION_CAN_ONLY_BE_USED_WHEN_MODULE_IS_SET_TO_PRESERVE_COMMONJS_OR_ES2015_OR_LAT,
                    ));
                }
            }
        }

        // TS6082: Only 'amd' and 'system' modules are supported alongside --outFile.
        // When outFile is set with a non-amd/system module, emit at both the module and outFile keys.
        if let Some(serde_json::Value::String(out_file_value)) = compiler_opts.get("outFile")
            && !out_file_value.is_empty()
            && !option_is_effectively_enabled(compiler_opts, &ts5024_keys, "emitDeclarationOnly")
            && let Some(serde_json::Value::String(mod_value)) = compiler_opts.get("module")
        {
            let mod_normalized =
                normalize_enum_option_value(mod_value.split(',').next().unwrap_or(mod_value));
            // `module=none` means no module system; tsc does not report TS6082 for it.
            if !matches!(mod_normalized.as_str(), "amd" | "system" | "none") {
                let msg = format_message(
                    diagnostic_messages::ONLY_AMD_AND_SYSTEM_MODULES_ARE_SUPPORTED_ALONGSIDE,
                    &["outFile"],
                );
                // Emit at the "module" key (matching tsc behavior)
                push_key_diagnostic(
                    &mut diagnostics,
                    file_path,
                    &stripped,
                    "module",
                    msg.clone(),
                    diagnostic_codes::ONLY_AMD_AND_SYSTEM_MODULES_ARE_SUPPORTED_ALONGSIDE,
                );
                // Emit at the "outFile" key (matching tsc behavior)
                push_key_diagnostic(
                    &mut diagnostics,
                    file_path,
                    &stripped,
                    "outFile",
                    msg,
                    diagnostic_codes::ONLY_AMD_AND_SYSTEM_MODULES_ARE_SUPPORTED_ALONGSIDE,
                );
            }
        }

        // TS5105: Option 'verbatimModuleSyntax' cannot be used when 'module' is set to 'UMD', 'AMD', or 'System'.
        if option_is_effectively_enabled(compiler_opts, &ts5024_keys, "verbatimModuleSyntax") {
            let module_bad = if let Some(serde_json::Value::String(mod_value)) =
                compiler_opts.get("module")
            {
                let mod_normalized =
                    normalize_enum_option_value(mod_value.split(',').next().unwrap_or(mod_value));
                matches!(mod_normalized.as_str(), "umd" | "amd" | "system")
            } else {
                false
            };
            if module_bad {
                push_key_diagnostic(
                    &mut diagnostics,
                    file_path,
                    &stripped,
                    "verbatimModuleSyntax",
                    diagnostic_messages::OPTION_VERBATIMMODULESYNTAX_CANNOT_BE_USED_WHEN_MODULE_IS_SET_TO_UMD_AMD_OR_SYST,
                    diagnostic_codes::OPTION_VERBATIMMODULESYNTAX_CANNOT_BE_USED_WHEN_MODULE_IS_SET_TO_UMD_AMD_OR_SYST,
                );
            }
        }

        // TS5069: Option '{0}' cannot be specified without specifying option '{1}' or option '{2}'.
        // Group 1: options that require 'declaration' or 'composite'
        let requires_decl_or_composite: &[&str] = &[
            "emitDeclarationOnly",
            "declarationMap",
            "isolatedDeclarations",
        ];
        for &opt in requires_decl_or_composite {
            let declaration_enabled =
                option_is_effectively_enabled(compiler_opts, &ts5024_keys, "declaration");
            let composite_enabled =
                option_is_effectively_enabled(compiler_opts, &ts5024_keys, "composite");
            if option_is_truthy(compiler_opts.get(opt))
                && !declaration_enabled
                && !composite_enabled
            {
                let msg = format_message(
                    diagnostic_messages::OPTION_CANNOT_BE_SPECIFIED_WITHOUT_SPECIFYING_OPTION_OR_OPTION,
                    &[opt, "declaration", "composite"],
                );
                let code =
                    diagnostic_codes::OPTION_CANNOT_BE_SPECIFIED_WITHOUT_SPECIFYING_OPTION_OR_OPTION;
                let mut related_keys = vec![opt];
                if option_key_present_or_invalidated(compiler_opts, &ts5024_keys, "declaration") {
                    related_keys.push("declaration");
                }
                if option_key_present_or_invalidated(compiler_opts, &ts5024_keys, "composite") {
                    related_keys.push("composite");
                }
                for key in related_keys {
                    push_key_diagnostic(
                        &mut diagnostics,
                        file_path,
                        &stripped,
                        key,
                        msg.clone(),
                        code,
                    );
                }
            }
        }

        // TS5096: allowImportingTsExtensions is only valid in no-emit modes
        // or when imports are rewritten before emit.
        if option_is_effectively_enabled(compiler_opts, &ts5024_keys, "allowImportingTsExtensions")
            && !option_is_effectively_enabled(compiler_opts, &ts5024_keys, "noEmit")
            && !option_is_effectively_enabled(compiler_opts, &ts5024_keys, "emitDeclarationOnly")
            && !option_is_effectively_enabled(
                compiler_opts,
                &ts5024_keys,
                "rewriteRelativeImportExtensions",
            )
        {
            let start = find_value_offset_in_source(&stripped, "allowImportingTsExtensions");
            let value_len = compiler_opts
                .get("allowImportingTsExtensions")
                .map_or(4, estimate_json_value_len);
            diagnostics.push(Diagnostic::error(
                file_path,
                start,
                value_len,
                diagnostic_messages::OPTION_ALLOWIMPORTINGTSEXTENSIONS_CAN_ONLY_BE_USED_WHEN_ONE_OF_NOEMIT_EMITDECLAR
                    .to_string(),
                diagnostic_codes::OPTION_ALLOWIMPORTINGTSEXTENSIONS_CAN_ONLY_BE_USED_WHEN_ONE_OF_NOEMIT_EMITDECLAR,
            ));
        }

        // Group 2: mapRoot requires 'sourceMap' or 'declarationMap'
        if compiler_opts.contains_key("mapRoot")
            && !option_is_effectively_enabled(compiler_opts, &ts5024_keys, "sourceMap")
            && !option_is_effectively_enabled(compiler_opts, &ts5024_keys, "declarationMap")
        {
            let msg = format_message(
                diagnostic_messages::OPTION_CANNOT_BE_SPECIFIED_WITHOUT_SPECIFYING_OPTION_OR_OPTION,
                &["mapRoot", "sourceMap", "declarationMap"],
            );
            push_key_diagnostic(
                &mut diagnostics,
                file_path,
                &stripped,
                "mapRoot",
                msg,
                diagnostic_codes::OPTION_CANNOT_BE_SPECIFIED_WITHOUT_SPECIFYING_OPTION_OR_OPTION,
            );
        }

        // TS5091: preserveConstEnums cannot be disabled when isolatedModules is enabled.
        // tsc emits this at both key positions; we emit once per enabler.
        if matches!(
            compiler_opts.get("preserveConstEnums"),
            Some(serde_json::Value::Bool(false))
        ) {
            let enablers: &[&str] = &["isolatedModules", "isolatedDeclarations"];
            for enabler in enablers {
                if option_is_effectively_enabled(compiler_opts, &ts5024_keys, enabler) {
                    let msg = format_message(
                        diagnostic_messages::OPTION_PRESERVECONSTENUMS_CANNOT_BE_DISABLED_WHEN_IS_ENABLED,
                        &[enabler],
                    );
                    push_key_diagnostic(
                        &mut diagnostics,
                        file_path,
                        &stripped,
                        "preserveConstEnums",
                        msg.clone(),
                        diagnostic_codes::OPTION_PRESERVECONSTENUMS_CANNOT_BE_DISABLED_WHEN_IS_ENABLED,
                    );
                    // tsc also emits at the enabler key position
                    push_key_diagnostic(
                        &mut diagnostics,
                        file_path,
                        &stripped,
                        enabler,
                        msg,
                        diagnostic_codes::OPTION_PRESERVECONSTENUMS_CANNOT_BE_DISABLED_WHEN_IS_ENABLED,
                    );
                }
            }
        }

        // TS6304: Composite projects may not disable declaration emit.
        // When composite: true, declaration must not be explicitly false.
        if option_is_effectively_enabled(compiler_opts, &ts5024_keys, "composite")
            && matches!(
                compiler_opts.get("declaration"),
                Some(serde_json::Value::Bool(false))
            )
        {
            push_key_diagnostic(
                &mut diagnostics,
                file_path,
                &stripped,
                "declaration",
                diagnostic_messages::COMPOSITE_PROJECTS_MAY_NOT_DISABLE_DECLARATION_EMIT,
                diagnostic_codes::COMPOSITE_PROJECTS_MAY_NOT_DISABLE_DECLARATION_EMIT,
            );
        }

        // TS6379: Composite projects may not disable incremental compilation.
        // When composite: true, incremental must not be explicitly false.
        // tsc anchors the error at the `compilerOptions` key itself (the
        // enclosing block that contains both interacting options), rather
        // than at `composite` or `incremental`.
        if option_is_effectively_enabled(compiler_opts, &ts5024_keys, "composite")
            && matches!(
                compiler_opts.get("incremental"),
                Some(serde_json::Value::Bool(false))
            )
        {
            let search = "\"compilerOptions\"";
            let start = stripped.find(search).map(|p| p as u32).unwrap_or(0);
            let key_len = search.len() as u32;
            diagnostics.push(Diagnostic::error(
                file_path,
                start,
                key_len,
                diagnostic_messages::COMPOSITE_PROJECTS_MAY_NOT_DISABLE_INCREMENTAL_COMPILATION
                    .to_string(),
                diagnostic_codes::COMPOSITE_PROJECTS_MAY_NOT_DISABLE_INCREMENTAL_COMPILATION,
            ));
        }

        // TS5052: Option '{0}' cannot be specified without specifying option '{1}'.
        // `checkJs` implies `allowJs` unless `allowJs` is explicitly disabled.
        if option_is_truthy(compiler_opts.get("checkJs"))
            && !option_is_effectively_enabled(compiler_opts, &ts5024_keys, "allowJs")
            && option_key_present_or_invalidated(compiler_opts, &ts5024_keys, "allowJs")
        {
            let msg = format_message(
                diagnostic_messages::OPTION_CANNOT_BE_SPECIFIED_WITHOUT_SPECIFYING_OPTION,
                &["checkJs", "allowJs"],
            );

            // Always emit at the checkJs key.
            push_key_diagnostic(
                &mut diagnostics,
                file_path,
                &stripped,
                "checkJs",
                msg.clone(),
                diagnostic_codes::OPTION_CANNOT_BE_SPECIFIED_WITHOUT_SPECIFYING_OPTION,
            );

            // If allowJs is explicitly present, emit at allowJs too (tsc parity).
            if compiler_opts.contains_key("allowJs") {
                push_key_diagnostic(
                    &mut diagnostics,
                    file_path,
                    &stripped,
                    "allowJs",
                    msg,
                    diagnostic_codes::OPTION_CANNOT_BE_SPECIFIED_WITHOUT_SPECIFYING_OPTION,
                );
            }
        }

        // TS5052: emitDecoratorMetadata requires experimentalDecorators.
        if option_is_truthy(compiler_opts.get("emitDecoratorMetadata"))
            && !option_is_effectively_enabled(compiler_opts, &ts5024_keys, "experimentalDecorators")
        {
            let msg = format_message(
                diagnostic_messages::OPTION_CANNOT_BE_SPECIFIED_WITHOUT_SPECIFYING_OPTION,
                &["emitDecoratorMetadata", "experimentalDecorators"],
            );
            push_key_diagnostic(
                &mut diagnostics,
                file_path,
                &stripped,
                "emitDecoratorMetadata",
                msg,
                diagnostic_codes::OPTION_CANNOT_BE_SPECIFIED_WITHOUT_SPECIFYING_OPTION,
            );
        }

        // TS5053: Option '{0}' cannot be specified with option '{1}'.
        // tsc emits for each conflicting key, pointing at the key's position.
        // The message always names the pair (A, B) regardless of which key is pointed at.
        let conflicting_pairs: &[(&str, &str)] = &[
            ("sourceMap", "inlineSourceMap"),
            ("mapRoot", "inlineSourceMap"),
            ("reactNamespace", "jsxFactory"),
            ("allowJs", "isolatedDeclarations"),
        ];
        // Issue #3732: tsc resolves `checkJs: true` (when `allowJs` is not
        // explicitly disabled) to an implied `allowJs: true` and still
        // emits TS5053 for the (allowJs, isolatedDeclarations) conflict.
        // Mirror that implication so the conflict pair fires even when
        // only `checkJs` is in the config.
        let allow_js_present = compiler_opts.contains_key("allowJs");
        let allow_js_implied_by_check_js =
            !allow_js_present && option_is_truthy(compiler_opts.get("checkJs"));
        let option_is_set_with_check_js_implication = |opt: &str| -> bool {
            if option_is_truthy(compiler_opts.get(opt)) {
                return true;
            }
            opt == "allowJs" && allow_js_implied_by_check_js
        };
        for &(opt_a, opt_b) in conflicting_pairs {
            if option_is_set_with_check_js_implication(opt_a)
                && option_is_set_with_check_js_implication(opt_b)
            {
                let resolve = |opt: &'static str| -> &'static str {
                    if opt == "allowJs" && allow_js_implied_by_check_js {
                        "checkJs"
                    } else {
                        opt
                    }
                };
                let key_a = resolve(opt_a);
                let key_b = resolve(opt_b);
                // Emit at the resolved-key position (issue #3732 anchors at
                // `checkJs` when allowJs is implied).
                let msg = format_message(
                    diagnostic_messages::OPTION_CANNOT_BE_SPECIFIED_WITH_OPTION,
                    &[opt_a, opt_b],
                );
                push_key_diagnostic(
                    &mut diagnostics,
                    file_path,
                    &stripped,
                    key_a,
                    msg.clone(),
                    diagnostic_codes::OPTION_CANNOT_BE_SPECIFIED_WITH_OPTION,
                );
                // Emit at opt_b's position (same message, different location)
                push_key_diagnostic(
                    &mut diagnostics,
                    file_path,
                    &stripped,
                    key_b,
                    msg,
                    diagnostic_codes::OPTION_CANNOT_BE_SPECIFIED_WITH_OPTION,
                );
            }
        }

        // TS5067: Invalid value for 'jsxFactory' — must be a valid identifier or qualified name.
        // A qualified name is one or more identifiers separated by dots (e.g. React.createElement, h).
        // Spaces, = signs, and other non-identifier characters make the value invalid.
        if let Some(serde_json::Value::String(jsx_factory_val)) = compiler_opts.get("jsxFactory")
            && !is_valid_identifier_or_qualified_name(jsx_factory_val)
        {
            let start = find_value_offset_in_source(&stripped, "jsxFactory");
            let msg = format_message(
                diagnostic_messages::INVALID_VALUE_FOR_JSXFACTORY_IS_NOT_A_VALID_IDENTIFIER_OR_QUALIFIED_NAME,
                &[jsx_factory_val.as_str()],
            );
            diagnostics.push(Diagnostic::error(
                file_path,
                start,
                jsx_factory_val.len() as u32 + 2, // include surrounding quotes
                msg,
                diagnostic_codes::INVALID_VALUE_FOR_JSXFACTORY_IS_NOT_A_VALID_IDENTIFIER_OR_QUALIFIED_NAME,
            ));
        }

        if let Some(serde_json::Value::String(react_namespace_val)) =
            compiler_opts.get("reactNamespace")
            && !is_valid_identifier(react_namespace_val)
        {
            let start = find_value_offset_in_source(&stripped, "reactNamespace");
            let msg = format_message(
                diagnostic_messages::INVALID_VALUE_FOR_REACTNAMESPACE_IS_NOT_A_VALID_IDENTIFIER,
                &[react_namespace_val.as_str()],
            );
            diagnostics.push(Diagnostic::error(
                file_path,
                start,
                react_namespace_val.len() as u32 + 2,
                msg,
                diagnostic_codes::INVALID_VALUE_FOR_REACTNAMESPACE_IS_NOT_A_VALID_IDENTIFIER,
            ));
        }

        // TS5070/TS5071 (`resolveJsonModule` vs classic resolution or
        // none/system/umd modules) are unreachable in TS7: every module or
        // resolution kind that could trigger them is itself removed (TS5108)
        // or invalid (TS6046), and tsc does not layer the resolveJsonModule
        // conflict on top of those reports.

        // TS5098: Option '{0}' can only be used when 'moduleResolution' is set to 'node16', 'nodenext', or 'bundler'.
        let requires_modern_mr: &[&str] = &[
            "resolvePackageJsonExports",
            "resolvePackageJsonImports",
            "customConditions",
        ];
        // Match the defaulting chain `resolve_compiler_options` uses so the
        // pre-resolve TS5098 gate doesn't disagree with the post-resolve
        // option state. tsz's defaults are:
        //   target unset → `default_module_kind_for_target` folds it to
        //                  `LatestStandard`
        //   module unset → `default_module_kind_for_target(target, explicit)`
        //                  (`ES2022` when target is unset)
        //   moduleResolution unset → `default_module_resolution_for_module(module)`
        // and `Bundler` / `Node16` / `NodeNext` all count as "modern". The
        // unset-target module (`ES2022`) still resolves to `Bundler`, so this
        // gate is unaffected by the exact module kind.
        // See https://github.com/tsz-org/tsz/issues/3509.
        let mr_is_modern = if let Some(serde_json::Value::String(mr_value)) =
            compiler_opts.get("moduleResolution")
        {
            let mr_normalized =
                normalize_enum_option_value(mr_value.split(',').next().unwrap_or(mr_value));
            matches!(mr_normalized.as_str(), "node16" | "nodenext" | "bundler")
        } else {
            // Resolve module from explicit value, or fall back through target
            // to the same default `resolve_compiler_options` would compute.
            let module_kind = if let Some(serde_json::Value::String(mod_value)) =
                compiler_opts.get("module")
            {
                let mod_normalized =
                    normalize_enum_option_value(mod_value.split(',').next().unwrap_or(mod_value));
                ModuleKind::from_ts_str(&mod_normalized)
            } else if let Some(serde_json::Value::String(tgt_value)) = compiler_opts.get("target") {
                let tgt_normalized =
                    normalize_enum_option_value(tgt_value.split(',').next().unwrap_or(tgt_value));
                ScriptTarget::from_ts_str(&tgt_normalized)
                    .map(|target| default_module_kind_for_target(target, true))
            } else {
                Some(default_module_kind_for_target(ScriptTarget::ESNext, false))
            };
            module_kind.is_some_and(|module| {
                matches!(
                    default_module_resolution_for_module(module),
                    ModuleResolutionKind::Node16
                        | ModuleResolutionKind::NodeNext
                        | ModuleResolutionKind::Bundler
                )
            })
        };
        if !mr_is_modern {
            for &opt in requires_modern_mr {
                if option_is_truthy(compiler_opts.get(opt)) {
                    let msg = format_message(
                        diagnostic_messages::OPTION_CAN_ONLY_BE_USED_WHEN_MODULERESOLUTION_IS_SET_TO_NODE16_NODENEXT_OR_BUNDL,
                        &[opt],
                    );
                    push_key_diagnostic(
                        &mut diagnostics,
                        file_path,
                        &stripped,
                        opt,
                        msg,
                        diagnostic_codes::OPTION_CAN_ONLY_BE_USED_WHEN_MODULERESOLUTION_IS_SET_TO_NODE16_NODENEXT_OR_BUNDL,
                    );
                }
            }
        }

        // TS6046: Validate option values for target, module, moduleResolution, jsx,
        // moduleDetection, newLine, and lib.
        // If a value is invalid, emit TS6046 and null it out so resolve_compiler_options
        // doesn't see it and bail.
        validate_option_value(
            compiler_opts,
            "target",
            &stripped,
            file_path,
            VALID_TARGET_VALUES,
            "--target",
            VALID_TARGET_DISPLAY,
            &mut diagnostics,
        );
        validate_option_value(
            compiler_opts,
            "moduleResolution",
            &stripped,
            file_path,
            VALID_MODULE_RESOLUTION_VALUES,
            "--moduleResolution",
            VALID_MODULE_RESOLUTION_DISPLAY,
            &mut diagnostics,
        );
        validate_option_value(
            compiler_opts,
            "jsx",
            &stripped,
            file_path,
            VALID_JSX_VALUES,
            "--jsx",
            VALID_JSX_DISPLAY,
            &mut diagnostics,
        );
        validate_option_value(
            compiler_opts,
            "moduleDetection",
            &stripped,
            file_path,
            VALID_MODULE_DETECTION_VALUES,
            "--moduleDetection",
            VALID_MODULE_DETECTION_DISPLAY,
            &mut diagnostics,
        );
        validate_option_value(
            compiler_opts,
            "newLine",
            &stripped,
            file_path,
            VALID_NEW_LINE_VALUES,
            "--newLine",
            VALID_NEW_LINE_DISPLAY,
            &mut diagnostics,
        );
        validate_lib_values(compiler_opts, &stripped, file_path, &mut diagnostics);

        // TS5063/TS5066: Validate paths substitution values.
        // TS5063: value should be an array (not string/number/etc.)
        // TS5066: array shouldn't be empty
        if let Some(serde_json::Value::Object(paths_obj)) = compiler_opts.get_mut("paths") {
            let mut bad_patterns: Vec<String> = Vec::new();
            for (pattern, value) in paths_obj.iter() {
                let search = format!("\"{pattern}\"");
                let paths_start = stripped.find("\"paths\"").unwrap_or(0);
                let key_pos = stripped[paths_start..]
                    .find(&search)
                    .map_or(0, |p| paths_start + p);
                let after_key = key_pos + search.len();
                let rest = &stripped[after_key..];
                let value_start = if let Some(colon_pos) = rest.find(':') {
                    let after_colon = &rest[(colon_pos + 1)..];
                    let ws = after_colon.len() - after_colon.trim_start().len();
                    (after_key + colon_pos + 1 + ws) as u32
                } else {
                    key_pos as u32
                };

                match value {
                    serde_json::Value::Array(arr) if arr.is_empty() => {
                        let msg = format_message(
                            diagnostic_messages::SUBSTITUTIONS_FOR_PATTERN_SHOULDNT_BE_AN_EMPTY_ARRAY,
                            &[pattern],
                        );
                        diagnostics.push(Diagnostic::error(
                            file_path,
                            value_start,
                            2, // "[]"
                            msg,
                            diagnostic_codes::SUBSTITUTIONS_FOR_PATTERN_SHOULDNT_BE_AN_EMPTY_ARRAY,
                        ));
                    }
                    serde_json::Value::Array(arr) => {
                        // TS5064: Substitution elements must be strings
                        for (idx, elem) in arr.iter().enumerate() {
                            if let Some(substitution) = elem.as_str() {
                                if !is_relative_path_mapping_substitution(substitution)
                                    && !is_rooted_path_mapping_substitution(substitution)
                                {
                                    // tsc 7.0.2 rejects every substitution that is neither
                                    // relative nor rooted (empty string included), regardless
                                    // of `baseUrl` — a present `baseUrl` separately gets the
                                    // TS5102 removed-option error but no longer legitimizes
                                    // non-relative substitutions.
                                    let elem_pos = {
                                        let arr_start = stripped[value_start as usize..]
                                            .find('[')
                                            .map_or(value_start as usize, |p| {
                                                value_start as usize + p + 1
                                            });
                                        let mut pos = arr_start;
                                        let mut found = 0;
                                        while found < idx && pos < stripped.len() {
                                            if stripped.as_bytes()[pos] == b',' {
                                                found += 1;
                                            }
                                            pos += 1;
                                        }
                                        while pos < stripped.len()
                                            && stripped.as_bytes()[pos].is_ascii_whitespace()
                                        {
                                            pos += 1;
                                        }
                                        pos as u32
                                    };
                                    let msg = diagnostic_messages::NON_RELATIVE_PATHS_ARE_NOT_ALLOWED_WHEN_BASEURL_IS_NOT_SET_DID_YOU_FORGET_A_LEAD.to_string();
                                    diagnostics.push(Diagnostic::error(
                                        file_path,
                                        elem_pos,
                                        estimate_json_value_len(elem),
                                        msg,
                                        diagnostic_codes::NON_RELATIVE_PATHS_ARE_NOT_ALLOWED_WHEN_BASEURL_IS_NOT_SET_DID_YOU_FORGET_A_LEAD,
                                    ));
                                }
                            } else {
                                let type_name = match elem {
                                    serde_json::Value::Number(_) => "number",
                                    serde_json::Value::Bool(_) => "boolean",
                                    serde_json::Value::Null => "null",
                                    serde_json::Value::Object(_) => "object",
                                    serde_json::Value::Array(_) => "Array",
                                    _ => "unknown",
                                };
                                let elem_display = match elem {
                                    serde_json::Value::Number(n) => n.to_string(),
                                    serde_json::Value::Bool(b) => b.to_string(),
                                    serde_json::Value::Null => "null".to_string(),
                                    _ => format!("{elem}"),
                                };
                                // Find the position of the element in the source text
                                let elem_pos = {
                                    let arr_start = stripped[value_start as usize..]
                                        .find('[')
                                        .map_or(value_start as usize, |p| {
                                            value_start as usize + p + 1
                                        });
                                    // Skip past idx elements (separated by commas)
                                    let mut pos = arr_start;
                                    let mut found = 0;
                                    while found < idx && pos < stripped.len() {
                                        if stripped.as_bytes()[pos] == b',' {
                                            found += 1;
                                        }
                                        pos += 1;
                                    }
                                    // Skip whitespace
                                    while pos < stripped.len()
                                        && stripped.as_bytes()[pos].is_ascii_whitespace()
                                    {
                                        pos += 1;
                                    }
                                    pos as u32
                                };
                                let msg = format_message(
                                    diagnostic_messages::SUBSTITUTION_FOR_PATTERN_HAS_INCORRECT_TYPE_EXPECTED_STRING_GOT,
                                    &[&elem_display, pattern, type_name],
                                );
                                diagnostics.push(Diagnostic::error(
                                    file_path,
                                    elem_pos,
                                    estimate_json_value_len(elem),
                                    msg,
                                    diagnostic_codes::SUBSTITUTION_FOR_PATTERN_HAS_INCORRECT_TYPE_EXPECTED_STRING_GOT,
                                ));
                                bad_patterns.push(pattern.clone());
                            }
                        }
                    }
                    _ => {
                        // TS5063: not an array
                        let value_len = estimate_json_value_len(value);
                        let msg = format_message(
                            diagnostic_messages::SUBSTITUTIONS_FOR_PATTERN_SHOULD_BE_AN_ARRAY,
                            &[pattern],
                        );
                        diagnostics.push(Diagnostic::error(
                            file_path,
                            value_start,
                            value_len,
                            msg,
                            diagnostic_codes::SUBSTITUTIONS_FOR_PATTERN_SHOULD_BE_AN_ARRAY,
                        ));
                        bad_patterns.push(pattern.clone());
                    }
                }
            }
            // Fix invalid values so serde can deserialize
            for pattern in &bad_patterns {
                if let Some(v) = paths_obj.get_mut(pattern) {
                    *v = serde_json::Value::Array(Vec::new());
                }
            }
        }

        // Propagate ts5024_keys out of this scope for use in resolve_compiler_options.
        ts5024_keys_outer = ts5024_keys;
    }

    // TS5024 for top-level tsconfig properties with wrong types. These
    // represented root selectors must be arrays; null invalidates the selector
    // without a diagnostic, matching serde's Option<T> representation.
    if let Some(obj) = raw.as_object_mut() {
        for key in ["include", "exclude", "files", "references"] {
            validate_top_level_array_option(obj, &mut diagnostics, &stripped, file_path, key);
        }
        // `compilerOptions` must be a JSON object; a scalar bypasses every
        // nested option validator and would otherwise surface as a generic
        // serde `invalid type` failure instead of TS5024.
        validate_top_level_object_option(
            obj,
            &mut diagnostics,
            &stripped,
            file_path,
            "compilerOptions",
        );
        // `compileOnSave` is a top-level boolean. tsc reports TS5024 when it
        // is set to a non-boolean (#3591 repro C); without this gate the
        // value is silently ignored.
        validate_top_level_boolean_option(
            obj,
            &mut diagnostics,
            &stripped,
            file_path,
            "compileOnSave",
        );
        // `typeAcquisition` keys are enumerated. Unknown keys must surface
        // as TS17010 to match tsc (#3591 repro B); an object that is not an
        // object is also flagged via the shared object validator above.
        validate_top_level_object_option(
            obj,
            &mut diagnostics,
            &stripped,
            file_path,
            "typeAcquisition",
        );
        validate_type_acquisition_known_keys(obj, &mut diagnostics, &stripped, file_path);

        // TS6046 for invalid `watchOptions.watchFile` / `watchDirectory` /
        // `fallbackPolling` enum values. tsc surfaces these as config
        // diagnostics before compiling; tsz used to skip them entirely.
        // See https://github.com/tsz-org/tsz/issues/3591 (repro A).
        if let Some(serde_json::Value::Object(watch_opts)) = obj.get_mut("watchOptions") {
            validate_option_value(
                watch_opts,
                "watchFile",
                &stripped,
                file_path,
                VALID_WATCH_FILE_VALUES,
                "--watchFile",
                VALID_WATCH_FILE_DISPLAY,
                &mut diagnostics,
            );
            validate_option_value(
                watch_opts,
                "watchDirectory",
                &stripped,
                file_path,
                VALID_WATCH_DIRECTORY_VALUES,
                "--watchDirectory",
                VALID_WATCH_DIRECTORY_DISPLAY,
                &mut diagnostics,
            );
            validate_option_value(
                watch_opts,
                "fallbackPolling",
                &stripped,
                file_path,
                VALID_FALLBACK_POLLING_VALUES,
                "--fallbackPolling",
                VALID_FALLBACK_POLLING_DISPLAY,
                &mut diagnostics,
            );
        }
    }

    let mut config: TsConfig =
        serde_json::from_value(raw).context("failed to parse tsconfig JSON")?;

    // Attach TS5024 invalidated keys so resolve_compiler_options knows not to apply defaults.
    if let Some(ref mut opts) = config.compiler_options {
        opts.invalidated_options = ts5024_keys_outer;
    }

    Ok(ParsedTsConfig {
        config,
        diagnostics,
    })
}
