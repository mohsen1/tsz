impl<'a> InferenceContext<'a> {
    // =========================================================================
    // Best Common Type
    // =========================================================================

    fn tuple_subtype_of(&self, source: &[TupleElement], target: &[TupleElement]) -> bool {
        let source_required = crate::utils::required_element_count(source);
        let target_required = crate::utils::required_element_count(target);

        if source_required < target_required {
            return false;
        }

        for (i, t_elem) in target.iter().enumerate() {
            if t_elem.rest {
                let expansion = self.expand_tuple_rest(t_elem.type_id);
                let outer_tail = &target[i + 1..];
                // Combined suffix = expansion.tail + outer_tail
                let combined_suffix: Vec<_> = expansion
                    .tail
                    .iter()
                    .chain(outer_tail.iter())
                    .cloned()
                    .collect();

                // Match combined suffix from the end
                let mut source_end = source.len();
                for tail_elem in combined_suffix.iter().rev() {
                    if source_end <= i {
                        if !tail_elem.optional {
                            return false;
                        }
                        break;
                    }
                    let s_elem = &source[source_end - 1];
                    if s_elem.rest {
                        if !tail_elem.optional {
                            return false;
                        }
                        break;
                    }
                    if !self.is_subtype(s_elem.type_id, tail_elem.type_id) {
                        if tail_elem.optional {
                            break;
                        }
                        return false;
                    }
                    source_end -= 1;
                }

                let mut source_iter = source.iter().take(source_end).skip(i);

                for t_fixed in &expansion.fixed {
                    match source_iter.next() {
                        Some(s_elem) => {
                            if s_elem.rest {
                                return false;
                            }
                            if !self.is_subtype(s_elem.type_id, t_fixed.type_id) {
                                return false;
                            }
                        }
                        None => {
                            if !t_fixed.optional {
                                return false;
                            }
                        }
                    }
                }

                if let Some(variadic) = expansion.variadic {
                    let variadic_array = self.interner.array(variadic);
                    for s_elem in source_iter {
                        if s_elem.rest {
                            if !self.is_subtype(s_elem.type_id, variadic_array) {
                                return false;
                            }
                        } else if !self.is_subtype(s_elem.type_id, variadic) {
                            return false;
                        }
                    }
                    return true;
                }

                if source_iter.next().is_some() {
                    return false;
                }
                return true;
            }

            if let Some(s_elem) = source.get(i) {
                if s_elem.rest {
                    return false;
                }
                if !self.is_subtype(s_elem.type_id, t_elem.type_id) {
                    return false;
                }
            } else if !t_elem.optional {
                return false;
            }
        }

        if source.len() > target.len() {
            return false;
        }

        if source.iter().any(|elem| elem.rest) {
            return false;
        }

        true
    }

    fn expand_tuple_rest(&self, type_id: TypeId) -> TupleRestExpansion {
        utils::expand_tuple_rest(self.interner, type_id)
    }
}
