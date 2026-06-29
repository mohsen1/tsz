//! Declaration-site identity for definitions re-created across checker arenas.

use super::{Atom, DefId, DefKind, DefinitionInfo, DefinitionStore};

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(super) struct DeclSiteKey {
    kind: DefKind,
    name: Atom,
    arity: usize,
    file_id: u32,
    span_start: u32,
}

impl DeclSiteKey {
    fn from_info(info: &DefinitionInfo) -> Option<Self> {
        let file_id = info.file_id?;
        if file_id == DefinitionStore::NON_PROGRAM_FILE_SENTINEL {
            return None;
        }
        let (span_start, _) = info.span?;
        Some(Self {
            kind: info.kind,
            name: info.name,
            arity: info.type_params.len(),
            file_id,
            span_start,
        })
    }
}

impl DefinitionStore {
    pub(super) fn decl_site_key_for_info(info: &DefinitionInfo) -> Option<DeclSiteKey> {
        DeclSiteKey::from_info(info)
    }

    pub(super) fn register_decl_site_identity(&self, def_id: DefId, info: &DefinitionInfo) {
        if let Some(key) = DeclSiteKey::from_info(info) {
            self.decl_site_to_def.entry(key).or_insert(def_id);
        }
    }

    pub(super) fn refresh_decl_site_identity(
        &self,
        def_id: DefId,
        old_key: Option<DeclSiteKey>,
        info: &DefinitionInfo,
    ) {
        self.remove_decl_site_key_if_points_to(def_id, old_key);
        self.register_decl_site_identity(def_id, info);
    }

    pub(super) fn remove_decl_site_identity_if_points_to(
        &self,
        def_id: DefId,
        info: &DefinitionInfo,
    ) {
        self.remove_decl_site_key_if_points_to(def_id, DeclSiteKey::from_info(info));
    }

    fn remove_decl_site_key_if_points_to(&self, def_id: DefId, key: Option<DeclSiteKey>) {
        let Some(key) = key else {
            return;
        };
        if let Some(entry) = self.decl_site_to_def.get(&key)
            && *entry == def_id
        {
            drop(entry);
            self.decl_site_to_def.remove(&key);
        }
    }

    fn decl_site_key(&self, id: DefId) -> Option<DeclSiteKey> {
        let info = self.definitions.get(&id)?;
        DeclSiteKey::from_info(&info)
    }

    /// Canonical representative for a binder declaration site, if known.
    ///
    /// This is deliberately narrower than [`Self::canonical_def_id`]: it does
    /// not chase aliases or rewrite unrelated definitions. It only maps
    /// arena-local recreations of the same binder declaration node to the
    /// first registered solver `DefId`.
    pub fn canonical_decl_site_def_id(&self, def_id: DefId) -> DefId {
        self.decl_site_key(def_id)
            .and_then(|key| self.decl_site_to_def.get(&key).map(|entry| *entry))
            .unwrap_or(def_id)
    }

    /// Canonical declaration-site representative for a raw binder `SymbolId`.
    pub fn canonical_decl_site_def_for_symbol(&self, symbol_id: u32) -> Option<DefId> {
        self.find_def_by_symbol(symbol_id)
            .map(|def_id| self.canonical_decl_site_def_id(def_id))
    }

    /// Whether two `DefId`s came from the same binder declaration site.
    ///
    /// It recognizes the same `(file, declaration-node)` entry re-created in
    /// different arenas, with kind/name/arity guards to avoid treating
    /// unrelated declarations as one.
    pub fn defs_have_same_decl_site(&self, a: DefId, b: DefId) -> bool {
        if a == b {
            return true;
        }
        self.decl_site_key(a)
            .zip(self.decl_site_key(b))
            .is_some_and(|(a_key, b_key)| a_key == b_key)
    }
}
