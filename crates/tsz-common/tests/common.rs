use super::*;

#[test]
fn test_script_target_comparisons() {
    assert!(ScriptTarget::ES3.is_es5());
    assert!(ScriptTarget::ES5.is_es5());
    assert!(!ScriptTarget::ES2015.is_es5());
    assert!(ScriptTarget::ES2015.supports_es2015());
    assert!(!ScriptTarget::ES5.supports_es2015());
    assert!(!ScriptTarget::ES5.supports_es2016());
    assert!(ScriptTarget::ES2016.supports_es2016());
    assert!(!ScriptTarget::ES2016.supports_es2019());
    assert!(!ScriptTarget::ES2016.supports_es2020());
    assert!(ScriptTarget::ES2019.supports_es2019());
    assert!(ScriptTarget::ES2025.supports_es2025());
    assert!(!ScriptTarget::ES2019.supports_es2024());
    assert!(ScriptTarget::ES2024.supports_es2024());
    assert!(ScriptTarget::ES2025.supports_es2024());
    assert!(!ScriptTarget::ES2022.supports_es2023());
    assert!(ScriptTarget::ES2023.supports_es2023());
    assert!(!ScriptTarget::ES5.supports_es2019());
    assert!(!ScriptTarget::ES2024.supports_es2025());
    assert!(ScriptTarget::ES2025.supports_es2025());
}

#[test]
fn test_script_target_from_ts_str() {
    assert_eq!(ScriptTarget::from_ts_str("ES5"), Some(ScriptTarget::ES5));
    assert_eq!(ScriptTarget::from_ts_str("es6"), Some(ScriptTarget::ES2015));
    assert_eq!(
        ScriptTarget::from_ts_str("es2023"),
        Some(ScriptTarget::ES2023)
    );
    assert_eq!(
        ScriptTarget::from_ts_str("es2025"),
        Some(ScriptTarget::ES2025)
    );
    assert_eq!(
        ScriptTarget::from_ts_str("ES5, ES2015"),
        Some(ScriptTarget::ES5)
    );
    assert_eq!(ScriptTarget::from_ts_str("not-a-target"), None);
    assert_eq!(
        ScriptTarget::from_ts_numeric(10),
        Some(ScriptTarget::ES2023)
    );
    assert_eq!(
        ScriptTarget::from_ts_numeric(12),
        Some(ScriptTarget::ES2025)
    );
    assert_eq!(
        ScriptTarget::from_ts_numeric(99),
        Some(ScriptTarget::ESNext)
    );
    assert_eq!(ScriptTarget::from_ts_numeric(42), None);
}

#[test]
fn test_module_kind_detection() {
    // CommonJS-like systems
    assert!(ModuleKind::CommonJS.is_commonjs());
    assert!(ModuleKind::UMD.is_commonjs());
    assert!(ModuleKind::Node16.is_commonjs());
    assert!(ModuleKind::NodeNext.is_commonjs());

    // Pure ES module systems (export = forbidden)
    assert!(ModuleKind::ES2015.is_es_module());
    assert!(ModuleKind::ES2020.is_es_module());
    assert!(ModuleKind::ES2022.is_es_module());
    assert!(ModuleKind::ESNext.is_es_module());

    // Hybrid systems or no modules (export = allowed)
    assert!(!ModuleKind::Node16.is_es_module()); // Hybrid - depends on file extension
    assert!(!ModuleKind::NodeNext.is_es_module()); // Hybrid - depends on file extension
    assert!(!ModuleKind::None.is_es_module());
    assert!(!ModuleKind::CommonJS.is_es_module());
    assert!(!ModuleKind::AMD.is_es_module());
    assert!(!ModuleKind::UMD.is_es_module());
    assert!(!ModuleKind::System.is_es_module());
    assert!(ModuleKind::Preserve.is_es_module());

    // Node-like systems still support dynamic import, but only the modern
    // targets accept the second `import()` options argument.
    assert!(ModuleKind::Node16.is_node_module());
    assert!(ModuleKind::Node18.is_node_module());
    assert!(ModuleKind::Node20.is_node_module());
    assert!(ModuleKind::NodeNext.is_node_module());
    assert!(!ModuleKind::CommonJS.is_node_module());
    assert!(ModuleKind::ES2020.supports_dynamic_import());
    assert!(ModuleKind::Node16.supports_dynamic_import());
    assert!(ModuleKind::NodeNext.supports_dynamic_import());
    assert!(!ModuleKind::ES2015.supports_dynamic_import());
    assert!(ModuleKind::ESNext.supports_dynamic_import_options());
    assert!(ModuleKind::Node16.supports_dynamic_import_options());
    assert!(ModuleKind::Node20.supports_dynamic_import_options());
    assert!(ModuleKind::Preserve.supports_dynamic_import_options());
    assert!(!ModuleKind::CommonJS.supports_dynamic_import_options());
}

#[test]
fn test_module_kind_from_ts_str() {
    assert_eq!(
        ModuleKind::from_ts_str("commonjs"),
        Some(ModuleKind::CommonJS)
    );
    assert_eq!(ModuleKind::from_ts_str("es6"), Some(ModuleKind::ES2015));
    assert_eq!(ModuleKind::from_ts_str("node18"), Some(ModuleKind::Node18));
    assert_eq!(ModuleKind::from_ts_str("node20"), Some(ModuleKind::Node20));
    assert_eq!(
        ModuleKind::from_ts_str("react-native"),
        None,
        "jsx spellings must not parse as module values"
    );
    assert_eq!(
        ModuleKind::from_ts_str("es2022, esnext"),
        Some(ModuleKind::ES2022)
    );
    assert_eq!(ModuleKind::from_ts_numeric(3), Some(ModuleKind::UMD));
    assert_eq!(ModuleKind::from_ts_numeric(5), Some(ModuleKind::ES2015));
    assert_eq!(ModuleKind::from_ts_numeric(101), Some(ModuleKind::Node18));
    assert_eq!(ModuleKind::from_ts_numeric(102), Some(ModuleKind::Node20));
    assert_eq!(ModuleKind::from_ts_numeric(255), None);
    assert_eq!(ModuleKind::NodeNext.ts_numeric_value(), 199);
}

#[test]
fn test_newline_kind() {
    assert_eq!(NewLineKind::LineFeed.as_str(), "\n");
    assert_eq!(NewLineKind::CarriageReturnLineFeed.as_str(), "\r\n");
    assert_eq!(NewLineKind::LineFeed.as_bytes(), b"\n");
    assert_eq!(NewLineKind::CarriageReturnLineFeed.as_bytes(), b"\r\n");
}

// =============================================================================
// Test fixtures
// =============================================================================

/// All `ScriptTarget` variants in ascending edition order.
const ALL_SCRIPT_TARGETS: &[ScriptTarget] = &[
    ScriptTarget::ES3,
    ScriptTarget::ES5,
    ScriptTarget::ES2015,
    ScriptTarget::ES2016,
    ScriptTarget::ES2017,
    ScriptTarget::ES2018,
    ScriptTarget::ES2019,
    ScriptTarget::ES2020,
    ScriptTarget::ES2021,
    ScriptTarget::ES2022,
    ScriptTarget::ES2023,
    ScriptTarget::ES2024,
    ScriptTarget::ES2025,
    ScriptTarget::ESNext,
];

/// All `ModuleKind` variants.
const ALL_MODULE_KINDS: &[ModuleKind] = &[
    ModuleKind::None,
    ModuleKind::CommonJS,
    ModuleKind::AMD,
    ModuleKind::UMD,
    ModuleKind::System,
    ModuleKind::ES2015,
    ModuleKind::ES2020,
    ModuleKind::ES2022,
    ModuleKind::ESNext,
    ModuleKind::Node16,
    ModuleKind::Node18,
    ModuleKind::Node20,
    ModuleKind::NodeNext,
    ModuleKind::Preserve,
];

// =============================================================================
// ScriptTarget - supports_* feature gates not covered above
// =============================================================================

#[test]
fn test_script_target_supports_es2017_es2018() {
    assert!(!ScriptTarget::ES2016.supports_es2017());
    assert!(ScriptTarget::ES2017.supports_es2017());
    assert!(ScriptTarget::ESNext.supports_es2017());

    assert!(!ScriptTarget::ES2017.supports_es2018());
    assert!(ScriptTarget::ES2018.supports_es2018());
    assert!(ScriptTarget::ESNext.supports_es2018());
}

#[test]
fn test_script_target_supports_es2020_es2021() {
    assert!(!ScriptTarget::ES2019.supports_es2020());
    assert!(ScriptTarget::ES2020.supports_es2020());
    // ES2021 introduced logical assignment (??=, ||=, &&=) and numeric separators.
    // The emitter checks `supports_es2021` to decide whether to lower numeric separators.
    assert!(!ScriptTarget::ES2020.supports_es2021());
    assert!(ScriptTarget::ES2021.supports_es2021());
    assert!(ScriptTarget::ESNext.supports_es2021());
}

#[test]
fn test_script_target_supports_es2022() {
    // ES2022 introduced class fields and the regex 'd' flag.
    // Used by checker (private lowering decision) and emitter (downlevel gating).
    assert!(!ScriptTarget::ES2021.supports_es2022());
    assert!(ScriptTarget::ES2022.supports_es2022());
    // Default (ESNext) supports ES2022.
    assert!(ScriptTarget::default().supports_es2022());
}

#[test]
fn test_script_target_supports_feature_gates_are_monotonic() {
    // Once a target supports an edition, every later target must also support it.
    for window in ALL_SCRIPT_TARGETS.windows(2) {
        let (lower, higher) = (window[0], window[1]);
        if lower.supports_es2017() {
            assert!(higher.supports_es2017());
        }
        if lower.supports_es2020() {
            assert!(higher.supports_es2020());
        }
        if lower.supports_es2022() {
            assert!(higher.supports_es2022());
        }
        if lower.supports_es2024() {
            assert!(higher.supports_es2024());
        }
    }
}

// =============================================================================
// ScriptTarget - ts_numeric_value / as_ts_str round-trip
// =============================================================================

#[test]
fn test_script_target_ts_numeric_value_endpoints() {
    assert_eq!(ScriptTarget::ES3.ts_numeric_value(), 0);
    assert_eq!(ScriptTarget::ES2015.ts_numeric_value(), 2);
    assert_eq!(ScriptTarget::ES2025.ts_numeric_value(), 12);
    assert_eq!(ScriptTarget::ESNext.ts_numeric_value(), 99);
}

#[test]
fn test_script_target_as_ts_str_canonical_spellings() {
    // Spot-check a few variants; full-set round-trip locks the rest.
    assert_eq!(ScriptTarget::ES3.as_ts_str(), "es3");
    assert_eq!(ScriptTarget::ES2015.as_ts_str(), "es2015");
    assert_eq!(ScriptTarget::ES2025.as_ts_str(), "es2025");
    assert_eq!(ScriptTarget::ESNext.as_ts_str(), "esnext");
}

#[test]
fn test_script_target_round_trips_through_from_ts_str_and_from_ts_numeric() {
    for &variant in ALL_SCRIPT_TARGETS {
        // Canonical string spelling round-trips.
        let canonical = variant.as_ts_str();
        assert_eq!(
            ScriptTarget::from_ts_str(canonical),
            Some(variant),
            "string round-trip failed for {canonical}"
        );
        // Numeric value round-trips.
        let n = u32::from(variant.ts_numeric_value());
        assert_eq!(
            ScriptTarget::from_ts_numeric(n),
            Some(variant),
            "numeric round-trip failed for {variant:?}"
        );
    }
}

// =============================================================================
// ScriptTarget - from_ts_str normalization (whitespace/dashes/underscores)
// =============================================================================

#[test]
fn test_script_target_from_ts_str_normalizes_internal_separators() {
    // Internal dashes, underscores, and whitespace are stripped before lookup.
    assert_eq!(
        ScriptTarget::from_ts_str("es-2015"),
        Some(ScriptTarget::ES2015)
    );
    assert_eq!(
        ScriptTarget::from_ts_str("es_2020"),
        Some(ScriptTarget::ES2020)
    );
    assert_eq!(
        ScriptTarget::from_ts_str("es 2022"),
        Some(ScriptTarget::ES2022)
    );
    assert_eq!(
        ScriptTarget::from_ts_str("ES_NEXT"),
        Some(ScriptTarget::ESNext)
    );
    // Outer whitespace before the comma split is trimmed.
    assert_eq!(
        ScriptTarget::from_ts_str("  es2024  "),
        Some(ScriptTarget::ES2024)
    );
}

#[test]
fn test_script_target_from_ts_str_empty_or_separator_only_returns_none() {
    assert_eq!(ScriptTarget::from_ts_str(""), None);
    assert_eq!(ScriptTarget::from_ts_str("   "), None);
    assert_eq!(ScriptTarget::from_ts_str(","), None);
}

// =============================================================================
// ModuleKind - as_ts_str canonical spellings + round-trips
// =============================================================================

#[test]
fn test_module_kind_as_ts_str_canonical_spellings() {
    // Spot-check; full-set round-trip locks the rest.
    assert_eq!(ModuleKind::None.as_ts_str(), "none");
    assert_eq!(ModuleKind::CommonJS.as_ts_str(), "commonjs");
    assert_eq!(ModuleKind::ESNext.as_ts_str(), "esnext");
    assert_eq!(ModuleKind::Node16.as_ts_str(), "node16");
    assert_eq!(ModuleKind::NodeNext.as_ts_str(), "nodenext");
    assert_eq!(ModuleKind::Preserve.as_ts_str(), "preserve");
}

#[test]
fn test_module_kind_round_trips_through_from_ts_str_and_from_ts_numeric() {
    for &variant in ALL_MODULE_KINDS {
        let canonical = variant.as_ts_str();
        assert_eq!(
            ModuleKind::from_ts_str(canonical),
            Some(variant),
            "string round-trip failed for {canonical}"
        );
        let n = variant.ts_numeric_value();
        assert_eq!(
            ModuleKind::from_ts_numeric(n),
            Some(variant),
            "numeric round-trip failed for {variant:?}"
        );
    }
}

// =============================================================================
// ModuleKind - is_node16_or_node18 (gates TS1479 emission)
// =============================================================================

#[test]
fn test_module_kind_is_node16_or_node18() {
    // Per TSC 6.0+, TS1479 (CJS importing ESM) is only emitted for Node16/Node18.
    // Node20+ supports `require()` of ESM, so the diagnostic is suppressed.
    for &variant in ALL_MODULE_KINDS {
        let expected = matches!(variant, ModuleKind::Node16 | ModuleKind::Node18);
        assert_eq!(
            variant.is_node16_or_node18(),
            expected,
            "is_node16_or_node18 wrong for {variant:?}"
        );
    }
}

// =============================================================================
// ModuleKind - is_commonjs full coverage
// =============================================================================

#[test]
fn test_module_kind_is_commonjs_full_set() {
    // CommonJS-like = CJS, UMD, and all Node-like (Node16/18/20/NodeNext).
    for &variant in ALL_MODULE_KINDS {
        let expected = matches!(
            variant,
            ModuleKind::CommonJS
                | ModuleKind::UMD
                | ModuleKind::Node16
                | ModuleKind::Node18
                | ModuleKind::Node20
                | ModuleKind::NodeNext
        );
        assert_eq!(
            variant.is_commonjs(),
            expected,
            "is_commonjs wrong for {variant:?}"
        );
    }
}

// =============================================================================
// ModuleKind - from_ts_str normalization
// =============================================================================

#[test]
fn test_module_kind_from_ts_str_normalizes_internal_separators() {
    // Dashes, underscores, and case are normalized.
    assert_eq!(
        ModuleKind::from_ts_str("Common-JS"),
        Some(ModuleKind::CommonJS)
    );
    assert_eq!(
        ModuleKind::from_ts_str("NODE-NEXT"),
        Some(ModuleKind::NodeNext)
    );
    assert_eq!(
        ModuleKind::from_ts_str("common_js"),
        Some(ModuleKind::CommonJS)
    );
    assert_eq!(ModuleKind::from_ts_str("node_16"), Some(ModuleKind::Node16));
    assert_eq!(
        ModuleKind::from_ts_str("  esnext  "),
        Some(ModuleKind::ESNext)
    );
}

#[test]
fn test_module_kind_from_ts_str_invalid_returns_none() {
    assert_eq!(ModuleKind::from_ts_str(""), None);
    assert_eq!(ModuleKind::from_ts_str("   "), None);
    assert_eq!(ModuleKind::from_ts_str("globalish"), None);
    assert_eq!(ModuleKind::from_ts_str("rollup"), None);
    // Empty first comma token still None.
    assert_eq!(ModuleKind::from_ts_str(",commonjs"), None);
}

// =============================================================================
// from_ts_numeric out-of-range gaps return None
// =============================================================================

#[test]
fn test_script_target_from_ts_numeric_gaps_return_none() {
    // 13..98 is a gap between ES2025 (12) and ESNext (99).
    assert_eq!(ScriptTarget::from_ts_numeric(13), None);
    assert_eq!(ScriptTarget::from_ts_numeric(98), None);
    assert_eq!(ScriptTarget::from_ts_numeric(100), None);
    assert_eq!(ScriptTarget::from_ts_numeric(u32::MAX), None);
}

#[test]
fn test_module_kind_from_ts_numeric_gaps_return_none() {
    // 8..98 gap between ES2022 (7) and ESNext (99).
    assert_eq!(ModuleKind::from_ts_numeric(8), None);
    // 103..198 gap between Node20 (102) and NodeNext (199).
    assert_eq!(ModuleKind::from_ts_numeric(103), None);
    assert_eq!(ModuleKind::from_ts_numeric(198), None);
    // 201+ out of range.
    assert_eq!(ModuleKind::from_ts_numeric(201), None);
    assert_eq!(ModuleKind::from_ts_numeric(u32::MAX), None);
}

// =============================================================================
// ScriptTarget::is_es5
// =============================================================================

#[test]
fn test_script_target_is_es5_pre_es2015_only() {
    for &variant in ALL_SCRIPT_TARGETS {
        let expected = matches!(variant, ScriptTarget::ES3 | ScriptTarget::ES5);
        assert_eq!(variant.is_es5(), expected, "is_es5 wrong for {variant:?}");
    }
}

// =============================================================================
// Visibility + NewLineKind small contracts
// =============================================================================

#[test]
fn test_visibility_default_and_distinct_variants() {
    let v: Visibility = Default::default();
    assert_eq!(v, Visibility::Public);
    assert_ne!(Visibility::Public, Visibility::Private);
    assert_ne!(Visibility::Private, Visibility::Protected);
    assert_ne!(Visibility::Public, Visibility::Protected);
}

#[test]
fn test_newline_kind_default_is_line_feed_and_distinct() {
    let nl: NewLineKind = Default::default();
    assert_eq!(nl, NewLineKind::LineFeed);
    assert_ne!(NewLineKind::LineFeed, NewLineKind::CarriageReturnLineFeed);
}
