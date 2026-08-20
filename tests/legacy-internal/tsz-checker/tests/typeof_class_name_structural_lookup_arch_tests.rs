/// Class-name derivation for TS18013 must use structural `TypeQuery` detection,
/// not the type renderer.
///
/// `get_private_identifier_declaring_class_name` needs the class name when the
/// receiver's type is `typeof X` (a `TypeData::TypeQuery`). The previous
/// implementation called `format_type_diagnostic`, then stripped the `"typeof "`
/// prefix from the rendered string, and filtered by character content — all
/// artifacts of using the printer as an identity oracle.
///
/// The correct approach is `classify_type_query`, which queries the solver's
/// structural representation directly and returns `TypeQueryKind::TypeQuery(sym_ref)`
/// when the type is a `TypeQuery`. The symbol name is then read from the binder,
/// not reconstructed by parsing a rendered string.
#[test]
fn typeof_class_name_derivation_uses_structural_query() {
    let src = include_str!("../types/queries/core.rs");

    assert!(
        !src.contains("strip_prefix(\"typeof \")"),
        "`get_private_identifier_declaring_class_name` must not strip a `typeof ` prefix \
         from a rendered type string; use `classify_type_query` to detect `TypeQuery` types \
         structurally"
    );

    assert!(
        src.contains("classify_type_query"),
        "`get_private_identifier_declaring_class_name` must detect `typeof X` types \
         structurally via `classify_type_query`, not by parsing the printer's output"
    );

    assert!(
        src.contains("TypeQueryKind::TypeQuery"),
        "`get_private_identifier_declaring_class_name` must match on `TypeQueryKind::TypeQuery` \
         to distinguish `typeof X` from other type kinds"
    );
}
