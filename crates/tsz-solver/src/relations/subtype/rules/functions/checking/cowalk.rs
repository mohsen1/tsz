//! #14345 WAVE-1 register-through-reduction co-walk.
//!
//! The alpha-rename registration in `check_function_subtype_impl` records the
//! two signatures' TOP-LEVEL type-param origin-pairs (source[i], target[i]).
//! But after both bodies are re-minted via `instantiate_function_shape`, the
//! reduced `Kind<F, A>` leaves reaching the relation consult carry THIRD
//! declaration origins (nested HKT type-constructor / caller-arg params) — no
//! leaf origin-pair equals a registered top-level pair, so the origin-keyed
//! consult never fires on them and two alpha-equivalent bodies fail to relate.
//!
//! This co-walk closes that gap: it walks the two re-minted bodies STRUCTURALLY
//! IN LOCKSTEP and registers the corresponding DEEPER leaf origin-pairs (the
//! arg-position `TypeParameter`s carrying authoritative declaration origins)
//! into `type_param_equivalences`.
//!
//! Soundness: a pair is registered ONLY at a structurally-CORRESPONDING
//! position — the walk descends both bodies together and, the moment the two
//! shapes DIVERGE in structural kind at any node, it stops descending that
//! branch (a genuine structural mismatch must still fail to relate). Because the
//! consult matches by exact carried declaration origin, registering the genuine
//! structural correspondence is sound; the only hazard — pairing
//! non-corresponding leaves — is prevented by the lockstep divergence guard.
//! Registration is further limited to `TypeParameter`-vs-`TypeParameter`
//! leaves where both carry authoritative declaration origins; any other leaf
//! pairing registers nothing.
//!
//! Gated behind `TSZ_DECL_ORIGIN_REDUCTION` (composing with
//! `TSZ_TYPEPARAM_DECL_IDENTITY`); flag-OFF this walk never runs.

use crate::types::{FunctionShape, TypeData, TypeId, TypeParamOrigin};

use super::super::super::super::{SubtypeChecker, TypeParamEquivalence, TypeResolver};

/// Bound on the co-walk recursion depth. The reduced bodies are finite, but
/// recursive type aliases can re-enter the same `(TypeId, TypeId)` pair; the
/// visited-pair set below is the primary cycle guard, and this depth cap is a
/// cheap secondary bound so a pathological body can never blow the stack.
const COWALK_MAX_DEPTH: u32 = 64;

impl<'a, R: TypeResolver> SubtypeChecker<'a, R> {
    /// Co-walk the two re-minted bodies in lockstep and register the deeper
    /// corresponding authoritative declaration-origin pairs. No-op unless the
    /// decl-origin-reduction flag is on.
    ///
    /// The pushed equivalences are appended after the top-level pairs the caller
    /// already registered; the caller truncates `type_param_equivalences` back to
    /// its scope start at every exit, so this augments the scope without owning
    /// its cleanup.
    pub(in crate::relations::subtype::rules::functions) fn register_cowalk_leaf_origins(
        &mut self,
        source: &FunctionShape,
        target: &FunctionShape,
    ) {
        if !Self::decl_origin_reduction_enabled() {
            return;
        }

        // Params pair positionally (the alpha-rename already aligned arity); walk
        // each corresponding param, plus `this` and the return, in lockstep.
        let mut visited: rustc_hash::FxHashSet<(TypeId, TypeId)> = rustc_hash::FxHashSet::default();
        let pair_count = source.params.len().min(target.params.len());
        for i in 0..pair_count {
            let s = source.params[i].type_id;
            let t = target.params[i].type_id;
            self.cowalk_register(s, t, 0, &mut visited);
        }
        if let (Some(s_this), Some(t_this)) = (source.this_type, target.this_type) {
            self.cowalk_register(s_this, t_this, 0, &mut visited);
        }
        self.cowalk_register(source.return_type, target.return_type, 0, &mut visited);
    }

    /// Walk `src`/`tgt` structurally in lockstep. At a `TypeParameter`-vs-
    /// `TypeParameter` leaf where both carry authoritative declaration origins,
    /// register the origin pair. Recurse into corresponding children only while
    /// the two structural kinds agree; on any kind divergence, stop (do not
    /// register past it).
    fn cowalk_register(
        &mut self,
        src: TypeId,
        tgt: TypeId,
        depth: u32,
        visited: &mut rustc_hash::FxHashSet<(TypeId, TypeId)>,
    ) {
        if depth >= COWALK_MAX_DEPTH {
            return;
        }
        // Identical ids share every leaf already; nothing new to register and no
        // reason to descend (also collapses the common re-mint fixed points).
        if src == tgt {
            return;
        }
        if !visited.insert((src, tgt)) {
            return;
        }

        let (Some(s_data), Some(t_data)) = (self.interner.lookup(src), self.interner.lookup(tgt))
        else {
            return;
        };

        match (s_data, t_data) {
            // The leaf we care about: two type parameters at the SAME structural
            // position. Register their origins when both are authoritatively
            // stamped.
            (TypeData::TypeParameter(s_info), TypeData::TypeParameter(t_info)) => {
                self.register_leaf_origin_pair(src, tgt, s_info.origin, t_info.origin);
            }

            // Applications: `Kind<F, A>` / `ReaderTaskEither<R, E, A>` etc. Walk
            // the base and each arg in lockstep. Arg-count divergence is a
            // structural mismatch — walk only the common prefix and stop (the
            // relation itself will reject a genuine arity mismatch).
            (TypeData::Application(s_app), TypeData::Application(t_app)) => {
                let s_app = self.interner.type_application(s_app);
                let t_app = self.interner.type_application(t_app);
                self.cowalk_register(s_app.base, t_app.base, depth + 1, visited);
                let n = s_app.args.len().min(t_app.args.len());
                for k in 0..n {
                    self.cowalk_register(s_app.args[k], t_app.args[k], depth + 1, visited);
                }
            }

            // Single-child transparent wrappers of the SAME kind: descend into
            // the one inner type. Grouped so the identical descent body is not
            // repeated per variant.
            (TypeData::Array(s_inner), TypeData::Array(t_inner))
            | (TypeData::ReadonlyType(s_inner), TypeData::ReadonlyType(t_inner))
            | (TypeData::KeyOf(s_inner), TypeData::KeyOf(t_inner))
            | (TypeData::NoInfer(s_inner), TypeData::NoInfer(t_inner)) => {
                self.cowalk_register(s_inner, t_inner, depth + 1, visited);
            }

            (TypeData::IndexAccess(s_obj, s_idx), TypeData::IndexAccess(t_obj, t_idx)) => {
                self.cowalk_register(s_obj, t_obj, depth + 1, visited);
                self.cowalk_register(s_idx, t_idx, depth + 1, visited);
            }

            // Unions / intersections: correspond only when the member lists have
            // the same length; pair positionally. A length mismatch is a genuine
            // structural difference — register nothing (the divergence guard).
            (TypeData::Union(s_list), TypeData::Union(t_list))
            | (TypeData::Intersection(s_list), TypeData::Intersection(t_list)) => {
                let s_members = self.interner.type_list(s_list);
                let t_members = self.interner.type_list(t_list);
                if s_members.len() == t_members.len() {
                    for (s_m, t_m) in s_members.iter().zip(t_members.iter()) {
                        self.cowalk_register(*s_m, *t_m, depth + 1, visited);
                    }
                }
            }

            (TypeData::Tuple(s_list), TypeData::Tuple(t_list)) => {
                let s_elems = self.interner.tuple_list(s_list);
                let t_elems = self.interner.tuple_list(t_list);
                if s_elems.len() == t_elems.len() {
                    for (s_e, t_e) in s_elems.iter().zip(t_elems.iter()) {
                        // Elements correspond only when their rest/optional
                        // shape matches; otherwise the tuple positions do not
                        // structurally align.
                        if s_e.rest == t_e.rest && s_e.optional == t_e.optional {
                            self.cowalk_register(s_e.type_id, t_e.type_id, depth + 1, visited);
                        }
                    }
                }
            }

            (TypeData::Conditional(s_cond), TypeData::Conditional(t_cond)) => {
                let s = self.interner.get_conditional(s_cond);
                let t = self.interner.get_conditional(t_cond);
                self.cowalk_register(s.check_type, t.check_type, depth + 1, visited);
                self.cowalk_register(s.extends_type, t.extends_type, depth + 1, visited);
                self.cowalk_register(s.true_type, t.true_type, depth + 1, visited);
                self.cowalk_register(s.false_type, t.false_type, depth + 1, visited);
            }

            (TypeData::Function(s_fn), TypeData::Function(t_fn)) => {
                let s_shape = self.interner.function_shape(s_fn);
                let t_shape = self.interner.function_shape(t_fn);
                let n = s_shape.params.len().min(t_shape.params.len());
                for k in 0..n {
                    self.cowalk_register(
                        s_shape.params[k].type_id,
                        t_shape.params[k].type_id,
                        depth + 1,
                        visited,
                    );
                }
                if let (Some(s_this), Some(t_this)) = (s_shape.this_type, t_shape.this_type) {
                    self.cowalk_register(s_this, t_this, depth + 1, visited);
                }
                self.cowalk_register(s_shape.return_type, t_shape.return_type, depth + 1, visited);
            }

            // Objects correspond property-by-property, matched by name (property
            // order is not part of structural correspondence). Only names present
            // on BOTH sides are walked; a name on one side alone is a structural
            // difference and registers nothing.
            (TypeData::Object(s_id), TypeData::Object(t_id))
            | (TypeData::ObjectWithIndex(s_id), TypeData::ObjectWithIndex(t_id)) => {
                let s_shape = self.interner.object_shape(s_id);
                let t_shape = self.interner.object_shape(t_id);
                for s_prop in &s_shape.properties {
                    if let Some(t_prop) = t_shape.properties.iter().find(|p| p.name == s_prop.name)
                    {
                        self.cowalk_register(s_prop.type_id, t_prop.type_id, depth + 1, visited);
                    }
                }
            }

            // Any other kind pairing (including differing kinds) is either a leaf
            // with no decl-origin to register or a structural divergence: stop.
            _ => {}
        }
    }

    /// Register a single deeper leaf origin-pair when both origins are
    /// authoritative and the pair is not already present. Duplicate suppression
    /// keeps the equivalence vector small and avoids re-registering the
    /// top-level pairs the caller already pushed.
    fn register_leaf_origin_pair(
        &mut self,
        src: TypeId,
        tgt: TypeId,
        s_origin: TypeParamOrigin,
        t_origin: TypeParamOrigin,
    ) {
        if !s_origin.is_decl_scoped() || !t_origin.is_decl_scoped() {
            return;
        }
        // Same declaration site on both sides is already an identity; nothing to
        // bridge.
        if s_origin == t_origin && src == tgt {
            return;
        }
        let already = self.type_param_equivalences.iter().any(|eq| {
            eq.origins == Some((s_origin, t_origin)) || eq.origins == Some((t_origin, s_origin))
        });
        if already {
            return;
        }
        self.type_param_equivalences.push(TypeParamEquivalence {
            source: src,
            target: tgt,
            origins: Some((s_origin, t_origin)),
        });
    }
}
