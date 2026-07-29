use crate::{
    query_boundaries::{
        state::type_environment as query, type_defaults::fill_application_defaults,
    },
    state::CheckerState,
};
use tsz_common::Atom;
use tsz_solver::{TypeId, TypeParamInfo};

pub(crate) struct ApplicationBaseBody {
    pub(crate) body_type: TypeId,
    pub(crate) type_params: Vec<TypeParamInfo>,
}

impl<'a> CheckerState<'a> {
    /// Record semantic (non-display) provenance from an evaluated structural
    /// result back to the nominal `Application` it came from.
    ///
    /// A class/interface instantiation that checker-side evaluation lowered
    /// to a structural shape keeps a reverse link to its application so the
    /// solver relation layer can recover the generic identity for the
    /// accept-only variance fast path. Gated on the result shape carrying a
    /// nominal `symbol` (class/interface instances), so anonymous alias
    /// expansions stay unrecorded.
    pub(crate) fn record_nominal_application_eval_origin(
        &mut self,
        result: TypeId,
        application: TypeId,
    ) {
        if crate::query_boundaries::common::object_shape_for_type(self.ctx.types, result)
            .is_some_and(|shape| shape.symbol.is_some())
        {
            self.ctx
                .types
                .as_type_database()
                .record_application_eval_origin(result, application);
        }
    }

    pub(crate) fn evaluate_application_type_for_property_access(
        &mut self,
        type_id: TypeId,
    ) -> TypeId {
        let Some((base, args)) = query::application_info(self.ctx.types, type_id) else {
            return type_id;
        };
        let Some(base_def_id) = query::lazy_def_id(self.ctx.types, base) else {
            return self.evaluate_application_type(type_id);
        };
        let Some(base_def_info) = self.ctx.definition_store.get(base_def_id) else {
            return self.evaluate_application_type(type_id);
        };
        let (type_id, args) = if let Some(type_params) = self.ctx.get_def_type_params(base_def_id)
            && args.len() < type_params.len()
            && let Some(defaulted_args) =
                fill_application_defaults(self.ctx.types, &args, &type_params)
        {
            (
                self.ctx.types.application(base, defaulted_args.clone()),
                defaulted_args,
            )
        } else {
            (type_id, args)
        };
        if base_def_info
            .file_id
            .is_none_or(|file_id| file_id == self.ctx.current_file_idx as u32)
            || base_def_info.is_declare
            || !matches!(
                base_def_info.kind,
                tsz_solver::def::DefKind::Interface | tsz_solver::def::DefKind::Class
            )
        {
            // A cross-arena lib interface (e.g. `Generator<Y, R>`) whose
            // type-parameter push collided with the current file arena cannot be
            // substituted by the shared evaluator; recover before delegating. See
            // `recover_arena_collided_application_for_property_access`.
            if let Some(recovered) =
                self.recover_arena_collided_application_for_property_access(type_id)
            {
                return recovered;
            }
            return self.evaluate_application_type(type_id);
        }
        let Some(ApplicationBaseBody {
            body_type,
            type_params,
        }) = self.resolve_application_base_body(base)
        else {
            return self.evaluate_application_type(type_id);
        };

        if body_type == TypeId::ANY || body_type == TypeId::ERROR {
            return type_id;
        }
        if type_params.is_empty() {
            return body_type;
        }
        self.instantiate_application_body_for_property_access(
            body_type,
            &type_params,
            &args,
            type_id,
        )
    }

    pub(crate) fn defaulted_property_access_receiver(&mut self, type_id: TypeId) -> Option<TypeId> {
        let (base, args) =
            if let Some((base, args)) = query::application_info(self.ctx.types, type_id) {
                (base, args)
            } else if query::lazy_def_id(self.ctx.types, type_id).is_some() {
                (type_id, Vec::new())
            } else if let Some(alias) = self.ctx.types.get_display_alias(type_id)
                && let Some((base, args)) = query::application_info(self.ctx.types, alias)
            {
                (base, args)
            } else {
                return None;
            };
        let def_id = query::lazy_def_id(self.ctx.types, base)?;
        // `ClassConstructor` is the value-space `typeof C` identity. Class
        // declaration parameters belong to its construct signatures; wrapping
        // the companion as `Application(Lazy(companion), defaults)` sends it
        // through type-position class instantiation and replaces statics with
        // the instance shape.
        if self.ctx.definition_store.get_kind(def_id)
            == Some(tsz_solver::def::DefKind::ClassConstructor)
        {
            return None;
        }
        let mut type_params = self.ctx.get_def_type_params(def_id);
        if type_params.as_ref().is_none_or(|params| {
            params
                .iter()
                .all(|param| param.constraint.is_none() && param.default.is_none())
        }) && let Some(sym_id) = self.ctx.def_to_symbol_id(def_id)
        {
            let declared_params = self.get_type_params_for_symbol(sym_id);
            if !declared_params.is_empty() {
                type_params = Some(declared_params);
            }
        }
        let type_params = type_params?;
        if type_params.is_empty() || args.len() >= type_params.len() {
            return None;
        }
        let defaulted_args = fill_application_defaults(self.ctx.types, &args, &type_params)?;
        Some(self.ctx.types.application(base, defaulted_args))
    }

    /// Materialize a property-access receiver that is a generic *type-alias*
    /// application forwarding its arguments into a cross-file generic interface
    /// (`type L<T> = Box<T>` where `Box` is reached through a barrel re-export,
    /// then `b: L<number>`).
    ///
    /// The shared evaluator substitutes the alias's own parameters correctly
    /// (`L<number>` → its body `Box<number>`), but evaluating that inner
    /// cross-arena interface application then drops the interface's parameter
    /// substitution (the inherited member surfaces as the unsubstituted declared
    /// `T`, a false `TS2322`). A *direct* interface receiver avoids this because
    /// `evaluate_application_type_for_property_access` materializes it through
    /// `resolve_application_base_body`; the alias wrapper never reaches that path
    /// because its base is a `TypeAlias`, not an `Interface`.
    ///
    /// This expands the alias one level — substituting the concrete arguments
    /// into the alias body — and, when the result is a cross-file generic
    /// interface/class application, routes it through the same interface
    /// materialization the direct receiver uses. Returns `None` for every other
    /// shape so the caller keeps the shared evaluator. Gated on *concrete*
    /// arguments so a still-generic receiver (whose free parameters are
    /// legitimately preserved) is untouched. Refs #13212 / #10663.
    pub(crate) fn materialize_alias_wrapped_interface_receiver(
        &mut self,
        type_id: TypeId,
    ) -> Option<TypeId> {
        use tsz_solver::def::DefKind;
        let db = self.ctx.types.as_type_database();
        let (alias_base, alias_args) = query::application_info(self.ctx.types, type_id)?;
        if alias_args.is_empty()
            || alias_args
                .iter()
                .any(|&arg| crate::query_boundaries::common::contains_type_parameters(db, arg))
        {
            return None;
        }
        let alias_def_id = query::lazy_def_id(self.ctx.types, alias_base)?;
        let alias_info = self.ctx.definition_store.get(alias_def_id)?;
        if alias_info.kind != DefKind::TypeAlias {
            return None;
        }
        let alias_params = self.ctx.get_def_type_params(alias_def_id)?;
        if alias_params.is_empty() || alias_params.len() != alias_args.len() {
            return None;
        }
        let alias_body = self.ctx.definition_store.get_body(alias_def_id)?;
        // A self-`Lazy(def)` placeholder body carries no structural shape to
        // expand; bail so the shared evaluator keeps ownership.
        if query::lazy_def_id(self.ctx.types, alias_body) == Some(alias_def_id) {
            return None;
        }
        let substitution =
            query::TypeSubstitution::from_args(self.ctx.types, &alias_params, &alias_args);
        let underlying = query::instantiate_type(self.ctx.types, alias_body, &substitution);

        // The expanded body must itself be a *cross-file* generic interface/class
        // application — exactly the shape the shared evaluator mis-substitutes and
        // the interface materialization handles. Same-file or lib bases already
        // resolve correctly through the shared path.
        let (underlying_base, _) = query::application_info(self.ctx.types, underlying)?;
        let underlying_def_id = query::lazy_def_id(self.ctx.types, underlying_base)?;
        let underlying_info = self.ctx.definition_store.get(underlying_def_id)?;
        if underlying_info
            .file_id
            .is_none_or(|file_id| file_id == self.ctx.current_file_idx as u32)
            || underlying_info.is_declare
            || !matches!(underlying_info.kind, DefKind::Interface | DefKind::Class)
        {
            return None;
        }
        let materialized = self.evaluate_application_type_for_property_access(underlying);
        (materialized != underlying).then_some(materialized)
    }

    /// Instantiate a cross-file generic declaration application from its exact
    /// `DefinitionStore` body and parameters.
    ///
    /// The def is the identity authority for a cross-file base: resolving the
    /// base through its raw `SymbolId` reads the *current* binder, where
    /// per-file binders mint colliding raw ids (#14344), so the shared
    /// evaluator could bind the wrong symbol and no-op the argument
    /// substitution (a free `T` leaks into members — false `TS2322`).
    ///
    /// Returns `None` for every shape the def store does not own — same-file
    /// bases, non-program (lib/synthetic) defs, `declare`d ambient defs,
    /// non-generic declaration defs, defs without a registered body or parameters
    /// — so the caller keeps the existing symbol-based path. `base`/`args`
    /// are `application`'s already-extracted parts (the caller holds them;
    /// re-deriving would re-clone the args). Refs #13212 / #10663.
    pub(crate) fn instantiate_cross_file_definition_application(
        &mut self,
        application: TypeId,
        base: TypeId,
        args: &[TypeId],
    ) -> Option<TypeId> {
        let base_def_id = query::lazy_def_id(self.ctx.types, base)?;
        let (file_id, kind, is_declare) =
            self.ctx.definition_store.get_classification(base_def_id)?;
        // A def is cross-file only when it names a genuine *program* file
        // that is not the current one — the non-program sentinel marks
        // lib/synthetic defs, and ambient `declare` defs stay on the symbol
        // path too.
        if file_id.is_none_or(|file_id| {
            file_id == self.ctx.current_file_idx as u32
                || file_id == tsz_solver::def::DefinitionStore::NON_PROGRAM_FILE_SENTINEL
        }) || is_declare
        {
            return None;
        }
        if kind == tsz_solver::def::DefKind::TypeAlias {
            // Direct lowering has already published this owner-qualified
            // program alias body and its source parameters. That identity is
            // authoritative for both source and external-package `.d.ts`
            // aliases; demoting either to a raw binder-local `SymbolId` can
            // select an unrelated declaration with the same numeric id.
            let body_type = self.published_program_alias_body(base_def_id)?;
            let type_params = self.ctx.get_def_type_params(base_def_id)?;
            if body_type == TypeId::ANY || body_type == TypeId::ERROR || type_params.is_empty() {
                return None;
            }
            // The solver owns generic-alias evaluation. Feed it the original
            // exact-`DefId` application so its per-definition recursion/growth
            // guards, application cache, argument-preservation rules, and
            // display-alias provenance remain active. Manually instantiating
            // and then env-evaluating the body bypasses those guards and can
            // build an unbounded sequence for mutually nested aliases.
            let evaluated = self.evaluate_type_with_env(application);
            return (evaluated != application
                && evaluated != TypeId::ANY
                && evaluated != TypeId::ERROR
                && (evaluated != TypeId::UNKNOWN || body_type == TypeId::UNKNOWN))
                .then_some(evaluated);
        }
        // A def in a declaration (`.d.ts`) file is ambient regardless of the
        // `declare` keyword. React's namespace-nested component interfaces can
        // carry `is_declare = false`; instantiating those here mis-relates the
        // library component types. Keep declaration-file interface/class bases
        // on the symbol path, while published aliases retain their exact-def
        // collision recovery above.
        if file_id.is_some_and(|file_id| self.file_idx_is_declaration_file(file_id)) {
            return None;
        }
        if !matches!(
            kind,
            tsz_solver::def::DefKind::Interface | tsz_solver::def::DefKind::Class
        ) {
            return None;
        }
        let ApplicationBaseBody {
            body_type,
            type_params,
        } = self.resolve_application_base_body(base)?;
        if body_type == TypeId::ANY || body_type == TypeId::ERROR || type_params.is_empty() {
            return None;
        }
        Some(self.instantiate_application_body_for_property_access(
            body_type,
            &type_params,
            args,
            application,
        ))
    }

    /// Whether the source file at `file_idx` is a TypeScript declaration file.
    /// Reads the parser's structural `is_declaration_file` flag on the source
    /// file (file kind), not a path/name string — a declaration-file def is
    /// ambient regardless of the `declare` keyword.
    fn file_idx_is_declaration_file(&self, file_idx: u32) -> bool {
        self.ctx
            .get_arena_for_file(file_idx)
            .source_files
            .first()
            .is_some_and(|source_file| source_file.is_declaration_file)
    }

    /// Instantiate a generic interface/class body with its type parameters bound
    /// to an application's arguments, then env-evaluate the result.
    ///
    /// The arguments are env-evaluated, substituted into `body_type`, polymorphic
    /// `this` is rebound to `application`, and the result is env-evaluated;
    /// evaluation that collapses to `ERROR` falls back to the pre-evaluation
    /// instantiation. Shared by `evaluate_application_type_for_property_access`
    /// and the arena-collision recovery so the two stay in lock-step.
    fn instantiate_application_body_for_property_access(
        &mut self,
        body_type: TypeId,
        type_params: &[TypeParamInfo],
        args: &[TypeId],
        application: TypeId,
    ) -> TypeId {
        let evaluated_args: Vec<TypeId> = args
            .iter()
            .map(|&arg| self.evaluate_type_with_env(arg))
            .collect();
        let substitution =
            query::TypeSubstitution::from_args(self.ctx.types, type_params, &evaluated_args);
        let mut instantiated = query::instantiate_type(self.ctx.types, body_type, &substitution);
        if query::contains_this_type(self.ctx.types, instantiated) {
            instantiated = query::substitute_this_type(self.ctx.types, instantiated, application);
        }
        let evaluated = self.evaluate_type_with_env(instantiated);
        if evaluated == TypeId::ERROR {
            instantiated
        } else {
            evaluated
        }
    }

    /// Recover a property-access receiver whose cross-arena type-parameter push
    /// collided with the current file arena, leaving the shared
    /// `evaluate_application_type` unable to substitute its arguments.
    ///
    /// `type_reference_symbol_type_with_params` pushes a lib interface's
    /// type-parameter nodes against the *current file* arena; when the owner
    /// arena differs the `NodeIndex`es collide with unrelated current-file nodes,
    /// so the pushed set disagrees in arity with the application arguments and the
    /// shared evaluator's substitution against that same body is a no-op (the
    /// interface's own parameters leak — false TS2322 on `Generator<Y,
    /// R>.next().value`). Detect that arity mismatch and re-instantiate the
    /// (identical) body with the symbol's canonical parameters, which match the
    /// body's identities. Returns `None` for every well-formed application so the
    /// caller keeps the shared evaluator.
    pub(crate) fn recover_arena_collided_application_for_property_access(
        &mut self,
        type_id: TypeId,
    ) -> Option<TypeId> {
        let (base, args) = query::application_info(self.ctx.types, type_id)?;
        if args.is_empty() {
            return None;
        }
        // Cheap gate before the heavier interface re-lowering below: the arena
        // collision only affects a cross-file interface/class base, whose
        // type-parameter nodes live in a different arena than the push reads.
        // Same-file generics and type-alias applications (`Partial`, `Pick`,
        // `Record`, …) push against their own arena and never collide, so they
        // keep the shared evaluator without paying the probe.
        let base_def_id = query::lazy_def_id(self.ctx.types, base)?;
        let base_def_info = self.ctx.definition_store.get(base_def_id)?;
        if base_def_info
            .file_id
            .is_none_or(|file_id| file_id == self.ctx.current_file_idx as u32)
            || !matches!(
                base_def_info.kind,
                tsz_solver::def::DefKind::Interface | tsz_solver::def::DefKind::Class
            )
        {
            return None;
        }
        let sym_id = self.ctx.resolve_type_to_symbol_id(base)?;
        // The body and pushed parameters come from the same query the shared
        // evaluator uses, so the arity comparison is exactly its substitution
        // input. A matching arity means the shared evaluator already substitutes
        // correctly; bail and leave it untouched.
        let (body_type, pushed_params) = self.type_reference_symbol_type_with_params(sym_id);
        if pushed_params.len() == args.len()
            || body_type == TypeId::ANY
            || body_type == TypeId::ERROR
        {
            return None;
        }
        let canonical = self.get_type_params_for_symbol(sym_id);
        if canonical.len() != args.len() {
            return None;
        }
        let instantiated = self.instantiate_application_body_for_property_access(
            body_type, &canonical, &args, type_id,
        );
        (instantiated != body_type).then_some(instantiated)
    }

    pub(crate) fn resolve_application_base_body(
        &mut self,
        base: TypeId,
    ) -> Option<ApplicationBaseBody> {
        let base_def_body = query::lazy_def_id(self.ctx.types, base).and_then(|def_id| {
            let def_info = self.ctx.definition_store.get(def_id)?;
            if def_info.is_declare
                || !matches!(
                    def_info.kind,
                    tsz_solver::def::DefKind::Interface | tsz_solver::def::DefKind::Class
                )
            {
                return None;
            }
            let body = def_info.body.or_else(|| {
                self.ctx
                    .type_env
                    .try_borrow()
                    .ok()
                    .and_then(|env| env.get_def(def_id))
            })?;
            let body = self
                .ctx
                .module_augmented_body_or_current(def_id, body, self.ctx.types);
            let params = self.ctx.get_def_type_params(def_id).unwrap_or_default();
            Some((def_id, body, params))
        });

        let (mut body_type, mut type_params, base_def_id, mut base_sym_id) =
            if let Some((def_id, body, params)) = base_def_body {
                (body, params, Some(def_id), None)
            } else {
                let sym_id = self.ctx.resolve_type_to_symbol_id(base)?;
                let (body, params) = self.type_reference_symbol_type_with_params(sym_id);
                (
                    body,
                    params,
                    self.ctx.get_existing_def_id(sym_id),
                    Some(sym_id),
                )
            };

        if let Some(def_id) = base_def_id
            && query::object_shape(self.ctx.types, body_type).is_some_and(|shape| {
                shape.properties.is_empty()
                    && shape.string_index.is_none()
                    && shape.number_index.is_none()
            })
            && let Some(sym_id) = self.ctx.def_to_symbol_id_with_fallback(def_id)
        {
            let (candidate_body, candidate_params) =
                self.type_reference_symbol_type_with_params(sym_id);
            let candidate_has_members =
                query::object_shape(self.ctx.types, candidate_body).is_some_and(|shape| {
                    !shape.properties.is_empty()
                        || shape.string_index.is_some()
                        || shape.number_index.is_some()
                }) || query::callable_shape(self.ctx.types, candidate_body).is_some_and(|shape| {
                    !shape.properties.is_empty()
                        || !shape.call_signatures.is_empty()
                        || !shape.construct_signatures.is_empty()
                });
            if candidate_has_members {
                body_type = candidate_body;
                if !candidate_params.is_empty() {
                    type_params = candidate_params;
                }
                base_sym_id = Some(sym_id);
            }
        }

        let is_typeof_query = query::type_query_symbol(self.ctx.types, base).is_some();
        if !is_typeof_query
            && (base_sym_id
                .and_then(|sym_id| {
                    self.get_cross_file_symbol(sym_id)
                        .or_else(|| self.ctx.binder.get_symbol(sym_id))
                })
                .is_some_and(|symbol| symbol.has_any_flags(tsz_binder::symbol_flags::CLASS))
                || base_def_id.is_some_and(|def_id| {
                    self.ctx
                        .definition_store
                        .get(def_id)
                        .is_some_and(|info| info.kind == tsz_solver::def::DefKind::Class)
                }))
            && query::callable_shape(self.ctx.types, body_type)
                .is_some_and(|shape| !shape.construct_signatures.is_empty())
            && let Some(def_id) = base_def_id
            && let Ok(env) = self.ctx.type_env.try_borrow()
            && let Some(instance_type) = env.get_class_instance_type(def_id)
        {
            body_type = instance_type;
        }

        Some(ApplicationBaseBody {
            body_type,
            type_params,
        })
    }

    pub(crate) fn instantiate_mapped_property_template_with_env(
        &mut self,
        mapped: &tsz_solver::MappedType,
        key_name: Atom,
    ) -> TypeId {
        let key_literal = self.ctx.types.literal_string_atom(key_name);
        let property_type =
            crate::query_boundaries::state::checking::instantiate_mapped_template_for_property(
                self.ctx.types,
                mapped.template,
                mapped.type_param.name,
                key_literal,
            );

        // When the template produces an IndexAccess (e.g., T[K] → ObjType["key"]),
        // resolve the object part through evaluate_type_with_resolution so that
        // Lazy(DefId) references become concrete types.  Then attempt property
        // access resolution with the resolved object.
        if let Some((obj, _idx)) = query::index_access_types(self.ctx.types, property_type) {
            let obj_type = self.evaluate_type_with_resolution(obj);

            let prop_name_arc = self.ctx.types.resolve_atom_ref(key_name);
            let prop_name: &str = &prop_name_arc;
            match self.resolve_property_access_with_env(obj_type, prop_name) {
                tsz_solver::operations::property::PropertyAccessResult::Success {
                    type_id, ..
                }
                | tsz_solver::operations::property::PropertyAccessResult::PossiblyNullOrUndefined {
                    property_type: Some(type_id),
                    ..
                } => return type_id,
                _ => {}
            }
        }

        self.evaluate_type_with_env(property_type)
    }
}
