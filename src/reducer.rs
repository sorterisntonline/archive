use std::collections::HashMap;

use crate::path_types::ItemId;

/// `(actor_uuid, min_item_id, max_item_id)` — one vote slot per human per pair.
/// Copied verbatim from sorter2 `server/src/reducer.rs` (plus `Clone` on
/// VoteData/ScopeVotes which the archive's read model needs).
pub type UuidVoteKey = (String, String, String);

pub fn canonical_pair_ids(a: &ItemId, b: &ItemId) -> (String, String) {
    let ak = a.as_str().to_string();
    let bk = b.as_str().to_string();
    if ak <= bk {
        (ak, bk)
    } else {
        (bk, ak)
    }
}

pub fn uuid_vote_key(actor_uuid: &str, a: &ItemId, b: &ItemId) -> UuidVoteKey {
    let (lo, hi) = canonical_pair_ids(a, b);
    (actor_uuid.to_string(), lo, hi)
}

/// Parsed pairwise vote (internal representation).
#[derive(Debug, Clone, PartialEq)]
pub struct VoteData {
    pub ts: i64,
    pub a: ItemId,
    pub b: ItemId,
    pub ratio_left: i32,
    pub ratio_right: i32,
    pub pseudonym: String,
    pub trust_weight: f64,
}

impl VoteData {
    /// Build a vote from validated event fields (replay / tests).
    pub fn from_event(
        ts: i64,
        a: &str,
        b: &str,
        ratio_left: i32,
        ratio_right: i32,
        pseudonym: String,
        trust_weight: f64,
    ) -> Option<Self> {
        let a = ItemId::from_storage(a)?;
        let b = ItemId::from_storage(b)?;
        if a == b {
            return None;
        }
        Some(Self {
            ts,
            a,
            b,
            ratio_left,
            ratio_right,
            pseudonym,
            trust_weight,
        })
    }
}

/// Votes cast within one ranking scope (parent node). Edges and rankings are
/// derived on demand from [`Self::uuid_votes`].
#[derive(Debug, Clone, Default)]
pub struct ScopeVotes {
    pub uuid_votes: HashMap<UuidVoteKey, VoteData>,
    pub recent_votes: Vec<VoteData>,
}

impl ScopeVotes {
    pub fn apply_vote(&mut self, vote: VoteData, actor_uuid: &str) {
        let key = uuid_vote_key(actor_uuid, &vote.a, &vote.b);
        self.uuid_votes.insert(key, vote.clone());
        self.recent_votes.push(vote);
    }
}
