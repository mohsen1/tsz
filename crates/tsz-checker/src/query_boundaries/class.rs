use crate::class_checker::{ClassMemberInfo, MemberVisibility};
use crate::classes_domain::class_summary::{
    ClassChainSummary, ClassInitializationSummary, ClassMemberKind,
};
use crate::state::CheckerState;
use rustc_hash::{FxHashMap, FxHashSet};
use tsz_parser::NodeIndex;
use tsz_solver::TypeId;

// =============================================================================
// Relation boundary helpers (thin wrappers over assignability)
// =============================================================================

pub(crate) fn should_report_member_type_mismatch(
    checker: &mut CheckerState<'_>,
    source: TypeId,
    target: TypeId,
    node_idx: NodeIndex,
) -> bool {
    checker.should_report_assignability_mismatch(source, target, node_idx)
}

pub(crate) fn should_report_member_type_mismatch_bivariant(
    checker: &mut CheckerState<'_>,
    source: TypeId,
    target: TypeId,
    node_idx: NodeIndex,
) -> bool {
    checker.should_report_assignability_mismatch_bivariant(source, target, node_idx)
}

// =============================================================================
// ClassMemberClosure — unified class/member/base-chain boundary summary
// =============================================================================

/// Summary of a single class's own members, extracted in one pass.
///
/// Contains ALL members (including private) and provides filtered views
/// for visible-only queries. Replaces the previous double-extraction pattern
/// where `extract_class_member_info` was called twice per member.
#[derive(Clone, Default)]
#[allow(dead_code)]
pub(crate) struct OwnMemberSummary {
    /// All instance members (including private).
    pub(crate) all_instance_members: Vec<ClassMemberInfo>,
    /// All static members (including private).
    pub(crate) all_static_members: Vec<ClassMemberInfo>,
    /// Display names keyed by lookup name (all visibility).
    pub(crate) all_instance_display_names: FxHashMap<String, String>,
    pub(crate) all_static_display_names: FxHashMap<String, String>,
    /// Member kinds keyed by lookup name (all visibility).
    pub(crate) all_instance_kinds: FxHashMap<String, ClassMemberKind>,
    pub(crate) all_static_kinds: FxHashMap<String, ClassMemberKind>,
    /// Parameter properties from the constructor (all visibility).
    pub(crate) all_parameter_properties: Vec<ClassMemberInfo>,
}

#[allow(dead_code)]
impl OwnMemberSummary {
    /// Iterate visible (non-private) instance members.
    pub(crate) fn visible_instance_members(&self) -> impl Iterator<Item = &ClassMemberInfo> {
        self.all_instance_members
            .iter()
            .filter(|m| m.visibility != MemberVisibility::Private)
    }

    /// Iterate visible (non-private) static members.
    pub(crate) fn visible_static_members(&self) -> impl Iterator<Item = &ClassMemberInfo> {
        self.all_static_members
            .iter()
            .filter(|m| m.visibility != MemberVisibility::Private)
    }

    /// Iterate visible (non-private) parameter properties.
    pub(crate) fn visible_parameter_properties(&self) -> impl Iterator<Item = &ClassMemberInfo> {
        self.all_parameter_properties
            .iter()
            .filter(|m| m.visibility != MemberVisibility::Private)
    }

    /// Visible instance display names (filters out private members).
    pub(crate) fn visible_instance_display_names(&self) -> FxHashMap<String, String> {
        self.all_instance_members
            .iter()
            .filter(|m| m.visibility != MemberVisibility::Private)
            .filter_map(|m| {
                self.all_instance_display_names
                    .get(&m.name)
                    .map(|dn| (m.name.clone(), dn.clone()))
            })
            .collect()
    }

    /// Visible static display names (filters out private members).
    pub(crate) fn visible_static_display_names(&self) -> FxHashMap<String, String> {
        self.all_static_members
            .iter()
            .filter(|m| m.visibility != MemberVisibility::Private)
            .filter_map(|m| {
                self.all_static_display_names
                    .get(&m.name)
                    .map(|dn| (m.name.clone(), dn.clone()))
            })
            .collect()
    }

    /// Visible instance member kinds.
    pub(crate) fn visible_instance_kinds(&self) -> FxHashMap<String, ClassMemberKind> {
        self.all_instance_members
            .iter()
            .filter(|m| m.visibility != MemberVisibility::Private)
            .filter_map(|m| {
                self.all_instance_kinds
                    .get(&m.name)
                    .map(|k| (m.name.clone(), *k))
            })
            .collect()
    }

    /// Visible static member kinds.
    pub(crate) fn visible_static_kinds(&self) -> FxHashMap<String, ClassMemberKind> {
        self.all_static_members
            .iter()
            .filter(|m| m.visibility != MemberVisibility::Private)
            .filter_map(|m| {
                self.all_static_kinds
                    .get(&m.name)
                    .map(|k| (m.name.clone(), *k))
            })
            .collect()
    }

    /// Set of all instance member names (all visibility).
    pub(crate) fn all_instance_names(&self) -> FxHashSet<String> {
        self.all_instance_members
            .iter()
            .map(|m| m.name.clone())
            .collect()
    }

    /// Set of all static member names (all visibility).
    pub(crate) fn all_static_names(&self) -> FxHashSet<String> {
        self.all_static_members
            .iter()
            .map(|m| m.name.clone())
            .collect()
    }

    /// Set of visible instance member names.
    pub(crate) fn visible_instance_names(&self) -> FxHashSet<String> {
        self.visible_instance_members()
            .map(|m| m.name.clone())
            .collect()
    }

    /// Set of visible static member names.
    pub(crate) fn visible_static_names(&self) -> FxHashSet<String> {
        self.visible_static_members()
            .map(|m| m.name.clone())
            .collect()
    }

    /// Lookup a member by name from the "all" (including private) sets.
    pub(crate) fn find_member(&self, name: &str, is_static: bool) -> Option<&ClassMemberInfo> {
        let members = if is_static {
            &self.all_static_members
        } else {
            &self.all_instance_members
        };
        members.iter().find(|m| m.name == name)
    }

    /// Lookup a visible member by name.
    pub(crate) fn find_visible_member(
        &self,
        name: &str,
        is_static: bool,
    ) -> Option<&ClassMemberInfo> {
        if is_static {
            self.visible_static_members().find(|m| m.name == name)
        } else {
            self.visible_instance_members().find(|m| m.name == name)
        }
    }
}

/// Combined closure of a class's own members, its base chain, and initialization summary.
///
/// This is the primary boundary type for class-checking consumers. It captures
/// everything needed for override checking, property inheritance compatibility,
/// strict property initialization, and parameter property validation.
///
/// Computed once per class via `build_class_member_closure()` and consumed by
/// all class-checking paths, replacing ad-hoc re-extraction of member info.
#[derive(Clone)]
#[allow(dead_code)]
pub(crate) struct ClassMemberClosure {
    /// Own members of this class (single-pass extraction, all visibility levels).
    pub(crate) own: OwnMemberSummary,
    /// Base class chain summary (excluding this class). Only `Some` when the
    /// class has a resolved base class.
    pub(crate) base_chain: Option<ClassChainSummary>,
    /// Initialization summary (field keys, constructor assignment, etc.).
    pub(crate) initialization: ClassInitializationSummary,
}

#[allow(dead_code)]
impl ClassMemberClosure {
    /// Get base chain member names usable for override checking (visible instance).
    pub(crate) fn base_visible_instance_names(&self) -> FxHashSet<String> {
        self.base_chain
            .as_ref()
            .map_or_else(FxHashSet::default, |chain| {
                chain.visible_instance_names.clone()
            })
    }

    /// Get base chain member names usable for override checking (visible static).
    pub(crate) fn base_visible_static_names(&self) -> FxHashSet<String> {
        self.base_chain
            .as_ref()
            .map_or_else(FxHashSet::default, |chain| {
                chain.visible_static_names.clone()
            })
    }

    /// Look up a base chain member by name.
    pub(crate) fn find_base_member(
        &self,
        name: &str,
        is_static: bool,
        skip_private: bool,
    ) -> Option<&ClassMemberInfo> {
        self.base_chain
            .as_ref()
            .and_then(|chain| chain.lookup(name, is_static, skip_private))
    }

    /// Whether this class has a resolved base class chain.
    pub(crate) fn has_base_chain(&self) -> bool {
        self.base_chain.is_some()
    }
}

// =============================================================================
// Construction boundary function
// =============================================================================

/// Build a class member closure: own members + base chain + initialization.
///
/// This is the canonical entry point for class-checking consumers. It replaces
/// ad-hoc patterns of calling `extract_class_member_info` in loops, walking
/// the base chain manually, and separately computing initialization summaries.
///
/// The `base_class_idx` is the resolved base class node index (from heritage
/// clause resolution). Pass `None` if the class has no base.
#[allow(dead_code)]
pub(crate) fn build_class_member_closure(
    checker: &mut CheckerState<'_>,
    class_idx: NodeIndex,
    class_data: &tsz_parser::parser::node::ClassData,
    base_class_idx: Option<NodeIndex>,
) -> ClassMemberClosure {
    let own = build_own_member_summary(checker, class_data);
    let base_chain = base_class_idx.map(|base_idx| checker.summarize_class_chain(base_idx));
    let initialization = checker.summarize_class_initialization(class_idx, class_data);
    ClassMemberClosure {
        own,
        base_chain,
        initialization,
    }
}

/// Build the own-member summary for a class via single-pass extraction.
///
/// Extracts each member once (with `skip_private=false`) and records it.
/// Visibility filtering is done lazily via `OwnMemberSummary` accessors.
pub(crate) fn build_own_member_summary(
    checker: &mut CheckerState<'_>,
    class_data: &tsz_parser::parser::node::ClassData,
) -> OwnMemberSummary {
    use tsz_parser::parser::syntax_kind_ext;

    let mut summary = OwnMemberSummary::default();

    for &member_idx in &class_data.members.nodes {
        let Some(member_node) = checker.ctx.arena.get(member_idx) else {
            continue;
        };

        // Extract member info once (skip_private=false → all members)
        if let Some(info) = checker.extract_class_member_info(member_idx, false) {
            let kind = member_kind(&info);
            let display_name = checker
                .get_member_name_display_text(info.name_idx)
                .unwrap_or_else(|| info.name.clone());

            if info.is_static {
                summary
                    .all_static_display_names
                    .entry(info.name.clone())
                    .or_insert(display_name);
                summary
                    .all_static_kinds
                    .entry(info.name.clone())
                    .or_insert(kind);
                summary.all_static_members.push(info);
            } else {
                summary
                    .all_instance_display_names
                    .entry(info.name.clone())
                    .or_insert(display_name);
                summary
                    .all_instance_kinds
                    .entry(info.name.clone())
                    .or_insert(kind);
                summary.all_instance_members.push(info);
            }
        }

        // Extract parameter properties from constructors
        if member_node.kind == syntax_kind_ext::CONSTRUCTOR {
            let Some(ctor) = checker.ctx.arena.get_constructor(member_node) else {
                continue;
            };
            for &param_idx in &ctor.parameters.nodes {
                let Some(param_node) = checker.ctx.arena.get(param_idx) else {
                    continue;
                };
                let Some(param) = checker.ctx.arena.get_parameter(param_node) else {
                    continue;
                };
                if !checker.has_parameter_property_modifier(&param.modifiers) {
                    continue;
                }
                // Extract parameter property once (skip_private=false)
                if let Some(info) = checker.parameter_property_member_info(param_idx, param, false)
                {
                    let display_name = checker
                        .get_member_name_display_text(info.name_idx)
                        .unwrap_or_else(|| info.name.clone());
                    summary
                        .all_instance_display_names
                        .entry(info.name.clone())
                        .or_insert(display_name);
                    let kind = member_kind(&info);
                    summary
                        .all_instance_kinds
                        .entry(info.name.clone())
                        .or_insert(kind);
                    summary.all_parameter_properties.push(info);
                }
            }
        }
    }

    summary
}

/// Derive member kind classification from member info.
const fn member_kind(info: &ClassMemberInfo) -> ClassMemberKind {
    if info.is_method || info.is_accessor {
        ClassMemberKind::MethodLike
    } else {
        ClassMemberKind::FieldLike
    }
}
