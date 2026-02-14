use super::*;

#[test]
fn test_script_target_comparisons() {
    assert!(ScriptTarget::ES3.is_es5());
    assert!(ScriptTarget::ES5.is_es5());
    assert!(!ScriptTarget::ES2015.is_es5());
    assert!(ScriptTarget::ES2015.supports_es2015());
    assert!(!ScriptTarget::ES5.supports_es2015());
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
}

#[test]
fn test_newline_kind() {
    assert_eq!(NewLineKind::LineFeed.as_str(), "\n");
    assert_eq!(NewLineKind::CarriageReturnLineFeed.as_str(), "\r\n");
    assert_eq!(NewLineKind::LineFeed.as_bytes(), b"\n");
    assert_eq!(NewLineKind::CarriageReturnLineFeed.as_bytes(), b"\r\n");
}
