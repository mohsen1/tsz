//! Spelling suggestion helpers (Levenshtein distance, property/identifier suggestions).

use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    // =========================================================================
    // Property Suggestion Helpers
    // =========================================================================

    /// Find a similar property name on a type for "did you mean?" suggestions (TS2551).
    /// Uses the same algorithm as tsc's `getSpellingSuggestion`.
    pub(super) fn find_similar_property(
        &mut self,
        prop_name: &str,
        type_id: TypeId,
    ) -> Option<String> {
        let evaluated_type = self.evaluate_type_for_assignability(type_id);
        let property_names = self.collect_accessible_type_property_names(evaluated_type);
        if property_names.is_empty() {
            return None;
        }

        let name_len = prop_name.len();
        let maximum_length_difference = if name_len * 34 / 100 > 2 {
            name_len * 34 / 100
        } else {
            2
        };
        let mut best_distance = name_len * 4 / 10 + 1;
        let mut best_candidate: Option<String> = None;

        for candidate in &property_names {
            if candidate == prop_name {
                continue;
            }
            let candidate_len = candidate.len();
            let len_diff = name_len.abs_diff(candidate_len);
            if len_diff > maximum_length_difference {
                continue;
            }
            if name_len < 3 && candidate.to_lowercase() != prop_name.to_lowercase() {
                continue;
            }
            if candidate.to_lowercase() == prop_name.to_lowercase() {
                let distance = 1;
                if distance < best_distance {
                    best_distance = distance;
                    best_candidate = Some(candidate.clone());
                }
                continue;
            }
            let distance = Self::levenshtein_distance(prop_name, candidate);
            if distance < best_distance {
                best_distance = distance;
                best_candidate = Some(candidate.clone());
            }
        }

        best_candidate
    }

    /// Returns true when `type_id` is a class instance type whose declaration extends
    /// a base expression currently typed as `any`.
    ///
    /// In that case TypeScript treats unknown member accesses as `any` and does not
    /// surface typo suggestions (TS2551).
    pub(super) fn class_extends_any_base(&mut self, type_id: TypeId) -> bool {
        use tsz_binder::symbol_flags;
        use tsz_scanner::SyntaxKind;

        let sym_id = self
            .ctx
            .resolve_type_to_symbol_id(type_id)
            .or_else(|| tsz_solver::type_queries::get_type_shape_symbol(self.ctx.types, type_id));
        let Some(sym_id) = sym_id else {
            return false;
        };
        let Some(symbol) = self.ctx.binder.get_symbol(sym_id) else {
            return false;
        };
        if symbol.flags & symbol_flags::CLASS == 0 {
            return false;
        }

        let decl_idx = if !symbol.value_declaration.is_none() {
            symbol.value_declaration
        } else if let Some(&first_decl) = symbol.declarations.first() {
            first_decl
        } else {
            return false;
        };
        let Some(class_decl) = self.ctx.arena.get_class_at(decl_idx) else {
            return false;
        };
        let Some(heritage_clauses) = &class_decl.heritage_clauses else {
            return false;
        };

        for &clause_idx in &heritage_clauses.nodes {
            let Some(clause) = self.ctx.arena.get_heritage_clause_at(clause_idx) else {
                continue;
            };
            if clause.token != SyntaxKind::ExtendsKeyword as u16 {
                continue;
            }
            let Some(&type_idx) = clause.types.nodes.first() else {
                continue;
            };
            let expr_idx =
                if let Some(expr_type_args) = self.ctx.arena.get_expr_type_args_at(type_idx) {
                    expr_type_args.expression
                } else {
                    type_idx
                };
            if self.get_type_of_node(expr_idx) == TypeId::ANY {
                return true;
            }
        }

        false
    }

    fn collect_accessible_type_property_names(&self, type_id: TypeId) -> Vec<String> {
        crate::query_boundaries::diagnostics::collect_accessible_property_names_for_suggestion(
            self.ctx.types,
            type_id,
            5,
        )
        .into_iter()
        .map(|name| self.ctx.types.resolve_atom_ref(name).to_string())
        .collect()
    }

    // =========================================================================
    // Identifier Suggestion Helpers
    // =========================================================================

    fn consider_identifier_suggestion(
        name: &str,
        candidate: &str,
        name_len: usize,
        maximum_length_difference: usize,
        best_distance: &mut f64,
        best_candidate: &mut Option<String>,
    ) {
        if candidate == name {
            return;
        }
        let candidate_len = candidate.len();

        // tsc: skip candidates whose length is too different
        let len_diff = name_len.abs_diff(candidate_len);
        if len_diff > maximum_length_difference {
            return;
        }

        // tsc: for short candidate names (<3), only suggest if differs by case
        if candidate_len < 3 && candidate.to_lowercase() != name.to_lowercase() {
            return;
        }

        if let Some(distance) = Self::levenshtein_with_max(name, candidate, *best_distance - 0.1)
            && distance < *best_distance
        {
            *best_distance = distance;
            *best_candidate = Some(candidate.to_string());
        }
    }

    /// Find the best spelling suggestion for a name, matching tsc's `getSpellingSuggestion`.
    /// Returns `Some(best_name)` if a close-enough match is found.
    ///
    /// `meaning_flags` controls which symbol categories are eligible as candidates.
    /// Pass `symbol_flags::VALUE` for value-position lookups, `symbol_flags::TYPE` for
    /// type-position lookups, or `symbol_flags::VALUE | symbol_flags::TYPE` for both.
    pub(crate) fn find_similar_identifiers(
        &self,
        name: &str,
        idx: NodeIndex,
        meaning_flags: u32,
    ) -> Option<Vec<String>> {
        let visible_names = self.ctx.binder.collect_visible_symbol_names_filtered(
            self.ctx.arena,
            idx,
            meaning_flags,
        );

        let name_len = name.len();
        // tsc: bestDistance = (name.length + 2) * 0.34 rounded down, min 2
        let maximum_length_difference = if name_len * 34 / 100 > 2 {
            name_len * 34 / 100
        } else {
            2
        };
        // tsc: initial bestDistance = floor(name.length * 0.4) + 1
        let mut best_distance = (name_len * 4 / 10 + 1) as f64;
        let mut best_candidate: Option<String> = None;

        for candidate in visible_names {
            Self::consider_identifier_suggestion(
                name,
                &candidate,
                name_len,
                maximum_length_difference,
                &mut best_distance,
                &mut best_candidate,
            );
        }

        // NOTE: We intentionally do NOT fall back to lib globals for spelling
        // suggestions. TSC's fallback is more targeted (only specific well-known
        // globals), while iterating all file_locals from lib.d.ts produces false
        // positives (e.g., Script→WScript, parse→parseFloat).

        best_candidate.map(|c| vec![c])
    }

    fn chars_equal_ignore_case(a: char, b: char) -> bool {
        a.to_lowercase().eq(b.to_lowercase())
    }

    /// Calculate Levenshtein distance between two strings.
    fn levenshtein_distance(a: &str, b: &str) -> usize {
        let a_chars: Vec<char> = a.chars().collect();
        let b_chars: Vec<char> = b.chars().collect();
        let a_len = a_chars.len();
        let b_len = b_chars.len();

        if a_len == 0 {
            return b_len;
        }
        if b_len == 0 {
            return a_len;
        }

        let mut prev = vec![0usize; b_len + 1];
        let mut curr = vec![0usize; b_len + 1];

        for (j, item) in prev.iter_mut().enumerate().take(b_len + 1) {
            *item = j;
        }

        for i in 1..=a_len {
            curr[0] = i;
            for j in 1..=b_len {
                let cost = usize::from(a_chars[i - 1] != b_chars[j - 1]);
                curr[j] = (prev[j] + 1).min(curr[j - 1] + 1).min(prev[j - 1] + cost);
            }
            std::mem::swap(&mut prev, &mut curr);
        }

        prev[b_len]
    }

    /// Levenshtein distance with threshold pruning, matching tsc's behavior.
    /// Case-only substitutions are cheaper than other substitutions.
    fn levenshtein_with_max(s1: &str, s2: &str, max: f64) -> Option<f64> {
        let s1_chars: Vec<char> = s1.chars().collect();
        let s2_chars: Vec<char> = s2.chars().collect();

        if s1_chars.is_empty() {
            let dist = s2_chars.len() as f64;
            return (dist <= max).then_some(dist);
        }
        if s2_chars.is_empty() {
            let dist = s1_chars.len() as f64;
            return (dist <= max).then_some(dist);
        }

        let mut previous = vec![0.0; s2_chars.len() + 1];
        let mut current = vec![0.0; s2_chars.len() + 1];
        let big = max + 0.01;

        for (i, value) in previous.iter_mut().enumerate() {
            *value = i as f64;
        }

        for i in 1..=s1_chars.len() {
            let c1 = s1_chars[i - 1];
            let min_j = if (i as f64) > max {
                ((i as f64) - max).ceil() as usize
            } else {
                1
            };
            let max_j = if (s2_chars.len() as f64) > (max + i as f64) {
                (max + i as f64).floor() as usize
            } else {
                s2_chars.len()
            };

            current[0] = i as f64;
            let mut col_min = i as f64;

            for value in current.iter_mut().take(min_j).skip(1) {
                *value = big;
            }

            for j in min_j..=max_j {
                let substitution_distance = if Self::chars_equal_ignore_case(c1, s2_chars[j - 1]) {
                    previous[j - 1] + 0.1
                } else {
                    previous[j - 1] + 2.0
                };
                let dist = if c1 == s2_chars[j - 1] {
                    previous[j - 1]
                } else {
                    (previous[j] + 1.0)
                        .min(current[j - 1] + 1.0)
                        .min(substitution_distance)
                };
                current[j] = dist;
                col_min = col_min.min(dist);
            }

            for value in current.iter_mut().take(s2_chars.len() + 1).skip(max_j + 1) {
                *value = big;
            }

            if col_min > max {
                return None;
            }

            std::mem::swap(&mut previous, &mut current);
        }

        let result = previous[s2_chars.len()];
        (result <= max).then_some(result)
    }
}
