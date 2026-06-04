use crate::transforms::ir::IRNode;

use rustc_hash::FxHashSet;

use tsz_parser::parser::syntax_kind_ext;

use tsz_parser::parser::{NodeIndex, NodeList};

use tsz_parser::syntax::transform_utils::is_private_identifier;

use tsz_scanner::SyntaxKind;

use super::{
    ES5ClassTransformer, Tc39Es5ComputedMemberInjection, Tc39Es5MemberDecorator, Tc39Es5MemberName,
    get_identifier_text, serialize_param_types, serialize_type_for_metadata,
    tc39_es5_propkey_temp_name,
};

include!("class_es5_ir_decorators_parts/part1.rs");
include!("class_es5_ir_decorators_parts/part2.rs");
