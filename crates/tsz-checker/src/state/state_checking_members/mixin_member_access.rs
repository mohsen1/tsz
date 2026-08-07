use crate::state::{CheckerState, MemberAccessInfo, MemberAccessLevel, MemberLookup};
use rustc_hash::FxHashSet;
use tsz_parser::parser::NodeIndex;
use tsz_solver::{PropertyInfo, TypeId, Visibility};

impl<'a> CheckerState<'a> {
    pub(crate) fn find_member_access_info(
        &mut self,
        class_idx: NodeIndex,
        name: &str,
        is_static: bool,
    ) -> Option<MemberAccessInfo> {
        // Per-file memo: classifying a member's access level walks the class's
        // declared members (and base-class chain) by name, which is `O(members)`
        // per access. Without memoization, one access per method on an
        // `N`-member class makes this `O(N^2)`. The classification is a pure
        // function of the immutable AST for the current file, keyed by
        // `(class node, member-name Atom, is_static)`. Interning the name to its
        // canonical `Atom` (`O(1)` amortized) gives an identity key that compares
        // in `O(1)`; the cached value reproduces the linear scan's result.
        let name_atom = self.ctx.types.intern_string(name);
        let cache_key = (class_idx, name_atom, is_static);
        if let Some(cached) = self.ctx.member_access_info_cache.borrow().get(&cache_key) {
            return cached.clone();
        }

        let (computed, stable) = self.compute_member_access_info_stable(class_idx, name, is_static);
        // Only memoize results that are a pure function of the immutable AST.
        // The base-chain fall-through into the mixin instance/static lookups
        // reads `class_instance_type_cache` / the base-constructor type, which
        // are populated lazily, so an early access can observe an incomplete
        // (often `None`) answer that later becomes a real restricted-access
        // classification. Caching that provisional answer froze the empty
        // result and dropped later protected/private access diagnostics
        // (e.g. `c4.p` in a sibling mixin subclass). Skip the memo in that case
        // and recompute until the lazy type caches are populated; the common
        // `this.x` hot path resolves through the pure AST scan and is cached.
        if stable {
            self.ctx
                .member_access_info_cache
                .borrow_mut()
                .insert(cache_key, computed.clone());
        }
        computed
    }

    fn compute_member_access_info(
        &mut self,
        class_idx: NodeIndex,
        name: &str,
        is_static: bool,
    ) -> Option<MemberAccessInfo> {
        self.compute_member_access_info_stable(class_idx, name, is_static)
            .0
    }

    /// Compute the member access classification and report whether the result
    /// is a pure function of the immutable AST (`stable == true`) or was derived
    /// from the lazily-populated mixin instance/static type caches
    /// (`stable == false`). Only stable results may be memoized; provisional
    /// results must be recomputed because the lazy caches can fill in later.
    fn compute_member_access_info_stable(
        &mut self,
        class_idx: NodeIndex,
        name: &str,
        is_static: bool,
    ) -> (Option<MemberAccessInfo>, bool) {
        let mut current = class_idx;
        let mut visited: FxHashSet<NodeIndex> = FxHashSet::default();

        while visited.insert(current) {
            match self.lookup_member_access_in_class(current, name, is_static) {
                MemberLookup::Restricted(level) => {
                    return (
                        Some(MemberAccessInfo {
                            level,
                            declaring_class_idx: current,
                            declaring_class_name: self
                                .get_class_name_with_type_params_from_decl(current),
                        }),
                        true,
                    );
                }
                MemberLookup::Public => return (None, true),
                MemberLookup::NotFound => {
                    let Some(base_idx) = self.get_base_class_idx(current) else {
                        // Base-chain end: the only remaining source is a mixin
                        // base whose instance/static shape comes from lazily
                        // built type caches (`class_instance_type_cache` /
                        // base-constructor type). A `None` here can be a cache
                        // miss that later resolves to a real restricted-access
                        // classification once those caches fill, so a `None` is
                        // provisional and must not be memoized. A `Some(..)` is a
                        // concrete private/protected classification read from an
                        // already-materialized shape and is stable.
                        let info = if is_static {
                            self.find_mixin_static_member_access_info(current, name)
                        } else {
                            self.find_mixin_instance_member_access_info(current, name)
                        };
                        let stable = info.is_some();
                        return (info, stable);
                    };
                    current = base_idx;
                }
            }
        }

        (None, true)
    }

    pub(crate) fn find_mixin_static_member_access_info(
        &mut self,
        class_idx: NodeIndex,
        name: &str,
    ) -> Option<MemberAccessInfo> {
        let prop = self.find_mixin_static_member_property_info(class_idx, name)?;
        let level = match prop.visibility {
            Visibility::Private => MemberAccessLevel::Private,
            Visibility::Protected => MemberAccessLevel::Protected,
            Visibility::Public => return None,
        };
        Some(MemberAccessInfo {
            level,
            declaring_class_idx: class_idx,
            declaring_class_name: format!(
                "typeof {}",
                self.get_class_name_with_type_params_from_decl(class_idx)
            ),
        })
    }

    pub(crate) fn find_mixin_static_member_type(
        &mut self,
        class_idx: NodeIndex,
        name: &str,
    ) -> Option<TypeId> {
        self.find_mixin_static_member_property_info(class_idx, name)
            .map(|prop| prop.type_id)
    }

    pub(crate) fn find_mixin_static_member_property_info(
        &mut self,
        class_idx: NodeIndex,
        name: &str,
    ) -> Option<PropertyInfo> {
        let base_ctor_type = self.mixin_base_constructor_type_for_class(class_idx)?;
        let name_atom = self.ctx.types.intern_string(name);
        let mut found = None;
        let mut visited = FxHashSet::default();
        self.collect_mixin_static_member_property_info(
            base_ctor_type,
            name_atom,
            &mut found,
            &mut visited,
        );
        found
    }

    fn collect_mixin_static_member_property_info(
        &mut self,
        type_id: TypeId,
        name: tsz_common::interner::Atom,
        found: &mut Option<PropertyInfo>,
        visited: &mut FxHashSet<TypeId>,
    ) {
        if !visited.insert(type_id) {
            return;
        }
        if let Some(members) =
            crate::query_boundaries::property_access::intersection_members(self.ctx.types, type_id)
        {
            for member in members {
                self.collect_mixin_static_member_property_info(member, name, found, visited);
            }
            return;
        }

        let Some(prop) = self.static_properties_from_type(type_id).remove(&name) else {
            return;
        };
        match found {
            None => *found = Some(prop),
            Some(existing) => {
                existing.visibility = match (existing.visibility, prop.visibility) {
                    (Visibility::Private, _) | (_, Visibility::Private) => Visibility::Private,
                    (Visibility::Public, _) | (_, Visibility::Public) => Visibility::Public,
                    (Visibility::Protected, Visibility::Protected) => Visibility::Protected,
                };
            }
        }
    }

    pub(crate) fn find_mixin_instance_member_access_info(
        &self,
        class_idx: NodeIndex,
        name: &str,
    ) -> Option<MemberAccessInfo> {
        let class_type = self
            .ctx
            .class_instance_type_cache
            .borrow()
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

    pub(crate) fn mixin_base_constructor_type_for_class(
        &mut self,
        class_idx: NodeIndex,
    ) -> Option<TypeId> {
        let class_node = self.ctx.arena.get(class_idx)?;
        let class_data = self.ctx.arena.get_class(class_node)?;
        let heritage_clauses = class_data.heritage_clauses.as_ref()?;

        for &clause_idx in &heritage_clauses.nodes {
            let heritage = self.ctx.arena.get_heritage_clause_at(clause_idx)?;
            if heritage.token != tsz_scanner::SyntaxKind::ExtendsKeyword as u16 {
                continue;
            }
            let &type_idx = heritage.types.nodes.first()?;
            let (expr_idx, type_arguments) = if let Some(type_node) = self.ctx.arena.get(type_idx)
                && let Some(expr_type_args) = self.ctx.arena.get_expr_type_args(type_node)
            {
                (
                    expr_type_args.expression,
                    expr_type_args.type_arguments.as_ref(),
                )
            } else {
                (type_idx, None)
            };
            let Some(expr_node) = self.ctx.arena.get(expr_idx) else {
                continue;
            };
            if expr_node.kind != tsz_parser::parser::syntax_kind_ext::CALL_EXPRESSION {
                continue;
            }
            return self.base_constructor_type_from_expression(expr_idx, type_arguments);
        }

        None
    }

    pub(crate) fn intersection_has_private_property_conflict(&mut self, type_id: TypeId) -> bool {
        let source_type = self.ctx.types.get_display_alias(type_id).unwrap_or(type_id);
        let Some(members) = crate::query_boundaries::property_access::intersection_members(
            self.ctx.types,
            source_type,
        ) else {
            return false;
        };
        if members.len() < 2 {
            return false;
        }

        let mut seen_private: rustc_hash::FxHashMap<
            tsz_common::interner::Atom,
            Option<tsz_binder::SymbolId>,
        > = rustc_hash::FxHashMap::default();
        let mut seen_non_private: FxHashSet<tsz_common::interner::Atom> = FxHashSet::default();

        for member in members {
            let member = self.evaluate_application_type(member);
            let member = self.resolve_type_for_property_access(member);
            let Some(shape) =
                crate::query_boundaries::common::object_shape_for_type(self.ctx.types, member)
            else {
                continue;
            };

            for prop in &shape.properties {
                let name = self.ctx.types.resolve_atom_ref(prop.name);
                if name.starts_with("__private_brand_")
                    || name.starts_with("__private_brand_node_")
                    || tsz_solver::utils::is_es_private_identifier_name(&name)
                {
                    continue;
                }

                match prop.visibility {
                    Visibility::Private => {
                        if seen_non_private.contains(&prop.name) {
                            return true;
                        }
                        if let Some(first_parent) = seen_private.get(&prop.name) {
                            if *first_parent != prop.parent_id {
                                return true;
                            }
                        } else {
                            seen_private.insert(prop.name, prop.parent_id);
                        }
                    }
                    Visibility::Protected | Visibility::Public => {
                        if seen_private.contains_key(&prop.name) {
                            return true;
                        }
                        seen_non_private.insert(prop.name);
                    }
                }
            }
        }

        false
    }
}
