use askama::Template;
use postgres::{Client, NoTls};
use std::collections::HashMap;
use std::path::PathBuf;

mod identity;
mod path_types;
mod ranking;
mod reducer;

use path_types::ItemId;
use reducer::{ScopeVotes, VoteData};

pub const SITE_CSS: &str = include_str!("css.txt");

// --- DB rows (sorter v1 schema) ---

fn safe_page(name: &str, taken: &mut std::collections::HashSet<String>) -> String {
    let slug = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '-'
            }
        })
        .collect::<String>()
        .to_lowercase();
    if taken.insert(slug.clone()) {
        return slug;
    }
    let mut n = 2;
    loop {
        let cand = format!("{}-{}", slug, n);
        if taken.insert(cand.clone()) {
            return cand;
        }
        n += 1;
    }
}

struct UserRow {
    name: String,
}
struct ItemRow {
    id: i32,
    owner: String,
    title: String,
    #[allow(dead_code)]
    slug: String,
}
struct TagRow {
    id: i32,
    owner: String,
    title: String,
    slug: String,
}
struct ItemTagRow {
    item_id: i32,
    tag_id: i32,
}
struct VoteRow {
    owner: String,
    left_item_id: i32,
    right_item_id: i32,
    magnitude: i32,
    created_at: String,
}

// --- template models ---

#[derive(Debug, Clone)]
struct TaggedUser {
    name: String,
    page: String,
    votes: usize,
    items: usize,
}
#[derive(Debug, Clone)]
struct TagEntry {
    slug: String,
    page: String,
    title: String,
    items: usize,
    owner: String,
}
#[derive(Debug, Clone)]
struct ScoredItem {
    rank: usize,
    title: String,
    owner: String,
    score: String,
}
#[derive(Debug, Clone)]
struct UserItem {
    rank: usize,
    title: String,
}

#[derive(Template)]
#[template(path = "index.html")]
struct IndexTemplate<'a> {
    title: &'a str,
    css: &'a str,
    top1: ScoredItem,
    top2: ScoredItem,
    user_count: usize,
    item_count: usize,
    vote_count: usize,
    tag_count: usize,
    tags: Vec<TagEntry>,
    top_users: Vec<TaggedUser>,
    hist_days: Vec<HistDay>,
    peak_day: String,
    peak_count: usize,
}
#[derive(Debug, Clone)]
struct HistDay {
    day: String,
    count: usize,
    height: usize,
}

#[derive(Template)]
#[template(path = "rankings.html")]
struct RankingsTemplate<'a> {
    title: &'a str,
    css: &'a str,
    item_count: usize,
    rows: Vec<ScoredItem>,
}

#[derive(Template)]
#[template(path = "tags.html")]
struct TagsTemplate<'a> {
    title: &'a str,
    css: &'a str,
    rows: Vec<TagEntry>,
}

#[derive(Template)]
#[template(path = "tag.html")]
struct TagTemplate<'a> {
    title: &'a str,
    css: &'a str,
    tag_title: String,
    item_count: usize,
    rows: Vec<ScoredItem>,
}

#[derive(Template)]
#[template(path = "users.html")]
struct UsersTemplate<'a> {
    title: &'a str,
    css: &'a str,
    rows: Vec<TaggedUser>,
}

#[derive(Template)]
#[template(path = "user.html")]
struct UserTemplate<'a> {
    title: &'a str,
    css: &'a str,
    user_name: String,
    votes: usize,
    items: usize,
    rows: Vec<UserItem>,
}

#[derive(Debug, Clone)]
struct VoteView {
    when: String,
    voter: String,
    left: String,
    right: String,
    magnitude: i32,
}

#[derive(Template)]
#[template(path = "votes.html")]
struct VotesTemplate<'a> {
    title: &'a str,
    css: &'a str,
    vote_count: usize,
    rows: Vec<VoteView>,
}

fn db_url() -> String {
    std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://postgres:temp@localhost:15455/sorter".into())
}

fn main() {
    let mut client = Client::connect(&db_url(), NoTls).expect("connect to sorter dump");
    let users: Vec<UserRow> = client
        .query("SELECT user_name FROM users ORDER BY created_at", &[])
        .unwrap()
        .iter()
        .map(|r| UserRow { name: r.get(0) })
        .collect();
    let items: Vec<ItemRow> = client
        .query("SELECT id, owner, title, slug FROM items ORDER BY created_at", &[])
        .unwrap()
        .iter()
        .map(|r| ItemRow {
            id: r.get(0),
            owner: r.get(1),
            title: r.get(2),
            slug: r.get(3),
        })
        .collect();
    let tags: Vec<TagRow> = client
        .query("SELECT id, owner, title, slug FROM tags ORDER BY title", &[])
        .unwrap()
        .iter()
        .map(|r| TagRow {
            id: r.get(0),
            owner: r.get(1),
            title: r.get(2),
            slug: r.get(3),
        })
        .collect();
    let item_tags: Vec<ItemTagRow> = client
        .query("SELECT item_id, tag_id FROM items_in_tags", &[])
        .unwrap()
        .iter()
        .map(|r| ItemTagRow {
            item_id: r.get(0),
            tag_id: r.get(1),
        })
        .collect();
    let votes: Vec<VoteRow> = client
        .query(
            "SELECT owner, left_item_id, right_item_id, magnitude, created_at::text FROM votes ORDER BY created_at",
            &[],
        )
        .unwrap()
        .iter()
        .map(|r| VoteRow {
            owner: r.get(0),
            left_item_id: r.get(1),
            right_item_id: r.get(2),
            magnitude: r.get(3),
            // render as YYYY-MM-DD HH:MM via text cast (no chrono FromSql)
            created_at: {
                let t: String = r.get(4);
                t.get(..16).unwrap_or(&t).to_string()
            },
        })
        .collect();

    // --- sorter2 random walk (exact copy semantics) ---
    let mut scope = ScopeVotes::default();
    for v in &votes {
        let a = ItemId::opaque(v.left_item_id.to_string());
        let b = ItemId::opaque(v.right_item_id.to_string());
        if let Some(vote) = VoteData::from_event(
            0,
            a.as_str(),
            b.as_str(),
            v.magnitude,
            100 - v.magnitude,
            v.owner.clone(),
            1.0,
        ) {
            scope.apply_vote(vote, &v.owner);
        }
    }
    let ranked = ranking::ranked_items(&scope);
    // sorter2 ItemIds are opaque numeric-id strings; rebuild title/owner lookups
    let item_by_id: HashMap<i32, &ItemRow> = items.iter().map(|i| (i.id, i)).collect();
    let mut ranked_full: Vec<(f64, &ItemRow)> = Vec::new();
    for r in &ranked {
        if let Ok(numeric) = r.item.as_str().parse::<i32>() {
            if let Some(i) = item_by_id.get(&numeric) {
                ranked_full.push((r.score, i));
            }
        }
    }
    let global_rank: HashMap<i32, usize> = ranked_full
        .iter()
        .enumerate()
        .map(|(idx, (_, i))| (i.id, idx + 1))
        .collect();
    // items with no votes keep score 0 and rank after, in title order
    let mut rest: Vec<&ItemRow> = items
        .iter()
        .filter(|i| !ranked_full.iter().any(|(_, x)| x.id == i.id))
        .collect();
    rest.sort_by(|a, b| a.title.cmp(&b.title));
    let ranked_items_for_web: Vec<(f64, &ItemRow)> = ranked_full
        .into_iter()
        .chain(rest.into_iter().map(|i| (0.0, i)))
        .collect();

    // per-user vote counts + per-user ranking by their own items
    let mut votes_by_user: HashMap<String, usize> = HashMap::new();
    for v in &votes {
        *votes_by_user.entry(v.owner.clone()).or_default() += 1;
    }

    // tag membership
    let mut tag2items: HashMap<i32, Vec<i32>> = HashMap::new();
    let mut item2tags: HashMap<i32, Vec<i32>> = HashMap::new();
    for it in &item_tags {
        tag2items.entry(it.tag_id).or_default().push(it.item_id);
        item2tags.entry(it.item_id).or_default().push(it.tag_id);
    }

    let out = PathBuf::from("public");
    // shallow-scout mutual sanity: ranked first/second must exist
    let (s1, t1) = &ranked_items_for_web[0];
    let (s2, t2) = &ranked_items_for_web[1];
    let top1 = ScoredItem {
        rank: 1,
        title: t1.title.clone(),
        owner: t1.owner.clone(),
        score: format!("{:.4}", s1),
    };
    let top2 = ScoredItem {
        rank: 2,
        title: t2.title.clone(),
        owner: t2.owner.clone(),
        score: format!("{:.4}", s2),
    };

    let tag_entries: Vec<TagEntry> = {
        let mut taken = std::collections::HashSet::new();
        tags.iter()
            .map(|t| {
                let page = safe_page(&t.slug, &mut taken);
                TagEntry {
                    slug: t.slug.clone(),
                    page,
                    title: t.title.clone(),
                    items: tag2items.get(&t.id).map(|v| v.len()).unwrap_or(0),
                    owner: t.owner.clone(),
                }
            })
            .collect()
    };
    let tagged_users: Vec<TaggedUser> = {
        let mut taken = std::collections::HashSet::new();
        let mut v: Vec<TaggedUser> = users
            .iter()
            .map(|u| TaggedUser {
                page: safe_page(&u.name, &mut taken),
                items: items
                    .iter()
                    .filter(|i| i.owner == u.name)
                    .count(),
                votes: *votes_by_user.get(&u.name).unwrap_or(&0),
                name: u.name.clone(),
            })
            .collect();
        v.sort_by(|a, b| b.votes.cmp(&a.votes));
        v
    };

    // histogram: last 60 days with votes
    let mut per_day: Vec<(String, usize)> = Vec::new();
    for v in &votes {
        let d = v.created_at[..10].to_string();
        if let Some(last) = per_day.last_mut() {
            if last.0 == d {
                last.1 += 1;
                continue;
            }
        }
        per_day.push((d, 1));
    }
    let shown = &per_day[per_day.len().saturating_sub(60)..];
    let peak = shown.iter().max_by_key(|(_, c)| c).cloned().unwrap_or_default();
    let max_count = shown.iter().map(|(_, c)| *c).max().unwrap_or(1);
    let hist_days: Vec<HistDay> = shown
        .iter()
        .map(|(d, c)| HistDay {
            day: d.clone(),
            count: *c,
            height: (100 * c / max_count) as usize,
        })
        .collect();

    let tag_page: HashMap<i32, String> = tag_entries
        .iter()
        .zip(tags.iter())
        .map(|(e, t)| (t.id, e.page.clone()))
        .collect();
    let user_page: HashMap<String, String> = tagged_users
        .iter()
        .map(|u| (u.name.clone(), u.page.clone()))
        .collect();

    let write = |rel: &str, html: String| {
        let p = out.join(rel);
        if let Some(parent) = p.parent() {
            std::fs::create_dir_all(parent).ok();
        }
        std::fs::write(&p, html).unwrap();
    };

    write(
        "index.html",
        IndexTemplate {
            title: "sorter archive",
            css: SITE_CSS,
            top1,
            top2,
            user_count: users.len(),
            item_count: items.len(),
            vote_count: votes.len(),
            tag_count: tags.len(),
            tags: tag_entries.clone(),
            top_users: tagged_users.iter().take(20).cloned().collect(),
            hist_days,
            peak_day: peak.0,
            peak_count: peak.1,
        }
        .render()
        .unwrap(),
    );

    write(
        "rankings.html",
        RankingsTemplate {
            title: "rankings — sorter archive",
            css: SITE_CSS,
            item_count: items.len(),
            rows: ranked_items_for_web
                .iter()
                .take(100)
                .enumerate()
                .map(|(idx, (s, i))| ScoredItem {
                    rank: idx + 1,
                    title: i.title.clone(),
                    owner: i.owner.clone(),
                    score: format!("{:.4}", s),
                })
                .collect(),
        }
        .render()
        .unwrap(),
    );

    write(
        "tags.html",
        TagsTemplate {
            title: "categories — sorter archive",
            css: SITE_CSS,
            rows: tag_entries.clone(),
        }
        .render()
        .unwrap(),
    );
    for t in &tags {
        let mut its: Vec<&ItemRow> = tag2items
            .get(&t.id)
            .map(|ids| ids.iter().filter_map(|id| item_by_id.get(id)).copied().collect())
            .unwrap_or_default();
        its.sort_by(|a, b| {
            let ra = global_rank.get(&a.id).copied().unwrap_or(usize::MAX);
            let rb = global_rank.get(&b.id).copied().unwrap_or(usize::MAX);
            ra.cmp(&rb)
        });
        let rows: Vec<ScoredItem> = its
            .iter()
            .take(200)
            .map(|i| ScoredItem {
                rank: global_rank.get(&i.id).copied().unwrap_or(0),
                title: i.title.clone(),
                owner: i.owner.clone(),
                score: String::new(),
            })
            .collect();
        write(
            &format!("tags/{}.html", tag_page.get(&t.id).unwrap()),
            TagTemplate {
                title: &format!("{} — sorter archive", t.title),
                css: SITE_CSS,
                tag_title: t.title.clone(),
                item_count: rows.len(),
                rows,
            }
            .render()
            .unwrap(),
        );
    }

    write(
        "users.html",
        UsersTemplate {
            title: "users — sorter archive",
            css: SITE_CSS,
            rows: tagged_users.clone(),
        }
        .render()
        .unwrap(),
    );
    for u in &users {
        let mut mine: Vec<&ItemRow> =
            items.iter().filter(|i| i.owner == u.name).collect();
        mine.sort_by(|a, b| {
            let ra = global_rank.get(&a.id).copied().unwrap_or(usize::MAX);
            let rb = global_rank.get(&b.id).copied().unwrap_or(usize::MAX);
            ra.cmp(&rb)
        });
        let rows: Vec<UserItem> = mine
            .iter()
            .take(100)
            .map(|i| UserItem {
                rank: global_rank.get(&i.id).copied().unwrap_or(0),
                title: i.title.clone(),
            })
            .collect();
        write(
            &format!("users/{}.html", user_page.get(&u.name).unwrap()),
            UserTemplate {
                title: &format!("{} — sorter archive", u.name),
                css: SITE_CSS,
                user_name: u.name.clone(),
                votes: *votes_by_user.get(&u.name).unwrap_or(&0),
                items: mine.len(),
                rows,
            }
            .render()
            .unwrap(),
        );
    }

    let recent: Vec<VoteView> = votes
        .iter()
        .rev()
        .take(200)
        .map(|v| VoteView {
            when: v.created_at.clone(),
            voter: v.owner.clone(),
            left: item_by_id
                .get(&v.left_item_id)
                .map(|i| i.title.clone())
                .unwrap_or_default(),
            right: item_by_id
                .get(&v.right_item_id)
                .map(|i| i.title.clone())
                .unwrap_or_default(),
            magnitude: v.magnitude,
        })
        .collect();
    write(
        "votes.html",
        VotesTemplate {
            title: "votes — sorter archive",
            css: SITE_CSS,
            vote_count: votes.len(),
            rows: recent,
        }
        .render()
        .unwrap(),
    );

    println!(
        "wrote {}: users={} items={} votes={} tags={}",
        out.display(),
        users.len(),
        items.len(),
        votes.len(),
        tags.len()
    );
}
