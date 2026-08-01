//! Symbol flag merge-compatibility rules for declaration merging.
//!
//! Split out of `binding.rs` to satisfy the source-file line cap.

use crate::state::BinderState;
use crate::symbol_flags;

impl BinderState {
    /// Check if two symbol flag sets can be merged.
    /// Made public for use in checker to detect duplicate identifiers (TS2300).
    #[must_use]
    pub const fn can_merge_flags(existing_flags: u32, new_flags: u32) -> bool {
        if (existing_flags & symbol_flags::INTERFACE) != 0
            && (new_flags & symbol_flags::INTERFACE) != 0
        {
            return true;
        }

        if (existing_flags & symbol_flags::CLASS != 0 && (new_flags & symbol_flags::INTERFACE) != 0)
            || (existing_flags & symbol_flags::INTERFACE != 0
                && (new_flags & symbol_flags::CLASS) != 0)
        {
            return true;
        }

        if (existing_flags & symbol_flags::MODULE) != 0 && (new_flags & symbol_flags::MODULE) != 0 {
            return true;
        }

        if (existing_flags & symbol_flags::MODULE) != 0
            && (new_flags & (symbol_flags::CLASS | symbol_flags::FUNCTION | symbol_flags::ENUM))
                != 0
        {
            return true;
        }
        if (new_flags & symbol_flags::MODULE) != 0
            && (existing_flags
                & (symbol_flags::CLASS | symbol_flags::FUNCTION | symbol_flags::ENUM))
                != 0
        {
            return true;
        }

        // Namespace/module can merge with interface
        if (existing_flags & symbol_flags::MODULE) != 0
            && (new_flags & symbol_flags::INTERFACE) != 0
        {
            return true;
        }
        if (new_flags & symbol_flags::MODULE) != 0
            && (existing_flags & symbol_flags::INTERFACE) != 0
        {
            return true;
        }

        if (existing_flags & symbol_flags::FUNCTION) != 0
            && (new_flags & symbol_flags::FUNCTION) != 0
        {
            return true;
        }

        // Allow function + class merging (TypeScript allows declare function + declare class)
        if (existing_flags & symbol_flags::FUNCTION) != 0 && (new_flags & symbol_flags::CLASS) != 0
        {
            return true;
        }
        if (existing_flags & symbol_flags::CLASS) != 0 && (new_flags & symbol_flags::FUNCTION) != 0
        {
            return true;
        }

        // Allow method overloads to merge (method signature + method implementation)
        if (existing_flags & symbol_flags::METHOD) != 0 && (new_flags & symbol_flags::METHOD) != 0 {
            return true;
        }

        // Allow VARIABLE + NAMESPACE_MODULE merging.
        // TypeScript's NamespaceModuleExcludes = 0 (can merge with anything) and
        // FunctionScopedVariableExcludes doesn't include NAMESPACE_MODULE.
        // e.g., `namespace m2 { ... } var m2: { ... };`
        if (existing_flags & symbol_flags::NAMESPACE_MODULE) != 0
            && (new_flags & symbol_flags::VARIABLE) != 0
        {
            return true;
        }
        if (new_flags & symbol_flags::NAMESPACE_MODULE) != 0
            && (existing_flags & symbol_flags::VARIABLE) != 0
        {
            return true;
        }

        // Allow INTERFACE to merge with VALUE symbols (e.g., `interface Object` + `declare var Object`)
        // This enables global types like Object, Array, Promise to be used as both types and constructors
        if (existing_flags & symbol_flags::INTERFACE) != 0 && (new_flags & symbol_flags::VALUE) != 0
        {
            return true;
        }
        if (new_flags & symbol_flags::INTERFACE) != 0 && (existing_flags & symbol_flags::VALUE) != 0
        {
            return true;
        }

        // Allow TYPE_ALIAS to merge with VALUE symbols
        // In TypeScript, type aliases and values exist in separate namespaces
        // and can share the same name:
        //   type Foo = number;
        //   export const Foo = 1;  // legal: Foo is both a type and a value
        if (existing_flags & symbol_flags::TYPE_ALIAS) != 0
            && (new_flags & symbol_flags::VALUE) != 0
        {
            return true;
        }
        if (new_flags & symbol_flags::TYPE_ALIAS) != 0
            && (existing_flags & symbol_flags::VALUE) != 0
        {
            return true;
        }

        // Allow TYPE_ALIAS to merge with a namespace/module. `TypeAliasExcludes`
        // in tsc is `Type` (Class|Interface|Enum|EnumMember|TypeLiteral|
        // TypeParameter|TypeAlias) — NamespaceModule/ValueModule are not in that
        // set, so `namespace X {} type X = number;` legally merges even when the
        // namespace is uninstantiated (NamespaceModule only, no VALUE bit). Kept
        // as a separate rule from the VALUE-based one above because an
        // uninstantiated namespace's NAMESPACE_MODULE flag is not part of
        // `symbol_flags::VALUE`.
        if (existing_flags & symbol_flags::MODULE) != 0
            && (new_flags & symbol_flags::TYPE_ALIAS) != 0
        {
            return true;
        }
        if (new_flags & symbol_flags::MODULE) != 0
            && (existing_flags & symbol_flags::TYPE_ALIAS) != 0
        {
            return true;
        }

        // Allow TYPE_PARAMETER to merge with VALUE symbols
        // e.g., `<T>(T: T) => T`
        if (existing_flags & symbol_flags::TYPE_PARAMETER) != 0
            && (new_flags & symbol_flags::VALUE) != 0
        {
            return true;
        }
        if (new_flags & symbol_flags::TYPE_PARAMETER) != 0
            && (existing_flags & symbol_flags::VALUE) != 0
        {
            return true;
        }

        // Allow ALIAS (import) to merge with VALUE symbols.
        // In TypeScript, imports and local value declarations can share the
        // same name — the import occupies the type namespace and the local
        // declaration occupies the value namespace:
        //   import type { A } from "./a";
        //   const A: A = "a";  // legal: A is both a type and a value
        if (existing_flags & symbol_flags::ALIAS) != 0 && (new_flags & symbol_flags::VALUE) != 0 {
            return true;
        }
        if (new_flags & symbol_flags::ALIAS) != 0 && (existing_flags & symbol_flags::VALUE) != 0 {
            return true;
        }

        // Allow ALIAS (import) to merge with local type declarations.
        // Import clauses can legally share a name with interfaces/type aliases
        // and form a single merged symbol that's usable in both namespaces:
        //   export default interface Foo {}
        //   import Foo from "./mod";
        //   export { Foo as default };
        if (existing_flags & symbol_flags::ALIAS) != 0
            && (new_flags & (symbol_flags::INTERFACE | symbol_flags::TYPE_ALIAS)) != 0
        {
            return true;
        }
        if (new_flags & symbol_flags::ALIAS) != 0
            && (existing_flags & (symbol_flags::INTERFACE | symbol_flags::TYPE_ALIAS)) != 0
        {
            return true;
        }

        // Allow ALIAS to merge with MODULE (namespace/module).
        // In TypeScript, AliasExcludes = Alias (only conflicts with other aliases)
        // and NamespaceModuleExcludes = 0 (can merge with anything).
        // This covers `export as namespace X` + `declare namespace X` coexisting:
        //   export = React;
        //   export as namespace React;  // creates ALIAS
        //   declare namespace React {}  // creates MODULE — must merge
        if (existing_flags & symbol_flags::ALIAS) != 0 && (new_flags & symbol_flags::MODULE) != 0 {
            return true;
        }
        if (new_flags & symbol_flags::ALIAS) != 0 && (existing_flags & symbol_flags::MODULE) != 0 {
            return true;
        }

        // Allow static and instance members to have the same name
        // TypeScript allows a class to have both a static member and an instance member with the same name
        // e.g., class C { static foo; foo; }
        let existing_is_static = (existing_flags & symbol_flags::STATIC) != 0;
        let new_is_static = (new_flags & symbol_flags::STATIC) != 0;
        if existing_is_static != new_is_static {
            // One is static, one is instance - allow merge
            return true;
        }

        false
    }
}
