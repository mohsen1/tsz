//! Exact finite property-key collection for mapped types and `keyof`.
//!
//! This query is intentionally key-only. It must not inspect mapped value
//! templates: callers use it to decide whether a property set is finite and
//! exact before relation or diagnostic work.

use super::accessors::get_object_shape;
use super::content_predicates::contains_type_parameters_db;
use super::signatures_and_advanced::prune_impossible_object_union_members;
use crate::construction::TypeDatabase;
use crate::evaluation::evaluate::TypeEvaluator;
use crate::evaluation::evaluate_rules::infer_pattern::InferPatternVisited;
use crate::relations::subtype::SubtypeChecker;
use crate::type_queries::traversal::collect_property_name_atoms_for_diagnostics;
use crate::types::{LiteralValue, MappedTypeId, TypeData, TypeId};
use rustc_hash::FxHashSet;
use std::cell::RefCell;
use tsz_common::Atom;

#[derive(Default)]
struct ExactKeyScratch {
    visited: FxHashSet<TypeId>,
    rollback_trail: Vec<TypeId>,
    active_mapped: FxHashSet<MappedTypeId>,
}

impl ExactKeyScratch {
    fn clear(&mut self) {
        self.visited.clear();
        self.rollback_trail.clear();
        self.active_mapped.clear();
    }

    fn retained_capacity(&self) -> usize {
        self.visited
            .capacity()
            .saturating_add(self.rollback_trail.capacity())
            .saturating_add(self.active_mapped.capacity())
    }
}

// Reusable scratch for the recursive exact-key DFS. The ordinary exact-key
// path touches only `visited`; mapped-source checkpoints lazily write the
// rollback trail, and all buffers retain capacity across queries.
thread_local! {
    static EXACT_KEY_SCRATCH_POOL: RefCell<Option<ExactKeyScratch>> =
        const { RefCell::new(None) };
}

const MAX_EXACT_KEY_TRAVERSAL_STEPS: usize = 65_536;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ExactLiteralPropertyKey {
    pub name: Atom,
    pub is_symbol_named: bool,
    pub is_string_named: bool,
}

struct ExactKeyTraversal<'a> {
    scratch: &'a mut ExactKeyScratch,
    steps: usize,
    checkpoint_depth: usize,
}

impl<'a> ExactKeyTraversal<'a> {
    const fn new(scratch: &'a mut ExactKeyScratch) -> Self {
        Self {
            scratch,
            steps: 0,
            checkpoint_depth: 0,
        }
    }

    const fn spend_step(&mut self) -> Option<()> {
        if self.steps >= MAX_EXACT_KEY_TRAVERSAL_STEPS {
            return None;
        }
        self.steps += 1;
        Some(())
    }

    fn enter_type(&mut self, type_id: TypeId) -> Option<bool> {
        self.spend_step()?;
        if self.scratch.visited.insert(type_id) {
            if self.checkpoint_depth != 0 {
                self.scratch.rollback_trail.push(type_id);
            }
            Some(true)
        } else {
            Some(false)
        }
    }

    const fn checkpoint(&mut self) -> usize {
        self.checkpoint_depth += 1;
        self.scratch.rollback_trail.len()
    }

    fn rollback(&mut self, checkpoint: usize) {
        debug_assert!(self.checkpoint_depth != 0);
        while self.scratch.rollback_trail.len() > checkpoint {
            let type_id = self
                .scratch
                .rollback_trail
                .pop()
                .expect("trail length checked");
            self.scratch.visited.remove(&type_id);
        }
        self.checkpoint_depth -= 1;
    }

    fn enter_mapped(&mut self, mapped_id: MappedTypeId) -> bool {
        self.scratch.active_mapped.insert(mapped_id)
    }

    fn leave_mapped(&mut self, mapped_id: MappedTypeId) {
        self.scratch.active_mapped.remove(&mapped_id);
    }
}

#[inline]
fn with_exact_key_scratch<R>(f: impl FnOnce(&mut ExactKeyScratch) -> R) -> R {
    let mut scratch = EXACT_KEY_SCRATCH_POOL
        .with(|pool| pool.borrow_mut().take())
        .unwrap_or_default();
    scratch.clear();
    let result = f(&mut scratch);
    debug_assert!(scratch.rollback_trail.is_empty());
    debug_assert!(scratch.active_mapped.is_empty());
    EXACT_KEY_SCRATCH_POOL.with(|pool| {
        let mut slot = pool.borrow_mut();
        let keep = match &*slot {
            None => true,
            Some(existing) => scratch.retained_capacity() >= existing.retained_capacity(),
        };
        if keep {
            *slot = Some(scratch);
        }
    });
    result
}

fn collect_exact_literal_property_keys_with_symbol_info_inner(
    db: &dyn TypeDatabase,
    type_id: TypeId,
    keys: &mut FxHashSet<ExactLiteralPropertyKey>,
    traversal: &mut ExactKeyTraversal<'_>,
) -> Option<()> {
    if !traversal.enter_type(type_id)? {
        return Some(());
    }

    // Preserve structural property-key metadata before resolver-less
    // evaluation can widen a canonical well-known symbol to plain `symbol`.
    if let Some(TypeData::KeyOf(operand)) = db.lookup(type_id) {
        return collect_exact_literal_property_keys_from_keyof_operand_with_symbol_info(
            db, operand, keys, traversal,
        );
    }

    let evaluated = crate::evaluation::evaluate::evaluate_type(db, type_id);
    if evaluated != type_id {
        return collect_exact_literal_property_keys_with_symbol_info_inner(
            db, evaluated, keys, traversal,
        );
    }

    match db.lookup(type_id) {
        Some(TypeData::Literal(LiteralValue::String(atom))) => {
            keys.insert(ExactLiteralPropertyKey {
                name: atom,
                is_symbol_named: false,
                is_string_named: true,
            });
            Some(())
        }
        Some(TypeData::Literal(LiteralValue::Number(number))) => {
            let atom = db.intern_string(&crate::utils::js_number_to_string(number.0));
            keys.insert(ExactLiteralPropertyKey {
                name: atom,
                is_symbol_named: false,
                is_string_named: false,
            });
            Some(())
        }
        Some(TypeData::UniqueSymbol(symbol)) => {
            let atom = db.intern_string(&format!("__unique_{}", symbol.0));
            keys.insert(ExactLiteralPropertyKey {
                name: atom,
                is_symbol_named: true,
                is_string_named: false,
            });
            Some(())
        }
        Some(TypeData::Union(members)) => {
            for &member in db.type_list(members).iter() {
                collect_exact_literal_property_keys_with_symbol_info_inner(
                    db, member, keys, traversal,
                )?;
            }
            Some(())
        }
        Some(TypeData::Intersection(members)) => {
            let mut saw_precise_member = false;
            for &member in db.type_list(members).iter() {
                if collect_exact_literal_property_keys_with_symbol_info_inner(
                    db, member, keys, traversal,
                )
                .is_some()
                {
                    saw_precise_member = true;
                    continue;
                }
                if intersection_member_preserves_literal_keys(db, member) {
                    continue;
                }
                return None;
            }
            saw_precise_member.then_some(())
        }
        Some(TypeData::Enum(_, members)) => {
            collect_exact_literal_property_keys_with_symbol_info_inner(db, members, keys, traversal)
        }
        Some(TypeData::Conditional(cond_id)) => {
            let conditional = db.conditional_type(cond_id);
            let branch = resolve_concrete_conditional_branch(db, &conditional)?;
            collect_exact_literal_property_keys_with_symbol_info_inner(db, branch, keys, traversal)
        }
        Some(TypeData::TypeParameter(info) | TypeData::Infer(info)) => {
            info.constraint.and_then(|constraint| {
                collect_exact_literal_property_keys_with_symbol_info_inner(
                    db, constraint, keys, traversal,
                )
            })
        }
        Some(TypeData::ReadonlyType(inner) | TypeData::NoInfer(inner)) => {
            collect_exact_literal_property_keys_with_symbol_info_inner(db, inner, keys, traversal)
        }
        Some(TypeData::Intrinsic(crate::types::IntrinsicKind::Never)) => Some(()),
        _ => None,
    }
}

pub fn collect_exact_literal_property_keys_with_symbol_info(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<FxHashSet<ExactLiteralPropertyKey>> {
    let mut keys = FxHashSet::default();
    let success = with_exact_key_scratch(|scratch| {
        let mut traversal = ExactKeyTraversal::new(scratch);
        collect_exact_literal_property_keys_with_symbol_info_inner(
            db,
            type_id,
            &mut keys,
            &mut traversal,
        )
    });
    success?;
    Some(keys)
}

pub fn collect_exact_literal_property_keys(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<FxHashSet<Atom>> {
    collect_exact_literal_property_keys_with_symbol_info(db, type_id)
        .map(|keys| keys.into_iter().map(|key| key.name).collect())
}

fn collect_exact_literal_property_keys_from_keyof_operand_with_symbol_info(
    db: &dyn TypeDatabase,
    operand: TypeId,
    keys: &mut FxHashSet<ExactLiteralPropertyKey>,
    traversal: &mut ExactKeyTraversal<'_>,
) -> Option<()> {
    traversal.spend_step()?;
    if let Some(TypeData::Mapped(mapped_id)) = db.lookup(operand) {
        return collect_exact_literal_property_keys_from_mapped(db, mapped_id, keys, traversal);
    }

    // Traverse raw composites member-by-member so an eager evaluation cannot
    // materialize a nested mapped member and erase its exact symbol metadata.
    let evaluated_operand = if matches!(
        db.lookup(operand),
        Some(TypeData::Union(_) | TypeData::Intersection(_))
    ) {
        operand
    } else {
        crate::evaluation::evaluate::evaluate_type(db, operand)
    };
    let operand = if evaluated_operand != operand {
        evaluated_operand
    } else {
        operand
    };

    match db.lookup(operand) {
        Some(TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id)) => {
            let shape = db.object_shape(shape_id);
            if shape.string_index.is_some() || shape.number_index.is_some() {
                return None;
            }
            for property in &shape.properties {
                keys.insert(ExactLiteralPropertyKey {
                    name: property.name,
                    is_symbol_named: property.is_symbol_named,
                    is_string_named: property.is_string_named,
                });
            }
            Some(())
        }
        Some(TypeData::Callable(shape_id)) => {
            let shape = db.callable_shape(shape_id);
            if shape.string_index.is_some() || shape.number_index.is_some() {
                return None;
            }
            for property in &shape.properties {
                keys.insert(ExactLiteralPropertyKey {
                    name: property.name,
                    is_symbol_named: property.is_symbol_named,
                    is_string_named: property.is_string_named,
                });
            }
            Some(())
        }
        Some(TypeData::Union(_)) => {
            let narrowed_operand = prune_impossible_object_union_members(db, operand);
            let members = match db.lookup(narrowed_operand) {
                Some(TypeData::Union(members)) => db.type_list(members).to_vec(),
                _ => {
                    return collect_exact_literal_property_keys_from_keyof_operand_with_symbol_info(
                        db,
                        narrowed_operand,
                        keys,
                        traversal,
                    );
                }
            };

            // `keyof (A | B)` contains only keys common to every surviving
            // branch. Each branch gets an isolated visit scope so a shared
            // nested type visited through A is still traversed through B.
            let mut common_keys: Option<FxHashSet<ExactLiteralPropertyKey>> = None;
            let mut branch_keys = FxHashSet::default();
            for member in members {
                branch_keys.clear();
                let checkpoint = traversal.checkpoint();
                let branch_result =
                    collect_exact_literal_property_keys_from_keyof_operand_with_symbol_info(
                        db,
                        member,
                        &mut branch_keys,
                        traversal,
                    );
                traversal.rollback(checkpoint);
                branch_result?;

                if let Some(common) = &mut common_keys {
                    common.retain(|key| branch_keys.contains(key));
                } else {
                    common_keys = Some(std::mem::take(&mut branch_keys));
                }
            }
            keys.extend(common_keys.unwrap_or_default());
            Some(())
        }
        Some(TypeData::Intersection(members)) => {
            let members = db.type_list(members);
            let mut saw_precise_member = false;
            for (member_idx, &member) in members.iter().enumerate() {
                let narrowed_member = if matches!(db.lookup(member), Some(TypeData::Mapped(_))) {
                    member
                } else {
                    narrow_keyof_intersection_member_by_literal_discriminants(
                        db, member, &members, member_idx,
                    )
                };
                if collect_exact_literal_property_keys_from_keyof_operand_with_symbol_info(
                    db,
                    narrowed_member,
                    keys,
                    traversal,
                )
                .is_some()
                {
                    saw_precise_member = true;
                    continue;
                }
                if intersection_member_preserves_literal_keys(db, narrowed_member) {
                    continue;
                }
                return None;
            }
            saw_precise_member.then_some(())
        }
        Some(TypeData::TypeParameter(info) | TypeData::Infer(info)) => {
            info.constraint.and_then(|constraint| {
                collect_exact_literal_property_keys_with_symbol_info_inner(
                    db, constraint, keys, traversal,
                )
            })
        }
        Some(TypeData::Mapped(mapped_id)) => {
            collect_exact_literal_property_keys_from_mapped(db, mapped_id, keys, traversal)
        }
        Some(TypeData::ReadonlyType(inner) | TypeData::NoInfer(inner)) => {
            collect_exact_literal_property_keys_from_keyof_operand_with_symbol_info(
                db, inner, keys, traversal,
            )
        }
        _ => {
            let atoms = collect_property_name_atoms_for_diagnostics(db, operand, 8);
            if atoms.is_empty() {
                None
            } else {
                for atom in atoms {
                    keys.insert(ExactLiteralPropertyKey {
                        name: atom,
                        is_symbol_named: false,
                        is_string_named: true,
                    });
                }
                Some(())
            }
        }
    }
}

fn collect_exact_literal_property_keys_from_mapped(
    db: &dyn TypeDatabase,
    mapped_id: MappedTypeId,
    keys: &mut FxHashSet<ExactLiteralPropertyKey>,
    traversal: &mut ExactKeyTraversal<'_>,
) -> Option<()> {
    if !traversal.enter_mapped(mapped_id) {
        return None;
    }

    let result = (|| {
        let mapped = db.mapped_type(mapped_id);
        let checkpoint = traversal.checkpoint();
        let mut source_keys = FxHashSet::default();
        let source_result = collect_exact_literal_property_keys_with_symbol_info_inner(
            db,
            mapped.constraint,
            &mut source_keys,
            traversal,
        );
        // Source-key visits wrote into a temporary output set. Roll them back
        // before collecting remapped keys into the caller's output so identity
        // remaps and constant-union remaps are not skipped as duplicates.
        traversal.rollback(checkpoint);
        source_result?;

        // No `as` clause means the mapped type preserves its exact source-key
        // identity. Keep canonical well-known-symbol atoms directly: this
        // resolver-less query cannot safely reconstruct their `SymbolRef`.
        if mapped.name_type.is_none() {
            keys.extend(source_keys);
            return Some(());
        }

        for source_key in source_keys {
            let remapped = crate::type_queries::mapped::remap_exact_mapped_property_key(
                db, &mapped, source_key,
            )?;
            collect_exact_literal_property_keys_with_symbol_info_inner(
                db, remapped, keys, traversal,
            )?;
        }
        Some(())
    })();

    traversal.leave_mapped(mapped_id);
    result
}

pub(crate) fn narrow_keyof_intersection_member_by_literal_discriminants(
    db: &dyn TypeDatabase,
    member: TypeId,
    intersection_members: &[TypeId],
    member_idx: usize,
) -> TypeId {
    let evaluated_member = crate::evaluation::evaluate::evaluate_type(db, member);
    let member = if evaluated_member != member {
        evaluated_member
    } else {
        member
    };

    let Some(TypeData::Union(list_id)) = db.lookup(member) else {
        return member;
    };

    let mut discriminants = Vec::new();
    for (other_idx, &other_member) in intersection_members.iter().enumerate() {
        if other_idx == member_idx {
            continue;
        }
        let evaluated_other = crate::evaluation::evaluate::evaluate_type(db, other_member);
        let other_member = if evaluated_other != other_member {
            evaluated_other
        } else {
            other_member
        };
        let Some(shape) = get_object_shape(db, other_member) else {
            continue;
        };
        for property in &shape.properties {
            if crate::type_queries::is_unit_type(db, property.type_id) {
                discriminants.push((property.name, property.type_id));
            }
        }
    }

    if discriminants.is_empty() {
        return member;
    }

    let union_members = db.type_list(list_id);
    let retained: Vec<_> = union_members
        .iter()
        .copied()
        .filter(|&branch| {
            let Some(shape) = get_object_shape(db, branch) else {
                return true;
            };

            discriminants.iter().all(|&(name, discriminant)| {
                let Some(property) = shape
                    .properties
                    .iter()
                    .find(|property| property.name == name)
                else {
                    return true;
                };
                !crate::type_queries::is_unit_type(db, property.type_id)
                    || crate::relations::subtype::is_subtype_of(db, discriminant, property.type_id)
            })
        })
        .collect();

    if retained.is_empty() || retained.len() == union_members.len() {
        member
    } else if retained.len() == 1 {
        retained[0]
    } else {
        db.union_preserve_members(retained)
    }
}

fn intersection_member_preserves_literal_keys(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    matches!(
        db.lookup(type_id),
        Some(
            TypeData::Intrinsic(crate::types::IntrinsicKind::String)
                | TypeData::Intrinsic(crate::types::IntrinsicKind::Number)
        )
    )
}

fn resolve_concrete_conditional_branch(
    db: &dyn TypeDatabase,
    conditional: &crate::types::ConditionalType,
) -> Option<TypeId> {
    let mut evaluator = TypeEvaluator::new(db);
    resolve_concrete_conditional_result(db, &mut evaluator, conditional, conditional.check_type)
}

fn resolve_concrete_conditional_result(
    db: &dyn TypeDatabase,
    evaluator: &mut TypeEvaluator<'_>,
    conditional: &crate::types::ConditionalType,
    check_input: TypeId,
) -> Option<TypeId> {
    let check_type = evaluator.evaluate(check_input);
    let extends_type = evaluator.evaluate(conditional.extends_type);

    if let Some(TypeData::Union(members)) = db.lookup(check_type) {
        let members = db.type_list(members);
        let mut results = Vec::new();
        for &member in members.iter() {
            results.push(resolve_concrete_conditional_result(
                db,
                evaluator,
                conditional,
                member,
            )?);
        }
        return Some(crate::utils::union_or_single(db, results));
    }

    if contains_type_parameters_db(db, check_type)
        || check_type.is_any_unknown_or_error()
        || extends_type.is_any_unknown_or_error()
    {
        return None;
    }

    if let Some(TypeData::StringIntrinsic { kind, type_arg }) = db.lookup(extends_type)
        && type_arg == TypeId::STRING
    {
        let transformed = evaluator.evaluate(db.string_intrinsic(kind, check_type));
        return Some(if transformed == check_type {
            conditional.true_type
        } else {
            conditional.false_type
        });
    }

    if contains_type_parameters_db(db, extends_type)
        && !contains_type_parameters_db(db, conditional.check_type)
    {
        if evaluator.type_contains_infer(conditional.extends_type) {
            let mut bindings = rustc_hash::FxHashMap::default();
            let mut visited = InferPatternVisited::default();
            let mut checker = SubtypeChecker::new(db);
            if evaluator.match_infer_pattern(
                check_type,
                conditional.extends_type,
                &mut bindings,
                &mut visited,
                &mut checker,
            ) {
                let substituted = evaluator.substitute_infer(conditional.true_type, &bindings);
                return Some(evaluator.evaluate(substituted));
            }
            return Some(conditional.false_type);
        }
        return None;
    }

    Some(
        if crate::relations::subtype::is_subtype_of(db, check_type, extends_type) {
            conditional.true_type
        } else {
            conditional.false_type
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::construction::TypeInterner;
    use crate::types::{ConditionalType, MappedType, PropertyInfo, TypeParamInfo};

    fn deferred_filter_map(
        interner: &TypeInterner,
        binder: &str,
        constraint: TypeId,
        retained: TypeId,
        recursive_template: TypeId,
    ) -> TypeId {
        let key_info = TypeParamInfo {
            name: interner.intern_string(binder),
            constraint: Some(constraint),
            default: None,
            is_const: false,
            origin: crate::TypeParamOrigin::User,
        };
        let key_param = interner.type_param(key_info);
        let filtered_remap = interner.conditional(ConditionalType {
            check_type: key_param,
            extends_type: retained,
            true_type: key_param,
            false_type: TypeId::NEVER,
            is_distributive: true,
        });
        let free_fallback = interner.type_param(TypeParamInfo {
            name: interner.intern_string(&format!("{binder}Fallback")),
            constraint: None,
            default: None,
            is_const: false,
            origin: crate::TypeParamOrigin::User,
        });
        let deferred_remap = interner.conditional(ConditionalType {
            check_type: key_param,
            extends_type: key_param,
            true_type: filtered_remap,
            false_type: free_fallback,
            is_distributive: false,
        });
        interner.mapped(MappedType {
            type_param: key_info,
            constraint,
            name_type: Some(deferred_remap),
            template: recursive_template,
            readonly_modifier: None,
            optional_modifier: None,
        })
    }

    fn identity_map(
        interner: &TypeInterner,
        binder: &str,
        constraint: TypeId,
        template: TypeId,
    ) -> TypeId {
        let type_param = TypeParamInfo {
            name: interner.intern_string(binder),
            constraint: Some(constraint),
            default: None,
            is_const: false,
            origin: crate::TypeParamOrigin::User,
        };
        interner.mapped(MappedType {
            type_param,
            constraint,
            name_type: None,
            template,
            readonly_modifier: None,
            optional_modifier: None,
        })
    }

    fn deferred_identity_remap(
        interner: &TypeInterner,
        binder: &str,
        constraint: TypeId,
        template: TypeId,
    ) -> TypeId {
        let type_param = TypeParamInfo {
            name: interner.intern_string(binder),
            constraint: Some(constraint),
            default: None,
            is_const: false,
            origin: crate::TypeParamOrigin::User,
        };
        let key = interner.type_param(type_param);
        let fallback = interner.type_param(TypeParamInfo {
            name: interner.intern_string(&format!("{binder}Fallback")),
            constraint: None,
            default: None,
            is_const: false,
            origin: crate::TypeParamOrigin::User,
        });
        let remap = interner.conditional(ConditionalType {
            check_type: key,
            extends_type: key,
            true_type: key,
            false_type: fallback,
            is_distributive: false,
        });
        interner.mapped(MappedType {
            type_param,
            constraint,
            name_type: Some(remap),
            template,
            readonly_modifier: None,
            optional_modifier: None,
        })
    }

    #[test]
    fn keyof_deferred_remapped_intersection_collects_only_output_keys() {
        let interner = TypeInterner::new();
        let retained = interner.literal_string("retainedKey");
        let payload = interner.literal_string("payloadKey");
        let filtered = interner.literal_string("filteredKey");
        let numeric = interner.literal_number(7.0);
        let numeric_string = interner.literal_string("7");
        let symbol = interner.unique_symbol(crate::types::SymbolRef(7001));
        let constraint = interner.union(vec![
            retained,
            payload,
            filtered,
            numeric,
            numeric_string,
            symbol,
        ]);
        let recursive_template = interner.object(vec![PropertyInfo::new(
            interner.intern_string("next"),
            interner.recursive(0),
        )]);
        let first =
            deferred_filter_map(&interner, "Entry", constraint, retained, recursive_template);
        let second =
            deferred_filter_map(&interner, "Slot", constraint, payload, recursive_template);
        let third =
            deferred_filter_map(&interner, "Index", constraint, numeric, recursive_template);
        let fourth =
            deferred_filter_map(&interner, "Marker", constraint, symbol, recursive_template);
        let fifth = deferred_filter_map(
            &interner,
            "QuotedIndex",
            constraint,
            numeric_string,
            recursive_template,
        );
        for mapped in [first, second, third, fourth, fifth] {
            let evaluated = crate::evaluation::evaluate::evaluate_type(&interner, mapped);
            assert!(
                matches!(interner.lookup(evaluated), Some(TypeData::Mapped(_))),
                "the witness must reach the deferred mapped-operand path"
            );
        }
        let operand = interner.intersection(vec![first, second, third, fourth, fifth]);

        let mut keys = FxHashSet::default();
        let (success, steps) = with_exact_key_scratch(|scratch| {
            let mut traversal = ExactKeyTraversal::new(scratch);
            let success = collect_exact_literal_property_keys_from_keyof_operand_with_symbol_info(
                &interner,
                operand,
                &mut keys,
                &mut traversal,
            );
            (success, traversal.steps)
        });
        success.expect("the remapped intersection has a finite exact key set");
        assert!(
            steps < 256,
            "finite key-only traversal should stay linear in this small graph; used {steps} steps"
        );
        let observed: Vec<_> = keys
            .into_iter()
            .map(|key| {
                (
                    interner.resolve_atom_ref(key.name).to_string(),
                    key.is_symbol_named,
                    key.is_string_named,
                )
            })
            .collect();
        assert_eq!(observed.len(), 5);
        assert!(
            observed
                .iter()
                .any(|key| key == &("retainedKey".into(), false, true))
        );
        assert!(
            observed
                .iter()
                .any(|key| key == &("payloadKey".into(), false, true))
        );
        assert!(
            observed
                .iter()
                .any(|key| key == &("7".into(), false, false))
        );
        assert!(observed.iter().any(|key| key == &("7".into(), false, true)));
        assert!(observed.iter().any(|key| key.1));
        assert!(!observed.iter().any(|key| key.0 == "filteredKey"));
    }

    #[test]
    fn keyof_identity_mapped_intersection_preserves_well_known_symbol_atom() {
        let interner = TypeInterner::new();
        let iterator_name = interner.intern_string("[Symbol.iterator]");
        let mut iterator_property = PropertyInfo::new(iterator_name, TypeId::STRING);
        iterator_property.is_symbol_named = true;
        let source = interner.object(vec![iterator_property]);
        let source_keys = interner.keyof(source);
        let first = identity_map(&interner, "Entry", source_keys, TypeId::STRING);
        let second = identity_map(&interner, "Slot", source_keys, TypeId::NUMBER);
        let operand = interner.intersect_types_raw(vec![first, second]);

        for mapped in [first, second] {
            let mut mapped_keys = FxHashSet::default();
            let mapped_success = with_exact_key_scratch(|scratch| {
                let mut traversal = ExactKeyTraversal::new(scratch);
                collect_exact_literal_property_keys_from_keyof_operand_with_symbol_info(
                    &interner,
                    mapped,
                    &mut mapped_keys,
                    &mut traversal,
                )
            });
            mapped_success.expect("each raw identity mapped operand has exact keys");
            assert_eq!(mapped_keys.len(), 1);
        }

        let mut keys = FxHashSet::default();
        let success = with_exact_key_scratch(|scratch| {
            let mut traversal = ExactKeyTraversal::new(scratch);
            collect_exact_literal_property_keys_from_keyof_operand_with_symbol_info(
                &interner,
                operand,
                &mut keys,
                &mut traversal,
            )
        });

        success.expect("raw identity mapped operands have an exact key set");
        assert_eq!(
            keys,
            FxHashSet::from_iter([ExactLiteralPropertyKey {
                name: iterator_name,
                is_symbol_named: true,
                is_string_named: false,
            }])
        );
    }

    #[test]
    fn keyof_union_inside_deferred_mapped_intersection_keeps_only_common_keys() {
        let interner = TypeInterner::new();
        let common = interner.intern_string("common");
        let branch_a = interner.object(vec![
            PropertyInfo::new(common, TypeId::STRING),
            PropertyInfo::new(interner.intern_string("a"), TypeId::STRING),
        ]);
        let branch_b = interner.object(vec![
            PropertyInfo::new(common, TypeId::STRING),
            PropertyInfo::new(interner.intern_string("b"), TypeId::STRING),
        ]);
        let union = interner.union_preserve_members(vec![branch_a, branch_b]);
        let recursive_template = interner.recursive(0);
        let common_map = deferred_identity_remap(
            &interner,
            "BranchKey",
            interner.keyof(union),
            recursive_template,
        );
        let marker_key = interner.literal_string("marker");
        let marker_map =
            deferred_identity_remap(&interner, "MarkerKey", marker_key, recursive_template);
        let operand = interner.intersection(vec![common_map, marker_map]);

        assert!(matches!(
            interner.lookup(common_map),
            Some(TypeData::Mapped(_))
        ));
        assert!(matches!(
            interner.lookup(marker_map),
            Some(TypeData::Mapped(_))
        ));
        let mut keys = FxHashSet::default();
        let success = with_exact_key_scratch(|scratch| {
            let mut traversal = ExactKeyTraversal::new(scratch);
            collect_exact_literal_property_keys_from_keyof_operand_with_symbol_info(
                &interner,
                operand,
                &mut keys,
                &mut traversal,
            )
        });
        success.expect("the deferred mapped intersection has finite exact keys");
        let names: FxHashSet<_> = keys.into_iter().map(|key| key.name).collect();

        assert_eq!(
            names,
            FxHashSet::from_iter([common, interner.intern_string("marker")])
        );
        assert!(!names.contains(&interner.intern_string("a")));
        assert!(!names.contains(&interner.intern_string("b")));
    }
}
