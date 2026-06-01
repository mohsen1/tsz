use crate::emitter::Printer;
use crate::emitter::core::PrivateFieldStorageKind;
use crate::transforms::private_fields_es5::{
    PrivateFieldInfo, get_private_field_name, is_private_identifier,
};
use rustc_hash::{FxHashMap, FxHashSet};
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::ClassData;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PrivateConflictMemberKind {
    Field,
    Method,
    GetAccessor,
    SetAccessor,
}

#[derive(Debug, Clone)]
struct PrivateConflictMember {
    member_idx: NodeIndex,
    name: String,
    kind: PrivateConflictMemberKind,
    is_static: bool,
}

#[derive(Debug, Clone)]
pub(super) struct SelectedConflictField {
    pub(super) helper_name: String,
    pub(super) storage_kind: PrivateFieldStorageKind,
}

#[derive(Debug, Default)]
pub(super) struct PrivateDuplicateConflictPlan {
    conflicting_members: FxHashSet<NodeIndex>,
    selected_fields: FxHashMap<String, SelectedConflictField>,
}

impl PrivateDuplicateConflictPlan {
    pub(super) fn is_conflicting(&self, member_idx: NodeIndex) -> bool {
        self.conflicting_members.contains(&member_idx)
    }

    pub(super) fn selected_field_for(&self, name: &str) -> Option<&SelectedConflictField> {
        self.selected_fields.get(name)
    }
}

pub(super) fn collect_private_duplicate_conflicts(
    printer: &Printer<'_>,
    class: &ClassData,
    private_fields: &[PrivateFieldInfo],
) -> PrivateDuplicateConflictPlan {
    let mut members_by_name: FxHashMap<String, Vec<PrivateConflictMember>> = FxHashMap::default();
    let field_by_member: FxHashMap<NodeIndex, &PrivateFieldInfo> = private_fields
        .iter()
        .map(|field| (field.member_idx, field))
        .collect();

    for &member_idx in &class.members.nodes {
        let Some(member_node) = printer.arena.get(member_idx) else {
            continue;
        };
        match member_node.kind {
            k if k == syntax_kind_ext::PROPERTY_DECLARATION => {
                let Some(prop) = printer.arena.get_property_decl(member_node) else {
                    continue;
                };
                if !is_private_identifier(printer.arena, prop.name)
                    || printer
                        .arena
                        .has_modifier(&prop.modifiers, SyntaxKind::AbstractKeyword)
                    || printer
                        .arena
                        .has_modifier(&prop.modifiers, SyntaxKind::DeclareKeyword)
                    || printer
                        .arena
                        .has_modifier(&prop.modifiers, SyntaxKind::AccessorKeyword)
                {
                    continue;
                }
                let Some(field_name) = get_private_field_name(printer.arena, prop.name) else {
                    continue;
                };
                let clean_name = field_name.strip_prefix('#').unwrap_or(&field_name);
                members_by_name
                    .entry(clean_name.to_string())
                    .or_default()
                    .push(PrivateConflictMember {
                        member_idx,
                        name: clean_name.to_string(),
                        kind: PrivateConflictMemberKind::Field,
                        is_static: printer
                            .arena
                            .has_modifier(&prop.modifiers, SyntaxKind::StaticKeyword),
                    });
            }
            k if k == syntax_kind_ext::METHOD_DECLARATION => {
                let Some(method) = printer.arena.get_method_decl(member_node) else {
                    continue;
                };
                if !is_private_identifier(printer.arena, method.name) || method.body.is_none() {
                    continue;
                }
                let Some(field_name) = get_private_field_name(printer.arena, method.name) else {
                    continue;
                };
                let clean_name = field_name.strip_prefix('#').unwrap_or(&field_name);
                members_by_name
                    .entry(clean_name.to_string())
                    .or_default()
                    .push(PrivateConflictMember {
                        member_idx,
                        name: clean_name.to_string(),
                        kind: PrivateConflictMemberKind::Method,
                        is_static: printer
                            .arena
                            .has_modifier(&method.modifiers, SyntaxKind::StaticKeyword),
                    });
            }
            k if k == syntax_kind_ext::GET_ACCESSOR || k == syntax_kind_ext::SET_ACCESSOR => {
                let Some(accessor) = printer.arena.get_accessor(member_node) else {
                    continue;
                };
                if !is_private_identifier(printer.arena, accessor.name) {
                    continue;
                }
                let Some(field_name) = get_private_field_name(printer.arena, accessor.name) else {
                    continue;
                };
                let clean_name = field_name.strip_prefix('#').unwrap_or(&field_name);
                members_by_name
                    .entry(clean_name.to_string())
                    .or_default()
                    .push(PrivateConflictMember {
                        member_idx,
                        name: clean_name.to_string(),
                        kind: if k == syntax_kind_ext::GET_ACCESSOR {
                            PrivateConflictMemberKind::GetAccessor
                        } else {
                            PrivateConflictMemberKind::SetAccessor
                        },
                        is_static: printer
                            .arena
                            .has_modifier(&accessor.modifiers, SyntaxKind::StaticKeyword),
                    });
            }
            _ => {}
        }
    }

    let mut plan = PrivateDuplicateConflictPlan::default();
    for members in members_by_name.values() {
        if members.len() <= 1 || is_valid_private_accessor_pair(members) {
            continue;
        }
        for member in members {
            plan.conflicting_members.insert(member.member_idx);
        }
        if let Some(last_member) = members.last()
            && last_member.kind == PrivateConflictMemberKind::Field
            && let Some(field) = field_by_member.get(&last_member.member_idx)
        {
            plan.selected_fields.insert(
                last_member.name.clone(),
                SelectedConflictField {
                    helper_name: field.weakmap_name.clone(),
                    storage_kind: if last_member.is_static {
                        PrivateFieldStorageKind::Value
                    } else {
                        PrivateFieldStorageKind::WeakMap
                    },
                },
            );
        }
    }

    plan
}

fn is_valid_private_accessor_pair(members: &[PrivateConflictMember]) -> bool {
    if members.len() != 2 || members[0].is_static != members[1].is_static {
        return false;
    }
    matches!(
        (members[0].kind, members[1].kind),
        (
            PrivateConflictMemberKind::GetAccessor,
            PrivateConflictMemberKind::SetAccessor
        ) | (
            PrivateConflictMemberKind::SetAccessor,
            PrivateConflictMemberKind::GetAccessor
        )
    )
}
