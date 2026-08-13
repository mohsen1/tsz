//! Support methods for `TypeEvaluator` argument expansion, simplification,
//! and visitor dispatch.

use crate::instantiation::instantiate::instantiate_generic_cached;

use super::*;

/// Maximum alias-chain hops when transitively normalising a `Lazy` type arg.
/// TypeScript disallows circular aliases, so real chains are 1–3 levels deep;
/// this ceiling only fires for malformed or pathological input.
const MAX_LAZY_CHAIN_DEPTH: usize = 32;

impl<'a, R: TypeResolver> TypeEvaluator<'a, R> {
    #[inline]
    pub(super) fn cached_generic_instantiation(
        &self,
        body: TypeId,
        type_params: &[TypeParamInfo],
        args: &[TypeId],
    ) -> TypeId {
        instantiate_generic_cached(self.interner, self.query_db, body, type_params, args)
    }

    /// Check if a type is a Conditional whose `extends_type` is an Application containing infer.
    /// This detects patterns like `T extends Promise<infer U> ? U : T`.
    pub(crate) fn is_conditional_with_application_infer(&self, type_id: TypeId) -> bool {
        if type_id.is_intrinsic() {
            return false;
        }
        let Some(TypeData::Conditional(cond_id)) = self.interner.lookup(type_id) else {
            return false;
        };
        let cond = self.interner.get_conditional(cond_id);
        matches!(
            self.interner.lookup(cond.extends_type),
            Some(TypeData::Application(_))
        )
    }

    /// Like `expand_type_args` but preserves Application types without evaluating them.
    /// Used for conditional type bodies so the conditional evaluator can match
    /// at the Application level for infer pattern matching.
    pub(crate) fn expand_type_args_preserve_applications(
        &mut self,
        args: &[TypeId],
    ) -> Vec<TypeId> {
        // Fast path: check if any non-Application arg needs expansion.
        let needs_expansion = args.iter().any(|&arg| {
            if arg.is_intrinsic() {
                return false;
            }
            matches!(
                self.interner.lookup(arg),
                Some(
                    TypeData::TypeQuery(_)
                        | TypeData::Conditional(_)
                        | TypeData::Mapped(_)
                        | TypeData::TemplateLiteral(_)
                        | TypeData::KeyOf(_)
                        | TypeData::Lazy(_)
                )
            )
        });
        if !needs_expansion {
            return args.to_vec();
        }
        let mut expanded = Vec::with_capacity(args.len());
        for &arg in args {
            let Some(key) = self.interner.lookup(arg) else {
                expanded.push(arg);
                continue;
            };
            match key {
                TypeData::Application(_) => {
                    expanded.push(arg);
                }
                _ => expanded.push(self.try_expand_type_arg(arg)),
            }
        }
        expanded
    }

    /// Expand type arguments by evaluating any that are `TypeQuery` or Application.
    /// Uses a loop instead of closure to allow mutable self access.
    pub(crate) fn expand_type_args<'b>(
        &mut self,
        args: &'b [TypeId],
    ) -> std::borrow::Cow<'b, [TypeId]> {
        // Fast path: check if any arg needs expansion before allocating.
        // Most type args are simple types that pass through unchanged.
        let needs_expansion = args.iter().any(|&arg| self.needs_type_arg_expansion(arg));
        if !needs_expansion {
            return std::borrow::Cow::Borrowed(args);
        }
        let mut expanded = Vec::with_capacity(args.len());
        for &arg in args {
            expanded.push(self.try_expand_type_arg(arg));
        }
        std::borrow::Cow::Owned(expanded)
    }

    /// Check if a type arg needs expansion (without actually expanding it).
    #[inline]
    fn needs_type_arg_expansion(&self, arg: TypeId) -> bool {
        if arg.is_intrinsic() {
            return false;
        }
        matches!(
            self.interner.lookup(arg),
            Some(
                TypeData::TypeQuery(_)
                    | TypeData::Application(_)
                    | TypeData::Conditional(_)
                    | TypeData::IndexAccess(_, _)
                    | TypeData::Mapped(_)
                    | TypeData::TemplateLiteral(_)
                    | TypeData::KeyOf(_)
                    | TypeData::Lazy(_)
            )
        )
    }

    /// Extract the reachable type-parameter infos of a type: the node's
    /// children's lists merged bottom-up in child order, deduplicated by
    /// parameter name on first occurrence — the same set and order the
    /// historical accumulate-into-one-vec DFS produced.
    ///
    /// The list is a pure function of the immutable interned structure (the
    /// walk never consults the resolver or the substitution environment), so
    /// every visited node — not just the query root — is memoized per
    /// `TypeId` on the shared interner. That per-node sharing is what lets a
    /// fresh root over a large shared interior reuse the interior's lists
    /// instead of re-walking them: each unwrap step of a recursive
    /// conditional mints a fresh root whose subtrees were all walked before
    /// (#14330, #13508). The interned type DAG is acyclic without resolver
    /// indirection (this walk never resolves `Lazy`), so no cycle guard is
    /// needed.
    ///
    /// A reachability gate prunes provably-empty subtrees without descending
    /// or allocating: no `TypeParameter` node and no `Callable` declaring
    /// signature type parameters is reachable on this collector's child
    /// surface (see `contains_extractable_type_params_db`).
    pub(crate) fn extract_type_params_from_type(
        &self,
        type_id: TypeId,
    ) -> std::sync::Arc<[TypeParamInfo]> {
        fn empty_params() -> std::sync::Arc<[TypeParamInfo]> {
            static EMPTY: std::sync::OnceLock<std::sync::Arc<[TypeParamInfo]>> =
                std::sync::OnceLock::new();
            std::sync::Arc::clone(EMPTY.get_or_init(|| std::sync::Arc::from(&[][..])))
        }
        /// First-occurrence-wins merge by parameter name (lists stay tiny, so
        /// the linear scan beats a per-node hash set).
        fn merge_params(out: &mut Vec<TypeParamInfo>, list: &[TypeParamInfo]) {
            for info in list {
                if !out.iter().any(|p| p.name == info.name) {
                    out.push(*info);
                }
            }
        }

        if type_id.is_intrinsic() {
            return empty_params();
        }
        if let Some(cached) = self.interner.extract_type_params_memo(type_id) {
            return cached;
        }
        if !crate::type_queries::contains_extractable_type_params_db(self.interner, type_id) {
            // The predicate cache already answers this O(1) per node; skip the
            // memo write so provably-empty subtrees cost no map entries.
            return empty_params();
        }
        let Some(key) = self.interner.lookup(type_id) else {
            return empty_params();
        };

        let mut out: Vec<TypeParamInfo> = Vec::new();
        match key {
            TypeData::TypeParameter(info) => out.push(info),
            TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id) => {
                let shape = self.interner.object_shape(shape_id);
                for prop in &shape.properties {
                    merge_params(&mut out, &self.extract_type_params_from_type(prop.type_id));
                }
            }
            TypeData::Function(shape_id) => {
                let shape = self.interner.function_shape(shape_id);
                for param in &shape.params {
                    merge_params(&mut out, &self.extract_type_params_from_type(param.type_id));
                }
                merge_params(
                    &mut out,
                    &self.extract_type_params_from_type(shape.return_type),
                );
            }
            TypeData::Union(members) | TypeData::Intersection(members) => {
                let members = self.interner.type_list(members);
                for &member in members.iter() {
                    merge_params(&mut out, &self.extract_type_params_from_type(member));
                }
            }
            TypeData::Array(elem) => {
                merge_params(&mut out, &self.extract_type_params_from_type(elem));
            }
            TypeData::Conditional(cond_id) => {
                let cond = self.interner.get_conditional(cond_id);
                for side in [
                    cond.check_type,
                    cond.extends_type,
                    cond.true_type,
                    cond.false_type,
                ] {
                    merge_params(&mut out, &self.extract_type_params_from_type(side));
                }
            }
            TypeData::Application(app_id) => {
                let app = self.interner.type_application(app_id);
                merge_params(&mut out, &self.extract_type_params_from_type(app.base));
                for &arg in &app.args {
                    merge_params(&mut out, &self.extract_type_params_from_type(arg));
                }
            }
            TypeData::Mapped(mapped_id) => {
                let mapped = self.interner.get_mapped(mapped_id);
                // Note: mapped.type_param is the iteration variable (e.g., K in "K in keyof T")
                // We should NOT add it directly - the outer type param (T) is found in the constraint.
                // For DeepPartial<T> = { [K in keyof T]?: DeepPartial<T[K]> }:
                //   - type_param is K (iteration var, NOT the outer param)
                //   - constraint is "keyof T" (contains T, the actual param to extract)
                //   - template is DeepPartial<T[K]> (also contains T)
                merge_params(
                    &mut out,
                    &self.extract_type_params_from_type(mapped.constraint),
                );
                merge_params(
                    &mut out,
                    &self.extract_type_params_from_type(mapped.template),
                );
                if let Some(name_type) = mapped.name_type {
                    merge_params(&mut out, &self.extract_type_params_from_type(name_type));
                }
            }
            TypeData::KeyOf(operand) => {
                // e.g., keyof T -> extract T
                merge_params(&mut out, &self.extract_type_params_from_type(operand));
            }
            TypeData::IndexAccess(obj, idx) => {
                // e.g., T[K] -> extract T and K
                merge_params(&mut out, &self.extract_type_params_from_type(obj));
                merge_params(&mut out, &self.extract_type_params_from_type(idx));
            }
            TypeData::TemplateLiteral(spans) => {
                let spans = self.interner.template_list(spans);
                for span in spans.iter() {
                    if let TemplateSpan::Type(inner) = span {
                        merge_params(&mut out, &self.extract_type_params_from_type(*inner));
                    }
                }
            }
            TypeData::Callable(cs_id) => {
                // Collect the type parameters declared by construct and call signatures.
                // This handles `typeof ClassName<T>` where the constructor type is a
                // Callable whose signatures own the class type parameters.
                let shape = self.interner.callable_shape(cs_id);
                for sig in shape
                    .construct_signatures
                    .iter()
                    .chain(shape.call_signatures.iter())
                {
                    merge_params(&mut out, &sig.type_params);
                }
            }
            _ => {}
        }

        let result: std::sync::Arc<[TypeParamInfo]> = std::sync::Arc::from(out.as_slice());
        self.interner
            .set_extract_type_params_memo(type_id, std::sync::Arc::clone(&result));
        result
    }

    /// Try to expand a type argument that may be a `TypeQuery` or Application.
    /// Returns the expanded type, or the original if it can't be expanded.
    /// This ensures type arguments are resolved before instantiation.
    ///
    /// NOTE: This method uses `self.evaluate()` for Application, Conditional, Mapped,
    /// and `TemplateLiteral` types to ensure recursion depth limits are enforced.
    pub(super) fn try_expand_type_arg(&mut self, arg: TypeId) -> TypeId {
        let Some(key) = self.interner.lookup(arg) else {
            return arg;
        };
        if matches!(
            key,
            TypeData::Application(_)
                | TypeData::Conditional(_)
                | TypeData::IndexAccess(_, _)
                | TypeData::Mapped(_)
                | TypeData::TemplateLiteral(_)
                | TypeData::KeyOf(_)
        ) && crate::contains_this_type(self.interner, arg)
        {
            return arg;
        }
        // A generic indexed access `T[K]` whose *object* is still generic must
        // stay deferred during type-argument expansion. Evaluating it would
        // resolve through `T`'s constraint (e.g. `(readonly unknown[])[number]`
        // -> `unknown`), baking out the type parameter before the surrounding
        // signature is instantiated. tsc keeps it as a deferred indexed-access
        // type (its base constraint is only consulted by relations), so a later
        // substitution `T = number[]` still resolves `T[number]` to the real
        // element type. Without this guard a nested alias argument such as
        // `NonEmptyArray<OrderRule<T[number]>>` collapses the callback
        // parameter to `unknown` (spurious `TS2345`/`TS2769`; remeda
        // `purryOrderRules`: `nthBy`, `firstBy`, `sortBy`).
        //
        // The guard is restricted to a generic *object*: when the object is
        // already concrete and only the *index* is a type parameter (e.g.
        // `AWrapped[K]` with `K extends keyof AWrapped`), tsc produces the
        // simplified substitute (the property-type union), and tsz must keep
        // evaluating it. Deferring those collapses the simplified access and
        // breaks unrelated relations such as the `Wrapper<BWrapped>` override
        // of `Wrapper<AWrapped>` (spurious `TS2416`;
        // `indexedAccessKeyofNestedSimplifiedSubstituteUnwrapped`).
        if let TypeData::IndexAccess(obj, _) = &key
            && crate::visitor::contains_type_parameters(self.interner, *obj)
        {
            return arg;
        }
        match key {
            TypeData::TypeQuery(sym_ref) => {
                // Resolve the TypeQuery to get the VALUE type (constructor for classes).
                // Use resolve_type_query which returns constructor types for classes,
                // unlike resolve_ref which may return instance types.
                if let Some(resolved) = self.resolver.resolve_type_query(sym_ref, self.interner) {
                    resolved
                } else if let Some(def_id) = self.resolver.symbol_to_def_id(sym_ref) {
                    match self.resolver.resolve_lazy(def_id, self.interner) {
                        Some(resolved) => resolved,
                        // No registered body on this query (#14347 deferral).
                        None => self.defer_unexpanded_type_arg(arg),
                    }
                } else {
                    // The `typeof` symbol resolves to no def yet (#14347 deferral).
                    self.defer_unexpanded_type_arg(arg)
                }
            }
            TypeData::Application(_)
            | TypeData::Conditional(_)
            | TypeData::IndexAccess(_, _)
            | TypeData::Mapped(_)
            | TypeData::TemplateLiteral(_)
            | TypeData::KeyOf(_) => {
                // Use evaluate() to ensure depth limits are enforced.
                // KeyOf must be expanded here so that after generic instantiation,
                // the mapped type constraint and template reference the same source
                // object TypeId (critical for homomorphic mapped type detection).
                self.evaluate(arg)
            }
            TypeData::Lazy(def_id) => {
                // Transitively resolve alias chains to their canonical structural
                // body so that `A = B = T` and direct `T` produce the same
                // expanded arg and share the `application_eval_cache` entry,
                // preventing alias fan-out from triggering repeated evaluations
                // of the same logical type (#10826).
                self.expand_lazy_arg_chain(def_id, arg)
            }
            TypeData::UnresolvedTypeName(atom) => {
                // A type-argument carried as `UnresolvedTypeName(name)` is the
                // display-preserving residue of a cross-file reference lowered
                // before the referenced declaration's name was resolvable in
                // this checker. The application *base* already resolves this
                // shape through `resolve_unresolved_type_name`
                // (see `resolve_application_def_id`); the *argument* path must
                // do the same. Without it, a generic whose body is a
                // distributive conditional over the argument
                // (`OptionalKeysOf<O> = O extends unknown ? ... : never`)
                // substitutes the still-unresolved name, cannot reduce the
                // key-space, and bails opaque — the well-typed default then
                // false-fails its constraint (`TS2344` on type-fest's
                // `ApplyDefaultOptions`/`RequiredKeysOf`, #13609). Resolve the
                // name to its def and follow the same alias-chain expansion as
                // the `Lazy` arm; keep the name opaque only when it genuinely
                // does not resolve (a registration-window artifact deferred via
                // `defer_unexpanded_type_arg` in the `else` below).
                let name = self.interner.resolve_atom(atom);
                if let Some(def_id) = self.resolver.resolve_unresolved_type_name(&name) {
                    self.expand_lazy_arg_chain(def_id, arg)
                } else {
                    // The name resolves to no def yet (#14347 deferral).
                    self.defer_unexpanded_type_arg(arg)
                }
            }
            _ => arg,
        }
    }

    /// Resolve an alias `DefId` to its canonical structural body, following a
    /// bounded chain of `Lazy` indirections (`A = B = T`). Returns `fallback`
    /// (the original argument) when the def has no resolvable body yet or the
    /// chain is circular/too deep, keeping the argument opaque so a later
    /// resolver pass can expand it. Shared by the `Lazy` and
    /// `UnresolvedTypeName` arms of [`Self::try_expand_type_arg`] so both
    /// argument shapes expand identically (#10826, #13609).
    fn expand_lazy_arg_chain(&mut self, def_id: DefId, fallback: TypeId) -> TypeId {
        let mut current_def = def_id;
        for _ in 0..MAX_LAZY_CHAIN_DEPTH {
            let Some(body) = self.resolver.resolve_lazy(current_def, self.interner) else {
                // The alias body is not registered on this query (a cross-file
                // alias whose declaring file has not published it yet): keep the
                // argument opaque and record the registration-window taint so the
                // enclosing application's under-expanded result is kept out of the
                // `TypeId`-keyed evaluation caches (#14347).
                return self.defer_unexpanded_type_arg(fallback);
            };
            match self.interner.lookup(body) {
                Some(TypeData::Lazy(next_def)) => current_def = next_def,
                _ => return body,
            }
        }
        // Circular or unusually deep alias chain — a depth bail, not an
        // unresolved-body artifact: keep it opaque without the taint.
        fallback
    }

    /// Keep an unexpanded type-argument opaque and record that its alias/name
    /// could not be expanded because the declaring body is not yet registered.
    ///
    /// The un-expanded argument flows into the enclosing
    /// application/instantiation evaluation, so that result is a function of the
    /// *registration window* it ran in, not of the input `TypeId` alone: once the
    /// declaring file publishes the real body, a fresh expansion yields a
    /// different, fully-reduced argument. Marking `unresolved_def_seen` keeps the
    /// registration-window result out of the `TypeId`-keyed evaluation caches
    /// (`closed_eval_cache` / `application_eval_cache` / the checker's env-eval
    /// backstop), exactly as the type-position `Lazy` visit ([`Self::visit_lazy`])
    /// and the application-base deferrals (`evaluate/application.rs`) already do.
    /// This is the type-argument arm of the `#14347` cache-purity invariant.
    const fn defer_unexpanded_type_arg(&mut self, fallback: TypeId) -> TypeId {
        self.mark_unresolved_def_seen();
        fallback
    }

    /// Check if a type is "complex" and requires full evaluation for identity.
    ///
    /// Complex types are those whose structural identity depends on evaluation context:
    /// - `TypeParameter`: Opaque until instantiation
    /// - Lazy: Requires resolution
    /// - Conditional: Requires evaluation of extends clause
    /// - Mapped: Requires evaluation of mapped type
    /// - `IndexAccess`: Requires evaluation of T[K]
    /// - `KeyOf`: Requires evaluation of keyof
    /// - Application: Requires expansion of Base<Args>
    /// - `TypeQuery`: Requires resolution of typeof
    /// - `TemplateLiteral`: Requires evaluation of template parts
    /// - `ReadonlyType`: Wraps another type
    /// - `StringIntrinsic`: Uppercase, Lowercase, Capitalize, Uncapitalize
    ///
    /// These types are NOT safe for simplification because bypassing evaluation
    /// would produce incorrect results (e.g., treating T[K] as a distinct type from
    /// the value it evaluates to).
    ///
    /// ## Task #37: Deep Structural Simplification
    ///
    /// After implementing the Canonicalizer (Task #32), we can now safely handle
    /// `Lazy` (type aliases) and `Application` (generics) structurally. These types
    /// are now "unlocked" for simplification because:
    /// - `Lazy` types are canonicalized using De Bruijn indices
    /// - `Application` types are recursively canonicalized
    /// - The `SubtypeChecker`'s fast-path (Task #36) uses O(1) structural identity
    ///
    /// Types that remain "complex" are those that are **inherently deferred**:
    /// - `TypeParameter`, `Infer`: Waiting for generic substitution
    /// - `Conditional`, `Mapped`, `IndexAccess`, `KeyOf`: Require type-level computation
    /// - These cannot be compared structurally until they are fully evaluated
    pub(super) fn is_complex_type(&self, type_id: TypeId) -> bool {
        if type_id.is_intrinsic() {
            return false;
        }
        let Some(key) = self.interner.lookup(type_id) else {
            return false;
        };

        match key {
            // `UnresolvedTypeName` is a display-preserving unresolved type
            // name (cross-file alias bodies lowered before the referenced
            // declaration's name is resolvable in this checker) and is just
            // as inherently deferred as the other variants in this arm: the
            // relation layer treats it as an error type that is related to
            // EVERYTHING, so any subtype-based simplification over it would
            // collapse distinct union members (e.g. `Expression<any> |
            // SelectQueryBuilderExpression<...>` losing its supertype arm).
            // The evaluator resolves these on demand later, so keep them out
            // of simplification entirely.
            TypeData::TypeParameter(_)
            | TypeData::Infer(_)
            | TypeData::Conditional(_)
            | TypeData::Mapped(_)
            | TypeData::IndexAccess(_, _)
            | TypeData::KeyOf(_)
            | TypeData::TypeQuery(_)
            | TypeData::TemplateLiteral(_)
            | TypeData::ReadonlyType(_)
            | TypeData::StringIntrinsic { .. }
            | TypeData::ThisType
            | TypeData::UnresolvedTypeName(_) => true,
            // Intersection/union types containing complex members are also complex.
            // Without this, the evaluator's subtype-based simplification can incorrectly
            // collapse union members like `(T&U&1) | (T&U&2) | (T&U&3)` to just `T&U&2`
            // because the constraint fallback determines some branches are always `never`.
            // TSC does not perform such simplification on unions with type parameters.
            TypeData::Intersection(list_id) | TypeData::Union(list_id) => {
                let members = self.interner.type_list(list_id);
                members.iter().any(|&m| self.is_complex_type(m))
            }
            // A generic-dependent application (`Foo<DB, TB>` with type-parameter
            // arguments) expands to a type the bypass-evaluation subtype checker
            // cannot judge soundly (e.g. an alias body `keyof DB[TB] & string`
            // looks string-like and gets absorbed by an object member). tsc does
            // not subtype-reduce union members that depend on unresolved type
            // parameters, so keep them. Fully-concrete applications stay
            // simplifiable via the canonicalizer.
            // An application whose BASE is an unresolved type name is just as
            // deferred as the bare name: `is_error_type` follows application
            // bases, so the bypass-evaluation subtype checker would judge the
            // whole application universally related and let simplification drop
            // a sibling member. Check the base alongside the arguments.
            TypeData::Application(app_id) => {
                let app = self.interner.type_application(app_id);
                self.is_complex_type(app.base)
                    || app.args.iter().any(|&arg| self.is_complex_type(arg))
            }
            TypeData::Array(_) | TypeData::Tuple(_) => self.has_nested_complex_marker(type_id),
            // Function types with Application/Lazy return *or parameter* types are
            // complex because the simplify-union subtype checker runs with
            // bypass_evaluation=true, which prevents Application/Lazy from being
            // expanded to their structural form during the comparison. Without
            // expansion, two distinct generic instantiations (e.g.,
            // `(x: Foo<any>) => void` vs `(x: Bar<any>) => void`, or
            // `() => Generator<T>` vs `() => AsyncGenerator<T>`) can be
            // incorrectly collapsed via remove_redundant_members.
            TypeData::Function(fn_id) => self.is_complex_function(fn_id),
            // Object types whose property types are functions with Application/Lazy
            // params/returns are also affected by the bypass_evaluation issue: when
            // the SubtypeChecker compares two such objects, comparing the function
            // properties contravariantly may incorrectly conclude they are mutually
            // compatible because Application bases aren't expanded structurally.
            // Without this guard, `(I1 | I2)["f"]` collapses I1/I2 before indexing
            // for `interface I1 { f: (e: Foo<any>) => void; }` shapes.
            //
            // We deliberately keep this check narrow — only flag when a property
            // is a *Function* with complex params/return — to avoid over-flagging
            // ordinary objects (e.g. React component types) that have generic
            // properties but whose union simplification is otherwise correct.
            TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id) => {
                let shape = self.interner.object_shape(shape_id);
                // Only treat the object as complex when the offending property
                // is an *optional* / *nullable* function-bearing union. The
                // bypass-evaluation collapse problem manifests when two
                // structurally-distinct objects each have an optional or
                // null-tolerating function property whose param/return
                // depends on Application expansion (the
                // `(I1 | I2)["f"]` indexed-access pattern).
                //
                // Non-optional, non-nullable function properties (e.g.
                // `interface Prop { children: (user: User) => JSX.Element }`)
                // are intentionally NOT covered: their union simplification
                // is well-behaved and the JSX render-prop diagnostic
                // prioritisation depends on it.
                shape.properties.iter().any(|p| {
                    (p.optional || self.is_nullable_union(p.type_id))
                        && self.contains_complex_function(p.type_id)
                })
            }
            _ => false,
        }
    }

    /// Returns true if `type_id` is an Application/Lazy type, or a
    /// union/intersection whose members contain Application/Lazy. Used by
    /// `is_complex_function` to detect when a function's params/return rely
    /// on Application expansion that `bypass_evaluation=true` forbids.
    fn has_nested_application_or_lazy(&self, type_id: TypeId) -> bool {
        if type_id.is_intrinsic() {
            return false;
        }
        match self.interner.lookup(type_id) {
            Some(TypeData::Application(_) | TypeData::Lazy(_)) => true,
            Some(TypeData::Union(list_id) | TypeData::Intersection(list_id)) => {
                let members = self.interner.type_list(list_id);
                members
                    .iter()
                    .any(|&m| self.has_nested_application_or_lazy(m))
            }
            Some(TypeData::Tuple(tuple_list)) => {
                let elements = self.interner.tuple_list(tuple_list);
                elements
                    .iter()
                    .any(|el| self.has_nested_application_or_lazy(el.type_id))
            }
            Some(TypeData::Array(elem)) => self.has_nested_application_or_lazy(elem),
            _ => false,
        }
    }

    /// Returns true if a function shape has parameters or a return type that
    /// contain Application/Lazy types. Such functions cannot be safely
    /// simplified by `simplify_union_members` because the `SubtypeChecker`
    /// runs with `bypass_evaluation=true`, which prevents structural
    /// expansion of Application bases during the comparison.
    fn is_complex_function(&self, fn_id: crate::types::FunctionShapeId) -> bool {
        let shape = self.interner.function_shape(fn_id);
        if self.has_nested_application_or_lazy(shape.return_type) {
            return true;
        }
        shape
            .params
            .iter()
            .any(|p| self.has_nested_application_or_lazy(p.type_id))
    }

    /// Returns true if `type_id` is a union containing `null` or `undefined`.
    /// Used to gate the Object-property complex flag so it only fires for
    /// nullable / optional function-bearing properties.
    fn is_nullable_union(&self, type_id: TypeId) -> bool {
        match self.interner.lookup(type_id) {
            Some(TypeData::Union(list_id)) => {
                let members = self.interner.type_list(list_id);
                members
                    .iter()
                    .any(|&m| m == TypeId::NULL || m == TypeId::UNDEFINED)
            }
            _ => false,
        }
    }

    /// Returns true if `type_id` is a complex Function, or a union/intersection
    /// containing one. Used by `is_complex_type` for Object shapes whose
    /// property types are nullable function types like `(... ) => T | null`.
    fn contains_complex_function(&self, type_id: TypeId) -> bool {
        if type_id.is_intrinsic() {
            return false;
        }
        match self.interner.lookup(type_id) {
            Some(TypeData::Function(fn_id)) => self.is_complex_function(fn_id),
            Some(TypeData::Union(list_id) | TypeData::Intersection(list_id)) => {
                let members = self.interner.type_list(list_id);
                members.iter().any(|&m| self.contains_complex_function(m))
            }
            _ => false,
        }
    }

    /// Evaluate an intersection type by recursively evaluating members and re-interning.
    /// This enables "deferred reduction" where intersections containing meta-types
    /// (e.g., `string & T[K]`) are reduced after the meta-types are evaluated.
    ///
    /// Example: `string & T[K]` where `T[K]` evaluates to `number` will become
    /// `string & number`, which then reduces to `never` via the interner's normalization.
    fn evaluate_intersection(&mut self, list_id: TypeListId) -> TypeId {
        let members = self.interner.type_list(list_id);

        // Suppress `this` binding during member evaluation so that methods
        // returning `this` keep it as `ThisType` rather than binding to
        // individual members. The `this` type will be correctly bound later
        // during property access when the full intersection receiver is known.
        let prev_suppress = self.suppress_this_binding;
        self.suppress_this_binding = true;

        let mut evaluated_members = Vec::with_capacity(members.len());
        for &member in members.iter() {
            let evaluated = self.evaluate_compound_member(member);
            tracing::trace!(
                target: "tsz::solver::eval_intersection",
                member = member.0,
                evaluated = evaluated.0,
                member_data = ?self.interner.lookup(member),
                evaluated_data = ?self.interner.lookup(evaluated),
                preserved_nominal = self.nominal_reference_expands_to_intersection(member, evaluated),
                "evaluate_intersection member"
            );
            // When an Application/Lazy member fails to reduce and falls back to
            // `unknown` or to the empty object `{}` (e.g. depth-limit / cycle /
            // cross-file resolution gap that can't expand the alias body), keep
            // the original opaque member instead. Letting either propagate
            // would cause intersection simplification to drop it via
            // `unknown & T = T` or `{} & T = T` (since `{}` has no
            // properties), silently erasing the structural shape the
            // unevaluated alias would contribute once expanded. Preserving
            // the original Application/Lazy keeps the intersection honest so
            // downstream passes can see the alias's structural shape.
            let opaque_orig = Self::is_opaque_under_bypass_eval(self.interner, member);
            let evaluated_is_empty_object = evaluated != member
                && crate::visitors::visitor_predicates::is_empty_object_type(
                    self.interner,
                    evaluated,
                );
            let preserved =
                if opaque_orig && (evaluated == TypeId::UNKNOWN || evaluated_is_empty_object) {
                    member
                } else if self.nominal_reference_expands_to_intersection(member, evaluated) {
                    // A bare `Lazy` reference to a class/interface that
                    // evaluates to an intersection (multi-parent heritage or
                    // merged declarations) must keep its reference identity
                    // here: re-interning below would splice the expansion's
                    // constituents into THIS member list, and the reference's
                    // `DefId` — which the nominal fast path and the
                    // coinductive cycle guard both key on — is unrecoverable
                    // from the spliced parts (#17332, the `Window & typeof
                    // globalThis <: Window` family). Downstream passes resolve
                    // `Lazy` members on demand, exactly as they do for the
                    // opaque fallback above.
                    member
                } else {
                    evaluated
                };
            evaluated_members.push(preserved);
        }

        self.suppress_this_binding = prev_suppress;

        // Deep structural simplification using SubtypeChecker
        self.simplify_intersection_members(&mut evaluated_members);

        let result = self.interner.intersection(evaluated_members);

        // Propagate display properties from original members to the result.
        self.propagate_display_properties_for_intersection(members.as_ref(), result);

        result
    }

    /// True when `member` is a bare `Lazy` reference to a class or interface
    /// def and `evaluated` is the intersection its body expanded into.
    ///
    /// A multi-parent heritage interface (`interface Window extends
    /// EventTarget, GlobalEventHandlers, ...`) resolves to an intersection of
    /// its parents plus its own shape, and a multi-declaration merge resolves
    /// to an intersection of per-declaration shapes. Neither expansion carries
    /// the def identity of the reference it came from.
    fn nominal_reference_expands_to_intersection(&self, member: TypeId, evaluated: TypeId) -> bool {
        if evaluated == member
            || !matches!(
                self.interner.lookup(evaluated),
                Some(TypeData::Intersection(_))
            )
        {
            return false;
        }
        // A plain interface/class reference is commonly interned either as a
        // bare `Lazy(def)` or as `Application(Lazy(def), args)` — even with
        // empty `args` (see `def_id_for_type_reference` in
        // `relations/subtype/rules/objects.rs`, which unwraps the same two
        // forms on the consuming side).
        let def_id = match self.interner.lookup(member) {
            Some(TypeData::Lazy(def_id)) => def_id,
            Some(TypeData::Application(app_id)) => {
                let base = self.interner.type_application(app_id).base;
                match self.interner.lookup(base) {
                    Some(TypeData::Lazy(def_id)) => def_id,
                    _ => return false,
                }
            }
            _ => return false,
        };
        matches!(
            self.resolver.get_def_kind(def_id),
            Some(crate::def::DefKind::Class | crate::def::DefKind::Interface)
        )
    }

    /// Propagate display properties from intersection members to the result.
    fn propagate_display_properties_for_intersection(
        &self,
        original_members: &[TypeId],
        result: TypeId,
    ) {
        let display_vec = crate::types::merge_display_properties_for_intersection(
            self.interner,
            original_members,
        );
        if !display_vec.is_empty() {
            display_provenance::record_fresh_object_literal_display(
                self.interner,
                FreshObjectLiteralDisplayProvenance {
                    type_id: result,
                    properties: display_vec,
                },
            );
        }
    }

    /// Evaluate a union type by recursively evaluating members and re-interning.
    /// This enables "deferred reduction" where unions containing meta-types
    /// (e.g., `string | T[K]`) are reduced after the meta-types are evaluated.
    ///
    /// Example: `string | T[K]` where `T[K]` evaluates to `string` will become
    /// `string | string`, which then reduces to `string` via the interner's normalization.
    fn evaluate_union(&mut self, type_id: TypeId, list_id: TypeListId) -> TypeId {
        let canonical_members = self.interner.type_list(list_id);
        let origin_members = self.interner.get_union_origin(type_id);
        let members = origin_members
            .as_deref()
            .map_or(canonical_members.as_ref(), Vec::as_slice);
        let mut evaluated_members = Vec::with_capacity(members.len());

        for &member in members {
            evaluated_members.push(self.evaluate_compound_member(member));
        }

        // Deep structural simplification using SubtypeChecker.
        //
        // tsc's instantiation/mapping path for unions (`mapType`/`mapTypeWithAlias`)
        // uses `UnionReduction.Literal` — it does NOT pairwise subtype-reduce here.
        // Under `TSZ_UNION_LITERAL_DEFAULT` we drop this evaluate-layer full-relation
        // reduce so evaluated unions re-intern through the (literal-mode) constructor,
        // matching tsc; the evaluate-reachable `.Subtype` construction sites recover
        // reduction explicitly through `subtype_reduced`. Flag-off keeps the historical
        // reduction so behavior is byte-identical.
        if !crate::intern::union_literal_default_enabled() {
            self.simplify_union_members(&mut evaluated_members);
        }

        let result = self.interner.union_from_slice(&evaluated_members);
        display_provenance::record_union_origin(
            self.interner,
            UnionOriginProvenance {
                union_type_id: result,
                origin_members: evaluated_members,
            },
        );
        result
    }

    /// Evaluate a member of a compound type (union/intersection) while
    /// preserving an outer `NoInfer<>` wrapper.
    ///
    /// `evaluate(NoInfer<T>)` strips the marker because tsc treats `NoInfer<>`
    /// as transparent at the *outermost* layer of the displayed type. When
    /// `NoInfer<T>` appears as a union or intersection member, the union (or
    /// intersection) is the outermost layer, not the wrapper — tsc keeps the
    /// `NoInfer<>` visible in messages like
    /// `NoInfer<{ x: string; }> | (() => NoInfer<{ x: string; }>)`. Stripping
    /// the wrapper here would silently rewrite the displayed type.
    fn evaluate_compound_member(&mut self, member: TypeId) -> TypeId {
        if let Some(&TypeData::NoInfer(inner)) = self.interner.lookup(member).as_ref() {
            let evaluated_inner = self.evaluate(inner);
            if evaluated_inner == inner {
                member
            } else {
                self.interner.no_infer(evaluated_inner)
            }
        } else {
            self.evaluate(member)
        }
    }

    pub(super) fn is_primitive_or_primitive_union(
        db: &dyn crate::caches::db::TypeDatabase,
        candidate: TypeId,
    ) -> bool {
        if crate::visitors::visitor_predicates::is_primitive_type(db, candidate) {
            return true;
        }
        let Some(TypeData::Union(members)) = db.lookup(candidate) else {
            return false;
        };
        db.type_list(members)
            .iter()
            .all(|&member| crate::visitors::visitor_predicates::is_primitive_type(db, member))
    }

    // =========================================================================
    // Visitor Pattern Implementation (North Star Rule 2)
    // =========================================================================

    /// Visit a `TypeData` and return its evaluated form.
    ///
    /// This is the visitor dispatch method that routes to specific visit_* methods.
    /// The `visiting.remove()` and `cache.insert()` are handled in `evaluate()` for symmetry.
    pub(super) fn visit_type_key(&mut self, type_id: TypeId, key: &TypeData) -> TypeId {
        match key {
            TypeData::Conditional(cond_id) => self.visit_conditional(*cond_id),
            TypeData::IndexAccess(obj, idx) => self.visit_index_access(*obj, *idx),
            TypeData::Mapped(mapped_id) => self.visit_mapped(*mapped_id),
            TypeData::KeyOf(operand) => self.visit_keyof(*operand),
            TypeData::TypeQuery(symbol) => self.visit_type_query(symbol.0, type_id),
            TypeData::Application(app_id) => self.visit_application(*app_id, type_id),
            TypeData::TemplateLiteral(spans) => self.visit_template_literal(*spans),
            TypeData::Lazy(def_id) => self.visit_lazy(*def_id, type_id),
            TypeData::StringIntrinsic { kind, type_arg } => {
                self.visit_string_intrinsic(*kind, *type_arg)
            }
            TypeData::Intersection(list_id) => self.visit_intersection(*list_id),
            TypeData::Union(list_id) => self.visit_union(type_id, *list_id),
            TypeData::Array(elem) => self.visit_array(*elem, type_id),
            TypeData::Tuple(tuple_list_id) => self.visit_tuple(*tuple_list_id, type_id),
            TypeData::NoInfer(inner) => {
                // NoInfer<T> evaluates to T (strip wrapper, evaluate inner)
                self.evaluate(*inner)
            }
            TypeData::UnresolvedTypeName(atom) => self.visit_unresolved_type_name(*atom, type_id),
            TypeData::Substitution {
                base_type,
                constraint,
            } => {
                // Evaluate the base and constraint (resolving any Lazy aliases),
                // then re-derive: a now-concrete base collapses the narrowing.
                let base = self.evaluate(*base_type);
                let constraint = self.evaluate(*constraint);
                self.interner.substitution(base, constraint)
            }
            // All other types pass through unchanged (default behavior)
            _ => type_id,
        }
    }

    /// Resolve a cross-file reference carried as `UnresolvedTypeName(name)`.
    ///
    /// The lowering pass leaves a bare `UnresolvedTypeName(name)` whenever a
    /// type reference inside a (typically generic) declaration body could not be
    /// bound to a `DefId` in the checker that first lowered it — most commonly a
    /// name that is in scope only in the *declaring* file and is reached through
    /// a generic alias body at a *consuming* file (e.g. `type Lookup<K> =
    /// Registry[K]` imported and applied as `Lookup<"a">`, where `Registry`
    /// stays an `UnresolvedTypeName` once the alias crosses the module/arena
    /// boundary). The application *base* path already recovers such names via
    /// [`Self::resolve_application_def_id`], and the type-*argument* path via
    /// [`Self::try_expand_type_arg`]; this arm gives every other position
    /// (an index-access object `Registry[K]`, a `keyof` operand, a conditional
    /// check, ...) the same recovery so deferred operators over the reference
    /// reduce exactly as the same-module path does.
    ///
    /// Resolution defers to the active resolver: the `TypeEnvironment` pass only
    /// answers from the map seeded by the checker's cross-arena registration
    /// (declaring-file scoped, collision-safe), while the wider `CheckerContext`
    /// pass walks the merged binder graph. When the name genuinely does not
    /// resolve (a true error, or a registration-window artifact), the original
    /// display-preserving `UnresolvedTypeName` is returned unchanged.
    fn visit_unresolved_type_name(&mut self, atom: Atom, original_type_id: TypeId) -> TypeId {
        let name = self.interner.resolve_atom_ref(atom);
        if let Some(def_id) = self.resolver.resolve_unresolved_type_name(&name)
            && self.resolver.resolve_lazy(def_id, self.interner).is_some()
        {
            // Only commit to the rewrite when the resolver surfaces a body for
            // the recovered def; otherwise keep the name opaque so a later pass
            // (with the body registered) expands it, rather than collapsing to a
            // bare unresolved `Lazy`. Evaluating the canonical `Lazy(def_id)`
            // (not its raw body) reuses `visit_lazy`'s default/`this`-binding
            // handling and follows any alias chain through the recursion guard.
            return self.evaluate(self.interner.lazy(def_id));
        }
        original_type_id
    }

    /// Visit a conditional type: T extends U ? X : Y
    fn visit_conditional(&mut self, cond_id: ConditionalTypeId) -> TypeId {
        let cond = self.interner.get_conditional(cond_id);
        // tsc propagates the error type through conditionals: when the check type
        // resolves to a genuine error (e.g. a failed indexed access `O[number]`),
        // the whole conditional resolves to the error type so neither branch is
        // selected and downstream diagnostics stay suppressed instead of cascading.
        //
        // An `UnresolvedTypeName` is NOT a genuine error — it is a display-preserving
        // reference the current resolver could not bind to a `DefId` (e.g. a cross-
        // file interface member typed by a sibling type whose bare name is not in the
        // consuming file's scope under a namespace import). The broad `is_error_type`
        // folds `UnresolvedTypeName` into "error", so bailing on it would fabricate
        // `error` for the whole conditional (and, through a homomorphic mapped body,
        // mint `{ k: error }`). `is_genuine_error_type` excludes it, so the check side
        // instead defers to `evaluate_conditional`. (Note: the *extends*-side
        // unresolved-name handling still folds one layer down; the durable fix is the
        // cross-arena member-type lowering campaign in #13044 / #13484.)
        let evaluated_check = self.evaluate(cond.check_type);
        if crate::visitor::is_genuine_error_type(self.interner, evaluated_check) {
            return TypeId::ERROR;
        }
        self.evaluate_conditional(&cond)
    }

    /// Visit an index access type: T[K]
    fn visit_index_access(&mut self, object_type: TypeId, index_type: TypeId) -> TypeId {
        self.evaluate_index_access(object_type, index_type)
    }

    /// Visit a mapped type: { [K in Keys]: V }
    fn visit_mapped(&mut self, mapped_id: MappedTypeId) -> TypeId {
        let mapped = self.interner.get_mapped(mapped_id);
        self.evaluate_mapped(&mapped)
    }

    /// Visit a keyof type: keyof T
    fn visit_keyof(&mut self, operand: TypeId) -> TypeId {
        let result = self.evaluate_keyof(operand);

        // Store a display alias so the formatter can display "keyof X" instead
        // of the expanded union of literal keys.  tsc preserves the `keyof`
        // form when the operand is a named type (interface / class / alias).
        //
        // We only store the alias when:
        //   - the result is a concrete union or literal (not never / intrinsic)
        //   - the operand looks like a named type (Lazy, Application, Enum, or
        //     has a def-store mapping)
        // This prevents anonymous-object keyof from displaying as
        // `keyof { a: string; b: number }` (tsc shows the expanded form there).
        if result != TypeId::NEVER && !result.is_intrinsic() {
            let keyof_type = self.interner().keyof(operand);
            if result != keyof_type {
                let operand_is_named = matches!(
                    self.interner().lookup(operand),
                    Some(
                        TypeData::Lazy(_)
                            | TypeData::Application(_)
                            | TypeData::Enum(_, _)
                            | TypeData::TypeQuery(_)
                    )
                );
                if operand_is_named {
                    display_provenance::record_alias_application(
                        self.interner(),
                        AliasApplicationProvenance {
                            evaluated: result,
                            application: keyof_type,
                        },
                        AliasApplicationPriority::PreserveExisting,
                    );
                }
            }
        }

        result
    }

    /// Visit a type query: typeof expr
    ///
    /// `TypeQuery` represents `typeof X` which must resolve to the VALUE-space type
    /// (constructor type for classes). We use `resolve_ref` which returns the
    /// constructor type stored under `SymbolRef`, NOT `resolve_lazy` which returns
    /// the instance type for classes. This distinction is critical: `typeof A`
    /// for a class A should give the constructor type (with static members and
    /// construct signatures), not the instance type.
    fn visit_type_query(&mut self, symbol_ref: u32, original_type_id: TypeId) -> TypeId {
        use crate::types::SymbolRef;
        let symbol = SymbolRef(symbol_ref);

        // Use resolve_type_query which returns the VALUE type (constructor for classes).
        // Unlike resolve_ref, resolve_type_query is aware that TypeQuery needs the
        // constructor type, not the instance type that may be stored under SymbolRef
        // in TypeEnvironment (inserted by type_reference_symbol_type).
        //
        // We must evaluate the resolved type (as visit_lazy does) because the resolver
        // may return a Lazy(DefId) that still needs unfolding — e.g. DateConstructor.
        if let Some(resolved) = self.resolver.resolve_type_query(symbol, self.interner) {
            return self.evaluate_resolved_or_original(resolved, original_type_id);
        }

        // Fallback: try DefId-based resolution if no SymbolRef mapping exists
        if let Some(def_id) = self.resolver.symbol_to_def_id(symbol)
            && let Some(resolved) = self.resolver.resolve_lazy(def_id, self.interner)
        {
            return self.evaluate_resolved_or_original(resolved, original_type_id);
        }

        original_type_id
    }

    /// Evaluate `resolved` if it differs from `original`; avoids re-entering a
    /// type that resolved to itself (which would trigger the cycle guard unnecessarily).
    #[inline]
    fn evaluate_resolved_or_original(&mut self, resolved: TypeId, original: TypeId) -> TypeId {
        if resolved == original {
            original
        } else {
            self.evaluate(resolved)
        }
    }

    /// Visit a generic type application: Base<Args>
    fn visit_application(&mut self, app_id: TypeApplicationId, original_type_id: TypeId) -> TypeId {
        self.evaluate_application(app_id, original_type_id)
    }

    /// Visit a template literal type: `hello${T}world`
    fn visit_template_literal(&mut self, spans: TemplateLiteralId) -> TypeId {
        self.evaluate_template_literal(spans)
    }

    /// Visit a lazy type reference: Lazy(DefId)
    fn visit_lazy(&mut self, def_id: DefId, original_type_id: TypeId) -> TypeId {
        if let Some(resolved) = self.resolver.resolve_lazy(def_id, self.interner) {
            if self.is_self_recursive_promise_union(resolved, def_id) {
                return original_type_id;
            }

            let resolved = if !self.suppress_this_binding
                && crate::contains_this_type(self.interner, resolved)
            {
                crate::instantiation::instantiate::substitute_this_type_cached(
                    self.interner,
                    self.query_db,
                    resolved,
                    original_type_id,
                )
            } else {
                resolved
            };

            // When a bare Lazy(DefId) is used without an Application wrapper,
            // but the underlying type has type parameters that all have defaults
            // (e.g., `Uint8Array<T extends ArrayBufferLike = ArrayBuffer>`),
            // we must instantiate the resolved body with those defaults.
            // Otherwise the body retains unsubstituted type parameters.
            let resolved = if let Some(type_params) = self.resolver.get_lazy_type_params(def_id) {
                if !type_params.is_empty() && type_params.iter().all(|p| p.default.is_some()) {
                    let default_args: Vec<_> = type_params
                        .iter()
                        .map(|p| p.default.unwrap_or(TypeId::ERROR))
                        .collect();
                    instantiate_generic_cached(
                        self.interner,
                        self.query_db,
                        resolved,
                        &type_params,
                        &default_args,
                    )
                } else {
                    resolved
                }
            } else {
                resolved
            };

            // Re-evaluate the resolved type in case it needs further evaluation
            self.evaluate(resolved)
        } else {
            // The `Lazy(DefId)` has no resolvable body on this query: the def is
            // mid-registration, or owned by a file whose checker has not yet
            // published it (the cross-file / cross-arena registration window).
            // The bare `Lazy` returned here is a *registration-window artifact*,
            // not a stable function of `original_type_id`: once the declaring
            // file registers the body, `evaluate` reduces the same `Lazy` to the
            // concrete type. Mark the taint so this opaque result is kept out of
            // the `TypeId`-keyed eval caches — the same discipline the other
            // unresolved-def deferral sites already apply (`evaluate_application`,
            // conditional reduction, `evaluate_keyof`, the indexed-access
            // visitor). Without it the under-resolved `Lazy` is memoized as
            // authoritative and permanently shadows the real type, the
            // cross-arena member-degradation class tracked under #14347
            // (witnessed by #13484 / #10663). This is the *canonical* bare-`Lazy`
            // evaluation path: every consumer that bottoms out at an unresolved
            // `Lazy` through `evaluate` (template-literal spans, string-intrinsic
            // arguments, mapped-type constraints, …) inherits the taint here, so
            // no per-consumer classification is required.
            //
            // `resolve_lazy` succeeding (including the self-recursive promise
            // cycle break above) never reaches this arm, so a genuinely resolved
            // — merely recursive — alias is not tainted.
            self.mark_unresolved_def_seen();
            original_type_id
        }
    }

    /// Detect recursive aliases whose recursion flows through a well-known
    /// promise-like wrapper, e.g. `type T = string | Promise<T>`.
    ///
    /// General recursive unions such as `Json` and recursive arrays must still
    /// expand so structural assignability can inspect their non-recursive arms.
    /// Promise fulfillment cycles are different: structural comparison of
    /// `Promise<T>`'s callbacks can chase `T -> Promise<T> -> T` indefinitely.
    /// Keep only those promise-recursive aliases opaque at the outer lazy
    /// boundary and let ordinary recursion continue through the normal
    /// evaluator guard.
    fn is_self_recursive_promise_union(&self, type_id: TypeId, def_id: DefId) -> bool {
        let Some(TypeData::Union(list_id)) = self.interner.lookup(type_id) else {
            return false;
        };

        self.interner
            .type_list(list_id)
            .iter()
            .any(|member| self.is_promise_application_containing_def(*member, def_id, 0))
    }

    fn is_promise_application_containing_def(
        &self,
        type_id: TypeId,
        def_id: DefId,
        depth: u8,
    ) -> bool {
        if depth > 8 {
            return false;
        }

        match self.interner.lookup(type_id) {
            Some(TypeData::Application(app_id)) => {
                let app = self.interner.type_application(app_id);
                let args_contain_def = app
                    .args
                    .iter()
                    .any(|arg| crate::visitor::contains_lazy_def_id(self.interner, *arg, def_id));
                (self.is_well_known_promise_base(app.base) && args_contain_def)
                    || app.args.iter().any(|arg| {
                        self.is_promise_application_containing_def(*arg, def_id, depth + 1)
                    })
            }
            Some(TypeData::Union(list_id)) => {
                self.interner.type_list(list_id).iter().any(|member| {
                    self.is_promise_application_containing_def(*member, def_id, depth + 1)
                })
            }
            _ => false,
        }
    }

    fn is_well_known_promise_base(&self, base: TypeId) -> bool {
        if base == TypeId::PROMISE_BASE {
            return true;
        }

        let Some(TypeData::Lazy(def_id)) = self.interner.lookup(base) else {
            return false;
        };
        let Some(name) = self.resolver.get_def_name(def_id) else {
            return false;
        };
        matches!(
            self.interner.resolve_atom(name).as_str(),
            "Promise" | "PromiseLike"
        )
    }

    /// Visit a string manipulation intrinsic type: Uppercase<T>, Lowercase<T>, etc.
    fn visit_string_intrinsic(&mut self, kind: StringIntrinsicKind, type_arg: TypeId) -> TypeId {
        self.evaluate_string_intrinsic(kind, type_arg)
    }

    /// Visit an intersection type: A & B & C
    fn visit_intersection(&mut self, list_id: TypeListId) -> TypeId {
        self.evaluate_intersection(list_id)
    }

    /// Visit a tuple type: [A, B, ...C]
    ///
    /// Evaluates each element's type if it is a meta-type that can simplify
    /// (`IndexAccess`, Mapped, Conditional, etc.). For rest/spread elements
    /// whose evaluated type is itself a tuple, flattens them inline.
    /// For example: `[string, ...([number, boolean])]` → `[string, number, boolean]`
    ///
    /// Conservative: only evaluates element types that are known meta-types
    /// to avoid exponential blowup with recursive conditional types that
    /// produce tuples.
    fn visit_tuple(&mut self, tuple_list_id: TupleListId, original_type_id: TypeId) -> TypeId {
        use crate::intern::TEMPLATE_LITERAL_EXPANSION_LIMIT;
        use tsz_common::limits::MAX_REPRESENTABLE_TUPLE_LENGTH;

        let elements = self.interner.tuple_list(tuple_list_id);

        // Quick check: does any element need evaluation or structural normalization?
        // Also triggers when a rest element holds a concrete Tuple that must be
        // flattened — e.g. `[L, ...R]` after infer-binding R to `[1, 2]` — or a
        // union of concrete tuples that must be distributed — e.g.
        // `[0, ...([2] | [3, 4]), 1]` fans out into `[0, 2, 1] | [0, 3, 4, 1]`.
        // See `union_is_fully_spreadable` for which unions qualify (tuple
        // members only; array-unions and generic spreads are left alone to
        // match tsc).
        // ReadonlyType(Tuple) rest elements are already caught by is_evaluable_meta_type.
        let needs_eval = elements.iter().any(|elem| {
            Self::is_evaluable_meta_type(self.interner, elem.type_id)
                || (elem.rest
                    && (matches!(self.interner.lookup(elem.type_id), Some(TypeData::Tuple(_)))
                        || Self::union_is_fully_spreadable(self.interner, elem.type_id)))
        });
        if !needs_eval {
            return original_type_id;
        }

        let mut alternatives: Vec<Vec<TupleElement>> = vec![Vec::with_capacity(elements.len())];
        let mut changed = false;
        let mut spread_product = 1usize;

        for elem in elements.iter() {
            // Only evaluate element types that are meta-types (IndexAccess,
            // Mapped, Lazy, Application, etc.) — skip type parameters,
            // primitives, and already-concrete types to avoid blowup.
            let evaluated = if Self::is_evaluable_meta_type(self.interner, elem.type_id) {
                self.evaluate(elem.type_id)
            } else {
                elem.type_id
            };
            if evaluated != elem.type_id {
                changed = true;
            }

            // For rest/spread elements, if the evaluated type is a tuple,
            // flatten its elements inline (spreading the inner tuple).
            if elem.rest {
                if let Some(count) = self.tuple_spread_alternative_count(evaluated) {
                    spread_product = spread_product.saturating_mul(count);
                    if spread_product >= TEMPLATE_LITERAL_EXPANSION_LIMIT {
                        self.interner.mark_union_too_complex();
                        return TypeId::ERROR;
                    }
                }

                let evaluated_inner =
                    crate::type_queries::data::unwrap_readonly(self.interner, evaluated);
                if let Some(TypeData::Tuple(inner_list_id)) = self.interner.lookup(evaluated_inner)
                {
                    let inner_elements = self.interner.tuple_list(inner_list_id);
                    let current_len = alternatives.iter().map(|a| a.len()).max().unwrap_or(0);
                    if current_len.saturating_add(inner_elements.len())
                        > MAX_REPRESENTABLE_TUPLE_LENGTH
                    {
                        self.interner.mark_tuple_too_large();
                        return TypeId::ERROR;
                    }
                    for alternative in &mut alternatives {
                        alternative.extend(inner_elements.iter().copied());
                    }
                    changed = true;
                    continue;
                } else if let Some(TypeData::Union(list_id)) = self.interner.lookup(evaluated) {
                    let members = self.interner.type_list(list_id);
                    let mut spread_alternatives: Vec<Vec<TupleElement>> =
                        Vec::with_capacity(members.len());
                    for &member in members.iter() {
                        let member_inner =
                            crate::type_queries::data::unwrap_readonly(self.interner, member);
                        match self.interner.lookup(member_inner) {
                            Some(TypeData::Tuple(inner_list_id)) => {
                                spread_alternatives
                                    .push(self.interner.tuple_list(inner_list_id).to_vec());
                            }
                            Some(TypeData::Array(_)) => {
                                // Keep the array form: a rest `TupleElement`'s
                                // `type_id` holds the spread operand (`E[]` for
                                // `...E[]`), matching the type-node lowering,
                                // the instantiator, and `expand_tuple_rest`.
                                // Unwrapping to the element type here corrupts
                                // the rebuilt tuple and makes the relation
                                // checker compare `E` against `E[]`.
                                spread_alternatives.push(vec![TupleElement {
                                    type_id: member_inner,
                                    name: elem.name,
                                    optional: elem.optional,
                                    rest: true,
                                }]);
                            }
                            _ => {
                                spread_alternatives.push(vec![TupleElement {
                                    type_id: member,
                                    name: elem.name,
                                    optional: elem.optional,
                                    rest: true,
                                }]);
                            }
                        }
                    }

                    let alternative_count =
                        alternatives.len().saturating_mul(spread_alternatives.len());
                    if alternative_count >= TEMPLATE_LITERAL_EXPANSION_LIMIT {
                        self.interner.mark_union_too_complex();
                        return TypeId::ERROR;
                    }

                    let max_prefix = alternatives.iter().map(|p| p.len()).max().unwrap_or(0);
                    let max_spread = spread_alternatives
                        .iter()
                        .map(|s| s.len())
                        .max()
                        .unwrap_or(0);
                    if max_prefix.saturating_add(max_spread) > MAX_REPRESENTABLE_TUPLE_LENGTH {
                        self.interner.mark_tuple_too_large();
                        return TypeId::ERROR;
                    }

                    let mut distributed = Vec::with_capacity(alternative_count);
                    for prefix in alternatives {
                        for spread in &spread_alternatives {
                            let mut next = Vec::with_capacity(prefix.len() + spread.len());
                            next.extend_from_slice(&prefix);
                            next.extend_from_slice(spread);
                            distributed.push(next);
                        }
                    }
                    alternatives = distributed;
                    changed = true;
                    continue;
                }
            }

            // Default: keep the (evaluated) type as-is. For rest elements this
            // preserves the spread-operand form (`E[]` for `...E[]`) that the
            // type-node lowering, the instantiator, and `expand_tuple_rest`
            // all assume; an earlier version unwrapped array rests to their
            // element type here, corrupting every evaluated tuple with a
            // concrete array rest (`[Box<T>, ...number[]]` became
            // `[Box<T>, ...number]`) and breaking recursive-generic relations.
            let next_element = TupleElement {
                type_id: evaluated,
                name: elem.name,
                optional: elem.optional,
                rest: elem.rest,
            };
            for alternative in &mut alternatives {
                alternative.push(next_element);
            }
        }

        if !changed {
            return original_type_id;
        }

        if alternatives.len() == 1 {
            self.interner.tuple(alternatives.pop().unwrap_or_default())
        } else {
            self.interner.union(
                alternatives
                    .into_iter()
                    .map(|elems| self.interner.tuple(elems))
                    .collect(),
            )
        }
    }

    /// A union is "fully spreadable" when it is non-empty and every member is a
    /// concrete tuple type (possibly `readonly`-wrapped). Such a union in spread
    /// position distributes into one tuple per member — `[a, ...(X | Y), b]`
    /// becomes `[a, ...X, b] | [a, ...Y, b]` — because the members have
    /// differing fixed shapes that a single tuple cannot encode.
    ///
    /// Members that are bare arrays are intentionally excluded: tsc keeps a
    /// union-of-arrays rest as a single rest element (e.g.
    /// `[a, b, ...(X[] | Y[])]` stays put rather than fanning out), since an
    /// unbounded rest already encodes the union without distribution.
    /// Unions containing a generic type parameter or any other non-tuple member
    /// are likewise left undistributed, matching tsc's lazy handling of generic
    /// spreads.
    fn union_is_fully_spreadable(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
        let Some(TypeData::Union(list_id)) = db.lookup(type_id) else {
            return false;
        };
        let members = db.type_list(list_id);
        !members.is_empty()
            && members.iter().all(|&member| {
                let inner = crate::type_queries::data::unwrap_readonly(db, member);
                matches!(db.lookup(inner), Some(TypeData::Tuple(_)))
            })
    }

    fn tuple_spread_alternative_count(&self, type_id: TypeId) -> Option<usize> {
        match self.interner.lookup(type_id) {
            Some(TypeData::Tuple(_)) => Some(1),
            Some(TypeData::Union(list_id)) => Some(self.interner.type_list(list_id).len()),
            _ => None,
        }
    }

    /// Constituent count of `type_id`: a union contributes its member count,
    /// any other type contributes one. Used by the union-complexity (TS2590)
    /// caps that bound mapped-type and index-access distribution.
    pub(crate) fn count_union_members(&self, type_id: TypeId) -> usize {
        match self.interner().lookup(type_id) {
            Some(TypeData::Union(list_id)) => self.interner().type_list(list_id).len(),
            _ => 1,
        }
    }

    /// Check if a type is a meta-type that would benefit from evaluation
    /// inside a tuple element. Excludes type parameters and concrete types
    /// to avoid recursive blowup.
    fn is_evaluable_meta_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
        if type_id.is_intrinsic() {
            return false;
        }
        let Some(key) = db.lookup(type_id) else {
            return false;
        };
        matches!(
            key,
            TypeData::IndexAccess(_, _)
                | TypeData::Mapped(_)
                | TypeData::Lazy(_)
                | TypeData::Application(_)
                | TypeData::KeyOf(_)
                | TypeData::TemplateLiteral(_)
                | TypeData::StringIntrinsic { .. }
                | TypeData::ReadonlyType(_)
                | TypeData::TypeQuery(_)
        )
    }

    /// Visit a union type: A | B | C
    fn visit_union(&mut self, type_id: TypeId, list_id: TypeListId) -> TypeId {
        self.evaluate_union(type_id, list_id)
    }

    /// Visit an array type: T[].
    ///
    /// Keep the same conservative policy as tuple element evaluation: only
    /// evaluate element types that are solver meta-types. This lets aliases in
    /// array element position simplify before printing without
    /// recursively expanding already-concrete element types.
    fn visit_array(&mut self, elem: TypeId, original_type_id: TypeId) -> TypeId {
        if !Self::is_evaluable_meta_type(self.interner, elem) {
            return original_type_id;
        }

        let evaluated = self.evaluate(elem);
        if evaluated == elem {
            original_type_id
        } else {
            self.interner.array(evaluated)
        }
    }
}

#[cfg(test)]
#[path = "../../../tests/extract_type_params_memo_tests.rs"]
mod extract_type_params_memo_tests;
