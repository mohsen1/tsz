use crate::transforms::async_es5_ir::AsyncES5Transformer;

use crate::transforms::ir::{
    IRMethodName, IRNode, IRParam, IRProperty, IRPropertyDescriptor, IRPropertyKey, IRPropertyKind,
};

use crate::transforms::ir_printer::IRPrinter;

use rustc_hash::FxHashSet;

use tsz_parser::parser::NodeIndex;

use tsz_parser::parser::syntax_kind_ext;

use tsz_parser::syntax::transform_utils::{
    contains_async_arrow_function, contains_new_target_reference, contains_this_keyword_reference,
    is_private_identifier,
};

use tsz_scanner::SyntaxKind;

use super::{
    ES5ClassTransformer, PropertyNameIR, collect_accessor_pairs, get_identifier_text,
    has_effective_static_modifier,
};

include!("class_es5_ir_members_parts/part1.rs");
include!("class_es5_ir_members_parts/part2.rs");
