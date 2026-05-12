use crate::state::{CheckerState, MemberAccessInfo, MemberAccessLevel, MemberLookup};
use rustc_hash::FxHashSet;
use tsz_parser::parser::NodeIndex;
use tsz_solver::Visibility;

impl<'a> CheckerState<'a> {
    pub(crate) fn find_member_access_info(
        &self,
        class_idx: NodeIndex,
        name: &str,
        is_static: bool,
    ) -> Option<MemberAccessInfo> {
        let mut current = class_idx;
        let mut visited: FxHashSet<NodeIndex> = FxHashSet::default();

        while visited.insert(current) {
            match self.lookup_member_access_in_class(current, name, is_static) {
                MemberLookup::Restricted(level) => {
                    return Some(MemberAccessInfo {
                        level,
                        declaring_class_idx: current,
                        declaring_class_name: self
                            .get_class_name_with_type_params_from_decl(current),
                    });
                }
                MemberLookup::Public => return None,
                MemberLookup::NotFound => {
                    let Some(base_idx) = self.get_base_class_idx(current) else {
                        return (!is_static)
                            .then(|| self.find_mixin_instance_member_access_info(current, name))
                            .flatten();
                    };
                    current = base_idx;
                }
            }
        }

        None
    }

    pub(crate) fn find_mixin_instance_member_access_info(
        &self,
        class_idx: NodeIndex,
        name: &str,
    ) -> Option<MemberAccessInfo> {
        let class_type = self
            .ctx
            .class_instance_type_cache
            .get(&class_idx)
            .copied()?;
        let name_atom = self.ctx.types.intern_string(name);
        let visibility =
            crate::query_boundaries::common::object_shape_for_type(self.ctx.types, class_type)
                .and_then(|shape| {
                    shape
                        .properties
                        .iter()
                        .find(|prop| prop.name == name_atom)
                        .map(|prop| prop.visibility)
                })?;
        let level = match visibility {
            Visibility::Private => MemberAccessLevel::Private,
            Visibility::Protected => MemberAccessLevel::Protected,
            Visibility::Public => return None,
        };
        Some(MemberAccessInfo {
            level,
            declaring_class_idx: class_idx,
            declaring_class_name: self.get_class_name_with_type_params_from_decl(class_idx),
        })
    }
}
