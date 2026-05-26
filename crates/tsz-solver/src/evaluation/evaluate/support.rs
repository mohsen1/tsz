//! Support methods for `TypeEvaluator` argument expansion, simplification,
//! and visitor dispatch.

use super::*;

impl<'a, R: TypeResolver> TypeEvaluator<'a, R> {
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

    /// Extract type parameter infos from a type by scanning for `TypeParameter` types.
    pub(crate) fn extract_type_params_from_type(&self, type_id: TypeId) -> Vec<TypeParamInfo> {
        let mut seen = FxHashSet::default();
        let mut params = Vec::new();
        self.collect_type_params(type_id, &mut seen, &mut params);
        params
    }

    /// Recursively collect `TypeParameter` types from a type.
    fn collect_type_params(
        &self,
        type_id: TypeId,
        seen: &mut FxHashSet<Atom>,
        params: &mut Vec<TypeParamInfo>,
    ) {
        if type_id.is_intrinsic() {
            return;
        }

        let Some(key) = self.interner.lookup(type_id) else {
            return;
        };

        match key {
            TypeData::TypeParameter(ref info) if !seen.contains(&info.name) => {
                seen.insert(info.name);
                params.push(*info);
            }
            TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id) => {
                let shape = self.interner.object_shape(shape_id);
                for prop in &shape.properties {
                    self.collect_type_params(prop.type_id, seen, params);
                }
            }
            TypeData::Function(shape_id) => {
                let shape = self.interner.function_shape(shape_id);
                for param in &shape.params {
                    self.collect_type_params(param.type_id, seen, params);
                }
                self.collect_type_params(shape.return_type, seen, params);
            }
            TypeData::Union(members) | TypeData::Intersection(members) => {
                let members = self.interner.type_list(members);
                for &member in members.iter() {
                    self.collect_type_params(member, seen, params);
                }
            }
            TypeData::Array(elem) => {
                self.collect_type_params(elem, seen, params);
            }
            TypeData::Conditional(cond_id) => {
                let cond = self.interner.get_conditional(cond_id);
                self.collect_type_params(cond.check_type, seen, params);
                self.collect_type_params(cond.extends_type, seen, params);
                self.collect_type_params(cond.true_type, seen, params);
                self.collect_type_params(cond.false_type, seen, params);
            }
            TypeData::Application(app_id) => {
                let app = self.interner.type_application(app_id);
                self.collect_type_params(app.base, seen, params);
                for &arg in &app.args {
                    self.collect_type_params(arg, seen, params);
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
                self.collect_type_params(mapped.constraint, seen, params);
                self.collect_type_params(mapped.template, seen, params);
                if let Some(name_type) = mapped.name_type {
                    self.collect_type_params(name_type, seen, params);
                }
            }
            TypeData::KeyOf(operand) => {
                // Extract type params from the operand of keyof
                // e.g., keyof T -> extract T
                self.collect_type_params(operand, seen, params);
            }
            TypeData::IndexAccess(obj, idx) => {
                // Extract type params from both object and index
                // e.g., T[K] -> extract T and K
                self.collect_type_params(obj, seen, params);
                self.collect_type_params(idx, seen, params);
            }
            TypeData::TemplateLiteral(spans) => {
                // Extract type params from template literal interpolations
                let spans = self.interner.template_list(spans);
                for span in spans.iter() {
                    if let TemplateSpan::Type(inner) = span {
                        self.collect_type_params(*inner, seen, params);
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
                    for tp in &sig.type_params {
                        if !seen.contains(&tp.name) {
                            seen.insert(tp.name);
                            params.push(*tp);
                        }
                    }
                }
            }
            _ => {}
        }
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
        match key {
            TypeData::TypeQuery(sym_ref) => {
                // Resolve the TypeQuery to get the VALUE type (constructor for classes).
                // Use resolve_type_query which returns constructor types for classes,
                // unlike resolve_ref which may return instance types.
                if let Some(resolved) = self.resolver.resolve_type_query(sym_ref, self.interner) {
                    resolved
                } else if let Some(def_id) = self.resolver.symbol_to_def_id(sym_ref) {
                    self.resolver
                        .resolve_lazy(def_id, self.interner)
                        .unwrap_or(arg)
                } else {
                    arg
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
                // Resolve Lazy types in type arguments
                // This helps with generic instantiation accuracy
                self.resolver
                    .resolve_lazy(def_id, self.interner)
                    .unwrap_or(arg)
            }
            _ => arg,
        }
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
    fn is_complex_type(&self, type_id: TypeId) -> bool {
        if type_id.is_intrinsic() {
            return false;
        }
        let Some(key) = self.interner.lookup(type_id) else {
            return false;
        };

        match key {
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
            | TypeData::ThisType => true,
            // Intersection/union types containing complex members are also complex.
            // Without this, the evaluator's subtype-based simplification can incorrectly
            // collapse union members like `(T&U&1) | (T&U&2) | (T&U&3)` to just `T&U&2`
            // because the constraint fallback determines some branches are always `never`.
            // TSC does not perform such simplification on unions with type parameters.
            TypeData::Intersection(list_id) | TypeData::Union(list_id) => {
                let members = self.interner.type_list(list_id);
                members.iter().any(|&m| self.is_complex_type(m))
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

        // Deep structural simplification using SubtypeChecker
        self.simplify_union_members(&mut evaluated_members);

        let result = self.interner.union(evaluated_members.clone());
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

    /// Simplify union members by removing redundant types using deep subtype checks.
    /// If A <: B, then A | B = B (A is redundant in the union).
    ///
    /// This uses `SubtypeChecker` with `bypass_evaluation=true` to prevent infinite
    /// recursion, since `TypeEvaluator` has already evaluated all members.
    ///
    /// Performance: O(N²) where N is the number of members. We skip simplification
    /// if the union has more than 25 members to avoid excessive computation.
    ///
    /// ## Strategy
    ///
    /// 1. **Early exit for large unions** (>25 members) to avoid O(N²) explosion
    /// 2. **Skip complex types** that require full resolution:
    ///    - `TypeParameter`, Infer, Conditional, Mapped, `IndexAccess`, `KeyOf`, `TypeQuery`
    ///    - `TemplateLiteral`, `ReadonlyType`, String manipulation types
    ///    - Note: Lazy and Application are NOW safe (Task #37: handled by Canonicalizer)
    /// 3. **Fast-path for any/unknown**: If any member is any, entire union becomes any
    /// 4. **Identity check**: O(1) structural identity via `SubtypeChecker` (Task #36 fast-path)
    /// 5. **Depth limit**: `MAX_SUBTYPE_DEPTH` enables deep recursive type simplification (Task #37)
    ///
    /// ## Example Reductions
    ///
    /// - `"a" | string` → `string` (literal absorbed by primitive)
    /// - `number | 1 | 2` → `number` (literals absorbed by primitive)
    /// - `{ a: string } | { a: string; b: number }` → `{ a: string; b: number }`
    fn simplify_union_members(&mut self, members: &mut Vec<TypeId>) {
        // Single-pass early-exit: check for unknown (skip entirely) and whether all
        // members are identity-comparable (disjoint, so O(n²) loop finds nothing).
        let mut all_identity = true;
        for &id in members.iter() {
            if id.is_unknown() {
                return;
            }
            if all_identity && !self.interner.is_identity_comparable_type(id) {
                all_identity = false;
            }
        }
        if all_identity {
            return;
        }
        // In a union, A <: B means A is redundant (B subsumes it).
        // E.g. `"a" | string` => "a" is redundant, result: `string`
        self.remove_redundant_members(members, SubtypeDirection::SourceSubsumedByOther);
    }

    /// Simplify intersection members by removing redundant types using deep subtype checks.
    /// If A <: B, then A & B = A (B is redundant in the intersection).
    ///
    /// ## Example Reductions
    ///
    /// - `{ a: string } & { a: string; b: number }` → `{ a: string; b: number }`
    /// - `{ readonly a: string } & { a: string }` → `{ readonly a: string }`
    /// - `number & 1` → `1` (literal is more specific)
    fn simplify_intersection_members(&mut self, members: &mut Vec<TypeId>) {
        // In an intersection, A <: B means B is redundant (A is more specific).
        // We check if other members are subtypes of the candidate to remove the supertype.
        self.remove_redundant_members(members, SubtypeDirection::OtherSubsumedBySource);
    }

    /// Remove redundant members from a type list using subtype checks.
    ///
    /// This is the shared O(n²) core for both union and intersection simplification.
    /// The `direction` parameter controls which subtype relationship makes a member
    /// redundant:
    /// - `SourceSubsumedByOther`: member[i] <: member[j] → i is redundant (union semantics)
    /// - `OtherSubsumedBySource`: member[j] <: member[i] → i is redundant (intersection semantics)
    ///
    /// Common early exits (size guards, `any` check, complex-type check) are applied here.
    fn remove_redundant_members(&mut self, members: &mut Vec<TypeId>, direction: SubtypeDirection) {
        // Performance guard: skip small or very large type lists
        const MAX_SIMPLIFICATION_SIZE: usize = 25;
        if members.len() < 2 || members.len() > MAX_SIMPLIFICATION_SIZE {
            return;
        }

        // Single-pass early-exit check instead of two separate O(N) scans.
        for &id in members.iter() {
            if id.is_any()
                || crate::contains_this_type(self.interner, id)
                || self.is_complex_type(id)
            {
                return;
            }
        }

        use crate::relations::subtype::{MAX_SUBTYPE_DEPTH, SubtypeChecker};
        let mut checker = SubtypeChecker::with_resolver(self.interner, self.resolver);
        checker.bypass_evaluation = true;
        checker.max_depth = MAX_SUBTYPE_DEPTH;
        checker.no_unchecked_indexed_access = self.no_unchecked_indexed_access;

        // Pre-compute property name sets for all members once, avoiding O(N²) FxHashSet
        // allocations in the inner loop. Each entry is None for non-object types.
        let prop_names: Vec<Option<FxHashSet<u32>>> = members
            .iter()
            .map(|&id| {
                let mut names = FxHashSet::default();
                Self::collect_property_names(self.interner, id, &mut names);
                if names.is_empty() { None } else { Some(names) }
            })
            .collect();

        // Use mark-and-compact instead of Vec::remove() which is O(N) per removal.
        // Since max size is 25 (from guard above), a u32 bitset avoids heap allocation.
        let len = members.len();
        let mut keep: u32 = (1u32 << len) - 1; // all bits set
        for i in 0..len {
            if keep & (1u32 << i) == 0 {
                continue;
            }
            for j in 0..len {
                if i == j || keep & (1u32 << j) == 0 {
                    continue;
                }
                if members[i] == members[j] {
                    continue;
                }

                let is_subtype = match direction {
                    SubtypeDirection::SourceSubsumedByOther => {
                        checker.is_subtype_of(members[i], members[j])
                            && !Self::has_unique_properties_cached(&prop_names[i], &prop_names[j])
                            // Don't remove a member with an index signature when the
                            // subsuming member lacks one. The index signature carries
                            // semantic information that affects assignability checks
                            // against targets with index signatures.
                            // E.g., `Dict<string> | {}` must not simplify to `{}`
                            // because Dict<string> has `[index: string]: string` which
                            // can fail assignability to `Record<string, number>`.
                            && !Self::has_index_signature_not_in(self.interner, members[i], members[j])
                            // Branded-primitive idiom: literal members of a union
                            // are NOT absorbed by `string & {}` (or friends). tsc's
                            // union literal absorption only triggers when the union
                            // directly contains the primitive intrinsic; an
                            // Intersection wrapping the primitive is a distinct
                            // type that carries the brand. Keeping the literals
                            // alive is what makes `(string & {}) | "literal"`
                            // retain its literal properties when used as a mapped
                            // type constraint (e.g., `Record<Alignment, V>`).
                            && !Self::is_literal_under_branded_primitive(
                                self.interner,
                                members[i],
                                members[j],
                            )
                    }
                    SubtypeDirection::OtherSubsumedBySource => {
                        // For intersections: member[j] <: member[i] means member[i] is
                        // a candidate for removal. But if member[i] contributes properties
                        // that member[j] doesn't have, it must be kept — removing it would
                        // lose those property declarations from the intersection type.
                        // This matters for optional properties: {a: string} <: {b?: number}
                        // but {a: string} & {b?: number} must preserve both properties.
                        //
                        // Opaque Application/Lazy guard: when bypass_evaluation prevents
                        // SubtypeChecker from expanding an unreduced Application or Lazy
                        // member, that member appears empty to the checker. A concrete
                        // sibling like `{path?: _}` would then trivially "subsume" it and
                        // get the Application dropped, even though the Application would
                        // contribute additional union/object members once expanded.
                        // Skip the drop so `Application(stripPath<U>) & {path?: _}` keeps
                        // both members and downstream property collection sees the union.
                        !Self::is_opaque_under_bypass_eval(self.interner, members[i])
                            && checker.is_subtype_of(members[j], members[i])
                            && !Self::has_unique_properties_cached(&prop_names[i], &prop_names[j])
                            // Branded-primitive idiom: keep `{}` paired with a widening
                            // primitive intrinsic so `string & {}` (and friends) stay as
                            // Intersection rather than collapsing to the bare primitive.
                            // Mirrors the same exception in `intern/intersection.rs` so
                            // unions like `(string & {}) | "literal"` retain their
                            // literal members.
                            && !Self::is_branded_primitive_pair(self.interner, members[i], members[j])
                    }
                };
                if is_subtype {
                    keep &= !(1u32 << i);
                    break;
                }
            }
        }
        // Compact: retain only non-redundant elements
        let mut write = 0;
        for read in 0..len {
            if keep & (1u32 << read) != 0 {
                if write != read {
                    members[write] = members[read];
                }
                write += 1;
            }
        }
        members.truncate(write);
    }

    /// Check if `candidate` has any property names that `subsuming` doesn't have,
    /// using pre-computed property name sets to avoid repeated allocation.
    fn has_unique_properties_cached(
        candidate_names: &Option<FxHashSet<u32>>,
        subsuming_names: &Option<FxHashSet<u32>>,
    ) -> bool {
        let Some(candidate) = candidate_names else {
            return false; // No properties → can't contribute unique ones
        };
        let Some(subsuming) = subsuming_names else {
            return true; // Candidate has properties but subsuming doesn't
        };
        candidate.iter().any(|name| !subsuming.contains(name))
    }

    /// Check whether a (candidate, subsuming) pair forms the branded-primitive
    /// idiom `string & {}` (or `number & {}`, `boolean & {}`, …) — i.e. the
    /// candidate is the empty-object brand and the subsuming member is a
    /// widening primitive intrinsic. Such pairs must not be reduced via
    /// subtype elimination so the Intersection shape survives and unions
    /// like `(string & {}) | "literal"` keep their literal members.
    fn is_branded_primitive_pair(
        db: &dyn crate::caches::db::TypeDatabase,
        candidate: TypeId,
        subsuming: TypeId,
    ) -> bool {
        crate::visitors::visitor_predicates::is_empty_object_type(db, candidate)
            && crate::visitors::visitor_predicates::is_widening_primitive_intrinsic(db, subsuming)
    }

    /// Returns true when `type_id` is an unreduced `Application` or `Lazy`
    /// whose structural shape cannot be inspected while `bypass_evaluation`
    /// is on. Such members must not be dropped from intersections via
    /// subtype-redundancy: under bypass eval the `SubtypeChecker` treats them
    /// as empty, so a concrete sibling object can falsely subsume them.
    fn is_opaque_under_bypass_eval(
        db: &dyn crate::caches::db::TypeDatabase,
        type_id: TypeId,
    ) -> bool {
        matches!(
            db.lookup(type_id),
            Some(TypeData::Application(_) | TypeData::Lazy(_))
        )
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

    /// Check whether a union member is a literal that's only "subsumed" by a
    /// branded-primitive intersection (`string & {}` and friends). tsc's
    /// union literal absorption keys off the bare primitive intrinsic, not
    /// an Intersection wrapping it, so these literals must be kept alive
    /// for unions like `(string & {}) | "literal"`. Returns true when
    /// `candidate` is a literal type and `subsuming` is an Intersection
    /// whose only structural members are widening primitive intrinsics
    /// and empty-object brands.
    fn is_literal_under_branded_primitive(
        db: &dyn crate::caches::db::TypeDatabase,
        candidate: TypeId,
        subsuming: TypeId,
    ) -> bool {
        if !crate::visitors::visitor_predicates::is_literal_type(db, candidate) {
            return false;
        }
        let Some(TypeData::Intersection(list_id)) = db.lookup(subsuming) else {
            return false;
        };
        let members = db.type_list(list_id);
        let mut has_widening_primitive = false;
        let mut has_empty_object = false;
        for &m in members.iter() {
            if crate::visitors::visitor_predicates::is_widening_primitive_intrinsic(db, m) {
                has_widening_primitive = true;
            } else if crate::visitors::visitor_predicates::is_empty_object_type(db, m) {
                has_empty_object = true;
            } else {
                return false;
            }
        }
        has_widening_primitive && has_empty_object
    }

    /// Check if `candidate` has an index signature that `subsuming` lacks.
    ///
    /// In a union, removing a member with an index signature when the subsuming
    /// member doesn't have one changes assignability behavior. TypeScript checks
    /// each union member individually against a target, so a member with
    /// `[index: string]: T` can fail assignability to `{[index: string]: U}`
    /// even though the plain `{}` supertype passes. Preserving the index-signature
    /// member ensures tsz matches tsc's per-member union assignability semantics.
    fn has_index_signature_not_in(
        db: &dyn crate::caches::db::TypeDatabase,
        candidate: TypeId,
        subsuming: TypeId,
    ) -> bool {
        let candidate_has_idx = matches!(db.lookup(candidate), Some(TypeData::ObjectWithIndex(_)));
        let subsuming_has_idx = matches!(db.lookup(subsuming), Some(TypeData::ObjectWithIndex(_)));
        candidate_has_idx && !subsuming_has_idx
    }

    /// Collect property name atoms from an object type into the provided set.
    fn collect_property_names(
        db: &dyn crate::caches::db::TypeDatabase,
        type_id: TypeId,
        names: &mut FxHashSet<u32>,
    ) {
        if type_id.is_intrinsic() {
            return;
        }
        match db.lookup(type_id) {
            Some(TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id)) => {
                let shape = db.object_shape(shape_id);
                for prop in &shape.properties {
                    names.insert(prop.name.0);
                }
            }
            Some(TypeData::Intersection(list_id)) => {
                let sub_members = db.type_list(list_id);
                for &sub in sub_members.iter() {
                    Self::collect_property_names(db, sub, names);
                }
            }
            // Array and Tuple types have implicit properties (length, push, etc.)
            // that aren't in the type data. Use a sentinel to mark them as having
            // unique properties, preventing incorrect union simplification.
            // Without this, `T | T[]` unions collapse to `T` when T has only
            // optional properties (the array vacuously satisfies the optional check
            // but loses its array semantics).
            Some(TypeData::Array(_) | TypeData::Tuple(_)) => {
                names.insert(u32::MAX);
            }
            _ => {}
        }
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
            // All other types pass through unchanged (default behavior)
            _ => type_id,
        }
    }

    /// Visit a conditional type: T extends U ? X : Y
    fn visit_conditional(&mut self, cond_id: ConditionalTypeId) -> TypeId {
        let cond = self.interner.get_conditional(cond_id);
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
                    instantiate_generic(self.interner, resolved, &type_params, &default_args)
                } else {
                    resolved
                }
            } else {
                resolved
            };

            // Re-evaluate the resolved type in case it needs further evaluation
            self.evaluate(resolved)
        } else {
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
                } else if let Some(TypeData::Array(element_type)) = self.interner.lookup(evaluated)
                {
                    // Rest element evaluating to an array stays as rest
                    let rest_element = TupleElement {
                        type_id: element_type,
                        name: elem.name,
                        optional: elem.optional,
                        rest: true,
                    };
                    for alternative in &mut alternatives {
                        alternative.push(rest_element);
                    }
                    if element_type != elem.type_id {
                        changed = true;
                    }
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
                            Some(TypeData::Array(element_type)) => {
                                spread_alternatives.push(vec![TupleElement {
                                    type_id: element_type,
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
