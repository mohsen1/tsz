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
