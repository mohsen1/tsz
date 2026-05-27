//! Union/intersection restricted-property (`private`/`protected`) presence rule.
//!
//! Extracted from `property_checker.rs` to keep that module under the 2000-LOC
//! ceiling. Mirrors tsc's `createUnionOrIntersectionProperty`: a union exposes a
//! restricted member only when every constituent shares a common declaration of
//! it, and an intersection constituent contributes the declarations of all of
//! its parts.

use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    /// AST classes in which `property_name` is declared as a *restricted*
    /// (private/protected) member, reachable from `member`.
    ///
    /// A single class contributes its own (possibly inherited) declaring class.
    /// An intersection contributes every constituent's restricted declaration,
    /// because an intersection's apparent property merges the declarations of
    /// all its constituents. `None` means the member exposes `property_name`
    /// only publicly, lacks it, or is not a class type — in any of those cases
    /// it cannot share a restricted declaration with another union member.
    fn restricted_property_declarations(
        &mut self,
        member: TypeId,
        property_name: &str,
        is_static: bool,
    ) -> Option<Vec<NodeIndex>> {
        let intersection_parts =
            crate::query_boundaries::property_access::intersection_members(self.ctx.types, member)
                .or_else(|| {
                    self.ctx.types.get_display_alias(member).and_then(|alias| {
                        crate::query_boundaries::property_access::intersection_members(
                            self.ctx.types,
                            alias,
                        )
                    })
                });
        if let Some(parts) = intersection_parts {
            let mut declarations: Vec<NodeIndex> = Vec::new();
            for part in parts {
                let part = self.resolve_type_for_property_access(part);
                if let Some(class_idx) = self.get_class_decl_from_type(part)
                    && let Some(info) =
                        self.find_member_access_info(class_idx, property_name, is_static)
                    && !declarations.contains(&info.declaring_class_idx)
                {
                    declarations.push(info.declaring_class_idx);
                }
            }
            return (!declarations.is_empty()).then_some(declarations);
        }

        let resolved = self.resolve_type_for_property_access(member);
        let class_idx = self.get_class_decl_from_type(resolved)?;
        let info = self.find_member_access_info(class_idx, property_name, is_static)?;
        Some(vec![info.declaring_class_idx])
    }

    /// tsc exposes a restricted (private/protected) member on a union only when
    /// every constituent shares a *common declaration* of it — not merely an
    /// identically-named symbol (see the
    /// `unionPropertyOfProtectedAndIntersectionProperty` conformance test). A
    /// constituent that exposes the member publicly, lacks it, or declares it
    /// in an unrelated class breaks that sharing and makes the property absent.
    ///
    /// The "common declaration" comparison is keyed on declaring-class identity,
    /// so it is independent of the class/property/type-parameter names chosen.
    pub(crate) fn union_restricted_property_is_missing(
        &mut self,
        property_name: &str,
        object_type: TypeId,
    ) -> bool {
        use crate::query_boundaries::state::checking;

        if self.ctx.enclosing_class.is_some() {
            return false;
        }

        let Some(members) = checking::union_members(self.ctx.types, object_type) else {
            return false;
        };

        if members.len() < 2 {
            return false;
        }

        let is_static = self.is_constructor_type(object_type);

        let mut has_restricted = false;
        let mut has_conflict = false;
        // Declarations of the first constituent that exposes the member as
        // restricted; later constituents must share at least one of them.
        let mut shared_declarations: Option<Vec<NodeIndex>> = None;

        for member in members {
            match self.restricted_property_declarations(member, property_name, is_static) {
                Some(declarations) => {
                    has_restricted = true;
                    match &shared_declarations {
                        None => shared_declarations = Some(declarations),
                        Some(reference) => {
                            if !declarations.iter().any(|decl| reference.contains(decl)) {
                                has_conflict = true;
                            }
                        }
                    }
                }
                // Public, missing, or non-class member: a different declaration
                // from any restricted member.
                None => has_conflict = true,
            }
        }

        has_restricted && has_conflict
    }
}
