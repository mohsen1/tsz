use super::cross_file::CrossFileQueryKind;

/// Discriminants must be stable: the `DefinitionStore` cache keys store
/// these as `u8` and re-using a discriminant for a different variant
/// would silently corrupt unrelated cache entries. If you intentionally
/// re-number variants, also bump a cache-format version somewhere
/// downstream and clear the affected cache.
#[test]
fn discriminants_match_historical_constants() {
    assert_eq!(CrossFileQueryKind::InterfaceType.as_storage_kind(), 1);
    assert_eq!(CrossFileQueryKind::ClassInstanceType.as_storage_kind(), 2);
    assert_eq!(
        CrossFileQueryKind::InterfaceMemberSimpleType.as_storage_kind(),
        3
    );
    assert_eq!(CrossFileQueryKind::SymbolType.as_storage_kind(), 4);
}
