use crate::def::DefId;
use tsz_common::interner::Atom;

/// Source identity used to reconstruct tsc construct-overload candidate order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ConstructSignatureOrigin {
    /// Stable owning definition; `None` models an anonymous declaration.
    pub owner: Option<DefId>,
    /// Interned source-file path of the containing declaration.
    pub declaration_file: Atom,
    /// Source span of the containing declaration, not the signature member.
    pub declaration_pos: u32,
    pub declaration_end: u32,
}
