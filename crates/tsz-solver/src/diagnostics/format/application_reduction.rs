//! Display reduction for generic type-alias `Application` nodes.
//!
//! tsc's `aliasSymbol` policy for a generic alias application is structural:
//! instantiation that *resolves the type away* — a conditional whose branch is
//! taken, a resolved indexed access or `keyof`, or an alias-forwarding chain
//! bottoming out at one of those — never stamps the enclosing alias onto the
//! result, so diagnostics render the evaluated type. Constructors that
//! *survive* instantiation (mapped, object, union, intersection) keep the
//! alias symbol and render as `Name<Args>`.
//!
//! The four reduction strategies here decide, per formatted `Application`
//! node, whether the alias surface is dropped and what to render instead. The
//! decision is memoized per `TypeId` (see
//! [`TypeFormatter::application_display_reduction`]) because the formatter
//! walks the receiver type as a tree while the type is a DAG (#13480).

use super::TypeFormatter;
use super::alias_underlying;
use crate::types::{TypeData, TypeId};
use rustc_hash::FxHashSet;

/// What an `Application` display reduction resolved to.
///
/// `Type` renders the reduced `TypeId` in place of the `Name<Args>` surface.
/// `OrderedUnion` renders the members joined with ` | ` in the given
/// (declaration) order — used for `keyof`-bodied alias applications, whose
/// resolved key union must follow the operand's property declaration order
/// (the interned union is canonically sorted and would lose it).
#[derive(Clone)]
pub(super) enum ApplicationDisplayReduction {
    Type(TypeId),
    OrderedUnion(Vec<TypeId>),
}

impl<'a> TypeFormatter<'a> {
    fn scalar_mapped_alias_application_display(
        &self,
        type_id: TypeId,
        base: TypeId,
        args: &[TypeId],
    ) -> Option<TypeId> {
        if !self.expand_scalar_mapped_alias_applications {
            return None;
        }

        let def_store = self.def_store?;
        let def_id =
            crate::type_queries::application_base_alias_def_id(self.interner, def_store, base)?;
        let def = def_store.get(def_id)?;
        if def.kind != crate::def::DefKind::TypeAlias {
            return None;
        }
        let body = crate::evaluation::evaluate::evaluate_type(self.interner, def.body?);
        let mapped_id = match self.interner.lookup(body) {
            Some(TypeData::Mapped(mapped_id)) => mapped_id,
            _ => return None,
        };
        let identity_info =
            crate::type_queries::mapped::classify_identity_mapped(self.interner, mapped_id)?;
        let source_arg_index = def
            .type_params
            .iter()
            .position(|param| param.name == identity_info.source_param_name)?;
        let evaluated = crate::type_queries::mapped::evaluate_identity_mapped_passthrough(
            self.interner,
            mapped_id,
            *args.get(source_arg_index)?,
        )?;
        if evaluated == type_id
            || evaluated == TypeId::ERROR
            || crate::type_queries::contains_type_parameters_db(self.interner, evaluated)
        {
            return None;
        }

        (crate::visitor::is_primitive_type(self.interner, evaluated)
            || matches!(self.interner.lookup(evaluated), Some(TypeData::Literal(_)))
            || evaluated == TypeId::NEVER)
            .then_some(evaluated)
    }

    /// A fully-instantiated application of a generic alias whose *declared body
    /// bottoms out at a reducing operator* — a conditional, an indexed access,
    /// a `keyof`, or an alias-forwarding chain to one — loses its alias symbol
    /// when the operator resolves: the operator resolves into its result and
    /// never stamps the enclosing alias onto it, so tsc renders the resolved
    /// type structurally for any shape — scalar (`Flatten<string[][]>` →
    /// `string`), tuple (`Parameters<F>` → `[a: number]`), object
    /// (`DeepReadonly<{ b: number }>` → `{ readonly b: number; }`, issue
    /// #10914), member type (`IdxAlias<{ x: X }>` → `X`), or key union
    /// (`KeyofAlias<{ p; q }>` → `"p" | "q"`) — not as `Name<Args>`.
    ///
    /// Returns the reduction to format in place of the application form.
    /// Returns `None` only when the alias cannot drop its name: a
    /// mapped/object/union-bodied application (`Partial<T>` keeps its alias
    /// symbol) fails the structural gate, a still-generic application is
    /// rejected (tsc only drops the alias once fully instantiated), and a
    /// result that fails to reduce (a still-deferred conditional or indexed
    /// access, or an evaluation that made no progress) keeps the application
    /// form. The distributive form is handled earlier by
    /// [`Self::distributed_conditional_application_display`].
    fn reducing_application_display(&self, type_id: TypeId) -> Option<ApplicationDisplayReduction> {
        let def_store = self.def_store?;
        // Cheap structural gate (shared with the checker boundary): the base
        // must resolve to a generic alias whose declared body chain bottoms
        // out at a reducing operator. Mapped/object/union-bodied applications
        // (`Partial<T>`, `Maybe<T> = T | undefined`) fail this and keep their
        // alias symbol.
        let body_kind = crate::type_queries::application_base_reducing_alias_body_kind(
            self.interner,
            def_store,
            type_id,
        )?;
        // tsc only drops the alias symbol once the application is fully
        // instantiated. A still-generic application (`Extract<Extract<T, Foo>,
        // Bar>` with free `T`) stays deferred and is displayed as `Name<Args>`,
        // even though the display-time evaluator may collapse the unconstrained
        // conditional to `never`. Gate on the *input* so such a result is never
        // mistaken for a finished reduction.
        if crate::type_queries::contains_type_parameters_db(self.interner, type_id) {
            return None;
        }
        // Iteratively resolve the alias-application chain. `instantiate_generic`
        // substitutes the def body directly, but the display-time evaluator has
        // no def-store resolver, so a nested alias application in the result —
        // the recursive `Flatten<string[]>` inside `Flatten<string[][]>`, or
        // the forwarded `CondResolved<string>` inside `NestedCond<string>` —
        // stays opaque after one step. Keep expanding, bounded, until the
        // result is no longer an alias application or evaluation stops making
        // progress.
        let mut current = type_id;
        let mut last_keyof_step = None;
        for _ in 0..8 {
            let Some(TypeData::Application(app_id)) = self.interner.lookup(current) else {
                break;
            };
            let app = self.interner.type_application(app_id);
            let Some(def_id) = crate::type_queries::application_base_alias_def_id(
                self.interner,
                def_store,
                app.base,
            ) else {
                break;
            };
            let Some(def) = def_store.get(def_id) else {
                break;
            };
            if def.kind != crate::def::DefKind::TypeAlias {
                break;
            }
            let Some(body) = def.body else {
                break;
            };
            // A `keyof` body is evaluated eagerly during instantiation (the
            // substituted body arrives as the key union, not `KeyOf(obj)`), so
            // remember the step whose *declared* body is the `keyof` — the
            // ordered-union path below instantiates its operand on demand to
            // recover the property declaration order. Only a `KeyOf`-terminal
            // chain consumes it.
            if body_kind == crate::type_queries::ReducingAliasBodyKind::KeyOf
                && let Some(TypeData::KeyOf(operand)) = self.interner.lookup(body)
            {
                last_keyof_step = Some((operand, def_id, app.args.to_vec()));
            }
            let instantiated = crate::computation::instantiate_generic(
                self.interner,
                body,
                &def.type_params,
                &app.args,
            );
            let evaluated = crate::evaluation::evaluate::evaluate_type(self.interner, instantiated);
            if evaluated == current {
                break;
            }
            current = evaluated;
        }
        if current == type_id
            || current == TypeId::ERROR
            || crate::type_queries::contains_type_parameters_db(self.interner, current)
        {
            return None;
        }
        // A conditional/indexed access still deferred after evaluation never
        // reduced (an unresolved operand); the raw node is no more informative
        // than the application form, so keep the application form.
        if matches!(
            self.interner.lookup(current),
            Some(TypeData::Conditional(_) | TypeData::IndexAccess(_, _))
        ) {
            return None;
        }
        if alias_underlying::application_reduces_to_displayable_shape(self.interner, current) {
            return Some(ApplicationDisplayReduction::Type(current));
        }
        // A `keyof`-bodied alias application reduces to the operand's key
        // union. The interned union is canonically sorted, but tsc renders the
        // keys in property declaration order, so reconstruct that order from
        // the instantiated operand. Bare literal and other union results stay
        // on the application surface (tsc applies literal-union display
        // widening there, a separate display concern).
        if body_kind == crate::type_queries::ReducingAliasBodyKind::KeyOf {
            let (operand, def_id, args) = last_keyof_step?;
            let def = def_store.get(def_id)?;
            let operand = crate::computation::instantiate_generic(
                self.interner,
                operand,
                &def.type_params,
                &args,
            );
            let members = self.keyof_reduction_ordered_members(operand, current)?;
            return Some(ApplicationDisplayReduction::OrderedUnion(members));
        }
        None
    }

    /// Reconstruct the property-declaration order for a resolved `keyof`
    /// reduction. `operand` is the keyof operand after substitution;
    /// `evaluated` is the resolved key union.
    ///
    /// Returns the union members as string-literal keys in the operand's
    /// property declaration order, or `None` when the shape is not the plain
    /// named-string-key case: a non-object operand, an operand with index
    /// signatures (`TypeData::ObjectWithIndex`), numeric or symbol keys, or a
    /// key set that does not exactly match the union members (so `keyof`
    /// results augmented by index signatures never render a wrong subset).
    fn keyof_reduction_ordered_members(
        &self,
        operand: TypeId,
        evaluated: TypeId,
    ) -> Option<Vec<TypeId>> {
        let operand = crate::evaluation::evaluate::evaluate_type(self.interner, operand);
        let Some(TypeData::Object(shape_id)) = self.interner.lookup(operand) else {
            return None;
        };
        let Some(TypeData::Union(list_id)) = self.interner.lookup(evaluated) else {
            return None;
        };
        let members = self.interner.type_list(list_id);
        let shape = self.interner.object_shape(shape_id);
        if shape.properties.len() != members.len() {
            return None;
        }
        // The interner sorts shape properties by atom for hash stability;
        // source declaration order lives in the 1-based `declaration_order`
        // field. A property without one (synthesized members) has no stable
        // source position, so bail to the alias surface.
        let mut props: Vec<_> = shape.properties.iter().collect();
        if props.iter().any(|prop| prop.declaration_order == 0) {
            return None;
        }
        props.sort_by_key(|prop| prop.declaration_order);
        let ordered: Vec<TypeId> = props
            .iter()
            .map(|prop| self.interner.literal_string_atom(prop.name))
            .collect();
        let member_set: FxHashSet<TypeId> = members.iter().copied().collect();
        if !ordered.iter().all(|key| member_set.contains(key)) {
            return None;
        }
        Some(ordered)
    }

    /// Memoized dispatch for the four `Application` display-reduction strategies.
    ///
    /// Tries, in order, `scalar_mapped_alias_application_display`,
    /// `distributed_conditional_application_display`,
    /// `reducing_application_display`, and
    /// `variadic_tuple_alias_application_display`; the first that fires wins and
    /// its reduction is returned for the caller to format in place of the
    /// `Name<Args>` application surface. Each strategy runs an
    /// `instantiate_generic` then `evaluate_type` over the alias body, which is
    /// expensive; because the
    /// formatter walks the receiver type as a tree but the type is a DAG, the
    /// same `Application` `TypeId` is reached through many parents. The result is
    /// memoized per `Application` `TypeId` so the cascade runs at most once per
    /// distinct node (#13480). The verdict is a pure function of `type_id` for a
    /// fixed formatter: every input derives from the application's base and args,
    /// and the interner is immutable for the formatter's lifetime.
    pub(super) fn application_display_reduction(
        &self,
        type_id: TypeId,
        app: &crate::types::TypeApplication,
    ) -> Option<ApplicationDisplayReduction> {
        if let Some(cached) = self.application_reduction_cache.borrow().get(&type_id) {
            return cached.clone();
        }
        let reduced = self
            .scalar_mapped_alias_application_display(type_id, app.base, &app.args)
            .map(ApplicationDisplayReduction::Type)
            .or_else(|| self.distributed_conditional_application_display(type_id, &app.args))
            .or_else(|| self.reducing_application_display(type_id))
            .or_else(|| {
                self.variadic_tuple_alias_application_display(app.base, &app.args)
                    .map(ApplicationDisplayReduction::Type)
            });
        self.application_reduction_cache
            .borrow_mut()
            .insert(type_id, reduced.clone());
        reduced
    }

    fn distributed_conditional_application_display(
        &self,
        type_id: TypeId,
        args: &[TypeId],
    ) -> Option<ApplicationDisplayReduction> {
        let def_store = self.def_store?;
        let check = crate::type_queries::distributive_conditional_alias_check(
            self.interner,
            def_store,
            type_id,
        )?;
        let mut members = check.members;

        let positions: Vec<_> = members
            .iter()
            .map(|&member| self.get_source_position_for_type(member, def_store))
            .collect();
        if positions.iter().all(|&(tier, _, _)| tier < 2) {
            let mut pairs: Vec<_> = members.iter().copied().zip(positions).collect();
            pairs.sort_by_key(|&(_, pos)| pos);
            members = pairs.into_iter().map(|(member, _)| member).collect();
        }

        // Only evaluate the distributed branches when the *other* type args are
        // fully concrete. If any non-check arg carries free type parameters
        // (e.g. `ChannelOfType<T, Channel>` where `T` is bound in an outer
        // scope), the conditional inside the body cannot be reliably resolved,
        // and tsc preserves the alias-application form
        // (`ChannelOfType<T, TextChannel> | ChannelOfType<T, EmailChannel>`).
        let other_args_concrete = args.iter().enumerate().all(|(i, &arg)| {
            i == check.check_index
                || !crate::visitors::visitor_predicates::contains_type_parameters(
                    self.interner,
                    arg,
                )
        });

        // Distribute into per-member branches. When other args are concrete we
        // can safely evaluate the conditional body and render the resolved
        // branch (`{ kind: "b" }`). Otherwise we keep each branch as an
        // Application so the formatter renders `Foo<member>` rather than a
        // misleading evaluation (which can collapse to `never` when relations
        // involve free type parameters).
        let distributed: Vec<TypeId> = if other_args_concrete {
            let def = def_store.get(check.def_id)?;
            members
                .iter()
                .map(|&member| {
                    let mut subst = crate::instantiation::instantiate::TypeSubstitution::new();
                    for (i, param) in def.type_params.iter().enumerate() {
                        let arg = if i == check.check_index {
                            member
                        } else {
                            match args.get(i) {
                                Some(&arg) => arg,
                                None => return TypeId::ERROR,
                            }
                        };
                        subst.insert(param.name, arg);
                    }
                    let substituted = crate::instantiation::instantiate::instantiate_type(
                        self.interner,
                        check.body,
                        &subst,
                    );
                    crate::evaluation::evaluate::evaluate_type(self.interner, substituted)
                })
                .collect()
        } else {
            let base = self.interner.lazy(check.def_id);
            members
                .iter()
                .map(|&member| {
                    let mut branch_args = args.to_vec();
                    branch_args[check.check_index] = member;
                    self.interner.application(base, branch_args)
                })
                .collect()
        };
        // Render the distributed branches directly, in the order computed
        // above. Returning the interned union `TypeId` instead would route
        // through the `Union` display arm, whose side-table origin (recorded
        // by earlier evaluation/instantiation of the same canonical union)
        // can differ from this distribution and repaint or reorder it.
        Some(ApplicationDisplayReduction::OrderedUnion(distributed))
    }

    /// Expand a raw `Application` of a *variadic* (spread) tuple type alias to
    /// its flattened tuple form for display.
    ///
    /// tsc instantiates spread tuple aliases (`Prepend<T, A> = [T, ...A]`,
    /// `Concat<A, B> = [...A, ...B]`, `IdTuple<T> = [...T]`) through tuple
    /// spreading, which yields a fresh tuple carrying no `aliasSymbol`; the
    /// display therefore shows the flattened tuple, not the alias name.
    fn variadic_tuple_alias_application_display(
        &self,
        base: TypeId,
        args: &[TypeId],
    ) -> Option<TypeId> {
        let def_store = self.def_store?;
        let def_id =
            crate::type_queries::application_base_alias_def_id(self.interner, def_store, base)?;
        let def = def_store.get(def_id)?;
        if def.kind != crate::def::DefKind::TypeAlias || def.type_params.len() != args.len() {
            return None;
        }
        let body = def.body?;
        // The declared body must be a tuple with at least one rest/spread
        // element — the structural marker for a variadic tuple alias.
        if !crate::type_queries::data::is_variadic_tuple(self.interner, body) {
            return None;
        }
        // Local flattening has no resolver, so only attempt it for concrete
        // arguments; generic args would leave unresolved spreads behind.
        if args.iter().any(|&arg| {
            crate::visitors::visitor_predicates::contains_type_parameters(self.interner, arg)
        }) {
            return None;
        }
        let mut subst = crate::instantiation::instantiate::TypeSubstitution::new();
        for (param, &arg) in def.type_params.iter().zip(args.iter()) {
            subst.insert(param.name, arg);
        }
        let substituted =
            crate::instantiation::instantiate::instantiate_type(self.interner, body, &subst);
        let evaluated = crate::evaluation::evaluate::evaluate_type(self.interner, substituted);
        // Require a fully flattened tuple (no leftover rest element): an
        // unresolved nested spread (e.g. `[..., ...Zip<...>]`) must keep the
        // named application form rather than render a half-expanded tuple.
        (evaluated != base
            && matches!(self.interner.lookup(evaluated), Some(TypeData::Tuple(_)))
            && !crate::type_queries::data::is_variadic_tuple(self.interner, evaluated))
        .then_some(evaluated)
    }

    /// Returns `true` when the application points to a distributive conditional
    /// alias whose `check_arg` is `boolean` or a union — i.e., the application
    /// would distribute via `distributed_conditional_application_display`. The
    /// display-alias chase should skip these so the formatter renders the
    /// structurally evaluated branches rather than redirecting back to the
    /// alias and re-entering the same evaluated form (which trips the
    /// `format_visiting` cycle protection and prints `...`).
    pub(super) fn application_alias_distributes(&self, alias_origin: TypeId) -> bool {
        self.def_store.is_some_and(|def_store| {
            crate::type_queries::application_distributes_over_union_check_arg(
                self.interner,
                def_store,
                alias_origin,
            )
        })
    }

    /// Returns `true` when an `Application` display alias is a direct application
    /// of a mapped-type alias. Those are the utility-style aliases whose
    /// evaluated object shape can collide with a hand-written literal annotation.
    pub(super) fn application_alias_base_has_mapped_body(&self, alias_origin: TypeId) -> bool {
        let Some(TypeData::Application(app_id)) = self.interner.lookup(alias_origin) else {
            return false;
        };
        let app = self.interner.type_application(app_id);
        let Some(def_store) = self.def_store else {
            return false;
        };
        let Some(def_id) =
            crate::type_queries::application_base_alias_def_id(self.interner, def_store, app.base)
        else {
            return false;
        };
        let Some(def) = def_store.get(def_id) else {
            return false;
        };
        def.kind == crate::def::DefKind::TypeAlias
            && def.body.is_some_and(|body| {
                crate::visitors::visitor_predicates::is_mapped_type(self.interner, body)
            })
    }
}
