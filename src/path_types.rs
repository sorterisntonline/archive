use std::hash::Hash;

/// Canonical identity: a real URL (with scheme) or an opaque non-URL key.
///
/// Minimal copy of sorter2's `path_types::ItemId` covering only what
/// `ranking` and `reducer` need. The archive only ever builds opaque ids
/// from v1 numeric item ids, so storage parsing is trimmed of sorter2's
/// URL normalization (which those ids would fail anyway).
#[derive(Debug, Clone, Hash, PartialEq, Eq, PartialOrd, Ord, Default)]
pub struct ItemId(String);

impl ItemId {
    /// Parse an already-canonical id. Empty string is invalid.
    pub fn parse(s: &str) -> Option<Self> {
        let t = s.trim();
        if t.is_empty() {
            return None;
        }
        Some(Self(t.to_string()))
    }

    /// Build an opaque item key (demo votes, non-URL items).
    pub fn opaque(s: impl Into<String>) -> Self {
        Self(s.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Normalize strings from forms, events, and imports into canonical identity.
    pub fn from_storage(s: &str) -> Option<Self> {
        let t = s.trim();
        if t.is_empty() {
            return None;
        }
        Self::parse(t)
    }
}
