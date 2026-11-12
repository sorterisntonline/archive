use std::collections::HashMap;

use durable::{Db, Durable};
use postgres::Client;

use crate::{
    archive_state::{load, privacy_filter},
    archive_store::{ArchiveStore, ArchiveStoreFields, ItemData, StoredVoteV1, TagData, UserData},
    path_types::ItemId,
    reducer::{ScopeVotes, VoteData},
};

pub fn open_store(path: &str) -> Db {
    std::fs::create_dir_all(path).expect("create data dir");
    Db::open(path).expect("open durable db")
}

pub struct ImportReport {
    pub users: usize,
    pub items: usize,
    pub tags: usize,
    pub members: usize,
    pub votes: usize,
    pub dropped_items: usize,
    pub dropped_votes: usize,
}

pub fn import(db: &Db, client: &mut Client) -> ImportReport {
    let dump = load(client);
    let before_items = dump.items.len();
    let before_votes = dump.votes.len();
    let dump = privacy_filter(dump);
    let root = ArchiveStore::root();
    let mut batch = db.batch();
    let mut n_users = 0;
    let mut n_items = 0;
    let mut n_tags = 0;
    let mut n_members = 0;
    for (name, ts) in &dump.users {
        batch.write(
            root.users()
                .key(name)
                .set(&UserData { ts: *ts, name: name.clone() }),
        );
        n_users += 1;
    }
    for item in &dump.items {
        let id = item.id.clone();
        batch.write(
            root.items()
                .key(&id)
                .set(&ItemData {
                    ts: item.ts,
                    id: id.clone(),
                    title: item.title.clone(),
                    slug: item.slug.clone(),
                    owner: item.owner.clone(),
                    url: item.url.clone(),
                    body: item.body.clone(),
                }),
        );
        let owner = item.owner.clone();
        batch.write(root.by_owner().key(&owner).key(&id).set(&true));
        n_items += 1;
    }
    for (id, title, owner) in {
        let mut attrs: Vec<(i32, String, String)> = client
            .query("SELECT id, title, owner FROM attributes", &[])
            .unwrap()
            .iter()
            .map(|r| (r.get(0), r.get(1), r.get(2)))
            .collect();
        attrs.sort();
        attrs
    } {
        batch.write(
            root.attributes()
                .key(&id.to_string())
                .set(&crate::archive_store::AttributeData {
                    id,
                    title,
                    owner,
                }),
        );
    }
    for tag in &dump.tags {
        batch.write(
            root.tags()
                .key(&tag.id)
                .set(&TagData {
                    id: tag.id.clone(),
                    title: tag.title.clone(),
                    slug: tag.slug.clone(),
                    owner: tag.owner.clone(),
                }),
        );
        n_tags += 1;
    }
    for (tag, item) in &dump.members {
        batch.write(root.membership().key(tag).key(item).set(&true));
        n_members += 1;
    }
    batch.commit().expect("commit archive projection");

    // votes: dedupe per sorter2 reducer semantics (one vote per voter per pair)
    let mut scope = ScopeVotes::default();
    let mut recent: HashMap<String, Vec<StoredVoteV1>> = HashMap::new();
    for v in &dump.votes {
        let vote = VoteData {
            ts: v.ts,
            a: ItemId::opaque(v.left.clone()),
            b: ItemId::opaque(v.right.clone()),
            ratio_left: v.ratio_left,
            ratio_right: v.ratio_right,
            pseudonym: v.owner.clone(),
            trust_weight: 1.0,
            attribute: v.attribute,
        };
        scope.apply_vote(vote, &v.owner);
        let stored = StoredVoteV1 {
            version: crate::archive_store::VOTE_VERSION,
            ts: v.ts,
            a: v.left.clone(),
            b: v.right.clone(),
            ratio_left: v.ratio_left,
            ratio_right: v.ratio_right,
            pseudonym: v.owner.clone(),
            trust_weight: 1.0,
            attribute: v.attribute,
        };
        recent.entry(v.owner.clone()).or_default().push(stored);
    }
    let mut batch = db.batch();
    for (key, vote) in &scope.uuid_votes {
        batch.write(
            root.uuid_votes()
                .key(key)
                .set(&StoredVoteV1::from(vote.clone())),
        );
    }
    for (actor, votes) in &recent {
        for v in votes.iter().rev() {
            batch.write(root.recent_votes().key(actor).push_op(v));
        }
    }
    batch.commit().expect("commit votes");

    ImportReport {
        users: n_users,
        items: dump.items.len(),
        tags: n_tags,
        members: n_members,
        votes: scope.uuid_votes.len(),
        dropped_items: before_items - dump.items.len(),
        dropped_votes: before_votes - dump.votes.len(),
    }
}
