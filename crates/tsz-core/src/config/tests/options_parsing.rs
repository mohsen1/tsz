//! Compiler-option parsing and scalar/option-type validation tests
//! (boolean/string parsing, TS5024/TS17010/TS5059/TS5025 option diagnostics,
//! type-acquisition keys, sound-mode options, typo suggestions).
//!
//! Split from `config/mod.rs` to keep each file under the 2000-line limit
//! (§19; ratchet tracked by #8280).

use super::super::*;

#[test]
fn test_parse_boolean_true() {
    let json = r#"{"strict": true}"#;
    let opts: CompilerOptions = serde_json::from_str(json).unwrap();
    assert_eq!(opts.strict, Some(true));
}

#[test]
fn test_parse_string_true() {
    let json = r#"{"strict": "true"}"#;
    let opts: CompilerOptions = serde_json::from_str(json).unwrap();
    assert_eq!(opts.strict, Some(true));
}

#[test]
fn test_parse_invalid_string() {
    let json = r#"{"strict": "invalid"}"#;
    let result: Result<CompilerOptions, _> = serde_json::from_str(json);
    assert!(result.is_err());
}

#[test]
fn test_esnext_date_and_temporal_are_valid_lib_values() {
    // esnext.date and esnext.temporal must be in VALID_LIB_VALUES so tsconfig
    // using "@lib: esnext.date,esnext.temporal" does not emit TS6046.
    assert!(
        VALID_LIB_VALUES.contains(&"esnext.date"),
        "esnext.date should be a recognized lib value"
    );
    assert!(
        VALID_LIB_VALUES.contains(&"esnext.temporal"),
        "esnext.temporal should be a recognized lib value"
    );
}

#[test]
fn test_esnext_date_temporal_lib_in_tsconfig_no_ts6046() {
    let source = r#"{"compilerOptions":{"lib":["esnext.date","esnext.temporal"]}}"#;
    let parsed = parse_tsconfig_with_diagnostics(source, "tsconfig.json").unwrap();
    let codes: Vec<u32> = parsed.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        !codes.contains(&6046),
        "esnext.date and esnext.temporal should not emit TS6046, got: {codes:?}"
    );
}

#[test]

fn test_ts5024_emitted_for_lib_replacement_string_value() {
    let source = r#"{"compilerOptions":{"libReplacement":"true"}}"#;
    let parsed = parse_tsconfig_with_diagnostics(source, "tsconfig.json").unwrap();
    let codes: Vec<u32> = parsed.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&5024),
        "Expected TS5024 for libReplacement string value, got: {codes:?}"
    );
}

#[test]
fn test_ts5024_emitted_for_scalar_compiler_options() {
    // Issue #3882: a top-level scalar `compilerOptions` would bypass every
    // nested option validator and trip serde's `invalid type` error
    // instead of the user-facing TS5024 diagnostic.
    let source = r#"{"compilerOptions":"bad","files":["a.ts"]}"#;
    let parsed = parse_tsconfig_with_diagnostics(source, "tsconfig.json").unwrap();
    let diag = parsed
        .diagnostics
        .iter()
        .find(|d| d.code == 5024 && d.message_text.contains("compilerOptions"))
        .unwrap_or_else(|| {
            panic!(
                "Expected TS5024 mentioning compilerOptions, got: {:?}",
                parsed
                    .diagnostics
                    .iter()
                    .map(|d| (d.code, d.message_text.clone()))
                    .collect::<Vec<_>>()
            )
        });
    assert!(
        diag.message_text.contains("object"),
        "TS5024 message should mention expected type 'object': {}",
        diag.message_text
    );
}

#[test]
fn test_scalar_compiler_options_does_not_break_files_array() {
    // The recovery path replaces the invalid scalar with `{}` so the
    // rest of the config (e.g. `files`) still parses cleanly.
    let source = r#"{"compilerOptions":42,"files":["a.ts"]}"#;
    let parsed = parse_tsconfig_with_diagnostics(source, "tsconfig.json").unwrap();
    assert_eq!(
        parsed.config.files.as_deref(),
        Some(&["a.ts".to_string()][..]),
        "files array must still parse after compilerOptions recovery"
    );
}

#[test]
fn test_ts5024_emitted_for_non_boolean_compile_on_save() {
    // Issue #3591 repro C: `compileOnSave` is a top-level boolean. A
    // string value must surface as TS5024, matching tsc.
    let source = r#"{
  "compilerOptions": {"noEmit": true},
  "compileOnSave": "yes",
  "files": ["a.ts"]
}"#;
    let parsed = parse_tsconfig_with_diagnostics(source, "tsconfig.json").unwrap();
    let diag = parsed
        .diagnostics
        .iter()
        .find(|d| d.code == 5024 && d.message_text.contains("compileOnSave"))
        .unwrap_or_else(|| {
            panic!(
                "Expected TS5024 mentioning compileOnSave, got: {:?}",
                parsed
                    .diagnostics
                    .iter()
                    .map(|d| (d.code, d.message_text.clone()))
                    .collect::<Vec<_>>()
            )
        });
    assert!(
        diag.message_text.contains("boolean"),
        "TS5024 message should mention expected type 'boolean': {}",
        diag.message_text
    );
}

#[test]
fn test_compile_on_save_boolean_value_does_not_emit_ts5024() {
    // The validator must only fire for non-boolean values; a real boolean
    // is accepted silently.
    let source = r#"{
  "compilerOptions": {"noEmit": true},
  "compileOnSave": true,
  "files": ["a.ts"]
}"#;
    let parsed = parse_tsconfig_with_diagnostics(source, "tsconfig.json").unwrap();
    assert!(
        !parsed
            .diagnostics
            .iter()
            .any(|d| d.code == 5024 && d.message_text.contains("compileOnSave")),
        "boolean compileOnSave must not emit TS5024, got: {:?}",
        parsed
            .diagnostics
            .iter()
            .map(|d| (d.code, d.message_text.clone()))
            .collect::<Vec<_>>()
    );
}

#[test]
fn test_ts17010_emitted_for_unknown_type_acquisition_option() {
    // Issue #3591 repro B: an unknown `typeAcquisition` key is silently
    // dropped; tsc reports TS17010 for it.
    let source = r#"{
  "compilerOptions": {"noEmit": true},
  "typeAcquisition": {"bogus": true},
  "files": ["a.ts"]
}"#;
    let parsed = parse_tsconfig_with_diagnostics(source, "tsconfig.json").unwrap();
    let diag = parsed
        .diagnostics
        .iter()
        .find(|d| d.code == 17010 && d.message_text.contains("bogus"))
        .unwrap_or_else(|| {
            panic!(
                "Expected TS17010 mentioning bogus, got: {:?}",
                parsed
                    .diagnostics
                    .iter()
                    .map(|d| (d.code, d.message_text.clone()))
                    .collect::<Vec<_>>()
            )
        });
    assert!(
        diag.message_text
            .to_lowercase()
            .contains("type acquisition"),
        "TS17010 message should reference 'type acquisition': {}",
        diag.message_text
    );
}

#[test]
fn test_known_type_acquisition_keys_do_not_emit_ts17010() {
    // The four known typeAcquisition options must not be flagged.
    let source = r#"{
  "compilerOptions": {"noEmit": true},
  "typeAcquisition": {
"enable": true,
"include": ["jquery"],
"exclude": ["lodash"],
"disableFilenameBasedTypeAcquisition": false
  },
  "files": ["a.ts"]
}"#;
    let parsed = parse_tsconfig_with_diagnostics(source, "tsconfig.json").unwrap();
    let count = parsed
        .diagnostics
        .iter()
        .filter(|d| d.code == 17010)
        .count();
    assert_eq!(
        count,
        0,
        "no TS17010 expected for known typeAcquisition keys, got: {:?}",
        parsed
            .diagnostics
            .iter()
            .map(|d| (d.code, d.message_text.clone()))
            .collect::<Vec<_>>()
    );
}

#[test]
fn test_ts5024_emitted_for_scalar_type_acquisition() {
    // A scalar typeAcquisition value (not an object) must surface as
    // TS5024 via the shared object-shape validator.
    let source = r#"{
  "compilerOptions": {"noEmit": true},
  "typeAcquisition": "bad",
  "files": ["a.ts"]
}"#;
    let parsed = parse_tsconfig_with_diagnostics(source, "tsconfig.json").unwrap();
    assert!(
        parsed
            .diagnostics
            .iter()
            .any(|d| d.code == 5024 && d.message_text.contains("typeAcquisition")),
        "expected TS5024 for scalar typeAcquisition, got: {:?}",
        parsed
            .diagnostics
            .iter()
            .map(|d| (d.code, d.message_text.clone()))
            .collect::<Vec<_>>()
    );
}

#[test]
fn test_ts5024_emitted_for_recognized_options_with_invalid_value_types() {
    for (option, value, expected_type) in [
        ("plugins", r#""not-an-array""#, "Array"),
        ("maxNodeModuleJsDepth", r#""not-a-number""#, "number"),
        ("traceResolution", r#""yes""#, "boolean"),
    ] {
        let source = format!(
            r#"{{
  "compilerOptions": {{
"noEmit": true,
"{option}": {value}
  }},
  "files": ["index.ts"]
}}"#
        );
        let parsed = parse_tsconfig_with_diagnostics(&source, "tsconfig.json")
            .unwrap_or_else(|err| panic!("{option} should report TS5024, not fail parse: {err}"));
        let diagnostic = parsed
            .diagnostics
            .iter()
            .find(|d| d.code == 5024 && d.message_text.contains(option))
            .unwrap_or_else(|| {
                panic!(
                    "Expected TS5024 for invalid {option}, got: {:?}",
                    parsed.diagnostics
                )
            });
        assert!(
            diagnostic.message_text.contains(expected_type),
            "Expected {option} TS5024 to mention {expected_type}, got: {diagnostic:?}"
        );
    }
}

#[test]
fn test_ts5059_emitted_for_invalid_react_namespace_value() {
    let source = r#"{
  "compilerOptions": {
"jsx": "react",
"reactNamespace": "my-React-Lib"
  }
}"#;
    let parsed = parse_tsconfig_with_diagnostics(source, "tsconfig.json").unwrap();
    let ts5059 = parsed
        .diagnostics
        .iter()
        .find(|d| {
            d.code == diagnostic_codes::INVALID_VALUE_FOR_REACTNAMESPACE_IS_NOT_A_VALID_IDENTIFIER
        })
        .unwrap_or_else(|| panic!("Expected TS5059, got: {:?}", parsed.diagnostics));

    assert_eq!(ts5059.file, "tsconfig.json");
    assert_eq!(
        ts5059.start,
        source
            .find("\"my-React-Lib\"")
            .expect("reactNamespace value") as u32
    );
    assert!(
        ts5059
            .message_text
            .contains("'my-React-Lib' is not a valid identifier"),
        "Unexpected TS5059 message: {}",
        ts5059.message_text
    );
}

#[test]
fn test_disable_size_limit_option_is_recognized() {
    let source = r#"{
  "compilerOptions": {
"disableSizeLimit": true
  }
}"#;
    let parsed = parse_tsconfig_with_diagnostics(source, "tsconfig.json").unwrap();
    let codes: Vec<u32> = parsed.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        !codes.contains(&diagnostic_codes::UNKNOWN_COMPILER_OPTION),
        "disableSizeLimit should not report unknown compiler option, got: {codes:?}"
    );
    assert!(
        !codes.contains(&diagnostic_codes::UNKNOWN_COMPILER_OPTION_DID_YOU_MEAN),
        "disableSizeLimit should not report did-you-mean diagnostic, got: {codes:?}"
    );
}

#[test]
fn test_disable_solution_searching_option_is_recognized() {
    let source = r#"{
  "compilerOptions": {
"disableSolutionSearching": true
  }
}"#;
    let parsed = parse_tsconfig_with_diagnostics(source, "tsconfig.json").unwrap();
    let codes: Vec<u32> = parsed.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        !codes.contains(&diagnostic_codes::UNKNOWN_COMPILER_OPTION),
        "disableSolutionSearching should not report unknown compiler option, got: {codes:?}"
    );
}

#[test]
fn test_tsz_only_compiler_options_report_unknown_from_tsconfig() {
    let source = r#"{
  "compilerOptions": {
"inlineConstants": true,
"disableSolutionTypeCheck": true,
"disableSolutionCaching": true,
"disableSolutionTypeChecking": true
  }
}"#;
    let parsed = parse_tsconfig_with_diagnostics(source, "tsconfig.json").unwrap();
    let codes: Vec<u32> = parsed.diagnostics.iter().map(|d| d.code).collect();
    assert_eq!(
        codes.len(),
        4,
        "unrecognized options should produce tsc-compatible diagnostics, got: {:?}",
        parsed.diagnostics
    );
    assert_eq!(
        codes
            .iter()
            .filter(|&&code| code == diagnostic_codes::UNKNOWN_COMPILER_OPTION)
            .count(),
        2,
        "expected two TS5023 diagnostics, got: {:?}",
        parsed.diagnostics
    );
    assert_eq!(
        codes
            .iter()
            .filter(|&&code| code == diagnostic_codes::UNKNOWN_COMPILER_OPTION_DID_YOU_MEAN)
            .count(),
        2,
        "expected two TS5025 diagnostics, got: {:?}",
        parsed.diagnostics
    );
    assert!(
        parsed
            .diagnostics
            .iter()
            .filter(|diag| diag.code == diagnostic_codes::UNKNOWN_COMPILER_OPTION_DID_YOU_MEAN)
            .all(|diag| diag.message_text.contains("disableSolutionSearching")),
        "disableSolution typo diagnostics should suggest disableSolutionSearching, got: {:?}",
        parsed.diagnostics
    );
}

#[test]
fn test_sound_tsconfig_option_enables_sound_mode() {
    let source = r#"{"compilerOptions":{"sound":true}}"#;
    let parsed = parse_tsconfig_with_diagnostics(source, "tsconfig.json").unwrap();
    assert!(
        parsed.diagnostics.is_empty(),
        "`sound` should be accepted as a known option, got: {:?}",
        parsed.diagnostics
    );
    let resolved = resolve_compiler_options(parsed.config.compiler_options.as_ref()).unwrap();
    assert!(
        resolved.checker.sound_mode,
        "sound: true in tsconfig should enable sound_mode"
    );
}

#[test]
fn test_sound_tsconfig_option_false_keeps_sound_mode_off() {
    let source = r#"{"compilerOptions":{"sound":false}}"#;
    let parsed = parse_tsconfig_with_diagnostics(source, "tsconfig.json").unwrap();
    assert!(
        parsed.diagnostics.is_empty(),
        "`sound: false` should be accepted without diagnostics, got: {:?}",
        parsed.diagnostics
    );
    let resolved = resolve_compiler_options(parsed.config.compiler_options.as_ref()).unwrap();
    assert!(
        !resolved.checker.sound_mode,
        "false must not flip sound_mode on"
    );
}

#[test]
fn test_sound_tsconfig_invalid_value_emits_ts5024() {
    let source = r#"{"compilerOptions":{"sound":"yes_please"}}"#;
    let parsed = parse_tsconfig_with_diagnostics(source, "tsconfig.json").unwrap();
    let codes: Vec<u32> = parsed.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&diagnostic_codes::COMPILER_OPTION_REQUIRES_A_VALUE_OF_TYPE),
        "non-boolean `sound` value should emit TS5024, got: {:?}",
        parsed.diagnostics
    );
    let resolved = resolve_compiler_options(parsed.config.compiler_options.as_ref()).unwrap();
    assert!(
        !resolved.checker.sound_mode,
        "invalid `sound` value should not enable sound_mode"
    );
}

#[test]
fn test_sound_family_tsconfig_options_accepted() {
    let source = r#"{
  "compilerOptions": {
"sound": true,
"soundCheckDeclarations": true,
"soundReportOnly": true,
"soundPedantic": true
  }
}"#;
    let parsed = parse_tsconfig_with_diagnostics(source, "tsconfig.json").unwrap();
    assert!(
        parsed.diagnostics.is_empty(),
        "sound family options should all be accepted, got: {:?}",
        parsed.diagnostics
    );
    let resolved = resolve_compiler_options(parsed.config.compiler_options.as_ref()).unwrap();
    assert!(
        resolved.checker.sound_mode,
        "sound: true should set sound_mode"
    );
    assert!(
        resolved.checker.sound_check_declarations,
        "soundCheckDeclarations: true should set sound_check_declarations"
    );
    assert!(
        resolved.checker.sound_report_only,
        "soundReportOnly: true should set sound_report_only"
    );
    assert!(
        resolved.checker.sound_pedantic,
        "soundPedantic: true should set sound_pedantic"
    );
}

#[test]
fn test_sound_family_tsconfig_options_miscased_emit_did_you_mean() {
    // All-uppercase-suffix spellings (e.g. soundPEDANTIC) still fall within
    // the Levenshtein threshold for getSpellingSuggestion, so they get TS5025
    // rather than a bare TS5023.
    let cases = [
        ("Sound", "sound"),
        ("soundCheckdeclarations", "soundCheckDeclarations"),
        ("soundreportonly", "soundReportOnly"),
        ("soundPEDANTIC", "soundPedantic"),
    ];
    for (typo, canonical) in cases {
        let source = format!(r#"{{"compilerOptions":{{"{typo}":true}}}}"#);
        let parsed = parse_tsconfig_with_diagnostics(&source, "tsconfig.json").unwrap();
        let diag = parsed
            .diagnostics
            .iter()
            .find(|d| d.code == diagnostic_codes::UNKNOWN_COMPILER_OPTION_DID_YOU_MEAN);
        assert!(
            diag.is_some(),
            "typo `{typo}` should emit TS5025, got: {:?}",
            parsed.diagnostics
        );
        assert!(
            diag.unwrap().message_text.contains(canonical),
            "TS5025 for `{typo}` should suggest `{canonical}`, got: {}",
            diag.unwrap().message_text
        );
    }
}

#[test]
fn test_disable_size_limit_miscase_reports_did_you_mean() {
    let source = r#"{
  "compilerOptions": {
"disableSizelimit": true
  }
}"#;
    let parsed = parse_tsconfig_with_diagnostics(source, "tsconfig.json").unwrap();
    let codes: Vec<u32> = parsed.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&diagnostic_codes::UNKNOWN_COMPILER_OPTION_DID_YOU_MEAN),
        "Expected TS5025-style did-you-mean for mis-cased disableSizeLimit, got: {codes:?}"
    );
}

#[test]
fn test_typo_suggestions_emit_ts5025_for_close_compiler_option_names() {
    // Issue #3831: typoed compiler-option names that the canonical option
    // list contains within Levenshtein range should be reported as TS5025
    // with a `Did you mean ...` suggestion, matching tsc.
    let cases = [
        ("stric", "strict"),
        ("noEmti", "noEmit"),
        ("moduleResoluton", "moduleResolution"),
    ];
    for (typo, expected) in cases {
        let source = format!("{{\"compilerOptions\":{{\"{typo}\":true}}}}");
        let parsed = parse_tsconfig_with_diagnostics(&source, "tsconfig.json").unwrap();
        let diag = parsed
            .diagnostics
            .iter()
            .find(|d| d.code == diagnostic_codes::UNKNOWN_COMPILER_OPTION_DID_YOU_MEAN)
            .unwrap_or_else(|| {
                panic!(
                    "Expected TS5025 for typo {typo}, got: {:?}",
                    parsed.diagnostics
                )
            });
        assert!(
            diag.message_text.contains(expected),
            "TS5025 for {typo} should suggest {expected}, got: {}",
            diag.message_text
        );
        assert!(
            !parsed
                .diagnostics
                .iter()
                .any(|d| d.code == diagnostic_codes::UNKNOWN_COMPILER_OPTION),
            "Bare TS5023 should not be emitted alongside TS5025 for {typo}, got: {:?}",
            parsed.diagnostics
        );
    }
}

#[test]
fn test_unrelated_unknown_compiler_option_still_falls_back_to_ts5023() {
    // A name that bears no resemblance to any canonical option must not
    // get a spurious TS5025 suggestion, just the bare TS5023.
    let source = r#"{"compilerOptions":{"completelyUnrelatedXYZ":true}}"#;
    let parsed = parse_tsconfig_with_diagnostics(source, "tsconfig.json").unwrap();
    let codes: Vec<u32> = parsed.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&diagnostic_codes::UNKNOWN_COMPILER_OPTION),
        "Expected TS5023 for unrelated unknown option, got: {codes:?}"
    );
    assert!(
        !codes.contains(&diagnostic_codes::UNKNOWN_COMPILER_OPTION_DID_YOU_MEAN),
        "Unrelated option should not emit a Did-you-mean suggestion, got: {codes:?}"
    );
}

#[test]
fn explain_files_compiler_option_is_recognized() {
    // Issue #3860: `explainFiles` was missing from the known-options
    // list, causing tsconfig-side `explainFiles: true` to surface a
    // false TS5023 even though tsc accepts it from config.
    let source = r#"{"compilerOptions":{"explainFiles":true,"noEmit":true}}"#;
    let parsed = parse_tsconfig_with_diagnostics(source, "tsconfig.json").unwrap();
    let codes: Vec<u32> = parsed.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        !codes.contains(&diagnostic_codes::UNKNOWN_COMPILER_OPTION),
        "explainFiles must not emit TS5023, got {codes:?}"
    );
}

#[test]
fn explain_files_lowercase_alias_is_recognized() {
    // Miscased `explainfiles` should map to `explainFiles` and emit
    // TS5025 (Did you mean), not the bare TS5023.
    let source = r#"{"compilerOptions":{"explainfiles":true,"noEmit":true}}"#;
    let parsed = parse_tsconfig_with_diagnostics(source, "tsconfig.json").unwrap();
    let codes: Vec<u32> = parsed.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&diagnostic_codes::UNKNOWN_COMPILER_OPTION_DID_YOU_MEAN),
        "miscased explainfiles must emit TS5025 Did you mean, got {codes:?}"
    );
}

#[test]
fn test_disable_source_of_project_reference_redirect_option_is_recognized() {
    let source = r#"{
  "compilerOptions": {
"disableSourceOfProjectReferenceRedirect": true
  }
}"#;
    let parsed = parse_tsconfig_with_diagnostics(source, "tsconfig.json").unwrap();
    let codes: Vec<u32> = parsed.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        !codes.contains(&diagnostic_codes::UNKNOWN_COMPILER_OPTION),
        "disableSourceOfProjectReferenceRedirect should not report unknown compiler option, got: {codes:?}"
    );
}

#[test]
fn test_resolve_compiler_options_sets_lib_replacement_flag() {
    let json = r#"{"compilerOptions":{"libReplacement":true}}"#;
    let config: TsConfig = serde_json::from_str(json).unwrap();
    let resolved = resolve_compiler_options(config.compiler_options.as_ref()).unwrap();
    assert!(resolved.lib_replacement);
}

#[test]
fn test_tsconfig_emit_flags_reach_printer_options() {
    let json = r#"{"compilerOptions":{"importHelpers":true,"preserveConstEnums":true,"downlevelIteration":true}}"#;
    let config: TsConfig = serde_json::from_str(json).unwrap();
    let resolved = resolve_compiler_options(config.compiler_options.as_ref()).unwrap();

    assert!(resolved.import_helpers);
    assert!(resolved.printer.import_helpers);
    assert!(resolved.printer.no_emit_helpers);
    assert!(resolved.checker.preserve_const_enums);
    assert!(resolved.printer.preserve_const_enums);
    assert!(resolved.checker.downlevel_iteration);
    assert!(resolved.printer.downlevel_iteration);
}
