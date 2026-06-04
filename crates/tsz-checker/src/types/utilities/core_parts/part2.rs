impl<'a> CheckerState<'a> {
    // ============================================================================
    // Section 52: Parameter Type Utilities
    // ============================================================================

    /// Determine what kind of index key a type represents.
    ///
    /// This function analyzes a type to determine if it can be used for string
    /// or numeric indexing. Returns a tuple of (`wants_string`, `wants_number`).
    ///
    /// ## Returns:
    /// - `Some((true, false))`: String index (e.g., `"foo"`, `string`)
    /// - `Some((false, true))`: Number index (e.g., `42`, `number`)
    /// - `Some((true, true))`: Both string and number (e.g., `"a" | 1 | 2`)
    /// - `None`: Not an index type
    ///
    /// ## Examples:
    /// ```typescript
    /// type A = "foo";        // (true, false) - string literal
    /// type B = 42;           // (false, true) - number literal
    /// type C = string;       // (true, false) - string type
    /// type D = "a" | "b";    // (true, false) - union of strings
    /// type E = "a" | 1;      // (true, true) - mixed literals
    /// ```
    pub(crate) fn get_index_key_kind(&self, index_type: TypeId) -> Option<(bool, bool)> {
        if self
            .enum_symbol_from_type(index_type)
            .is_some_and(|sym_id| self.enum_kind(sym_id) == Some(EnumKind::Numeric))
        {
            return Some((false, true));
        }

        match query::classify_index_key(self.ctx.types, index_type) {
            query::IndexKeyKind::String
            | query::IndexKeyKind::StringLiteral
            | query::IndexKeyKind::TemplateLiteralString => Some((true, false)),
            query::IndexKeyKind::Number | query::IndexKeyKind::NumberLiteral => Some((false, true)),
            // `${number}` is a numeric string type — valid for both string and number
            // index signatures. Arrays have number index signatures, and objects may
            // have string index signatures, so this type can index both.
            query::IndexKeyKind::NumericStringLike => Some((true, true)),
            query::IndexKeyKind::Union(members) => {
                let mut wants_string = false;
                let mut wants_number = false;
                for member in members {
                    let (member_string, member_number) = self.get_index_key_kind(member)?;
                    wants_string |= member_string;
                    wants_number |= member_number;
                }
                Some((wants_string, wants_number))
            }
            query::IndexKeyKind::Other => {
                crate::query_boundaries::common::type_parameter_constraint(
                    self.ctx.types,
                    index_type,
                )
                .and_then(|constraint| {
                    (constraint != index_type).then(|| self.get_index_key_kind(constraint))
                })
                .flatten()
            }
        }
    }

    /// Check if a type key supports element indexing.
    ///
    /// This function determines if a type supports element access with the
    /// specified index kind (string, number, or both).
    ///
    /// ## Parameters:
    /// - `object_key`: The type key to check
    /// - `wants_string`: Whether string indexing is needed
    /// - `wants_number`: Whether numeric indexing is needed
    ///
    /// ## Returns:
    /// - `true`: The type supports the requested indexing
    /// - `false`: The type does not support the requested indexing
    ///
    /// ## Examples:
    /// ```typescript
    /// // Array supports numeric indexing:
    /// const arr: number[] = [1, 2, 3];
    /// arr[0];  // OK
    ///
    /// // Object with string index supports string indexing:
    /// const obj: { [key: string]: number } = {};
    /// obj["foo"];  // OK
    ///
    /// // Object without index signature doesn't support indexing:
    /// const plain: { a: number } = { a: 1 };
    /// plain["b"];  // Error: No index signature
    /// ```
    pub(crate) fn is_element_indexable(
        &self,
        object_type: TypeId,
        wants_string: bool,
        wants_number: bool,
    ) -> bool {
        // Use the resolver-aware classifier so that `Application(Lazy(DefId), args)`
        // wrappers — including those nested inside intersection / union members —
        // are expanded through the checker's `TypeEnvironment` before classification.
        // Without this, an intersection like `{ a: number } & Record<string, V>`
        // keeps the `Record` member opaque (classifier returns `Other`), which
        // causes a false TS7053 for indexed accesses on a type parameter
        // constrained to that intersection. The recursive call below stays on the
        // same path, so the resolver is threaded into every member as well.
        match query::classify_element_indexable_with_resolver(
            self.ctx.types,
            &self.ctx,
            object_type,
        ) {
            query::ElementIndexableKind::Array
            | query::ElementIndexableKind::Tuple
            | query::ElementIndexableKind::StringLike => wants_number,
            query::ElementIndexableKind::ObjectWithIndex {
                has_string,
                has_number,
            } => (wants_string && has_string) || (wants_number && (has_number || has_string)),
            query::ElementIndexableKind::Union(members) => members
                .iter()
                .all(|&member| self.is_element_indexable(member, wants_string, wants_number)),
            query::ElementIndexableKind::Intersection(members) => members
                .iter()
                .any(|&member| self.is_element_indexable(member, wants_string, wants_number)),
            query::ElementIndexableKind::Other => false,
        }
    }
}
