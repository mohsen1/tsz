include!("emit_es6_large_methods/emit_class_es6_with_emit_options_17_2.rs");

use super::super::super::{Printer, ScriptTarget};
use super::AutoAccessorInfo;
use super::duplicate_private_names::{
    PrivateDuplicateConflictPlan, collect_private_duplicate_conflicts,
};
use super::emit_es6_after_body::ClassEs6AfterBody;
use super::emit_es6_field_inits::ClassFieldInitCollection;
use super::emit_es6_members::ClassEs6MemberEmit;
use super::emit_es6_options::ClassEs6EmitOptions;
use super::emit_es6_private_accessors::{
    PrivateAutoAccessorInfo, collect_private_auto_accessors_with_reserved,
};
use crate::emitter::core::{
    PrivateFieldStorageKind, PrivateMemberInfo, PrivateMethodDef, StaticPrivateInit,
};
use crate::transforms::private_fields_es5::{
    PrivateAccessorInfo, PrivateFieldInfo, PrivateMethodInfo,
    collect_enclosing_source_binding_names, collect_private_members_with_reserved,
    get_private_field_name, is_private_identifier, make_unique_private_name, private_helper_base,
};
use rustc_hash::FxHashMap;
use std::sync::Arc;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::{Node, NodeAccess};
use tsz_parser::parser::syntax_kind_ext;
use tsz_parser::syntax::transform_utils::{
    contains_async_arrow_function, contains_super_reference, contains_this_reference,
};
use tsz_scanner::SyntaxKind;

impl<'a> Printer<'a> {
    pub(in crate::emitter) fn emit_class_es6_with_options(
        &mut self,
        node: &Node,
        _idx: NodeIndex,
        suppress_modifiers: bool,
        assignment_prefix: Option<(&str, String)>,
        assignment_alias: Option<&str>,
        static_initializer_self_alias: Option<&str>,
        emit_assignment_static_elements_as_statements: bool,
    ) {
        self.emit_class_es6_with_emit_options(
            node,
            _idx,
            ClassEs6EmitOptions {
                suppress_modifiers,
                assignment_prefix,
                assignment_alias,
                static_initializer_self_alias,
                emit_assignment_static_elements_as_statements,
                assignment_suffix: None,
            },
        );
    }

    pub(in crate::emitter) fn emit_class_es6_assignment_with_suffix(
        &mut self,
        node: &Node,
        _idx: NodeIndex,
        assignment_target: String,
        assignment_suffix: &str,
    ) {
        self.emit_class_es6_with_emit_options(
            node,
            _idx,
            ClassEs6EmitOptions {
                suppress_modifiers: false,
                assignment_prefix: Some(("", assignment_target)),
                assignment_alias: None,
                static_initializer_self_alias: None,
                emit_assignment_static_elements_as_statements: false,
                assignment_suffix: Some(assignment_suffix),
            },
        );
    }

    __tsz_split_emit_es6_emit_class_es6_with_emit_options_17_2!();
}
