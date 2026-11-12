//! Archive producers derived from the v1 relational dump.

use std::collections::HashMap;

use postgres::{Client, NoTls};

pub fn connect(url: &str) -> Client {
    Client::connect(url, NoTls).expect("connect to sorter dump")
}

pub struct Dump {
    pub users: Vec<(String, i64)>,
    pub items: Vec<Item>,
    pub tags: Vec<Tag>,
    pub members: Vec<(String, String)>,
    pub votes: Vec<Vote>,
}

pub struct Item {
    pub id: String,
    pub title: String,
    pub slug: String,
    pub owner: String,
    pub url: String,
    pub body: String,
    pub ts: i64,
    pub ts_naive: String,
}

pub struct Tag {
    pub id: String,
    pub title: String,
    pub slug: String,
    pub owner: String,
    pub unlisted: bool,
}

pub struct Vote {
    pub owner: String,
    pub left: String,
    pub right: String,
    pub ratio_left: i32,
    pub ratio_right: i32,
    pub attribute: i32,
    pub ts: i64,
    pub ts_naive: String,
}

pub fn load(client: &mut Client) -> Dump {
    let users: Vec<(String, i64)> = client
        .query("SELECT user_name, floor(extract(epoch from created_at))::bigint FROM users", &[])
        .unwrap()
        .iter()
        .map(|r| (r.get(0), r.get(1)))
        .collect();
    let items: Vec<Item> = client
        .query(
            "SELECT id::text, title, slug, owner, coalesce(url,''), coalesce(body,''), floor(extract(epoch from created_at))::bigint, created_at::text FROM items",
            &[],
        )
        .unwrap()
        .iter()
        .map(|r| Item {
            id: r.get(0),
            title: r.get(1),
            slug: r.get(2),
            owner: r.get(3),
            url: r.get(4),
            body: r.get(5),
            ts: r.get(6),
            ts_naive: r.get(7),
        })
        .collect();
    let tags: Vec<Tag> = client
        .query("SELECT id::text, title, slug, owner, unlisted FROM tags", &[])
        .unwrap()
        .iter()
        .map(|r| Tag {
            id: r.get(0),
            title: r.get(1),
            slug: r.get(2),
            owner: r.get(3),
            unlisted: r.get(4),
        })
        .collect();
    let members: Vec<(String, String)> = client
        .query("SELECT tag_id::text, item_id::text FROM items_in_tags", &[])
        .unwrap()
        .iter()
        .map(|r| (r.get::<_, String>(0), r.get::<_, String>(1)))
        .collect();
    let votes: Vec<Vote> = client
        .query(
            "SELECT owner, left_item_id::text, right_item_id::text, magnitude, attribute, floor(extract(epoch from created_at))::bigint, created_at::text FROM votes ORDER BY created_at",
            &[],
        )
        .unwrap()
        .iter()
        .map(|r| Vote {
            owner: r.get(0),
            left: r.get(1),
            right: r.get(2),
            ratio_left: r.get(3),
            ratio_right: 100 - r.get::<_, i32>(3),
            attribute: r.get(4),
            ts: r.get(5),
            ts_naive: r.get(6),
        })
        .collect();
    Dump { users, items, tags, members, votes }
}

/// Privacy filter: drop unlisted tags and everything attached to them
/// (their items + votes among them), mirroring the site's settings.
pub fn privacy_filter(dump: Dump) -> Dump {
    let hidden_tags: std::collections::HashSet<String> = dump
        .tags
        .iter()
        .filter(|t| t.unlisted)
        .map(|t| t.id.clone())
        .collect();
    let hidden_items: std::collections::HashSet<String> = dump
        .members
        .iter()
        .filter(|(tag, _)| hidden_tags.contains(tag))
        .map(|(_, item)| item.clone())
        .collect();
    let tags = dump
        .tags
        .into_iter()
        .filter(|t| !t.unlisted)
        .collect::<Vec<_>>();
    let members = dump
        .members
        .into_iter()
        .filter(|(tag, item)| {
            !hidden_tags.contains(tag) && !hidden_items.contains(item)
        })
        .collect::<Vec<_>>();
    let items = dump
        .items
        .into_iter()
        .filter(|i| !hidden_items.contains(&i.id))
        .collect::<Vec<_>>();
    let keep: std::collections::HashSet<String> =
        items.iter().map(|i| i.id.clone()).collect();
    let votes = dump
        .votes
        .into_iter()
        .filter(|v| keep.contains(&v.left) && keep.contains(&v.right))
        .collect::<Vec<_>>();
    let users = dump.users;
    Dump { users, items, tags, members, votes }
}

pub fn vote_index(dump: &Dump) -> HashMap<String, (String, String)> {
    // kept for parity with the site's item-title lookups
    dump.items
        .iter()
        .map(|i| (i.id.clone(), (i.title.clone(), i.owner.clone())))
        .collect()
}
