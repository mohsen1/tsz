use super::check_module_resolution_compatibility;
use crate::config::ResolvedCompilerOptions;
use tsz::config::ModuleResolutionKind;
use tsz_common::common::ModuleKind;

#[test]
fn test_module_resolution_requires_matching_module() {
    let mut resolved = ResolvedCompilerOptions::default();
    resolved.module_resolution = Some(ModuleResolutionKind::Node16);
    resolved.printer.module = ModuleKind::CommonJS;

    let diag = check_module_resolution_compatibility(&resolved, None);
    assert!(diag.is_some());
}
