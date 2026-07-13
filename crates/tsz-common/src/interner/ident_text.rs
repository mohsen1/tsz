//! Shared, immutable identifier text.
//!
//! [`IdentText`] is a cheap-to-clone handle to identifier text: a refcounted
//! pointer into the per-file [`Interner`](super::Interner)'s string table (or,
//! for synthesized/recovery identifiers, a standalone shared string). It
//! replaces per-node owned `String`s so that the many occurrences of the same
//! identifier in a file share one allocation instead of each carrying a copy.
//!
//! The type intentionally mirrors `String`'s read surface (`Deref<Target =
//! str>`, `as_str`, `Display`, `Debug`, comparisons against `str`/`String`,
//! ordering, hashing) so that read-only call sites compile unchanged. It has
//! no mutation surface: identifier text is fixed at construction.
//!
//! Serialization is byte-compatible with `String` (serde serializes it as a
//! plain string), so JSON IPC payloads and bincode arena snapshots keep their
//! existing format.

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::borrow::Borrow;
use std::fmt;
use std::hash::{Hash, Hasher};
use std::sync::{Arc, OnceLock};

/// Shared immutable identifier text (see module docs).
#[derive(Clone)]
pub struct IdentText(Arc<str>);

fn empty_arc() -> &'static Arc<str> {
    static EMPTY: OnceLock<Arc<str>> = OnceLock::new();
    EMPTY.get_or_init(|| Arc::from(""))
}

impl IdentText {
    /// The empty identifier text. Does not allocate.
    #[must_use]
    pub fn empty() -> Self {
        Self(Arc::clone(empty_arc()))
    }

    /// Wrap an existing shared string without copying it.
    #[must_use]
    pub const fn from_arc(text: Arc<str>) -> Self {
        Self(text)
    }

    /// View as `&str`.
    #[must_use]
    #[inline]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Clone the underlying shared string handle.
    #[must_use]
    pub fn to_arc(&self) -> Arc<str> {
        Arc::clone(&self.0)
    }
}

impl Default for IdentText {
    fn default() -> Self {
        Self::empty()
    }
}

impl std::ops::Deref for IdentText {
    type Target = str;
    #[inline]
    fn deref(&self) -> &str {
        &self.0
    }
}

impl AsRef<str> for IdentText {
    #[inline]
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl Borrow<str> for IdentText {
    #[inline]
    fn borrow(&self) -> &str {
        &self.0
    }
}

impl From<&str> for IdentText {
    fn from(s: &str) -> Self {
        if s.is_empty() {
            return Self::empty();
        }
        Self(Arc::from(s))
    }
}

impl From<String> for IdentText {
    fn from(s: String) -> Self {
        if s.is_empty() {
            return Self::empty();
        }
        Self(Arc::from(s))
    }
}

impl From<Arc<str>> for IdentText {
    fn from(s: Arc<str>) -> Self {
        Self(s)
    }
}

impl From<IdentText> for String {
    fn from(s: IdentText) -> Self {
        s.as_str().to_string()
    }
}

impl From<IdentText> for std::borrow::Cow<'_, str> {
    fn from(s: IdentText) -> Self {
        std::borrow::Cow::Owned(s.as_str().to_string())
    }
}

impl<'a> From<&'a IdentText> for std::borrow::Cow<'a, str> {
    fn from(s: &'a IdentText) -> Self {
        std::borrow::Cow::Borrowed(s.as_str())
    }
}

impl From<&IdentText> for String {
    fn from(s: &IdentText) -> Self {
        s.as_str().to_string()
    }
}

impl fmt::Display for IdentText {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self.as_str(), f)
    }
}

/// Matches `String`'s `Debug` (quoted string) so log/trace output is
/// unchanged by the `String -> IdentText` migration.
impl fmt::Debug for IdentText {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self.as_str(), f)
    }
}

impl PartialEq for IdentText {
    #[inline]
    fn eq(&self, other: &Self) -> bool {
        // Same-interner occurrences share one Arc; fall back to content
        // comparison for text minted by different interners.
        Arc::ptr_eq(&self.0, &other.0) || self.0 == other.0
    }
}

impl Eq for IdentText {}

impl PartialEq<str> for IdentText {
    #[inline]
    fn eq(&self, other: &str) -> bool {
        self.as_str() == other
    }
}

impl PartialEq<&str> for IdentText {
    #[inline]
    fn eq(&self, other: &&str) -> bool {
        self.as_str() == *other
    }
}

impl PartialEq<String> for IdentText {
    #[inline]
    fn eq(&self, other: &String) -> bool {
        self.as_str() == other.as_str()
    }
}

impl PartialEq<IdentText> for str {
    #[inline]
    fn eq(&self, other: &IdentText) -> bool {
        self == other.as_str()
    }
}

impl PartialEq<IdentText> for &str {
    #[inline]
    fn eq(&self, other: &IdentText) -> bool {
        *self == other.as_str()
    }
}

impl PartialEq<IdentText> for String {
    #[inline]
    fn eq(&self, other: &IdentText) -> bool {
        self.as_str() == other.as_str()
    }
}

/// Hashes like `str`/`String` (and consistently with the `Borrow<str>` impl),
/// so `IdentText` map keys behave exactly like the `String` keys they replace.
impl Hash for IdentText {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.as_str().hash(state);
    }
}

impl PartialOrd for IdentText {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for IdentText {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.as_str().cmp(other.as_str())
    }
}

impl Serialize for IdentText {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for IdentText {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct V;
        impl serde::de::Visitor<'_> for V {
            type Value = IdentText;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("a string")
            }
            fn visit_str<E: serde::de::Error>(self, v: &str) -> Result<IdentText, E> {
                Ok(IdentText::from(v))
            }
            fn visit_string<E: serde::de::Error>(self, v: String) -> Result<IdentText, E> {
                Ok(IdentText::from(v))
            }
        }
        deserializer.deserialize_str(V)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_is_shared_and_empty() {
        let a = IdentText::empty();
        let b = IdentText::default();
        assert!(a.is_empty());
        assert_eq!(a, b);
        assert!(Arc::ptr_eq(&a.0, &b.0));
    }

    #[test]
    fn compares_like_string() {
        let t = IdentText::from("foo");
        assert_eq!(t, "foo");
        assert_eq!(t, *"foo");
        assert_eq!("foo", t);
        assert_eq!(t, String::from("foo"));
        assert_eq!(String::from("foo"), t);
        assert_ne!(t, "bar");
    }

    #[test]
    fn ptr_sharing_and_content_equality() {
        let shared: Arc<str> = Arc::from("name");
        let a = IdentText::from_arc(Arc::clone(&shared));
        let b = IdentText::from_arc(shared);
        let c = IdentText::from("name");
        assert_eq!(a, b);
        assert_eq!(a, c); // different Arc, same content
    }

    #[test]
    fn debug_and_display_match_string() {
        let t = IdentText::from("x\\y");
        let s = String::from("x\\y");
        assert_eq!(format!("{t}"), format!("{s}"));
        assert_eq!(format!("{t:?}"), format!("{s:?}"));
    }

    #[test]
    fn serde_is_string_compatible() {
        let t = IdentText::from("hello");
        let json = serde_json::to_string(&t).unwrap();
        assert_eq!(json, "\"hello\"");
        let back: IdentText = serde_json::from_str("\"hello\"").unwrap();
        assert_eq!(back, t);
        // A String round-trips into IdentText and vice versa.
        let from_string: IdentText =
            serde_json::from_str(&serde_json::to_string(&String::from("s")).unwrap()).unwrap();
        assert_eq!(from_string, "s");
    }

    #[test]
    fn hashes_like_str() {
        use std::collections::HashMap;
        let mut m: HashMap<IdentText, u32> = HashMap::new();
        m.insert(IdentText::from("k"), 1);
        // Borrow<str> lookup
        assert_eq!(m.get("k"), Some(&1));
    }
}
