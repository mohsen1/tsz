//! Tuple-failure explanation for subtype checking.
//!
//! Split out of `explain.rs` to keep that module under the source-shard cap:
//! it computes the structured `SubtypeFailureReason` for a failed tuple-to-tuple
//! relation (arity mismatch vs. a specific element-type mismatch, including
//! variadic/rest expansion). `explain_tuple_failure` is the entry point used by
//! `explain_failure_inner`.

use crate::def::resolver::TypeResolver;
use crate::diagnostics::SubtypeFailureReason;
use crate::relations::subtype::SubtypeChecker;
use crate::types::{TupleElement, TypeId};
use crate::visitor::is_type_parameter;

impl<'a, R: TypeResolver> SubtypeChecker<'a, R> {
    /// Build a `TupleElementTypeMismatch` for a failing element pair, recursing
    /// into the element failure so the rendered chain carries the inner reason
    /// (matching tsc, which walks a tuple element exactly like a numerically
    /// keyed object property).
    fn tuple_element_type_mismatch(
        &mut self,
        index: usize,
        target_index: usize,
        source_element: TypeId,
        target_element: TypeId,
        multi_element: bool,
    ) -> SubtypeFailureReason {
        let nested_reason = self
            .explain_failure(source_element, target_element)
            .map(Box::new);
        SubtypeFailureReason::TupleElementTypeMismatch {
            index,
            target_index,
            source_element,
            target_element,
            nested_reason,
            multi_element,
        }
    }

    /// Explain why a tuple type assignment failed.
    pub(super) fn explain_tuple_failure(
        &mut self,
        source: &[TupleElement],
        target: &[TupleElement],
    ) -> Option<SubtypeFailureReason> {
        // tsc gates a tuple-to-tuple relation on an arity check *before* it
        // compares individual elements (`tupleTypesRelated` in `checker.ts`).
        // `classify_tuple_arity` mirrors that gate exactly, selecting the
        // correct `TS2618`–`TS2621` family from each side's *arity* (slot count)
        // and *minimum length* (required-element count).
        //
        // This classifier runs for *every* tuple pair, closed or variadic. A
        // closed target that carries optional elements still has
        // `minLength < arity`, so the "requires"/"allows only" counts tsc
        // reports are the minimum length, not the raw slot count. The earlier
        // implementation gated the classifier behind "a rest element is
        // present" and fell back to a coarse `TupleElementMismatch` that used
        // `target.len()` for the required count — over-reporting whenever a
        // closed target held optional (or nested) elements (e.g.
        // `[number, string?]` reported "requires 2" instead of tsc's
        // "requires 1"). Routing closed tuples through the same gate restores
        // parity across the whole `TS2618`–`TS2621` family.
        if let Some(arity) = crate::utils::classify_tuple_arity(source, target) {
            return Some(SubtypeFailureReason::TupleArityMismatch(arity));
        }

        for (i, t_elem) in target.iter().enumerate() {
            if t_elem.rest {
                let expansion = self.expand_tuple_rest(t_elem.type_id);
                let outer_tail = &target[i + 1..];
                // Combined suffix = expansion.tail + outer_tail, each paired
                // with its TARGET position: elements expanded out of the rest
                // slot report the slot's own position `i` (tsc numbers the
                // target as written), while outer-tail elements keep their
                // written index after the rest slot.
                let combined_suffix: Vec<(usize, TupleElement)> = expansion
                    .tail
                    .iter()
                    .map(|element| (i, *element))
                    .chain(
                        outer_tail
                            .iter()
                            .enumerate()
                            .map(|(k, element)| (i + 1 + k, *element)),
                    )
                    .collect();

                let mut source_end = source.len();
                for (tail_target_pos, tail_elem) in combined_suffix.iter().rev() {
                    if source_end <= i {
                        if !tail_elem.optional {
                            return Some(SubtypeFailureReason::TupleElementMismatch {
                                source_count: source.len(),
                                target_count: target.len(),
                            });
                        }
                        break;
                    }
                    // Type parameter rest spread requires matching rest in source
                    if tail_elem.rest && is_type_parameter(self.interner, tail_elem.type_id) {
                        let s_elem = &source[source_end - 1];
                        if s_elem.rest {
                            let tp_array = self.interner.array(tail_elem.type_id);
                            if !self.check_subtype(s_elem.type_id, tp_array).is_true() {
                                return Some(self.tuple_element_type_mismatch(
                                    source_end - 1,
                                    *tail_target_pos,
                                    s_elem.type_id,
                                    tail_elem.type_id,
                                    // Rest/variadic tuples are multi-position;
                                    // keep the positional disambiguation line.
                                    true,
                                ));
                            }
                            source_end -= 1;
                            continue;
                        }
                        return Some(SubtypeFailureReason::TypeMismatch {
                            source_type: source.first().map(|e| e.type_id).unwrap_or(TypeId::NEVER),
                            target_type: tail_elem.type_id,
                        });
                    }
                    let s_elem = &source[source_end - 1];
                    if s_elem.rest {
                        if !tail_elem.optional {
                            return Some(SubtypeFailureReason::TupleElementMismatch {
                                source_count: source.len(),
                                target_count: target.len(),
                            });
                        }
                        break;
                    }
                    let assignable = self
                        .check_subtype(s_elem.type_id, tail_elem.type_id)
                        .is_true();
                    if tail_elem.optional && !assignable {
                        break;
                    }
                    if !assignable {
                        return Some(self.tuple_element_type_mismatch(
                            source_end - 1,
                            *tail_target_pos,
                            s_elem.type_id,
                            tail_elem.type_id,
                            true,
                        ));
                    }
                    source_end -= 1;
                }

                let mut source_iter = source.iter().enumerate().take(source_end).skip(i);

                for t_fixed in &expansion.fixed {
                    match source_iter.next() {
                        Some((j, s_elem)) => {
                            if s_elem.rest {
                                return Some(SubtypeFailureReason::TupleElementMismatch {
                                    source_count: source.len(),
                                    target_count: target.len(),
                                });
                            }
                            if !self
                                .check_subtype(s_elem.type_id, t_fixed.type_id)
                                .is_true()
                            {
                                return Some(self.tuple_element_type_mismatch(
                                    j,
                                    i,
                                    s_elem.type_id,
                                    t_fixed.type_id,
                                    true,
                                ));
                            }
                        }
                        None => {
                            if !t_fixed.optional {
                                return Some(SubtypeFailureReason::TupleElementMismatch {
                                    source_count: source.len(),
                                    target_count: target.len(),
                                });
                            }
                        }
                    }
                }

                if let Some(variadic) = expansion.variadic {
                    let variadic_is_type_param = is_type_parameter(self.interner, variadic);
                    let variadic_array = self.interner.array(variadic);
                    // The source positions aligned to this single target rest slot
                    // span `[variadic_start ..= variadic_end]`: everything after the
                    // expansion's leading fixed elements and before the trailing
                    // suffix that `source_end` already excluded. tsc reports this
                    // full span (even passing positions) against the rest slot
                    // index `i`, so a failure carries the span, not the single
                    // failing index.
                    let variadic_start = i + expansion.fixed.len();
                    let variadic_end = source_end.saturating_sub(1);
                    for (j, s_elem) in source_iter {
                        if s_elem.rest {
                            if !self.check_subtype(s_elem.type_id, variadic_array).is_true() {
                                return Some(self.tuple_element_type_mismatch(
                                    j,
                                    i,
                                    s_elem.type_id,
                                    variadic_array,
                                    true,
                                ));
                            }
                        } else if variadic_is_type_param {
                            return Some(SubtypeFailureReason::TypeMismatch {
                                source_type: s_elem.type_id,
                                target_type: variadic,
                            });
                        } else if !self.check_subtype(s_elem.type_id, variadic).is_true() {
                            return Some(SubtypeFailureReason::TupleVariadicPositionMismatch {
                                source_start: variadic_start,
                                source_end: variadic_end,
                                target_position: i,
                                source_element: s_elem.type_id,
                                target_element: variadic,
                                nested_reason: self
                                    .explain_failure(s_elem.type_id, variadic)
                                    .map(Box::new),
                            });
                        }
                    }
                    return None;
                }

                if source_iter.next().is_some() {
                    return Some(SubtypeFailureReason::TupleElementMismatch {
                        source_count: source.len(),
                        target_count: target.len(),
                    });
                }
                return None;
            }

            if let Some(s_elem) = source.get(i) {
                if s_elem.rest {
                    // Source has rest but target expects fixed element
                    return Some(SubtypeFailureReason::TupleElementMismatch {
                        source_count: source.len(), // Approximate "infinity"
                        target_count: target.len(),
                    });
                }

                // Element-flag mismatch: the target requires a value at this
                // position but the source element is optional, so the source
                // may not provide one. tsc reports this (`TS2623`,
                // `Source_provides_no_match_for_required_element_at_position…`)
                // ahead of any element-*type* comparison, so a closed source
                // whose optional slot also has an incompatible type still
                // surfaces the flag mismatch first (matching `tupleTypesRelated`
                // in `checker.ts`, which relates element flags before types).
                if t_elem.is_required() && s_elem.optional {
                    return Some(SubtypeFailureReason::SourceProvidesNoMatch {
                        position: i,
                        variadic: false,
                    });
                }

                if !self.check_subtype(s_elem.type_id, t_elem.type_id).is_true() {
                    // Drill into the nested failure: if the element mismatch is due to a
                    // missing property (e.g., {} vs {a: string}), return MissingProperty
                    // to produce TS2741 instead of generic TS2322. This matches tsc behavior
                    // for tuple literals where elements have missing properties.
                    // Reuse the single `explain_failure` walk both to detect the
                    // missing-property short-circuit and as the element's nested
                    // reason, avoiding a second recursive type walk.
                    let nested = self.explain_failure(s_elem.type_id, t_elem.type_id);
                    if matches!(
                        nested,
                        Some(
                            SubtypeFailureReason::MissingProperty { .. }
                                | SubtypeFailureReason::MissingProperties { .. }
                        )
                    ) {
                        return nested;
                    }
                    return Some(SubtypeFailureReason::TupleElementTypeMismatch {
                        index: i,
                        target_index: i,
                        source_element: s_elem.type_id,
                        target_element: t_elem.type_id,
                        nested_reason: nested.map(Box::new),
                        // Single-element tuples have no position to disambiguate,
                        // so tsc omits the TS2626 positional line and relates the
                        // element types directly.
                        multi_element: target.len() > 1,
                    });
                }
            } else if !t_elem.optional {
                return Some(SubtypeFailureReason::TupleElementMismatch {
                    source_count: source.len(),
                    target_count: target.len(),
                });
            }
        }

        // Target is closed. Check for extra elements in source.
        if source.len() > target.len() {
            return Some(SubtypeFailureReason::TupleElementMismatch {
                source_count: source.len(),
                target_count: target.len(),
            });
        }

        for s_elem in source {
            if s_elem.rest {
                return Some(SubtypeFailureReason::TupleElementMismatch {
                    source_count: source.len(), // implies open
                    target_count: target.len(),
                });
            }
        }

        None
    }
}
