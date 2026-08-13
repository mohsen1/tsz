//! Cross-base TS2320 folding for a base interface's non-interface ancestor.
//!
//! `check_interface_extension_compatibility`'s cross-base member walk
//! (`class_checker_compat.rs`) only enqueues an ancestor interface's own
//! bases onto its traversal worklist when the ancestor resolves to an
//! actual interface declaration. A base interface can itself extend a
//! non-interface type — a mapped-type application (`extends Partial<T>`) or
//! a plain type-alias application — and inherit members from it
//! structurally; those members never reached the worklist, so they were
//! silently dropped from the cross-base comparison and a real conflict went
//! unreported. tsc's `getBaseTypes` has no such declaration-kind
//! restriction: a base's fully-resolved property set is part of an
//! interface's inherited surface regardless of how it arrived there.
//!
//! Extracted from `class_checker_compat.rs` to keep that file under the
//! repo's 2000-LOC cap, mirroring the existing
//! `class_checker_compat_{overloads,this}` split.

use crate::diagnostics::diagnostic_codes;
use crate::query_boundaries::common::{
    TypeSubstitution, instantiate_type, object_shape_for_type, remove_undefined,
};
use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    /// Effective type of an optional property for the TS2320 cross-base
    /// "identical types" check.
    ///
    /// tsc's `isTypeIdenticalTo` for TS2320 compares optional properties
    /// through `addOptionality`, which unions in `undefined` unless
    /// `exactOptionalPropertyTypes` is set (in which case the property's
    /// declared type is compared as written). Two callers can disagree on
    /// whether that union has already happened before this function sees
    /// the type — a directly declared interface member's simple type
    /// already carries an unconditional `| undefined` for any `?:` member,
    /// while a property folded in from a mapped-type ancestor (e.g.
    /// `extends Partial<T>`) carries its raw declared type. Applying the
    /// union here, gated on the flag, normalizes both representations to
    /// the same effective type instead of comparing them as declared.
    pub(crate) fn ts2320_identity_type(&self, ty: TypeId, optional: bool) -> TypeId {
        if optional && !self.ctx.compiler_options.exact_optional_property_types {
            self.ctx.types.factory().union(vec![ty, TypeId::UNDEFINED])
        } else {
            ty
        }
    }

    /// Undoes `get_type_of_interface_member_simple`'s unconditional
    /// `| undefined` union for a directly declared optional interface
    /// member, so it lines up with the raw-declared convention `PropertyInfo`
    /// members (e.g. ones folded in from a mapped-type ancestor above) use
    /// for the TS2320 identity comparison.
    ///
    /// That union is correct for `get_type_of_interface_member_simple`'s
    /// other caller — cross-arena property-read delegation, where a read
    /// always sees `T | undefined` regardless of the flag — so it can't be
    /// removed at the source; it's undone here only under
    /// `exactOptionalPropertyTypes`, and only when the source annotation
    /// didn't spell `undefined` out itself.
    pub(crate) fn ts2320_declared_property_type(
        &self,
        iface_arena: &tsz_parser::parser::node::NodeArena,
        question_token: bool,
        type_annotation: NodeIndex,
        raw_member_type: TypeId,
    ) -> TypeId {
        if question_token
            && self.ctx.compiler_options.exact_optional_property_types
            && type_annotation.is_some()
            && !crate::types_domain::type_node_helpers::type_node_includes_explicit_undefined(
                iface_arena,
                type_annotation,
            )
        {
            remove_undefined(self.ctx.types.as_type_database(), raw_member_type)
        } else {
            raw_member_type
        }
    }

    /// A base interface's direct property member, resolved for the TS2320
    /// identity comparison — the delegated cross-arena type if one exists,
    /// else the local `get_type_of_interface_member_simple` computation,
    /// passed through [`Self::ts2320_declared_property_type`].
    pub(crate) fn ts2320_direct_property_type(
        &mut self,
        member_idx: NodeIndex,
        delegated_member_types: Option<&rustc_hash::FxHashMap<NodeIndex, TypeId>>,
        substitution: &TypeSubstitution,
        iface_arena: &tsz_parser::parser::node::NodeArena,
        question_token: bool,
        type_annotation: NodeIndex,
    ) -> TypeId {
        let raw = delegated_member_types
            .and_then(|types| types.get(&member_idx).copied())
            .unwrap_or_else(|| {
                instantiate_type(
                    self.ctx.types,
                    self.get_type_of_interface_member_simple(member_idx),
                    substitution,
                )
            });
        self.ts2320_declared_property_type(iface_arena, question_token, type_annotation, raw)
    }

    /// Routes one `extends` heritage entry of a base interface's own bases
    /// (its "grandparent" chain) to the right cross-base TS2320 handling: an
    /// ancestor that resolves to an interface declaration is pushed onto
    /// `worklist` to walk like any other interface base; one that doesn't
    /// (a mapped type or plain type-alias application, e.g. `extends
    /// Partial<T>`) is folded structurally instead, since it never reaches
    /// `worklist` on its own.
    ///
    /// Returns `true` when the structural fold found and reported a
    /// conflict — the caller must pop its type-parameter scope and return,
    /// matching every other early exit in `check_interface_extension_compatibility`.
    pub(crate) fn route_ancestor_for_ts2320(
        &mut self,
        ancestor_sym_id: tsz_binder::SymbolId,
        ancestor_type_args: &Option<Vec<TypeId>>,
        worklist: &mut Vec<(tsz_binder::SymbolId, NodeIndex, Option<Vec<TypeId>>)>,
        ctx: &mut Ts2320CrossBaseCtx<'_>,
    ) -> bool {
        let Some(ancestor_sym) = self.ctx.binder.get_symbol(ancestor_sym_id) else {
            return false;
        };
        let interface_entries: Vec<_> = ancestor_sym
            .declarations
            .iter()
            .filter_map(|&decl_idx| {
                let decl_arena = self.ctx.binder.arena_for_declaration_or(
                    ancestor_sym_id,
                    decl_idx,
                    self.ctx.arena,
                );
                let node = decl_arena.get(decl_idx)?;
                decl_arena.get_interface(node)?;
                Some((ancestor_sym_id, decl_idx, ancestor_type_args.clone()))
            })
            .collect();
        if !interface_entries.is_empty() {
            worklist.extend(interface_entries);
            return false;
        }
        self.fold_mapped_ancestor_into_ts2320(ancestor_sym_id, ancestor_type_args.as_deref(), ctx)
    }

    /// Folds a base interface's non-interface ancestor's properties into
    /// the same cross-base `inherited_member_sources` TS2320 check used for
    /// interface-declared ancestors, keyed by the top-level base's
    /// `type_idx`/`base_name` so the diagnostic names the base actually
    /// written in the `extends` clause, not the mapped type.
    ///
    /// Returns `true` when a conflict was found and reported — the caller
    /// must pop its type-parameter scope and return, matching every other
    /// early exit in `check_interface_extension_compatibility`.
    pub(crate) fn fold_mapped_ancestor_into_ts2320(
        &mut self,
        ancestor_sym_id: tsz_binder::SymbolId,
        ancestor_type_args: Option<&[TypeId]>,
        ctx: &mut Ts2320CrossBaseCtx<'_>,
    ) -> bool {
        let ancestor_type = match ancestor_type_args {
            Some(args) if !args.is_empty() => {
                let def_id = self.ctx.get_or_create_def_id(ancestor_sym_id);
                let lazy_type = self.ctx.types.factory().lazy(def_id);
                let app = self
                    .ctx
                    .types
                    .factory()
                    .application(lazy_type, args.to_vec());
                self.evaluate_type_with_env(app)
            }
            _ => self.get_type_of_symbol(ancestor_sym_id),
        };
        let ancestor_type = instantiate_type(self.ctx.types, ancestor_type, ctx.substitution);

        let Some(shape) = object_shape_for_type(self.ctx.types, ancestor_type) else {
            return false;
        };

        for prop in &shape.properties {
            if prop.is_method {
                continue;
            }
            let member_key = self.ctx.types.resolve_atom(prop.name);
            if !ctx.seen_member_keys.insert(member_key.clone()) {
                continue;
            }
            // A structural override of a mapped-type-inherited property is
            // left to the interface's own TS2430 override check; this
            // cross-base TS2320 check only covers properties the derived
            // interface does not redeclare.
            if ctx.derived_members.iter().any(|d| d.0 == member_key) {
                continue;
            }
            let member_type = prop.type_id;
            let member_optional = prop.optional;
            if let Some((prev_heritage_idx, prev_base_name, prev_member_type, prev_optional, _)) =
                ctx.inherited_member_sources.get(&member_key)
            {
                if *prev_heritage_idx != ctx.type_idx {
                    let optionality_differs = member_optional != *prev_optional;
                    let member_type_norm = self.ts2320_identity_type(member_type, member_optional);
                    let prev_type_norm =
                        self.ts2320_identity_type(*prev_member_type, *prev_optional);
                    let type_incompatible =
                        !self.are_var_decl_types_compatible(member_type_norm, prev_type_norm);
                    if type_incompatible || optionality_differs {
                        let (derived_name, base_name) = (ctx.derived_name, ctx.base_name);
                        self.error_at_node(
                            ctx.iface_name_node,
                            &format!(
                                "Interface '{derived_name}' cannot simultaneously extend types '{prev_base_name}' and '{base_name}'."
                            ),
                            diagnostic_codes::INTERFACE_CANNOT_SIMULTANEOUSLY_EXTEND_TYPES_AND,
                        );
                        return true;
                    }
                }
            } else {
                let entry = (
                    ctx.type_idx,
                    ctx.base_name.to_string(),
                    member_type,
                    member_optional,
                    false,
                );
                ctx.inherited_member_sources.insert(member_key, entry);
            }
        }
        false
    }
}

/// Bundles the top-level base's identity and the caller's mutable cross-base
/// bookkeeping for [`CheckerState::route_ancestor_for_ts2320`] and
/// [`CheckerState::fold_mapped_ancestor_into_ts2320`], keeping both under
/// clippy's argument-count lint without a suppression.
pub(crate) struct Ts2320CrossBaseCtx<'a> {
    pub substitution: &'a TypeSubstitution,
    pub type_idx: NodeIndex,
    pub base_name: &'a str,
    pub derived_name: &'a str,
    pub iface_name_node: NodeIndex,
    pub derived_members: &'a [(String, TypeId, NodeIndex, u16, bool, bool)],
    pub seen_member_keys: &'a mut rustc_hash::FxHashSet<String>,
    pub inherited_member_sources:
        &'a mut rustc_hash::FxHashMap<String, (NodeIndex, String, TypeId, bool, bool)>,
}
