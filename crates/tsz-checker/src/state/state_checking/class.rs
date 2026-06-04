use crate::EnclosingClassInfo;

use crate::context::TypingRequest;

use crate::flow_analysis::PropertyKey;

use crate::query_boundaries::class_type as class_query;

use crate::query_boundaries::definite_assignment::check_constructor_property_use_before_assignment;

use crate::state::CheckerState;

use rustc_hash::{FxHashMap, FxHashSet};

use tsz_parser::parser::syntax_kind_ext;

use tsz_parser::parser::{NodeIndex, NodeList};

use tsz_solver::TypeId;

include!("class_parts/part1.rs");
include!("class_parts/part2.rs");
