//! Early rough construct signatures and the self-referential rough return
//! type for partial class constructor types.
//!
//! These run at the very start of `get_class_constructor_type_inner`, before
//! the rough instance scan, so that re-entrant lookups of the class (and of
//! ctor-less subclasses computed nested inside this class's resolution
//! window) observe the correct construct-signature arity instead of the
//! default zero-parameter fallback.

use crate::query_boundaries::class_type::construct_signatures_for_type;
use crate::state::CheckerState;
use tsz_binder::SymbolId;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;
use tsz_solver::{CallSignature, TypeId, TypeParamInfo};

impl<'a> CheckerState<'a> {
    /// Self-referential instance type used as the return type of rough
    /// construct signatures: `Application(Lazy(ClassDef), [T...])` (bare
    /// `Lazy` for non-generic classes). Using the deferred reference —
    /// rather than a structural snapshot of declared members — preserves
    /// class identity so that `new C(...)` typed against a partial
    /// constructor (inside C's own static initializers, or while C's type
    /// is mid-resolution) relates to an annotated `C<T>` return type by
    /// identity instead of failing structurally against a partial member
    /// list (false TS2739/TS2740/TS2345).
    pub(super) fn rough_self_instance_reference(
        &mut self,
        current_sym: Option<SymbolId>,
        class_type_params: &[TypeParamInfo],
    ) -> Option<TypeId> {
        let sym_id = current_sym?;
        let factory = self.ctx.types.factory();
        let def_id = self.ctx.get_or_create_def_id(sym_id);
        let lazy_ref = factory.lazy(def_id);
        if class_type_params.is_empty() {
            Some(lazy_ref)
        } else {
            let args: Vec<TypeId> = class_type_params
                .iter()
                .map(|param| {
                    let name = self.ctx.types.resolve_atom_ref(param.name).to_string();
                    self.ctx
                        .type_parameter_scope
                        .get(&name)
                        .copied()
                        .unwrap_or(TypeId::ANY)
                })
                .collect();
            Some(factory.application(lazy_ref, args))
        }
    }

    /// Rough construct signatures for the partial constructor type, computed
    /// from the class's own constructor declarations, or inherited from the
    /// base class for ctor-less classes (so that `new Derived(...)` inside
    /// `Derived`'s `static create = ...` initializers reflects the base's
    /// arity instead of the default zero-argument fallback). This is a rough
    /// approximation: there is no substitution for the class's type
    /// arguments yet, so inherited signatures may reference base type
    /// parameters. For arity checking inside static initializers that's
    /// fine — the precise instantiation runs later in
    /// `get_class_constructor_type_inner`.
    pub(super) fn early_rough_construct_signatures(
        &mut self,
        class: &tsz_parser::parser::node::ClassData,
        rough_sig_return_type: TypeId,
        class_type_params: &[TypeParamInfo],
    ) -> Vec<CallSignature> {
        let mut has_ctor_overloads = false;
        for &member_idx in &class.members.nodes {
            let Some(member_node) = self.ctx.arena.get(member_idx) else {
                continue;
            };
            if member_node.kind == syntax_kind_ext::CONSTRUCTOR
                && let Some(ctor) = self.ctx.arena.get_constructor(member_node)
                && ctor.body.is_none()
            {
                has_ctor_overloads = true;
                break;
            }
        }
        let mut sigs = Vec::with_capacity(4);
        for &member_idx in &class.members.nodes {
            let Some(member_node) = self.ctx.arena.get(member_idx) else {
                continue;
            };
            if member_node.kind != syntax_kind_ext::CONSTRUCTOR {
                continue;
            }
            let Some(ctor) = self.ctx.arena.get_constructor(member_node) else {
                continue;
            };
            if has_ctor_overloads {
                if ctor.body.is_none() {
                    sigs.push(self.call_signature_from_constructor(
                        ctor,
                        member_idx,
                        rough_sig_return_type,
                        class_type_params,
                    ));
                }
            } else {
                sigs.push(self.call_signature_from_constructor(
                    ctor,
                    member_idx,
                    rough_sig_return_type,
                    class_type_params,
                ));
                break;
            }
        }
        if sigs.is_empty() {
            let inherited_rough_sigs: Option<Vec<CallSignature>> = (|| {
                let heritage_clauses = class.heritage_clauses.as_ref()?;
                for &clause_idx in &heritage_clauses.nodes {
                    let clause_node = self.ctx.arena.get(clause_idx)?;
                    let heritage = self.ctx.arena.get_heritage_clause(clause_node)?;
                    if heritage.token != SyntaxKind::ExtendsKeyword as u16 {
                        continue;
                    }
                    let &type_idx = heritage.types.nodes.first()?;
                    let type_node = self.ctx.arena.get(type_idx)?;
                    let (expr_idx, type_arguments) = if let Some(expr_type_args) =
                        self.ctx.arena.get_expr_type_args(type_node)
                    {
                        (
                            expr_type_args.expression,
                            expr_type_args.type_arguments.as_ref(),
                        )
                    } else {
                        (type_idx, None)
                    };
                    let base_constructor_type =
                        self.base_constructor_type_from_expression(expr_idx, type_arguments)?;
                    let base_sigs =
                        construct_signatures_for_type(self.ctx.types, base_constructor_type)?;
                    if base_sigs.is_empty() {
                        return None;
                    }
                    return Some(
                        base_sigs
                            .iter()
                            .map(|sig| CallSignature {
                                type_params: class_type_params.to_vec(),
                                params: sig.params.clone(),
                                this_type: sig.this_type,
                                return_type: rough_sig_return_type,
                                type_predicate: sig.type_predicate,
                                is_method: false,
                            })
                            .collect(),
                    );
                }
                None
            })();
            if let Some(inherited) = inherited_rough_sigs {
                sigs = inherited;
            } else {
                // Default construct signature (like the default constructor).
                sigs.push(CallSignature {
                    type_params: class_type_params.to_vec(),
                    params: Vec::new(),
                    this_type: None,
                    return_type: rough_sig_return_type,
                    type_predicate: None,
                    is_method: false,
                });
            }
        }
        sigs
    }

    /// Publish a partial constructor type under the class's symbol(s) so
    /// re-entrant value-position lookups of the class observe a callable
    /// with the correct construct-signature arity.
    pub(super) fn publish_partial_ctor_symbol_types(
        &mut self,
        current_sym: Option<SymbolId>,
        class_name_sym: Option<SymbolId>,
        partial_ctor: TypeId,
    ) {
        if let Some(sym_id) = current_sym {
            self.ctx.symbol_types.insert(sym_id, partial_ctor);
        }
        if let Some(name_sym) = class_name_sym {
            self.ctx.symbol_types.insert(name_sym, partial_ctor);
        }
    }

    /// Restore a symbol-type entry that was temporarily replaced by a
    /// published partial constructor type.
    pub(super) fn restore_published_symbol_type(
        &mut self,
        sym: Option<SymbolId>,
        prev: Option<TypeId>,
    ) {
        let Some(sym_id) = sym else {
            return;
        };
        match prev {
            Some(prev_type) => {
                self.ctx.symbol_types.insert(sym_id, prev_type);
            }
            None => {
                self.ctx.symbol_types.remove(&sym_id);
            }
        }
    }
}
