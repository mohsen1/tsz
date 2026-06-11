use crate::TypeLowering;
use tsz_parser::parser::NodeArena;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::ParserState;
use tsz_parser::parser::syntax_kind_ext;
use tsz_solver::construction::TypeInterner;
use tsz_solver::*;

include!("lower_tests_parts/helpers.rs");
include!("lower_tests_parts/fundamental_types.rs");
include!("lower_tests_parts/object_template_and_advanced_types.rs");
include!("lower_tests_parts/member_pipeline_parity.rs");
