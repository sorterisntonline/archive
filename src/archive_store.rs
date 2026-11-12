//! Archive storage schema over durable (redb).
//!
//! Mirrors sorter2's `storage_schema` shape, flattened for v1's relational
//! model: named maps keyed by string identity, with deep keyed-set maps for
//! membership (presence = member, the durable idiom for an on-disk set).

use durable::{Durable, Leaf, List, Map};

use crate::path_types::ItemId;
use crate::reducer::VoteData;

pub const USER_VERSION: u32 = 1;
pub const ITEM_VERSION: u32 = 1;
pub const TAG_VERSION: u32 = 1;
pub const VOTE_VERSION: u32 = 2;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct UserData {
    pub ts: i64,
    pub name: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ItemData {
    pub ts: i64,
    pub id: String,
    pub title: String,
    pub slug: String,
    pub owner: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TagData {
    pub id: String,
    pub title: String,
    pub slug: String,
    pub owner: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct StoredVoteV1 {
    pub version: u32,
    pub ts: i64,
    pub a: String,
    pub b: String,
    pub ratio_left: i32,
    pub ratio_right: i32,
    pub pseudonym: String,
    pub trust_weight: f64,
}

impl From<VoteData> for StoredVoteV1 {
    fn from(v: VoteData) -> Self {
        Self {
            version: VOTE_VERSION,
            ts: v.ts,
            a: v.a.as_str().to_string(),
            b: v.b.as_str().to_string(),
            ratio_left: v.ratio_left,
            ratio_right: v.ratio_right,
            pseudonym: v.pseudonym,
            trust_weight: v.trust_weight,
        }
    }
}

impl From<StoredVoteV1> for VoteData {
    fn from(v: StoredVoteV1) -> Self {
        Self {
            ts: v.ts,
            a: ItemId::parse(&v.a).unwrap_or_else(|| ItemId::opaque(v.a.clone())),
            b: ItemId::parse(&v.b).unwrap_or_else(|| ItemId::opaque(v.b.clone())),
            ratio_left: v.ratio_left,
            ratio_right: v.ratio_right,
            pseudonym: v.pseudonym,
            trust_weight: v.trust_weight,
        }
    }
}

#[derive(Durable)]
#[allow(dead_code)]
pub struct ArchiveStore {
    pub users: Map<String, Leaf<UserData>>,
    pub items: Map<String, Leaf<ItemData>>,
    pub tags: Map<String, Leaf<TagData>>,
    /// Deeply-nested on-disk sets: tagId -> itemId -> present?
    pub membership: Map<String, Map<String, Leaf<bool>>>,
    /// owner -> itemId -> present?
    pub by_owner: Map<String, Map<String, Leaf<bool>>>,
    /// (actor, lo, hi) -> vote (sorter2 dedupe semantics).
    pub uuid_votes: Map<crate::reducer::UuidVoteKey, Leaf<StoredVoteV1>>,
    /// Most recent-first audit trail per actor for pagination.
    pub recent_votes: Map<String, List<Leaf<StoredVoteV1>>>,
    /// Archive meta: import counters / version marker.
    pub meta: Map<String, Leaf<u64>>,
}
