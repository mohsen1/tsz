use crate::context::TypingRequest;

use crate::query_boundaries::common as common_query;

use crate::query_boundaries::flow as flow_boundary;

use crate::query_boundaries::state::checking as query;

use crate::state::CheckerState;

use tsz_binder::SymbolId;

use tsz_common::interner::Atom;

use tsz_parser::parser::NodeIndex;

use tsz_parser::parser::node::NodeAccess;

use tsz_parser::parser::syntax_kind_ext;

use tsz_scanner::SyntaxKind;

use tsz_solver::TypeId;

/// Returns the tsc apparent-type display name used in destructuring TS2339
/// messages (e.g. `string` → `String`, `object` → `{}`). Returns `None` for
/// types that use their regular diagnostic formatting.
fn apparent_type_display_for_destructuring(type_id: TypeId) -> Option<String> {
    match type_id {
        TypeId::OBJECT => Some("{}".to_string()),
        TypeId::STRING => Some("String".to_string()),
        TypeId::NUMBER => Some("Number".to_string()),
        TypeId::BOOLEAN => Some("Boolean".to_string()),
        TypeId::BIGINT => Some("BigInt".to_string()),
        TypeId::SYMBOL => Some("Symbol".to_string()),
        _ => None,
    }
}
