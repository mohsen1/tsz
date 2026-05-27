use crate::TypeLowering;
use tsz_parser::parser::NodeArena;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::ParserState;
use tsz_parser::parser::syntax_kind_ext;
use tsz_solver::construction::TypeInterner;
use tsz_solver::*;

include!("lower_tests_parts/part_00.rs");
include!("lower_tests_parts/part_01.rs");
