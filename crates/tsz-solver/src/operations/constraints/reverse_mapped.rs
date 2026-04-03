//! Reverse mapped type inference and discriminant filtering.
//!
//! Contains reverse inference through homomorphic mapped types, iterator result
//! union handling, template literal reversal, and discriminant-based filtering.

use crate::inference::infer::InferenceContext;
use crate::instantiation::instantiate::{TypeSubstitution, instantiate_type};
use crate::operations::{AssignabilityChecker, CallEvaluator};
use crate::types::{
    MappedModifier, ObjectShape, PropertyInfo, TupleElement, TypeData, TypeId, TypeListId,
    Visibility,
};
use rustc_hash::{FxHashMap, FxHashSet};
use tracing::trace;

impl<'a, C: AssignabilityChecker> CallEvaluator<'a, C> {
    /// IteratorResult-specific inference: infer from yield branches only.
    ///
    /// For unions like `{ done: false, value: T } | { done: true, value: undefined }`,
    /// collect candidates from non-completed branches and avoid inferring from the
    /// completion branch (`done: true`).
    pub(super) fn constrain_iterator_result_unions(
        &mut self,
        ctx: &mut InferenceContext,
        var_map: &FxHashMap<TypeId, crate::inference::infer::InferenceVar>,
        source_members: TypeListId,
        target_members: TypeListId,
        priority: crate::types::InferencePriority,
    ) -> bool {
        let done_name = self.interner.intern_string("done");
        let value_name = self.interner.intern_string("value");

        let classify_iterator_result_member = |ty: TypeId| -> Option<(bool, TypeId)> {
            let shape_id = match self.interner.lookup(ty) {
                Some(TypeData::Object(id) | TypeData::ObjectWithIndex(id)) => id,
                _ => return None,
            };
            let shape = self.interner.object_shape(shape_id);
            let done_prop = PropertyInfo::find_in_slice(&shape.properties, done_name)?;
            let done_is_true = match self.interner.lookup(done_prop.type_id) {
                Some(TypeData::Literal(crate::LiteralValue::Boolean(true))) => true,
                Some(TypeData::Literal(crate::LiteralValue::Boolean(false))) => false,
                _ => return None,
            };
            let value_prop = PropertyInfo::find_in_slice(&shape.properties, value_name)?;
            Some((done_is_true, value_prop.type_id))
        };

        let source_union = self.interner.type_list(source_members);
        let target_union = self.interner.type_list(target_members);

        let mut source_has_true = false;
        let mut source_has_false = false;
        let mut source_values = Vec::new();
        for &m in source_union.iter() {
            if let Some((done_true, value_type)) = classify_iterator_result_member(m) {
                if done_true {
                    source_has_true = true;
                } else {
                    source_has_false = true;
                    source_values.push(value_type);
                }
            }
        }

        let mut target_has_true = false;
        let mut target_has_false = false;
        let mut target_values = Vec::new();
        for &m in target_union.iter() {
            if let Some((done_true, value_type)) = classify_iterator_result_member(m) {
                if done_true {
                    target_has_true = true;
                } else {
                    target_has_false = true;
                    target_values.push(value_type);
                }
            }
        }

        // Only apply this specialized path for actual IteratorResult-like unions.
        if !(source_has_true && source_has_false && target_has_true && target_has_false) {
            return false;
        }

        if source_values.is_empty() || target_values.is_empty() {
            return false;
        }

        for &s in &source_values {
            for &t in &target_values {
                self.constrain_types(ctx, var_map, s, t, priority);
            }
        }

        true
    }

    /// Check if `candidate` matches `target_placeholder`, accounting for
    /// intersection-typed placeholders. When `target_placeholder` is an
    /// intersection (e.g. `T & {}` from `LowInfer<T>`), `candidate` matches
    /// if it equals the intersection OR any of its members.
    pub(super) fn is_placeholder_match(&self, candidate: TypeId, target_placeholder: TypeId) -> bool {
        if candidate == target_placeholder {
            return true;
        }
        if let Some(TypeData::Intersection(members_id)) = self.interner.lookup(target_placeholder) {
            let members = self.interner.type_list(members_id);
            return members.contains(&candidate);
        }
        false
    }

    /// Find the `keyof T` inference target from a mapped type constraint,
    /// decomposing Union and Intersection constraints recursively.
    ///
    /// This follows tsc's `inferToMappedType` which handles:
    /// - Direct `keyof T` → returns T if T is an inference placeholder
    /// - Direct `keyof X` → returns X if X structurally contains placeholders
    /// - `keyof T & keyof Constraint` (Intersection) → recurses into members
    /// - `keyof A | keyof B` (Union) → recurses into members
    ///
    /// Returns the first `T` found where `keyof T` appears and T contains inference placeholders.
    pub(super) fn find_keyof_inference_target(
        &self,
        constraint: TypeId,
        var_map: &FxHashMap<TypeId, crate::inference::infer::InferenceVar>,
    ) -> Option<TypeId> {
        match self.interner.lookup(constraint) {
            Some(TypeData::KeyOf(keyof_target)) => {
                if var_map.contains_key(&keyof_target) {
                    return Some(keyof_target);
                }
                let mut visited = FxHashSet::default();
                if self.type_contains_placeholder(keyof_target, var_map, &mut visited) {
                    return Some(keyof_target);
                }
                None
            }
            Some(TypeData::Intersection(members) | TypeData::Union(members)) => {
                let member_list = self.interner.type_list(members);
                for &member in member_list.iter() {
                    if let Some(target) = self.find_keyof_inference_target(member, var_map) {
                        return Some(target);
                    }
                }
                None
            }
            _ => None,
        }
    }

    /// Attempt reverse mapped type inference for homomorphic mapped types.
    ///
    /// Given source `{ a: Box<number>, b: Box<string> }` and mapped type
    /// `{ [P in keyof T]: Box<T[P]> }` where T is a placeholder, builds the
    /// reverse object `{ a: number, b: string }` and constrains it against T.
    ///
    /// Returns `true` if reverse inference succeeded (all properties reversed),
    /// `false` if any property couldn't be reversed through the template.
    pub(super) fn constrain_reverse_mapped_type(
        &mut self,
        ctx: &mut InferenceContext,
        var_map: &FxHashMap<TypeId, crate::inference::infer::InferenceVar>,
        source_obj: &ObjectShape,
        mapped: &crate::types::MappedType,
        target_placeholder: TypeId,
    ) -> bool {
        let template = mapped.template;
        let iter_param_name = mapped.type_param.name;
        trace!(
            template = ?template,
            target_placeholder = ?target_placeholder,
            num_source_props = source_obj.properties.len(),
            "constrain_reverse_mapped_type"
        );

        let mut reverse_properties = Vec::new();
        let mut any_reversed = false;

        for prop in &source_obj.properties {
            // Substitute the iteration parameter K with the property name literal
            let key_literal = self.interner.literal_string_atom(prop.name);
            let mut subst = TypeSubstitution::new();
            subst.insert(iter_param_name, key_literal);
            let instantiated_template = instantiate_type(self.interner, template, &subst);

            // Reverse-infer through the template: find what T[K] should be.
            let reversed_value = match self.reverse_infer_through_template(
                prop.type_id,
                instantiated_template,
                target_placeholder,
            ) {
                Some(v) => {
                    any_reversed = true;
                    v
                }
                None => {
                    // When reversal fails because the source property is a function
                    // with only `any`-typed parameters (from untyped method shorthands),
                    // treat the reversal as successful with `unknown`. This matches
                    // tsc's getPartiallyInferableType behavior: implicit `any` params
                    // don't contribute to inference, producing `unknown` instead of
                    // falling through to the reverse-keyof `{ key: any }` path.
                    any_reversed = true;
                    TypeId::UNKNOWN
                }
            };

            // Reverse the mapped type's modifier directives to reconstruct T's modifiers.
            // If the mapped type adds a modifier, the reverse removes it (and vice versa).
            // If the mapped type has no modifier directive (None), it preserves the source's
            // modifier in the forward direction, so the reverse also preserves it.
            let optional = match mapped.optional_modifier {
                Some(MappedModifier::Add) => false,   // undo addition
                Some(MappedModifier::Remove) => true, // undo removal
                None => prop.optional,                // preserve source
            };
            let readonly = match mapped.readonly_modifier {
                Some(MappedModifier::Add) => false,   // undo addition
                Some(MappedModifier::Remove) => true, // undo removal
                None => prop.readonly,                // preserve source
            };

            reverse_properties.push(PropertyInfo {
                name: prop.name,
                type_id: reversed_value,
                write_type: reversed_value,
                optional,
                readonly,
                is_method: false,
                is_class_prototype: false,
                visibility: Visibility::Public,
                parent_id: None,
                declaration_order: 0,
                is_string_named: false,
            });
        }

        // Also reverse index signatures. For dictionary-like sources
        // (e.g., { [x: string]: Box<number>|Box<string>|Box<boolean> }),
        // reverse through the template to build the inferred T's index signature.
        let mut reverse_string_index = None;
        let mut reverse_number_index = None;

        if let Some(ref sig) = source_obj.string_index
            && let Some(reversed_value) =
                self.reverse_infer_through_template(sig.value_type, template, target_placeholder)
        {
            any_reversed = true;
            let readonly = match mapped.readonly_modifier {
                Some(MappedModifier::Add) => false,
                Some(MappedModifier::Remove) => true,
                None => sig.readonly,
            };
            reverse_string_index = Some(crate::types::IndexSignature {
                key_type: sig.key_type,
                value_type: reversed_value,
                readonly,
                param_name: sig.param_name,
            });
        }
        if let Some(ref sig) = source_obj.number_index
            && let Some(reversed_value) =
                self.reverse_infer_through_template(sig.value_type, template, target_placeholder)
        {
            any_reversed = true;
            let readonly = match mapped.readonly_modifier {
                Some(MappedModifier::Add) => false,
                Some(MappedModifier::Remove) => true,
                None => sig.readonly,
            };
            reverse_number_index = Some(crate::types::IndexSignature {
                key_type: sig.key_type,
                value_type: reversed_value,
                readonly,
                param_name: sig.param_name,
            });
        }

        // Only commit the reverse inference if at least one property or index sig was
        // successfully reversed. If ALL failed, abort and let the fallback paths handle it.
        if !any_reversed {
            return false;
        }

        // Build the reverse mapped object and constrain it against the placeholder T
        // using HomomorphicMappedType priority (lower than direct NakedTypeVariable inference).
        let reverse_object = if reverse_string_index.is_some() || reverse_number_index.is_some() {
            self.interner.object_with_index(ObjectShape {
                flags: crate::types::ObjectFlags::empty(),
                properties: reverse_properties,
                string_index: reverse_string_index,
                number_index: reverse_number_index,
                symbol: None,
            })
        } else {
            self.interner.object(reverse_properties)
        };
        self.constrain_types(
            ctx,
            var_map,
            reverse_object,
            target_placeholder,
            crate::types::InferencePriority::HomomorphicMappedType,
        );
        true
    }

    /// Reverse-infer a tuple source through a homomorphic mapped type.
    ///
    /// When source is a tuple like `[Box<number>, Box<string>]` and the mapped type is
    /// `{ [K in keyof T]: Box<T[K]> }`, this reverses each element through the template
    /// to reconstruct T as a tuple `[number, string]`.
    ///
    /// Returns `true` if reverse inference succeeded, `false` if it should be abandoned.
    pub(super) fn constrain_reverse_mapped_tuple(
        &mut self,
        ctx: &mut InferenceContext,
        var_map: &FxHashMap<TypeId, crate::inference::infer::InferenceVar>,
        source_elems: &[TupleElement],
        mapped: &crate::types::MappedType,
        target_placeholder: TypeId,
    ) -> bool {
        let template = mapped.template;
        let iter_param_name = mapped.type_param.name;

        let mut reverse_elements = Vec::with_capacity(source_elems.len());
        let mut any_reversed = false;

        for (i, elem) in source_elems.iter().enumerate() {
            // Skip rest elements — they complicate reverse inference
            if elem.rest {
                return false;
            }

            // Substitute the iteration parameter K with the numeric key literal "0", "1", ...
            let key_str = i.to_string();
            let key_atom = self.interner.intern_string(&key_str);
            let key_literal = self.interner.literal_string_atom(key_atom);
            let mut subst = TypeSubstitution::new();
            subst.insert(iter_param_name, key_literal);
            let instantiated_template = instantiate_type(self.interner, template, &subst);

            // Reverse-infer through the template: find what T[K] should be.
            let reversed_value = match self.reverse_infer_through_template(
                elem.type_id,
                instantiated_template,
                target_placeholder,
            ) {
                Some(v) => {
                    any_reversed = true;
                    v
                }
                None => TypeId::UNKNOWN,
            };

            // Reverse mapped type modifiers (same as object case)
            let optional = match mapped.optional_modifier {
                Some(MappedModifier::Add) => false,
                Some(MappedModifier::Remove) => true,
                None => elem.optional,
            };

            reverse_elements.push(TupleElement {
                type_id: reversed_value,
                name: elem.name,
                optional,
                rest: false,
            });
        }

        if !any_reversed {
            return false;
        }

        // Build the reverse tuple and constrain it against the placeholder T
        let reverse_tuple = self.interner.tuple(reverse_elements);
        self.constrain_types(
            ctx,
            var_map,
            reverse_tuple,
            target_placeholder,
            crate::types::InferencePriority::HomomorphicMappedType,
        );
        true
    }

    /// Reverse-infer a single property value through a mapped type template.
    ///
    /// Given `source_value` (e.g., `Box<number>`) and `template` (e.g., `Box<T["a"]>`),
    /// extracts what `T["a"]` must be (e.g., `number`).
    ///
    /// Returns `None` if the template is too complex to reverse (e.g., function types,
    /// conditional types, etc.), signaling that reverse inference should be abandoned.
    pub(super) fn reverse_infer_through_template(
        &mut self,
        source_value: TypeId,
        template: TypeId,
        target_placeholder: TypeId,
    ) -> Option<TypeId> {
        // Case 1: template is directly IndexAccess(T, key) → source IS the reversed value.
        // Also handles when target_placeholder is `T & {}` (from LowInfer<T> = T & {})
        // but the IndexAccess references the raw T — we check if T is a member of
        // the intersection.
        //
        // Additionally, when the checker's evaluate_type resolves the placeholder through
        // its constraint (e.g., `T extends object` → IndexAccess(object, P) instead of
        // IndexAccess(T_placeholder, P)), we recognize that the IndexAccess object type
        // is the constraint of the target placeholder and still accept the match.
        if let Some(TypeData::IndexAccess(obj, _idx)) = self.interner.lookup(template) {
            if self.is_placeholder_match(obj, target_placeholder) {
                return Some(source_value);
            }
            // Check if obj is the constraint of the target placeholder.
            // This happens when evaluate_type resolves T_placeholder[P] through T's
            // constraint, producing constraint[P]. We should still treat this as a
            // placeholder match for reverse mapped inference.
            if let Some(TypeData::TypeParameter(info)) = self.interner.lookup(target_placeholder)
                && let Some(constraint) = info.constraint
                && obj == constraint
            {
                return Some(source_value);
            }
        }

        // Case 2: template is Application(F, args) and source is Application(F, args')
        // with same base → recurse into matching args to find the T[K] position
        if let Some(TypeData::Application(template_app_id)) = self.interner.lookup(template) {
            let template_app = self.interner.type_application(template_app_id);
            if let Some(TypeData::Application(source_app_id)) = self.interner.lookup(source_value) {
                let source_app = self.interner.type_application(source_app_id);
                if template_app.base == source_app.base {
                    if template_app.args.len() == source_app.args.len() {
                        for (t_arg, s_arg) in template_app.args.iter().zip(source_app.args.iter()) {
                            if let Some(rev) = self.reverse_infer_through_template(
                                *s_arg,
                                *t_arg,
                                target_placeholder,
                            ) {
                                return Some(rev);
                            }
                        }
                        // Single type arg shortcut (Box<T[P]> → unwrap the single arg)
                        if template_app.args.len() == 1 {
                            return Some(source_app.args[0]);
                        }
                    } else if source_app.args.len() < template_app.args.len() {
                        // Source has fewer args due to defaulted type parameters
                        // (e.g., Reducer<number> has 1 arg, Reducer<S[K], A> has 2).
                        // Only try reverse inference for the args present in source,
                        // and only succeed if the target placeholder is found within
                        // those shared positions.
                        for (t_arg, s_arg) in template_app.args.iter().zip(source_app.args.iter()) {
                            if let Some(rev) = self.reverse_infer_through_template(
                                *s_arg,
                                *t_arg,
                                target_placeholder,
                            ) {
                                return Some(rev);
                            }
                        }
                    }
                }
            }

            // Case 2b: source is a Union of Applications with the same base as template.
            // Distribute the reverse inference over union members and combine results.
            // E.g., Box<number> | Box<string> | Box<boolean> against Box<T[P]>
            // → reverse each member → number | string | boolean
            if let Some(TypeData::Union(members_id)) = self.interner.lookup(source_value) {
                let members = self.interner.type_list(members_id);
                let mut reversed_parts = Vec::new();
                let mut all_reversed = true;
                for &member in members.iter() {
                    if let Some(rev) =
                        self.reverse_infer_through_template(member, template, target_placeholder)
                    {
                        reversed_parts.push(rev);
                    } else {
                        all_reversed = false;
                        break;
                    }
                }
                if all_reversed && !reversed_parts.is_empty() {
                    return Some(if reversed_parts.len() == 1 {
                        reversed_parts[0]
                    } else {
                        self.interner.union(reversed_parts)
                    });
                }
            }

            // Template is an Application but source doesn't match.
            // First try expanding the type alias without evaluation — this preserves
            // inference variables in the body (e.g., Wrap<T[K]> → {primitive: T[K]}).
            // Falls back to evaluate_type which may resolve inference variables.
            let expanded = self.checker.expand_type_alias_application(template);
            let evaluated_template =
                expanded.unwrap_or_else(|| self.checker.evaluate_type(template));
            if evaluated_template != template {
                let reversed = self.reverse_infer_through_template(
                    source_value,
                    evaluated_template,
                    target_placeholder,
                );
                if reversed.is_some() {
                    return reversed;
                }
            }

            // When expansion produced an intermediate form (e.g., a mapped type body)
            // that couldn't be reversed, also try full evaluation. This handles cases
            // like `Identity<T[K]>` where expansion gives `{ [K in keyof T[K]]: T[K][K] }`
            // (a mapped type we can't reverse through) but evaluation resolves T through
            // its constraint to produce `string[]` (matching source).
            let fully_evaluated = if expanded.is_some() {
                let eval_result = self.checker.evaluate_type(template);
                if eval_result != template && eval_result != evaluated_template {
                    eval_result
                } else {
                    evaluated_template
                }
            } else {
                evaluated_template
            };

            // Case 2c: Evaluation collapsed the placeholder (resolved T through its
            // constraint), producing a type structurally equal to the source. This means
            // the Application is identity-like (e.g., KeepLiteralStrings<T[K]> = { [K in keyof T]: T[K] }
            // evaluates to string[] when T extends Record<string, string[]>, matching source string[]).
            // In this case, try to reverse through the Application's type arguments directly.
            //
            // Guard: Only apply when evaluated result equals the source. This prevents
            // incorrect reversal through non-transparent Applications like Reducer<S[K], A>
            // where the Application wraps the placeholder in a different structure.
            if fully_evaluated == source_value {
                for &t_arg in &template_app.args {
                    if let Some(rev) =
                        self.reverse_infer_through_template(source_value, t_arg, target_placeholder)
                    {
                        return Some(rev);
                    }
                }
            }
            return None;
        }

        // Case 3: template is a Function type (from mapped type template like `() => T[K]`
        // or `(val: T[K]) => boolean`) and source is also a Function.
        // Reverse through parameters and return type to find the placeholder.
        if let Some(TypeData::Function(template_fn_id)) = self.interner.lookup(template) {
            let template_fn = self.interner.function_shape(template_fn_id);
            if let Some(TypeData::Function(source_fn_id)) = self.interner.lookup(source_value) {
                let source_fn = self.interner.function_shape(source_fn_id);
                // Try reversing through parameters first (handles contravariant case:
                // source `(v: string) => bool` against template `(val: T["foo"]) => bool`
                // → T["foo"] = string)
                //
                // Try reversing through parameters first (handles contravariant case:
                // source `(v: string) => bool` against template `(val: T["foo"]) => bool`
                // → T["foo"] = string)
                //
                // Apply "partially inferable" semantics: when the source parameter
                // type is `any` (typically from untyped method shorthand or callback),
                // treat it as `unknown` for reversal. This prevents implicit `any`
                // from flowing through as T[K] = any. Matches tsc's
                // getPartiallyInferableType behavior. We return Some(unknown) rather
                // than None so that the caller knows this property DID participate
                // in the reverse mapping (just with an uninformative type), preventing
                // fallback to the reverse-keyof `{ key: any }` path.
                let min_params = template_fn.params.len().min(source_fn.params.len());
                let mut _any_param_matched_placeholder = false;
                for i in 0..min_params {
                    if source_fn.params[i].type_id == TypeId::ANY {
                        // Check if the template param references the target placeholder.
                        // If so, record that we have an `any`-param match that should
                        // produce `unknown` rather than `any`.
                        if let Some(TypeData::IndexAccess(obj, _)) =
                            self.interner.lookup(template_fn.params[i].type_id)
                            && self.is_placeholder_match(obj, target_placeholder)
                        {
                            _any_param_matched_placeholder = true;
                        }
                        continue;
                    }
                    if let Some(reversed) = self.reverse_infer_through_template(
                        source_fn.params[i].type_id,
                        template_fn.params[i].type_id,
                        target_placeholder,
                    ) {
                        return Some(reversed);
                    }
                }
                // If only `any`-typed params matched the placeholder, return None
                // so the Object case (Case 4) tries the next property. The caller
                // (`constrain_reverse_mapped_type`) already defaults to UNKNOWN when
                // all property reversals fail.
                // Try reversing through the return type (covariant case:
                // source `() => number` against template `() => T["bar"]` → T["bar"] = number)
                return self.reverse_infer_through_template(
                    source_fn.return_type,
                    template_fn.return_type,
                    target_placeholder,
                );
            }
            // Source is not a matching function — can't reverse
            return None;
        }

        // Case 4: template is an Object type — recurse through matching properties.
        // This handles templates like `{ dependencies: KeepLiteralStrings<T[K]> }` where
        // the source is an object with the same properties. We find a property whose
        // template value contains the target placeholder and reverse through it.
        if let Some(
            TypeData::Object(template_shape_id) | TypeData::ObjectWithIndex(template_shape_id),
        ) = self.interner.lookup(template)
        {
            let template_obj = self.interner.object_shape(template_shape_id);
            if let Some(
                TypeData::Object(source_shape_id) | TypeData::ObjectWithIndex(source_shape_id),
            ) = self.interner.lookup(source_value)
            {
                let source_obj = self.interner.object_shape(source_shape_id);
                // Match properties by name and try to reverse through each
                let template_props = template_obj.properties.clone();
                let source_props = source_obj.properties.clone();
                for t_prop in &template_props {
                    for s_prop in &source_props {
                        if t_prop.name == s_prop.name
                            && let Some(reversed) = self.reverse_infer_through_template(
                                s_prop.type_id,
                                t_prop.type_id,
                                target_placeholder,
                            )
                        {
                            return Some(reversed);
                        }
                    }
                }
            }
            // Template is an Object but source doesn't match or no property reversed — can't reverse
            return None;
        }

        // Case 5: template is a Union type — try reversing through each member.
        // This handles templates like `((ctx: T) => T[K]) | T[K]` where the
        // source value can match one of the union members.
        //
        // Important: `IndexAccess(T, K)` (i.e. `T[K]`) is a catch-all that matches
        // any source value. When the union also contains structural members (functions,
        // objects, applications), we must try those first. Otherwise a function source
        // would match `T[K]` directly, inferring T.prop = fn_type instead of reversing
        // through the function template to extract T.prop = return_type.
        if let Some(TypeData::Union(members_id)) = self.interner.lookup(template) {
            let members = self.interner.type_list(members_id);

            // Partition: try structural members first, then IndexAccess catch-all.
            // T[K] is a catch-all that matches any source value, so structural
            // members (functions, objects, etc.) must be tried first.
            let mut catch_all: Option<TypeId> = None;
            for &member in members.iter() {
                if let Some(TypeData::IndexAccess(obj, _)) = self.interner.lookup(member)
                    && self.is_placeholder_match(obj, target_placeholder)
                {
                    debug_assert!(
                        catch_all.is_none(),
                        "multiple IndexAccess catch-all members in union template"
                    );
                    catch_all = Some(member);
                    continue;
                }
                if let Some(reversed) =
                    self.reverse_infer_through_template(source_value, member, target_placeholder)
                {
                    return Some(reversed);
                }
            }
            // Fall back to the catch-all T[K] if no structural member matched
            if let Some(ca) = catch_all
                && let Some(reversed) =
                    self.reverse_infer_through_template(source_value, ca, target_placeholder)
            {
                return Some(reversed);
            }
            return None;
        }

        // Case 6: template is a Mapped type (from recursive type alias expansion).
        // When a recursive type alias like `Spec<T[K]>` evaluates to a mapped type
        // `{ [P in keyof T[K]]: Func<T[K][P]> | Spec<T[K][P]> }`, and the source is
        // an object, perform a nested reverse-mapped inference with T[K] as the new
        // target placeholder. This reconstructs the inner type from source properties.
        if let Some(TypeData::Mapped(mapped_id)) = self.interner.lookup(template) {
            let mapped = self.interner.get_mapped(mapped_id);
            // Extract the new target placeholder from the constraint:
            // keyof X → X becomes the new placeholder for recursive reversal
            if let Some(TypeData::KeyOf(inner_placeholder)) =
                self.interner.lookup(mapped.constraint)
                && let Some(
                    TypeData::Object(source_shape_id) | TypeData::ObjectWithIndex(source_shape_id),
                ) = self.interner.lookup(source_value)
            {
                let source_obj = self.interner.object_shape(source_shape_id);
                let source_props = source_obj.properties.clone();
                let mut reverse_properties = Vec::new();
                let mut any_reversed = false;

                for prop in &source_props {
                    // Instantiate the mapped template with the concrete key
                    let key_literal = self.interner.literal_string_atom(prop.name);
                    let mut subst = TypeSubstitution::new();
                    subst.insert(mapped.type_param.name, key_literal);
                    let instantiated_template =
                        instantiate_type(self.interner, mapped.template, &subst);

                    // Recursively reverse through the instantiated template,
                    // using the inner placeholder (e.g., T["nested"]) instead of T
                    let reversed_value = match self.reverse_infer_through_template(
                        prop.type_id,
                        instantiated_template,
                        inner_placeholder,
                    ) {
                        Some(v) => {
                            any_reversed = true;
                            v
                        }
                        None => TypeId::UNKNOWN,
                    };

                    // Reverse modifiers (same logic as the outer level)
                    let optional = match mapped.optional_modifier {
                        Some(MappedModifier::Add) => false,
                        Some(MappedModifier::Remove) => true,
                        None => prop.optional,
                    };
                    let readonly = match mapped.readonly_modifier {
                        Some(MappedModifier::Add) => false,
                        Some(MappedModifier::Remove) => true,
                        None => prop.readonly,
                    };

                    reverse_properties.push(PropertyInfo {
                        name: prop.name,
                        type_id: reversed_value,
                        write_type: reversed_value,
                        optional,
                        readonly,
                        is_method: false,
                        is_class_prototype: false,
                        visibility: Visibility::Public,
                        parent_id: None,
                        declaration_order: 0,
                        is_string_named: false,
                    });
                }

                if any_reversed {
                    return Some(self.interner.object(reverse_properties));
                }
            }
            return None;
        }

        // Case 7: template is a Conditional type.
        // For mapped type templates like `T[K] extends U ? Wrap<T[K]> : never`,
        // try to reverse through the true branch (and optionally the false branch).
        // In the context of reverse-mapped inference, the source value corresponds to
        // a property that was produced by either the true or false branch. We try
        // the true branch first (more common pattern: `T[K] extends X ? F<T[K]> : never`),
        // then fall back to the false branch.
        if let Some(TypeData::Conditional(cond_id)) = self.interner.lookup(template) {
            let cond = self.interner.get_conditional(cond_id);
            // Try the true branch first — this is the common case where the false branch
            // is `never` and all real values flow through the true branch.
            if let Some(reversed) = self.reverse_infer_through_template(
                source_value,
                cond.true_type,
                target_placeholder,
            ) {
                return Some(reversed);
            }
            // Try the false branch if it's not `never` (the source might come from the
            // false branch in a conditional like `T[K] extends string ? string : T[K]`).
            if cond.false_type != TypeId::NEVER
                && let Some(reversed) = self.reverse_infer_through_template(
                    source_value,
                    cond.false_type,
                    target_placeholder,
                )
            {
                return Some(reversed);
            }
            return None;
        }

        // For any other template shape, we can't safely reverse.
        None
    }

    /// Check if two types share the same outer structure for constraint matching.
    ///
    /// Used to prefer structural matches over naked type params when constraining
    /// against union targets with multiple placeholder members.
    pub(super) fn types_share_outer_structure_for_constraint(&self, source: TypeId, target: TypeId) -> bool {
        // Unwrap ReadonlyType on both sides — it's a modifier, not a distinct
        // structural kind. This ensures `Array<number>` matches `ReadonlyType(Array<U>)`
        // when constraining against union targets like `U | ReadonlyArray<U>`.
        let unwrap_readonly = |ty: TypeId| -> TypeId {
            if let Some(TypeData::ReadonlyType(inner)) = self.interner.lookup(ty) {
                inner
            } else {
                ty
            }
        };
        let source = unwrap_readonly(source);
        let target = unwrap_readonly(target);

        let (Some(s_key), Some(t_key)) =
            (self.interner.lookup(source), self.interner.lookup(target))
        else {
            return false;
        };
        match (s_key, t_key) {
            (TypeData::Application(s_app_id), TypeData::Application(t_app_id)) => {
                let s_app = self.interner.type_application(s_app_id);
                let t_app = self.interner.type_application(t_app_id);
                s_app.base == t_app.base
            }
            (TypeData::Object(_), TypeData::Object(_))
            | (TypeData::ObjectWithIndex(_), TypeData::ObjectWithIndex(_))
            | (TypeData::Callable(_), TypeData::Callable(_))
            | (TypeData::Function(_), TypeData::Function(_))
            | (TypeData::Tuple(_), TypeData::Tuple(_))
            | (TypeData::Array(_), TypeData::Array(_)) => true,
            _ => false,
        }
    }

    /// Filter target union members by discriminant properties.
    ///
    /// When the source is an object with properties whose types are unit/literal
    /// types (e.g., `kind: 'b'`), check each target member for corresponding
    /// properties with literal types. Only keep members whose discriminant values
    /// match the source's discriminant values. If no discriminant is found or
    /// filtering eliminates all members, return the original list.
    pub(super) fn filter_by_discriminant(&self, source: TypeId, targets: &[TypeId]) -> Vec<TypeId> {
        // Get source object properties
        let source_shape_id = match self.interner.lookup(source) {
            Some(TypeData::Object(id) | TypeData::ObjectWithIndex(id)) => id,
            _ => return targets.to_vec(),
        };
        let source_obj = self.interner.object_shape(source_shape_id);

        // Find discriminant properties in the source: properties with literal types.
        // Store (property_name_atom_raw, literal_type_id) pairs.
        let mut discriminants: Vec<(tsz_common::interner::Atom, TypeId)> = Vec::new();
        for prop in &source_obj.properties {
            if let Some(TypeData::Literal(_)) = self.interner.lookup(prop.type_id) {
                discriminants.push((prop.name, prop.type_id));
            }
        }

        if discriminants.is_empty() {
            return targets.to_vec();
        }

        // Filter targets: keep members whose discriminant properties match
        let filtered: Vec<TypeId> = targets
            .iter()
            .filter(|&&target_member| {
                let target_shape_id = match self.interner.lookup(target_member) {
                    Some(TypeData::Object(id) | TypeData::ObjectWithIndex(id)) => id,
                    _ => return true, // Non-object targets pass through
                };
                let target_obj = self.interner.object_shape(target_shape_id);

                // For each source discriminant, check if the target has a matching
                // property with a specific literal type
                for &(disc_name, disc_type) in &discriminants {
                    if let Some(target_prop) =
                        target_obj.properties.iter().find(|p| p.name == disc_name)
                    {
                        // Target has this property - check if it has a specific literal
                        // type that differs from the source's literal
                        if let Some(TypeData::Literal(_)) =
                            self.interner.lookup(target_prop.type_id)
                            && target_prop.type_id != disc_type
                        {
                            return false; // Discriminant mismatch
                        }
                        // If target property is a type parameter (contains placeholder),
                        // it's not a discriminant in the target - skip this property
                    }
                }
                true
            })
            .copied()
            .collect();

        // Only use filtered result if it's non-empty
        if filtered.is_empty() {
            targets.to_vec()
        } else {
            filtered
        }
    }

}
