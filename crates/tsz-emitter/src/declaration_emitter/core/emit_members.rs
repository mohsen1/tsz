use tsz_parser::parser::node::{IndexSignatureData, MethodDeclData, Node};

use tsz_parser::parser::syntax_kind_ext;

use tsz_parser::parser::{NodeIndex, NodeList};

use tsz_scanner::SyntaxKind;

use tsz_solver::type_queries;

use super::DeclarationEmitter;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::declaration_emitter) enum ClassMemberKind {
    Property,
    Method,
    Accessor,
    Signature,
    IndexSignature,
    Constructor,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::declaration_emitter) struct ClassMemberInfo {
    pub kind: ClassMemberKind,
    pub name: Option<NodeIndex>,
    pub is_static: bool,
}

include!("emit_members_parts/part1.rs");
include!("emit_members_parts/part2.rs");
