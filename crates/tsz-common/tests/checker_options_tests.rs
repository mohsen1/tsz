use super::*;

// =============================================================================
// Default values
// =============================================================================

#[test]
fn test_default_strict_is_true() {
    let opts = CheckerOptions::default();
    assert!(opts.strict);
}

#[test]
fn test_default_strict_null_checks() {
    let opts = CheckerOptions::default();
    assert!(opts.strict_null_checks);
}

#[test]
fn test_default_no_implicit_any() {
    let opts = CheckerOptions::default();
    assert!(opts.no_implicit_any);
}

#[test]
fn test_default_strict_function_types() {
    let opts = CheckerOptions::default();
    assert!(opts.strict_function_types);
}

#[test]
fn test_default_strict_property_initialization() {
    let opts = CheckerOptions::default();
    assert!(opts.strict_property_initialization);
}

#[test]
fn test_default_no_implicit_this() {
    let opts = CheckerOptions::default();
    assert!(opts.no_implicit_this);
}

#[test]
fn test_default_use_unknown_in_catch_variables() {
    let opts = CheckerOptions::default();
    assert!(opts.use_unknown_in_catch_variables);
}

#[test]
fn test_default_strict_bind_call_apply() {
    let opts = CheckerOptions::default();
    assert!(opts.strict_bind_call_apply);
}

#[test]
fn test_default_always_strict() {
    let opts = CheckerOptions::default();
    assert!(opts.always_strict);
}

// =============================================================================
// Default: options that default to false
// =============================================================================

#[test]
fn test_default_no_implicit_returns_is_false() {
    let opts = CheckerOptions::default();
    assert!(!opts.no_implicit_returns);
}

#[test]
fn test_default_isolated_modules_is_false() {
    let opts = CheckerOptions::default();
    assert!(!opts.isolated_modules);
}

#[test]
fn test_default_no_unchecked_indexed_access_is_false() {
    let opts = CheckerOptions::default();
    assert!(!opts.no_unchecked_indexed_access);
}

#[test]
fn test_default_exact_optional_property_types_is_false() {
    let opts = CheckerOptions::default();
    assert!(!opts.exact_optional_property_types);
}

#[test]
fn test_default_no_lib_is_false() {
    let opts = CheckerOptions::default();
    assert!(!opts.no_lib);
}

#[test]
fn test_default_no_types_and_symbols_is_false() {
    let opts = CheckerOptions::default();
    assert!(!opts.no_types_and_symbols);
}

#[test]
fn test_default_es_module_interop_is_false() {
    let opts = CheckerOptions::default();
    assert!(!opts.es_module_interop);
}

#[test]
fn test_default_allow_synthetic_default_imports_is_false() {
    let opts = CheckerOptions::default();
    assert!(!opts.allow_synthetic_default_imports);
}

#[test]
fn test_default_allow_unreachable_code_is_none() {
    let opts = CheckerOptions::default();
    assert!(opts.allow_unreachable_code.is_none());
}

#[test]
fn test_default_no_property_access_from_index_signature_is_false() {
    let opts = CheckerOptions::default();
    assert!(!opts.no_property_access_from_index_signature);
}

#[test]
fn test_default_sound_mode_is_false() {
    let opts = CheckerOptions::default();
    assert!(!opts.sound_mode);
}

#[test]
fn test_default_experimental_decorators_is_false() {
    let opts = CheckerOptions::default();
    assert!(!opts.experimental_decorators);
}

#[test]
fn test_default_no_unused_locals_is_false() {
    let opts = CheckerOptions::default();
    assert!(!opts.no_unused_locals);
}

#[test]
fn test_default_no_unused_parameters_is_false() {
    let opts = CheckerOptions::default();
    assert!(!opts.no_unused_parameters);
}

#[test]
fn test_default_no_implicit_use_strict_is_false() {
    let opts = CheckerOptions::default();
    assert!(!opts.no_implicit_use_strict);
}

#[test]
fn test_default_resolve_json_module_is_false() {
    let opts = CheckerOptions::default();
    assert!(!opts.resolve_json_module);
}

#[test]
fn test_default_check_js_is_false() {
    let opts = CheckerOptions::default();
    assert!(!opts.check_js);
}

#[test]
fn test_default_no_resolve_is_false() {
    let opts = CheckerOptions::default();
    assert!(!opts.no_resolve);
}

#[test]
fn test_default_no_implicit_override_is_false() {
    let opts = CheckerOptions::default();
    assert!(!opts.no_implicit_override);
}

#[test]
fn test_default_suppress_excess_property_errors_is_false() {
    let opts = CheckerOptions::default();
    assert!(!opts.suppress_excess_property_errors);
}

#[test]
fn test_default_suppress_implicit_any_index_errors_is_false() {
    let opts = CheckerOptions::default();
    assert!(!opts.suppress_implicit_any_index_errors);
}

#[test]
fn test_default_allow_importing_ts_extensions_is_false() {
    let opts = CheckerOptions::default();
    assert!(!opts.allow_importing_ts_extensions);
}

#[test]
fn test_default_rewrite_relative_import_extensions_is_false() {
    let opts = CheckerOptions::default();
    assert!(!opts.rewrite_relative_import_extensions);
}

#[test]
fn test_default_implied_classic_resolution_is_false() {
    let opts = CheckerOptions::default();
    assert!(!opts.implied_classic_resolution);
}

// =============================================================================
// Default: JSX options
// =============================================================================

#[test]
fn test_default_jsx_factory() {
    let opts = CheckerOptions::default();
    assert_eq!(opts.jsx_factory, "React.createElement");
}

#[test]
fn test_default_jsx_factory_from_config_is_false() {
    let opts = CheckerOptions::default();
    assert!(!opts.jsx_factory_from_config);
}

#[test]
fn test_default_jsx_fragment_factory() {
    let opts = CheckerOptions::default();
    assert_eq!(opts.jsx_fragment_factory, "React.Fragment");
}

#[test]
fn test_default_jsx_fragment_factory_from_config_is_false() {
    let opts = CheckerOptions::default();
    assert!(!opts.jsx_fragment_factory_from_config);
}

#[test]
fn test_default_jsx_mode_is_none() {
    let opts = CheckerOptions::default();
    assert_eq!(opts.jsx_mode, JsxMode::None);
}

#[test]
fn test_default_jsx_import_source_is_empty() {
    let opts = CheckerOptions::default();
    assert!(opts.jsx_import_source.is_empty());
}

#[test]
fn test_default_module_explicitly_set_is_false() {
    let opts = CheckerOptions::default();
    assert!(!opts.module_explicitly_set);
}

// =============================================================================
// Default: Target and Module
// =============================================================================

#[test]
fn test_default_target() {
    let opts = CheckerOptions::default();
    assert_eq!(opts.target, ScriptTarget::default());
}

#[test]
fn test_default_module() {
    let opts = CheckerOptions::default();
    assert_eq!(opts.module, ModuleKind::default());
}

// =============================================================================
// apply_strict_defaults - strict=true enables all strict sub-flags
// =============================================================================

#[test]
fn test_apply_strict_defaults_enables_all_strict_flags() {
    let opts = CheckerOptions {
        strict: true,
        no_implicit_any: false,
        no_implicit_this: false,
        strict_null_checks: false,
        strict_function_types: false,
        strict_bind_call_apply: false,
        strict_property_initialization: false,
        use_unknown_in_catch_variables: false,
        always_strict: false,
        ..CheckerOptions::default()
    };

    let opts = opts.apply_strict_defaults();

    // All should be re-enabled by strict
    assert!(opts.no_implicit_any);
    assert!(opts.no_implicit_this);
    assert!(opts.strict_null_checks);
    assert!(opts.strict_function_types);
    assert!(opts.strict_bind_call_apply);
    assert!(opts.strict_property_initialization);
    assert!(opts.use_unknown_in_catch_variables);
    assert!(opts.always_strict);
}

#[test]
fn test_apply_strict_defaults_strict_false_preserves_flags() {
    let opts = CheckerOptions {
        strict: false,
        no_implicit_any: false,
        strict_null_checks: false,
        strict_function_types: false,
        ..CheckerOptions::default()
    };

    let opts = opts.apply_strict_defaults();

    // With strict=false, individual flags should NOT be overridden
    assert!(!opts.no_implicit_any);
    assert!(!opts.strict_null_checks);
    assert!(!opts.strict_function_types);
}

#[test]
fn test_apply_strict_defaults_does_not_enable_exact_optional() {
    // exactOptionalPropertyTypes is NOT part of the --strict family
    let opts = CheckerOptions {
        strict: true,
        exact_optional_property_types: false,
        ..CheckerOptions::default()
    };

    let opts = opts.apply_strict_defaults();
    assert!(!opts.exact_optional_property_types);
}

#[test]
fn test_apply_strict_defaults_returns_self() {
    // Verify the builder-style pattern works
    let opts = CheckerOptions {
        strict: true,
        ..CheckerOptions::default()
    }
    .apply_strict_defaults();

    assert!(opts.strict);
    assert!(opts.no_implicit_any);
}

// =============================================================================
// JsxMode
// =============================================================================

#[test]
fn test_jsx_mode_default_is_none() {
    let mode: JsxMode = Default::default();
    assert_eq!(mode, JsxMode::None);
}

#[test]
fn test_jsx_mode_equality() {
    assert_eq!(JsxMode::React, JsxMode::React);
    assert_ne!(JsxMode::React, JsxMode::ReactJsx);
    assert_ne!(JsxMode::Preserve, JsxMode::ReactNative);
}

#[test]
fn test_jsx_mode_copy() {
    let mode = JsxMode::ReactJsxDev;
    let copy = mode;
    assert_eq!(mode, copy);
}

#[test]
fn test_jsx_mode_debug() {
    let debug = format!("{:?}", JsxMode::React);
    assert_eq!(debug, "React");
}

#[test]
fn test_jsx_mode_all_variants() {
    // Ensure all variants exist and are distinct
    let variants = [
        JsxMode::None,
        JsxMode::Preserve,
        JsxMode::React,
        JsxMode::ReactJsx,
        JsxMode::ReactJsxDev,
        JsxMode::ReactNative,
    ];
    for i in 0..variants.len() {
        for j in (i + 1)..variants.len() {
            assert_ne!(
                variants[i], variants[j],
                "Variants at index {i} and {j} should be distinct"
            );
        }
    }
}

// =============================================================================
// CheckerOptions - clone
// =============================================================================

#[test]
fn test_checker_options_clone() {
    let opts = CheckerOptions {
        strict: false,
        no_implicit_any: false,
        jsx_factory: "h".to_string(),
        ..CheckerOptions::default()
    };

    let cloned = opts;
    assert!(!cloned.strict);
    assert!(!cloned.no_implicit_any);
    assert_eq!(cloned.jsx_factory, "h");
}

#[test]
fn test_checker_options_debug() {
    let opts = CheckerOptions::default();
    let debug = format!("{opts:?}");
    assert!(debug.contains("CheckerOptions"));
    assert!(debug.contains("strict"));
}

// =============================================================================
// CheckerOptions - custom configurations
// =============================================================================

#[test]
fn test_non_strict_with_individual_flags() {
    // Simulate a tsconfig with strict: false but some individual flags enabled
    let opts = CheckerOptions {
        strict: false,
        no_implicit_any: true,
        strict_null_checks: true,
        strict_function_types: false,
        ..CheckerOptions::default()
    };

    assert!(!opts.strict);
    assert!(opts.no_implicit_any);
    assert!(opts.strict_null_checks);
    assert!(!opts.strict_function_types);
}

#[test]
fn test_no_unchecked_side_effect_imports_default() {
    let opts = CheckerOptions::default();
    assert!(opts.no_unchecked_side_effect_imports);
}
