use super::super::DeclarationEmitter;

use super::type_inference_object_unions::{
    NestedObjectMemberArmsByProperty, ObjectTypeLiteralArm, ObjectTypeLiteralEntry,
    ObjectTypeLiteralMember,
};

use tsz_parser::parser::NodeIndex;

use tsz_parser::parser::node::NodeAccess;

use tsz_parser::parser::syntax_kind_ext;

use tsz_scanner::SyntaxKind;

include!("type_inference_expression_literals_parts/part1.rs");
include!("type_inference_expression_literals_parts/part2.rs");
