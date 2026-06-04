use super::DeclarationEmitter;

use rustc_hash::FxHashSet;

use tsz_binder::symbol_flags;

use tsz_parser::parser::NodeIndex;

use tsz_parser::parser::syntax_kind_ext;

use tsz_scanner::SyntaxKind;

use tsz_solver::type_queries;

include!("mod_parts/part1.rs");
include!("mod_parts/part2.rs");

mod imports_and_modules;

mod parameters_and_heritage;

mod value_declarations;
