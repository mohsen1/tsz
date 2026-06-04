use crate::class_checker::ClassMemberInfo;

use crate::flow_analysis::{ComputedKey, PropertyKey};

use crate::query_boundaries::common::{callable_shape_for_type, object_shape_for_type};

use crate::query_boundaries::definite_assignment::constructor_assigned_properties;

use crate::state::CheckerState;

use rustc_hash::{FxHashMap, FxHashSet};

use tsz_lowering::TypeLowering;

use tsz_parser::parser::NodeIndex;

use tsz_parser::parser::syntax_kind_ext;

use tsz_scanner::SyntaxKind;

use tsz_solver::computation::TypeSubstitution;

use tsz_solver::{TypeId, Visibility};

#[derive(Clone)]
pub(crate) struct ClassPropertyInitializationInfo {
    pub(crate) name_idx: NodeIndex,
    pub(crate) key: Option<PropertyKey>,
    pub(crate) lookup_name: Option<String>,
    pub(crate) display_name: String,
    pub(crate) position: usize,
    pub(crate) has_no_initializer: bool,
    pub(crate) is_abstract: bool,
    pub(crate) requires_initialization: bool,
}

#[derive(Clone, Default)]
pub(crate) struct ClassInitializationSummary {
    pub(crate) requires_super: bool,
    pub(crate) constructor_body: Option<NodeIndex>,
    pub(crate) has_super_call_position_sensitive_members: bool,
    pub(crate) all_instance_field_keys: FxHashSet<PropertyKey>,
    /// Fields to check for TS2565 "used before assigned". Includes ES-decorated fields that
    /// are excluded from TS2564 strict-init tracking in `required_instance_fields`.
    pub(crate) ts2565_field_keys: FxHashSet<PropertyKey>,
    pub(crate) parameter_property_names: FxHashSet<String>,
    pub(crate) field_initializer_keys: FxHashSet<PropertyKey>,
    pub(crate) constructor_assigned_fields: FxHashSet<PropertyKey>,
    pub(crate) required_instance_fields: Vec<ClassPropertyInitializationInfo>,
    member_positions: FxHashMap<NodeIndex, usize>,
    instance_property_by_name: FxHashMap<String, usize>,
    ordered_instance_properties: Vec<ClassPropertyInitializationInfo>,
}

impl ClassInitializationSummary {
    pub(crate) fn member_position(&self, member_idx: NodeIndex) -> Option<usize> {
        self.member_positions.get(&member_idx).copied()
    }

    pub(crate) fn instance_property_named(
        &self,
        name: &str,
    ) -> Option<&ClassPropertyInitializationInfo> {
        self.instance_property_by_name
            .get(name)
            .and_then(|&idx| self.ordered_instance_properties.get(idx))
    }
}

/// Unified per-member entry that stores all attributes in one allocation.
/// Replaces 3 separate hashmaps (lookup, `display_name`, kind) per axis.
#[derive(Clone)]
pub(crate) struct MemberEntry {
    pub(crate) info: ClassMemberInfo,
    pub(crate) display_name: String,
    pub(crate) kind: ClassMemberKind,
    pub(crate) is_visible: bool,
}

#[derive(Clone, Default)]
struct ClassOwnMemberSummary {
    initialization: ClassInitializationSummary,
    /// Unified instance member map: name -> entry (replaces 6 separate maps)
    instance_members: FxHashMap<String, MemberEntry>,
    /// Unified static member map: name -> entry (replaces 6 separate maps)
    static_members: FxHashMap<String, MemberEntry>,
    /// Externally-visible overload-method types per instance method name.
    /// Mirrors `ClassChainSummary::instance_method_overloads`, but in the
    /// owning class's own type-parameter scope (no substitution applied).
    instance_method_overloads: FxHashMap<String, TypeId>,
    /// Externally-visible overload-method types per static method name.
    static_method_overloads: FxHashMap<String, TypeId>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum ClassMemberKind {
    MethodLike,
    FieldLike,
}

#[derive(Clone, Default)]
pub(crate) struct ClassChainSummary {
    /// Unified instance member map: name -> entry (replaces 6 maps + 1 set)
    instance_members: FxHashMap<String, MemberEntry>,
    /// Unified static member map: name -> entry (replaces 6 maps + 1 set)
    static_members: FxHashMap<String, MemberEntry>,
    /// Externally-visible overload-method types per instance method name.
    /// An entry exists for each method that has multiple `METHOD_DECLARATION`
    /// nodes at the level of the chain that first declares it. The TypeId
    /// is a `CallableShape` whose `call_signatures` are the externally
    /// visible overload signatures (bodyless declarations if any, otherwise
    /// the single implementation signature). Types are substituted into the
    /// root class's type-parameter scope by the chain summary.
    instance_method_overloads: FxHashMap<String, TypeId>,
    /// Externally-visible overload-method types per static method name.
    static_method_overloads: FxHashMap<String, TypeId>,
}

impl ClassChainSummary {
    pub(crate) fn lookup(
        &self,
        target_name: &str,
        target_is_static: bool,
        skip_private: bool,
    ) -> Option<&ClassMemberInfo> {
        self.member_info(target_name, target_is_static, skip_private)
    }

    pub(crate) fn member_info(
        &self,
        target_name: &str,
        target_is_static: bool,
        skip_private: bool,
    ) -> Option<&ClassMemberInfo> {
        let map = if target_is_static {
            &self.static_members
        } else {
            &self.instance_members
        };
        map.get(target_name).and_then(|entry| {
            if skip_private && !entry.is_visible {
                None
            } else {
                Some(&entry.info)
            }
        })
    }

    pub(crate) fn member_kind(
        &self,
        target_name: &str,
        target_is_static: bool,
        skip_private: bool,
    ) -> Option<ClassMemberKind> {
        let map = if target_is_static {
            &self.static_members
        } else {
            &self.instance_members
        };
        map.get(target_name).and_then(|entry| {
            if skip_private && !entry.is_visible {
                None
            } else {
                Some(entry.kind)
            }
        })
    }

    pub(crate) fn member_display_name(
        &self,
        target_name: &str,
        target_is_static: bool,
        skip_private: bool,
    ) -> Option<&str> {
        let map = if target_is_static {
            &self.static_members
        } else {
            &self.instance_members
        };
        map.get(target_name).and_then(|entry| {
            if skip_private && !entry.is_visible {
                None
            } else {
                Some(entry.display_name.as_str())
            }
        })
    }

    /// Get the set of visible instance member names.
    pub(crate) fn visible_instance_names(&self) -> impl Iterator<Item = &String> {
        self.instance_members
            .iter()
            .filter(|(_, entry)| entry.is_visible)
            .map(|(name, _)| name)
    }

    /// Get the set of visible static member names.
    pub(crate) fn visible_static_names(&self) -> impl Iterator<Item = &String> {
        self.static_members
            .iter()
            .filter(|(_, entry)| entry.is_visible)
            .map(|(name, _)| name)
    }

    /// Externally-visible overload-method type (substituted) for the named
    /// method on this chain, if it has multiple declarations. Returns `None`
    /// for non-overloaded methods and for non-method members.
    pub(crate) fn method_overload_type(
        &self,
        target_name: &str,
        target_is_static: bool,
    ) -> Option<TypeId> {
        let map = if target_is_static {
            &self.static_method_overloads
        } else {
            &self.instance_method_overloads
        };
        map.get(target_name).copied()
    }
}

#[derive(Clone)]
struct JsImplicitMemberName {
    lookup_name: String,
    display_name: String,
}

struct ClassMemberKindLookup {
    kind: ClassMemberKind,
    display_name: String,
    is_visible: bool,
}
