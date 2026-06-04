impl TypeInterner {
    /// Try to reduce a large union by partitioning members by a discriminant property.
    /// Returns `Some(reduced_vec)` if partitioning was successful, None otherwise.
    fn try_partition_union_reduction(&self, members: &[TypeId]) -> Option<TypeListBuffer> {
        // 1. Identify a candidate discriminant property common to many members.
        // We look for a property that appears in at least 50% of object members.
        let mut prop_counts: FxHashMap<Atom, usize> = FxHashMap::default();
        let mut object_count = 0;

        for &member in members {
            if let Some(shape_id) = crate::visitor::object_shape_id(self, member)
                .or_else(|| crate::visitor::object_with_index_shape_id(self, member))
            {
                object_count += 1;
                let shape = self.object_shape(shape_id);
                for prop in &shape.properties {
                    *prop_counts.entry(prop.name).or_insert(0) += 1;
                }
            }
        }

        if object_count < 8 {
            return None;
        }

        let discriminant_prop = prop_counts
            .into_iter()
            .filter(|&(_, count)| count >= object_count / 2)
            .max_by_key(|&(_, count)| count)
            .map(|(name, _)| name)?;

        // 2. Partition members by their value for this property.
        // Non-objects and objects missing the property go into a "fallback" group.
        let mut partitions: FxHashMap<TypeId, Vec<TypeId>> = FxHashMap::default();
        let mut fallback: Vec<TypeId> = Vec::new();

        for &member in members {
            let val = crate::visitor::object_shape_id(self, member)
                .or_else(|| crate::visitor::object_with_index_shape_id(self, member))
                .and_then(|sid| {
                    let shape = self.object_shape(sid);
                    crate::utils::lookup_property(
                        self,
                        &shape.properties,
                        Some(sid),
                        discriminant_prop,
                    )
                    .map(|p| p.type_id)
                });

            if let Some(v) = val {
                partitions.entry(v).or_default().push(member);
            } else {
                fallback.push(member);
            }
        }

        // 3. Reduce each partition independently.
        let mut result: TypeListBuffer = SmallVec::new();
        for (_, group) in partitions {
            let mut group_buf = TypeListBuffer::from_vec(group);
            self.reduce_union_subtypes_quadratic(&mut group_buf);
            result.extend(group_buf);
        }

        // 4. Reduce fallback group and then check fallback against all winners.
        if !fallback.is_empty() {
            let mut fallback_buf = TypeListBuffer::from_vec(fallback);
            self.reduce_union_subtypes_quadratic(&mut fallback_buf);
            result.extend(fallback_buf);
        }

        // Final quadratic pass if the result is still large, but usually partitioning
        // significantly reduces the remaining work.
        if result.len() < members.len() {
            self.reduce_union_subtypes_quadratic(&mut result);
            Some(result)
        } else {
            None
        }
    }

    /// quadratic implementation of union reduction, used within partitions.
    fn reduce_union_subtypes_quadratic(&self, flat: &mut TypeListBuffer) {
        let len = flat.len();
        if len <= 1 {
            return;
        }
        // Use a u64 bitset instead of heap-allocated Vec<bool>.
        // Safe because callers guard len (partitions are always small subsets of
        // the already-guarded union, and the direct caller caps at 25 members).
        debug_assert!(len <= 64, "reduce_union_subtypes_quadratic: len={len} > 64");
        // Initialize bitset with first `len` bits set. Guard against shift overflow at len==64.
        let mut keep: u64 = if len >= 64 {
            u64::MAX
        } else {
            (1u64 << len) - 1
        };
        for i in 0..len {
            if keep & (1u64 << i) == 0 {
                continue;
            }
            for j in 0..len {
                if i == j || keep & (1u64 << j) == 0 {
                    continue;
                }
                if self.is_subtype_shallow(flat[i], flat[j]) {
                    keep &= !(1u64 << i);
                    break;
                }
            }
        }
        let mut write = 0;
        for read in 0..len {
            if keep & (1u64 << read) != 0 {
                flat[write] = flat[read];
                write += 1;
            }
        }
        flat.truncate(write);
    }

    /// Remove redundant types from an intersection using shallow subtype checks.
    /// If A <: B, then A & B = A (B is redundant).
    pub(crate) fn reduce_intersection_subtypes(&self, flat: &mut TypeListBuffer) {
        // Performance guard: skip O(N²) reduction for large intersections.
        // This is an optimization (removing redundant supertypes), not required for correctness.
        // For very large intersections (e.g., T extends A & B & C & ...), the O(N²) pairwise
        // subtype checks are prohibitively expensive. Skip and keep all members.
        const MAX_REDUCTION_SIZE: usize = 25;
        if flat.len() > MAX_REDUCTION_SIZE {
            return;
        }

        // Mark redundant elements using a u64 bitset (max 25 members from guard above),
        // then compact in one pass. Avoids heap allocation for the keep-set.
        let len = flat.len();
        debug_assert!(len <= 64, "reduce_intersection_subtypes: len={len} > 64");
        let mut keep: u64 = (1u64 << len) - 1; // all bits set
        for i in 0..len {
            if keep & (1u64 << i) == 0 {
                continue;
            }
            for j in 0..len {
                if i == j || keep & (1u64 << j) == 0 {
                    continue;
                }
                // If j is a subtype of i, i is the supertype and redundant in an intersection
                if self.is_subtype_shallow(flat[j], flat[i]) {
                    keep &= !(1u64 << i);
                    break;
                }
            }
        }
        // Compact: retain only non-redundant elements
        let mut write = 0;
        for read in 0..len {
            if keep & (1u64 << read) != 0 {
                flat[write] = flat[read];
                write += 1;
            }
        }
        flat.truncate(write);
    }

    /// Distribute an intersection over unions: A & (B | C) → (A & B) | (A & C)
    ///
    /// This is a critical normalization rule for the Judge layer that enables
    /// better simplification and canonical form detection.
    ///
    /// # Cardinality Guard
    /// To prevent exponential explosion (e.g., (A|B) & (C|D) & (E|F)...),
    /// we limit distribution to cases where the resulting union would have ≤ 25 members.
    ///
    /// # Returns
    /// - Some(result) if distribution was applied and should replace the intersection
    /// - None if no distribution occurred (no union members, or would exceed cardinality limit)
    pub(crate) fn distribute_intersection_over_unions(
        &self,
        flat: &TypeListBuffer,
    ) -> Option<TypeId> {
        // Find all union members in the intersection and calculate total combinations.
        // Two-pass approach: first compute the full cross-product size to check TS2590,
        // then apply the conservative distribution guard.
        let mut union_indices = Vec::with_capacity(flat.len());
        let mut total_combinations: usize = 1;

        for (i, &id) in flat.iter().enumerate() {
            if let Some(TypeData::Union(members)) = self.lookup(id) {
                let member_count = self.type_list(members).len();
                total_combinations = total_combinations.saturating_mul(member_count);
                union_indices.push(i);
            }
        }

        // TS2590: tsc checkCrossProductUnion bails at 100,000.
        // Must check BEFORE the conservative distribution guard so that
        // intersections like `(A|B) & (C|D) & ... & (Y|Z)` (18+ unions)
        // correctly trigger the too-complex flag even though we won't distribute.
        if total_combinations >= 100_000 {
            self.set_union_too_complex();
            return None;
        }

        // Conservative guard: skip distribution if would produce > 25 members
        if total_combinations > 25 {
            return None;
        }

        // No unions to distribute
        if union_indices.is_empty() {
            return None;
        }

        // Build the distributed union
        // Start with the first non-union member as the base
        let base_members: Vec<_> = flat
            .iter()
            .enumerate()
            .filter(|(i, _)| !union_indices.contains(i))
            .map(|(_, &id)| id)
            .collect();

        // If all members are unions, start with an empty intersection (unknown)
        let initial_intersection = if base_members.is_empty() {
            vec![]
        } else {
            base_members
        };

        // Recursively distribute: for each union, create intersections with all combinations
        let mut combinations = vec![initial_intersection];

        for &union_idx in &union_indices {
            let union_type = flat[union_idx];
            let TypeData::Union(union_members) = self.lookup(union_type)? else {
                continue;
            };
            let union_members = self.type_list(union_members);

            // For each existing combination, create new combinations with each union member
            let mut new_combinations =
                Vec::with_capacity(combinations.len().saturating_mul(union_members.len()));
            for combination in &combinations {
                for &union_member in union_members.iter() {
                    let mut new_combination = combination.clone();
                    new_combination.push(union_member);
                    new_combinations.push(new_combination);
                }
            }
            combinations = new_combinations;
        }

        // Convert each combination to an intersection TypeId
        let intersection_results: Vec<_> = combinations
            .iter()
            .map(|combination| self.intersection(combination.clone()))
            .collect();

        // Return the union of all intersections
        Some(self.union(intersection_results))
    }
}
