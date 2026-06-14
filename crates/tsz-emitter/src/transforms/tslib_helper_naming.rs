//! Single source of truth for naming runtime helper functions under
//! `importHelpers`.
//!
//! Several ES5/decorator/async transformers need to turn a bare runtime-helper
//! name (e.g. `__decorate`, `__awaiter`) into the token actually emitted at the
//! call site. The rules are identical everywhere:
//!
//! - under CommonJS `importHelpers`, the helper is reached through the tslib
//!   import binding, so the call is `<binding>.<name>` (e.g. `tslib_1.__decorate`);
//! - under ESM `importHelpers`, the helper may have been imported under a renamed
//!   local alias (e.g. `__awaiter` -> `__awaiter_1`), so the call uses the alias;
//! - otherwise the helper is referenced by its bare name.
//!
//! These rules used to be copy-pasted as a `tslib_prefix: bool` +
//! `tslib_import_binding: String` (+ sometimes `helper_import_aliases`) field
//! triple with byte-identical prefixing methods across five transformer structs,
//! and the copies had already drifted (only the IR printer consulted the ESM
//! alias map). This type collapses them onto one value so every transformer
//! resolves helper names the same way.

use rustc_hash::FxHashMap;

/// Resolves runtime-helper call names for `importHelpers` emit.
#[derive(Clone, Debug)]
pub(crate) struct TslibHelperNaming {
    /// When true, prefix helper calls with `<binding>.` (CommonJS `importHelpers`).
    prefix: bool,
    /// The tslib import binding name (e.g. `tslib_1`) used when `prefix` is true.
    binding: String,
    /// ESM `importHelpers` renamed-helper aliases (e.g. `__awaiter` -> `__awaiter_1`).
    aliases: FxHashMap<String, String>,
}

impl Default for TslibHelperNaming {
    fn default() -> Self {
        Self {
            prefix: false,
            binding: "tslib_1".to_string(),
            aliases: FxHashMap::default(),
        }
    }
}

impl TslibHelperNaming {
    /// Enable/disable the CommonJS `<binding>.` prefix.
    pub(crate) const fn set_prefix(&mut self, prefix: bool) {
        self.prefix = prefix;
    }

    /// Set the tslib import binding name used by the CommonJS prefix.
    pub(crate) fn set_binding(&mut self, binding: String) {
        self.binding = binding;
    }

    /// Set the ESM renamed-helper alias map.
    pub(crate) fn set_aliases(&mut self, aliases: FxHashMap<String, String>) {
        self.aliases = aliases;
    }

    /// Whether the CommonJS `<binding>.` prefix is active.
    pub(crate) const fn prefix(&self) -> bool {
        self.prefix
    }

    /// The tslib import binding name used by the CommonJS prefix.
    pub(crate) fn binding(&self) -> &str {
        &self.binding
    }

    /// The ESM renamed-helper alias map.
    pub(crate) const fn aliases(&self) -> &FxHashMap<String, String> {
        &self.aliases
    }

    /// Resolve the runtime call token for `name` as an owned `String`.
    pub(crate) fn helper_name(&self, name: &str) -> String {
        let mut buf = String::new();
        self.write_into(&mut buf, name);
        buf
    }

    /// Append the runtime call token for `name` into `buf` without an
    /// intermediate allocation.
    pub(crate) fn write_into(&self, buf: &mut String, name: &str) {
        if self.prefix {
            buf.push_str(&self.binding);
            buf.push('.');
            buf.push_str(name);
            return;
        }
        if let Some(alias) = self.aliases.get(name) {
            buf.push_str(alias);
            return;
        }
        buf.push_str(name);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bare_name_when_no_prefix_or_alias() {
        let naming = TslibHelperNaming::default();
        assert_eq!(naming.helper_name("__decorate"), "__decorate");
        let mut buf = String::new();
        naming.write_into(&mut buf, "__decorate");
        assert_eq!(buf, "__decorate");
    }

    #[test]
    fn commonjs_prefix_takes_precedence_over_alias() {
        let mut naming = TslibHelperNaming::default();
        naming.set_prefix(true);
        naming.set_binding("tslib_1".to_string());
        let mut aliases = FxHashMap::default();
        aliases.insert("__awaiter".to_string(), "__awaiter_1".to_string());
        naming.set_aliases(aliases);
        assert_eq!(naming.helper_name("__awaiter"), "tslib_1.__awaiter");
        let mut buf = String::new();
        naming.write_into(&mut buf, "__awaiter");
        assert_eq!(buf, "tslib_1.__awaiter");
    }

    #[test]
    fn esm_alias_used_when_present_and_unprefixed() {
        let mut naming = TslibHelperNaming::default();
        let mut aliases = FxHashMap::default();
        aliases.insert("__awaiter".to_string(), "__awaiter_1".to_string());
        naming.set_aliases(aliases);
        assert_eq!(naming.helper_name("__awaiter"), "__awaiter_1");
        assert_eq!(naming.helper_name("__generator"), "__generator");
        let mut buf = String::new();
        naming.write_into(&mut buf, "__awaiter");
        assert_eq!(buf, "__awaiter_1");
    }

    #[test]
    fn custom_binding_is_honored() {
        let mut naming = TslibHelperNaming::default();
        naming.set_prefix(true);
        naming.set_binding("tslib_42".to_string());
        assert_eq!(naming.helper_name("__extends"), "tslib_42.__extends");
    }
}
