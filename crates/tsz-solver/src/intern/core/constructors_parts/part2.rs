impl TypeInterner {
    // =========================================================================
    // Convenience methods for common type constructions
    // =========================================================================

    /// Estimated in-memory size of the entire type interner in bytes.
    ///
    /// This is a best-effort heuristic for memory pressure tracking and
    /// eviction decisions in the LSP. It reads only atomic counters and
    /// `DashMap::len()` calls — no per-entry iteration.
    ///
    /// The estimate accounts for:
    /// - Per-type overhead in sharded storage (two `DashMap` entries per type)
    /// - Sub-interners for type lists, tuple lists, template lists, shapes
    /// - Auxiliary caches (`identity_comparable`, `alloc_order`, `display_properties`)
    /// - Fixed-size fields (`array_base_type`, `boxed_types`, etc.)
    #[must_use]
    pub fn estimated_size_bytes(&self) -> usize {
        let mut size = std::mem::size_of::<Self>();

        // --- Sharded type storage ---
        // Each interned type lives in a DashMap (key_to_index) and a flat Vec (index_to_key).
        // DashMap overhead per entry is roughly 64 bytes (bucket + hash + padding).
        // TypeData is Copy and small (~32 bytes), stored inline.
        const DASHMAP_ENTRY_OVERHEAD: usize = 64;
        let type_data_size = std::mem::size_of::<TypeData>();
        // key_to_index: DashMap<TypeData, u32> + index_to_key: Vec<TypeData>
        let per_type_cost = (DASHMAP_ENTRY_OVERHEAD + type_data_size + 4) + type_data_size;

        let type_count = self.len();
        size += type_count * per_type_cost;

        // Shard Vec allocation
        size += self.shards.capacity() * std::mem::size_of::<TypeShard>();

        // --- Slice interners (type_lists, tuple_lists, template_lists) ---
        // Each entry: two DashMap entries (id->Arc<[T]> and Arc<[T]>->id) + Arc heap alloc.
        // Average slice length is ~3 elements for type lists, ~2 for tuples/templates.
        let type_list_count = self.type_lists.next_id.load(Ordering::Relaxed) as usize;
        let avg_type_list_elements = 3usize;
        size += type_list_count
            * (2 * DASHMAP_ENTRY_OVERHEAD
                + std::mem::size_of::<Arc<[TypeId]>>()
                + avg_type_list_elements * std::mem::size_of::<TypeId>());

        let tuple_list_count = self.tuple_lists.next_id.load(Ordering::Relaxed) as usize;
        let avg_tuple_elements = 2usize;
        size += tuple_list_count
            * (2 * DASHMAP_ENTRY_OVERHEAD
                + std::mem::size_of::<Arc<[TupleElement]>>()
                + avg_tuple_elements * std::mem::size_of::<TupleElement>());

        let template_list_count = self.template_lists.next_id.load(Ordering::Relaxed) as usize;
        let avg_template_elements = 2usize;
        size += template_list_count
            * (2 * DASHMAP_ENTRY_OVERHEAD
                + std::mem::size_of::<Arc<[TemplateSpan]>>()
                + avg_template_elements * std::mem::size_of::<TemplateSpan>());

        // --- Value interners (object/function/callable/conditional/mapped/application shapes) ---
        // Each entry: two DashMap entries + Arc<T> heap alloc.
        let value_interner_cost = |count: usize, value_size: usize| -> usize {
            count * (2 * DASHMAP_ENTRY_OVERHEAD + std::mem::size_of::<usize>() * 2 + value_size)
        };

        size += value_interner_cost(
            self.object_shapes.next_id.load(Ordering::Relaxed) as usize,
            std::mem::size_of::<ObjectShape>(),
        );
        size += value_interner_cost(
            self.function_shapes.next_id.load(Ordering::Relaxed) as usize,
            std::mem::size_of::<FunctionShape>(),
        );
        size += value_interner_cost(
            self.callable_shapes.next_id.load(Ordering::Relaxed) as usize,
            std::mem::size_of::<CallableShape>(),
        );
        size += value_interner_cost(
            self.conditional_types.next_id.load(Ordering::Relaxed) as usize,
            std::mem::size_of::<ConditionalType>(),
        );
        size += value_interner_cost(
            self.mapped_types.next_id.load(Ordering::Relaxed) as usize,
            std::mem::size_of::<MappedType>(),
        );
        size += value_interner_cost(
            self.applications.next_id.load(Ordering::Relaxed) as usize,
            std::mem::size_of::<TypeApplication>(),
        );

        // --- Auxiliary caches ---
        size += self.identity_comparable_cache.len()
            * (DASHMAP_ENTRY_OVERHEAD + std::mem::size_of::<TypeId>() + 1);
        size += self.contains_this_cache.len()
            * (DASHMAP_ENTRY_OVERHEAD + std::mem::size_of::<TypeId>() + 1);
        size += self.contains_infer_cache.len()
            * (DASHMAP_ENTRY_OVERHEAD + std::mem::size_of::<TypeId>() + 1);
        size += self.contains_type_query_cache.len()
            * (DASHMAP_ENTRY_OVERHEAD + std::mem::size_of::<TypeId>() + 1);
        size += self.contains_conditional_cache.len()
            * (DASHMAP_ENTRY_OVERHEAD + std::mem::size_of::<TypeId>() + 1);
        // alloc_order is now stored per-shard alongside index_to_key (4 bytes per type)
        size += type_count * 4;
        size += self.display_properties.len()
            * (DASHMAP_ENTRY_OVERHEAD
                + std::mem::size_of::<TypeId>()
                + std::mem::size_of::<Arc<Vec<PropertyInfo>>>());
        size +=
            self.display_alias.len() * (DASHMAP_ENTRY_OVERHEAD + std::mem::size_of::<TypeId>() * 2);
        size += self.boxed_types.len() * (DASHMAP_ENTRY_OVERHEAD + 16);
        size += self.boxed_def_ids.len() * (DASHMAP_ENTRY_OVERHEAD + 32);
        size += self.this_type_marker_def_ids.len() * (DASHMAP_ENTRY_OVERHEAD + 8);

        // Object property map index (if initialized)
        if let Some(prop_map) = self.object_property_maps.get() {
            size += prop_map.len() * (DASHMAP_ENTRY_OVERHEAD + 128);
        }

        size
    }
}
