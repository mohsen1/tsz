use crate::query_boundaries::type_computation::complex as query;

use crate::state::CheckerState;

use crate::symbols_domain::name_text::static_element_access_key_text_in_arena;

use tsz_parser::parser::NodeIndex;

use tsz_parser::parser::syntax_kind_ext;

use tsz_solver::TypeId;

/// Prototype-derived members collected from sibling statements surrounding a
/// constructor function. `method_bindings` are `FuncName.prototype.X = ...`
/// assignments; `this_props` are `this.X = ...` assignments inside prototype
/// method bodies; `has_evidence` is `true` when any prototype pattern was
/// observed (used to decide whether to synthesize a JS class type).
pub(crate) struct PrototypeMembers {
    pub method_bindings: Vec<(tsz_common::interner::Atom, tsz_solver::PropertyInfo)>,
    pub this_props: Vec<(tsz_common::interner::Atom, tsz_solver::PropertyInfo)>,
    pub has_evidence: bool,
}

include!("complex_constructors_parts/part1.rs");
include!("complex_constructors_parts/part2.rs");
