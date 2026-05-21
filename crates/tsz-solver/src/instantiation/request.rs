//! Typed entry request for generic type instantiation.
//!
//! The instantiator still owns recursion guards, shadowing, and the actual
//! type traversal. This module names the request/options stage so cache keys,
//! mode flags, and `this_type` propagation stay in one place as the monolithic
//! instantiator is split.
//!
//! The legacy `instantiate_type_*_cached` entries each built an
//! [`InstantiationCacheKey`] inline with their own copy of the mode-bit
//! packing. Routing all of them through [`InstantiationRequest::cache_key`]
//! removes that duplication and gives downstream stages a single typed
//! boundary to consume.
//!
//! See [`super::result::InstantiationResult`] for the matching result
//! boundary.

use super::instantiate::TypeSubstitution;
use crate::caches::instantiation_cache::{CanonicalSubst, InstantiationCacheKey};
use crate::types::TypeId;

/// Bit positions for the packed instantiator mode byte. This byte layout is
/// the wire format for [`InstantiationCacheKey::mode_bits`]; the
/// `mode_bits_match_legacy_constants` test below pins it so accidentally
/// renumbering a flag cannot silently alias entries from before the change.
const MODE_SUBSTITUTE_INFER: u8 = 0b0001;
const MODE_PRESERVE_META: u8 = 0b0010;
const MODE_PRESERVE_UNSUBSTITUTED: u8 = 0b0100;
const MODE_SHALLOW_THIS_ONLY: u8 = 0b1000;

/// Options that affect how a type is instantiated.
///
/// Each option corresponds to one of the boolean flags on
/// [`super::instantiate::TypeInstantiator`]. Packing them into a single
/// [`InstantiationOptions`] value lets cache key construction and
/// instantiator setup share one source of truth.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct InstantiationOptions {
    substitute_infer: bool,
    preserve_meta_types: bool,
    preserve_unsubstituted_type_params: bool,
    shallow_this_only: bool,
}

impl InstantiationOptions {
    /// Construct the default option set (every flag off).
    pub const fn new() -> Self {
        Self {
            substitute_infer: false,
            preserve_meta_types: false,
            preserve_unsubstituted_type_params: false,
            shallow_this_only: false,
        }
    }

    pub const fn with_substitute_infer(mut self, enabled: bool) -> Self {
        self.substitute_infer = enabled;
        self
    }

    pub const fn with_preserve_meta_types(mut self, enabled: bool) -> Self {
        self.preserve_meta_types = enabled;
        self
    }

    pub const fn with_preserve_unsubstituted_type_params(mut self, enabled: bool) -> Self {
        self.preserve_unsubstituted_type_params = enabled;
        self
    }

    pub const fn with_shallow_this_only(mut self, enabled: bool) -> Self {
        self.shallow_this_only = enabled;
        self
    }

    pub const fn substitute_infer(self) -> bool {
        self.substitute_infer
    }

    pub const fn preserve_meta_types(self) -> bool {
        self.preserve_meta_types
    }

    pub const fn preserve_unsubstituted_type_params(self) -> bool {
        self.preserve_unsubstituted_type_params
    }

    pub const fn shallow_this_only(self) -> bool {
        self.shallow_this_only
    }

    /// Pack the option set into the `u8` mode-bits expected by
    /// [`InstantiationCacheKey`].
    pub const fn mode_bits(self) -> u8 {
        let mut bits = 0u8;
        if self.substitute_infer {
            bits |= MODE_SUBSTITUTE_INFER;
        }
        if self.preserve_meta_types {
            bits |= MODE_PRESERVE_META;
        }
        if self.preserve_unsubstituted_type_params {
            bits |= MODE_PRESERVE_UNSUBSTITUTED;
        }
        if self.shallow_this_only {
            bits |= MODE_SHALLOW_THIS_ONLY;
        }
        bits
    }
}

/// A normalized request to instantiate `type_id` under `substitution` and
/// `options`.
///
/// The substitution is borrowed because it is owned by the caller and may be
/// rebuilt cheaply. `this_type` is carried separately because it does not
/// participate in the substitution map but does participate in cache keys
/// (`substitute_this_type` always passes an empty substitution but distinct
/// `this_type` values must not alias).
#[derive(Clone, Copy, Debug)]
pub struct InstantiationRequest<'a> {
    type_id: TypeId,
    substitution: &'a TypeSubstitution,
    options: InstantiationOptions,
    this_type: Option<TypeId>,
}

impl<'a> InstantiationRequest<'a> {
    /// Construct a default request: no options, no `this_type`. Chain
    /// [`Self::with_options`] and [`Self::with_this_type`] to customize.
    pub const fn new(type_id: TypeId, substitution: &'a TypeSubstitution) -> Self {
        Self {
            type_id,
            substitution,
            options: InstantiationOptions::new(),
            this_type: None,
        }
    }

    pub const fn with_options(mut self, options: InstantiationOptions) -> Self {
        self.options = options;
        self
    }

    pub const fn with_this_type(mut self, this_type: TypeId) -> Self {
        self.this_type = Some(this_type);
        self
    }

    pub const fn type_id(self) -> TypeId {
        self.type_id
    }

    pub const fn substitution(self) -> &'a TypeSubstitution {
        self.substitution
    }

    pub const fn options(self) -> InstantiationOptions {
        self.options
    }

    pub const fn this_type(self) -> Option<TypeId> {
        self.this_type
    }

    /// Build the [`InstantiationCacheKey`] that this request would consult.
    ///
    /// Cache key construction always canonicalizes the substitution and packs
    /// the option set into mode bits; callers no longer need to reach for
    /// either primitive directly.
    pub fn cache_key(self) -> InstantiationCacheKey {
        let subst = if self.substitution.is_empty() {
            CanonicalSubst::empty()
        } else {
            CanonicalSubst::from_pairs(self.substitution.canonical_pairs())
        };
        InstantiationCacheKey::new(
            self.type_id,
            subst,
            self.options.mode_bits(),
            self.this_type,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::{InstantiationOptions, InstantiationRequest};
    use crate::TypeInterner;
    use crate::caches::instantiation_cache::{CanonicalSubst, InstantiationCacheKey};
    use crate::instantiation::instantiate::{
        TypeSubstitution, instantiate_type, instantiate_type_with_request,
    };
    use crate::types::{TypeId, TypeParamInfo};

    #[test]
    fn default_options_mode_bits_are_zero() {
        let options = InstantiationOptions::new();
        assert_eq!(options.mode_bits(), 0);
        assert!(!options.substitute_infer());
        assert!(!options.preserve_meta_types());
        assert!(!options.preserve_unsubstituted_type_params());
        assert!(!options.shallow_this_only());
    }

    #[test]
    fn mode_bits_match_legacy_constants() {
        // These values must stay in sync with the private MODE_* constants in
        // `instantiate.rs`. If either side moves, the cache key shape changes
        // and cross-version entries would alias incorrectly.
        assert_eq!(
            InstantiationOptions::new()
                .with_substitute_infer(true)
                .mode_bits(),
            0b0001
        );
        assert_eq!(
            InstantiationOptions::new()
                .with_preserve_meta_types(true)
                .mode_bits(),
            0b0010
        );
        assert_eq!(
            InstantiationOptions::new()
                .with_preserve_unsubstituted_type_params(true)
                .mode_bits(),
            0b0100
        );
        assert_eq!(
            InstantiationOptions::new()
                .with_shallow_this_only(true)
                .mode_bits(),
            0b1000
        );
    }

    #[test]
    fn combined_options_pack_into_one_byte() {
        let options = InstantiationOptions::new()
            .with_preserve_unsubstituted_type_params(true)
            .with_shallow_this_only(true);
        assert_eq!(options.mode_bits(), 0b1100);
        assert!(options.preserve_unsubstituted_type_params());
        assert!(options.shallow_this_only());
        assert!(!options.substitute_infer());
        assert!(!options.preserve_meta_types());
    }

    #[test]
    fn default_request_cache_key_is_empty_substitution() {
        let subst = TypeSubstitution::new();
        let request = InstantiationRequest::new(TypeId::STRING, &subst);
        let expected = InstantiationCacheKey::new(TypeId::STRING, CanonicalSubst::empty(), 0, None);
        assert_eq!(request.cache_key(), expected);
        assert_eq!(request.type_id(), TypeId::STRING);
        assert!(request.this_type().is_none());
    }

    #[test]
    fn request_cache_key_includes_options_and_this_type() {
        let subst = TypeSubstitution::new();
        let options = InstantiationOptions::new()
            .with_preserve_unsubstituted_type_params(true)
            .with_shallow_this_only(true);
        let request = InstantiationRequest::new(TypeId::STRING, &subst)
            .with_options(options)
            .with_this_type(TypeId::NUMBER);
        let key = request.cache_key();
        assert_eq!(key.type_id, TypeId::STRING);
        assert_eq!(key.mode_bits, 0b1100);
        assert_eq!(key.this_type, Some(TypeId::NUMBER));
        assert!(key.subst.is_empty());
    }

    #[test]
    fn request_engine_substitutes_type_parameter() {
        // The staged request boundary must produce the same `TypeId` as the
        // legacy `instantiate_type` entry for an ordinary substitution.
        let interner = TypeInterner::new();
        let t_name = interner.intern_string("T");
        let t_param = interner.type_param(TypeParamInfo {
            is_const: false,
            name: t_name,
            constraint: None,
            default: None,
        });
        let array_of_t = interner.array(t_param);

        let mut subst = TypeSubstitution::new();
        subst.insert(t_name, TypeId::NUMBER);

        let legacy = instantiate_type(&interner, array_of_t, &subst);
        let staged =
            instantiate_type_with_request(&interner, InstantiationRequest::new(array_of_t, &subst));
        assert!(!staged.depth_exceeded());
        assert_eq!(staged.type_id(), legacy);
        assert_eq!(legacy, interner.array(TypeId::NUMBER));
    }
}
