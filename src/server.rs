use std::collections::HashMap;
use std::net::SocketAddr;

use askama::Template;
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::Html,
    routing::get,
    Router,
};
use durable::Db;

use crate::{
    archive_store::{
        ArchiveStore, ArchiveStoreFields, ItemData, StoredVoteV1, TagData,
    },
    path_types::ItemId,
    ranking,
    reducer::{ScopeVotes, VoteData},
};

const VOTES_PER_PAGE: usize = 100;

#[derive(Clone)]
pub struct AppState {
    pub db: Db,
    pub score_of: HashMap<String, f64>,
    pub items: HashMap<String, ItemData>,
    pub tags: HashMap<String, TagData>,
    pub item2tags: HashMap<String, Vec<String>>, // item id -> tag ids
    pub tag2items: HashMap<String, Vec<String>>, // tag id -> item ids
    pub owner2items: HashMap<String, Vec<String>>,
    pub users: Vec<String>,
}

fn human_day(ts: i64) -> String {
    // v1 epoch seconds; "x years ago" with the absolute date in `title`
    let secs = i64::max(0, ts);
    if secs <= 0 {
        return String::new();
    }
    let dt = chrono::DateTime::from_timestamp(secs, 0).unwrap();
    let age = chrono::offset::Utc::now() - dt;
    timeago::Formatter::new().convert(std::time::Duration::from_millis(age.num_milliseconds().max(0) as u64))
}

fn magnitude_bar(pct: i32) -> String {
    // center axis: left half takes pct% of the track, right half the rest
    let p = pct.clamp(0, 100);
    let left = format!("width:{p}%");
    let right = format!("width:{}%", 100 - p);
    format!(
        r#"<span class="slide" aria-label="{p} percent left"><span class="fill l" style="{left}"></span><span class="fill r" style="{right}"></span></span>"#
    )
}

#[derive(serde::Deserialize)]
pub struct PageQuery {
    pub page: Option<usize>,
}

fn load_scores(db: &Db) -> (HashMap<String, f64>, Vec<String>) {
    let root = ArchiveStore::root();
    let mut scope = ScopeVotes::default();
    for (key, v) in root.uuid_votes().iter(db).expect("scan uuid_votes") {
        let stored: StoredVoteV1 = v;
        let vote = VoteData {
            ts: stored.ts,
            a: ItemId::opaque(stored.a.clone()),
            b: ItemId::opaque(stored.b.clone()),
            ratio_left: stored.ratio_left,
            ratio_right: stored.ratio_right,
            pseudonym: stored.pseudonym.clone(),
            trust_weight: stored.trust_weight,
        };
        scope.apply_vote(vote, &stored.pseudonym);
        let _ = key;
    }
    let (comps, _isolates, ids) = ranking::scope_components(&scope);
    let largest: Vec<usize> = comps.into_iter().max_by_key(|c| c.len()).unwrap_or_default();
    let ranked = ranking::ranked_items_subset(&scope, &largest, ranking::MAX_ITERS, ranking::TOL);
    let mut map = HashMap::new();
    for r in &ranked {
        map.insert(r.item.as_str().to_string(), r.score);
    }
    let _ = ids;
    let order: Vec<String> = ranked.iter().map(|r| r.item.as_str().to_string()).collect();
    (map, order)
}

pub fn build_state(db: Db) -> AppState {
    let (score_of, _order) = load_scores(&db);
    let root = ArchiveStore::root();
    let items = root
        .items()
        .iter(&db)
        .expect("scan items")
        .into_iter()
        .map(|(k, v)| (k, v))
        .collect::<HashMap<_, _>>();
    let tags = root
        .tags()
        .iter(&db)
        .expect("scan tags")
        .into_iter()
        .map(|(k, v)| (k, v))
        .collect::<HashMap<_, _>>();
    let mut item2tags: HashMap<String, Vec<String>> = HashMap::new();
    let mut tag2items: HashMap<String, Vec<String>> = HashMap::new();
    for tag in root
        .membership()
        .keys(&db)
        .expect("scan membership")
    {
        let mut v = Vec::new();
        let inner = root.membership().key(&tag);
        for item in inner.keys(&db).expect("scan membership inner") {
            v.push(item.clone());
            item2tags.entry(item.clone()).or_default().push(tag.clone());
        }
        v.sort();
        tag2items.insert(tag, v);
    }
    let mut owner2items: HashMap<String, Vec<String>> = HashMap::new();
    for owner in root.by_owner().keys(&db).expect("scan by_owner") {
        let mut v = Vec::new();
        for item in root.by_owner().key(&owner).keys(&db).expect("scan by_owner inner") {
            v.push(item.clone());
        }
        v.sort();
        owner2items.insert(owner, v);
    }
    let users = root
        .users()
        .keys(&db)
        .expect("scan users");
    AppState { db, score_of, items, tags, item2tags, tag2items, owner2items, users }
}

#[derive(Debug, Clone)]
struct TaggedRow {
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
    rank: String,
    title: String,
    owner: String,
    page: String,
    score: String,
}

#[derive(Debug, Clone)]
struct UserItemView {
    rank: String,
    title: String,
}

#[derive(Debug, Clone)]
struct VoteView {
    when: String,
    when_title: String,
    voter: String,
    voter_slug: String,
    left: String,
    right: String,
    left_bar: String,
    right_bar: String,
    magnitude: i32,
}

#[derive(Debug, Clone)]
struct DayBar {
    day: String,
    count: usize,
    height: usize,
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
    top_users: Vec<TaggedRow>,
    hist_days: Vec<DayBar>,
    peak_day: String,
    peak_count: usize,
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
    rows: Vec<TaggedRow>,
}

#[derive(Template)]
#[template(path = "user.html")]
struct UserTemplate<'a> {
    title: &'a str,
    css: &'a str,
    user_name: String,
    votes: usize,
    items: usize,
    rows: Vec<UserItemView>,
}

#[derive(Template)]
#[template(path = "votes.html")]
struct VotesTemplate<'a> {
    title: &'a str,
    css: &'a str,
    vote_count: usize,
    page: usize,
    pages: usize,
    rows: Vec<VoteView>,
}

async fn index(State(state): State<AppState>) -> Result<Html<String>, StatusCode> {
    let mut ranked: Vec<(&String, &f64)> = state.score_of.iter().collect();
    ranked.sort_by(|a, b| b.1.partial_cmp(a.1).unwrap());
    let score_item = |id: &String, s: &f64| ScoredItem {
        rank: String::new(),
        title: state
            .items
            .get(id)
            .map(|i| i.title.clone())
            .unwrap_or_else(|| id.clone()),
        owner: state
            .items
            .get(id)
            .map(|i| i.owner.clone())
            .unwrap_or_default(),
        page: state
            .items
            .get(id)
            .map(|i| safe_slug(&i.owner))
            .unwrap_or_default(),
        score: format!("{:.4}", s),
    };
    let (top1, top2) = match ranked.len() {
        0 => (
            ScoredItem { rank: String::new(), title: "—".into(), owner: String::new(), page: String::new(), score: "—".into() },
            ScoredItem { rank: String::new(), title: "—".into(), owner: String::new(), page: String::new(), score: "—".into() },
        ),
        1 => (score_item(ranked[0].0, ranked[0].1), ScoredItem { rank: String::new(), title: "—".into(), owner: String::new(), page: String::new(), score: "—".into() }),
        _ => (score_item(ranked[0].0, ranked[0].1), score_item(ranked[1].0, ranked[1].1)),
    };
    let tags: Vec<TagEntry> = {
        let mut v: Vec<TagEntry> = state
            .tags
            .values()
            .map(|t| TagEntry {
                slug: t.slug.clone(),
                page: safe_slug(&t.slug),
                title: t.title.clone(),
                items: state.tag2items.get(&t.id).map(|x| x.len()).unwrap_or(0),
                owner: t.owner.clone(),
            })
            .collect();
        v.sort_by(|a, b| b.items.cmp(&a.items));
        v
    };
    let top_users: Vec<TaggedRow> = {
        let root = ArchiveStore::root();
        let mut v: Vec<TaggedRow> = state
            .users
            .iter()
            .map(|name| TaggedRow {
                page: safe_slug(name),
                items: state.owner2items.get(name).map(|x| x.len()).unwrap_or(0),
                votes: recent_count(&state.db, name),
                name: name.clone(),
            })
            .collect();
        v.sort_by(|a, b| b.votes.cmp(&a.votes));
        v.truncate(20);
        v
    };
    let hist_days = histogram(&state.db);
    let peak = hist_days
        .iter()
        .max_by_key(|d| d.count)
        .cloned()
        .unwrap_or(DayBar { day: String::new(), count: 0, height: 0 });
    let t = IndexTemplate {
        title: "sorter archive",
        css: crate::SITE_CSS,
        top1,
        top2,
        user_count: state.users.len(),
        item_count: state.items.len(),
        vote_count: uuid_vote_count(&state.db),
        tag_count: state.tags.len(),
        tags,
        top_users,
        hist_days,
        peak_day: peak.day.clone(),
        peak_count: peak.count,
    };
    Ok(Html(t.render().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?))
}

fn recent_count(db: &Db, user: &str) -> usize {
    ArchiveStore::root()
        .recent_votes()
        .key(&user.to_string())
        .len(db)
        .unwrap_or(0) as usize
}

fn uuid_vote_count(db: &Db) -> usize {
    ArchiveStore::root()
        .uuid_votes()
        .keys(db)
        .unwrap_or_default()
        .len()
}

fn recent_votes_all(db: &Db) -> Vec<StoredVoteV1> {
    let root = ArchiveStore::root();
    let mut all: Vec<StoredVoteV1> = Vec::new();
    for actor in root.recent_votes().keys(db).unwrap_or_default() {
        for v in root
            .recent_votes()
            .key(&actor)
            .iter(db)
            .unwrap_or_default()
        {
            all.push(v);
        }
    }
    all
}

fn histogram(db: &Db) -> Vec<DayBar> {
    let mut per_day: Vec<(String, usize)> = Vec::new();
    for v in recent_votes_all(db) {
        let day = {
            let dt = chrono::DateTime::from_timestamp(v.ts, 0).unwrap();
            dt.format("%Y-%m-%d").to_string()
        };
        if let Some(last) = per_day.last_mut() {
            if last.0 == day {
                last.1 += 1;
                continue;
            }
        }
        per_day.push((day, 1));
    }
    let shown = &per_day[per_day.len().saturating_sub(60)..];
    let max_count = shown.iter().map(|(_, c)| *c).max().unwrap_or(1);
    shown
        .iter()
        .map(|(d, c)| DayBar {
            day: d.clone(),
            count: *c,
            height: (100 * c / max_count) as usize,
        })
        .collect()
}

fn safe_slug(name: &str) -> String {
    name.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '-'
            }
        })
        .collect::<String>()
        .to_lowercase()
}

pub fn app(state: AppState) -> Router {
    Router::new()
        .route("/", get(index))
        .route("/rankings", get(rankings))
        .route("/tags", get(tags_index))
        .route("/tags/{slug}", get(tag_page))
        .route("/users", get(users_index))
        .route("/users/{name}", get(user_page))
        .route("/votes", get(votes_page))
        .with_state(state)
}

async fn rankings(State(state): State<AppState>) -> Result<Html<String>, StatusCode> {
    let mut ranked: Vec<(&String, &f64)> = state.score_of.iter().collect();
    ranked.sort_by(|a, b| b.1.partial_cmp(a.1).unwrap());
    ranked.truncate(100);
    let rows = ranked
        .into_iter()
        .map(|(id, s)| {
            let item = state.items.get(id);
            ScoredItem {
                rank: String::new(),
                title: item.map(|i| i.title.clone()).unwrap_or_else(|| id.clone()),
                owner: item.map(|i| i.owner.clone()).unwrap_or_default(),
                page: item.map(|i| safe_slug(&i.owner)).unwrap_or_default(),
                score: format!("{:.4}", s),
            }
        })
        .collect();
    let t = RankingsTemplate {
        title: "rankings — sorter archive",
        css: crate::SITE_CSS,
        item_count: state.items.len(),
        rows,
    };
    Ok(Html(t.render().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?))
}

async fn tags_index(State(state): State<AppState>) -> Result<Html<String>, StatusCode> {
    let mut rows: Vec<TagEntry> = state
        .tags
        .values()
        .map(|t| TagEntry {
            slug: t.slug.clone(),
            page: safe_slug(&t.slug),
            title: t.title.clone(),
            items: state.tag2items.get(&t.id).map(|x| x.len()).unwrap_or(0),
            owner: t.owner.clone(),
        })
        .collect();
    rows.sort_by(|a, b| b.items.cmp(&a.items));
    let t = TagsTemplate {
        title: "categories — sorter archive",
        css: crate::SITE_CSS,
        rows,
    };
    Ok(Html(t.render().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?))
}

async fn tag_page(
    State(state): State<AppState>,
    Path(slug): Path<String>,
) -> Result<Html<String>, StatusCode> {
    let tag = state
        .tags
        .values()
        .find(|t| safe_slug(&t.slug) == slug)
        .cloned()
        .ok_or(StatusCode::NOT_FOUND)?;
    let mut ids = state.tag2items.get(&tag.id).cloned().unwrap_or_default();
    ids.sort_by(|a, b| {
        let sa = state.score_of.get(a).copied().unwrap_or(0.0);
        let sb = state.score_of.get(b).copied().unwrap_or(0.0);
        sb.partial_cmp(&sa).unwrap()
    });
    let rows: Vec<ScoredItem> = ids
        .iter()
        .take(200)
        .map(|id| {
            let item = state.items.get(id);
            let s = state.score_of.get(id).copied().unwrap_or(0.0);
            ScoredItem {
                rank: String::new(),
                title: item.map(|i| i.title.clone()).unwrap_or_else(|| id.clone()),
                owner: item.map(|i| i.owner.clone()).unwrap_or_default(),
                page: item.map(|i| safe_slug(&i.owner)).unwrap_or_default(),
                score: format!("{:.4}", s),
            }
        })
        .collect();
    let t = TagTemplate {
        title: &format!("{} — sorter archive", tag.title),
        css: crate::SITE_CSS,
        tag_title: tag.title.clone(),
        item_count: rows.len(),
        rows,
    };
    Ok(Html(t.render().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?))
}

async fn users_index(State(state): State<AppState>) -> Result<Html<String>, StatusCode> {
    let root = ArchiveStore::root();
    let mut rows: Vec<TaggedRow> = state
        .users
        .iter()
        .map(|name| TaggedRow {
            page: safe_slug(name),
            items: state.owner2items.get(name).map(|x| x.len()).unwrap_or(0),
            votes: recent_count(&state.db, name),
            name: name.clone(),
        })
        .collect();
    rows.sort_by(|a, b| b.votes.cmp(&a.votes));
    let t = UsersTemplate {
        title: "users — sorter archive",
        css: crate::SITE_CSS,
        rows,
    };
    Ok(Html(t.render().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?))
}

async fn user_page(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<Html<String>, StatusCode> {
    let real = state
        .users
        .iter()
        .find(|u| safe_slug(u) == name)
        .cloned()
        .ok_or(StatusCode::NOT_FOUND)?;
    let mut ids = state.owner2items.get(&real).cloned().unwrap_or_default();
    ids.sort_by(|a, b| {
        let sa = state.score_of.get(a).copied().unwrap_or(0.0);
        let sb = state.score_of.get(b).copied().unwrap_or(0.0);
        sb.partial_cmp(&sa).unwrap()
    });
    let rows: Vec<UserItemView> = ids
        .iter()
        .take(100)
        .map(|id| UserItemView {
            rank: {
                let s = state.score_of.get(id).copied().unwrap_or(0.0);
                format!("{:.4}", s)
            },
            title: state
                .items
                .get(id)
                .map(|i| i.title.clone())
                .unwrap_or_else(|| id.clone()),
        })
        .collect();
    let root = ArchiveStore::root();
    let t = UserTemplate {
        title: &format!("{} — sorter archive", real),
        css: crate::SITE_CSS,
        user_name: real.clone(),
        votes: recent_count(&state.db, &real),
        items: ids.len(),
        rows,
    };
    Ok(Html(t.render().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?))
}

async fn votes_page(
    State(state): State<AppState>,
    Query(query): Query<PageQuery>,
) -> Result<Html<String>, StatusCode> {
    let page = query.page.unwrap_or(1).max(1);
    let mut all = recent_votes_all(&state.db);
    all.sort_by_key(|v| v.ts);
    let total = all.len();
    let pages = (total + VOTES_PER_PAGE - 1) / VOTES_PER_PAGE;
    let page = page.min(pages.max(1));
    let start = (page - 1) * VOTES_PER_PAGE;
    let slice = all[start..(start + VOTES_PER_PAGE).min(total)].to_vec();
    let rows: Vec<VoteView> = slice
        .iter()
        .map(|v| {
            let title = |id: &str| {
                state
                    .items
                    .get(id)
                    .map(|i| i.title.clone())
                    .unwrap_or_else(|| id.to_string())
            };
            VoteView {
                when: human_day(v.ts),
                when_title: {
                    let dt = chrono::DateTime::from_timestamp(v.ts, 0).unwrap();
                    dt.format("%Y-%m-%d %H:%M").to_string()
                },
                voter: v.pseudonym.clone(),
                voter_slug: safe_slug(&v.pseudonym),
                left: title(&v.a),
                right: title(&v.b),
                left_bar: magnitude_bar(v.ratio_left),
                right_bar: magnitude_bar(v.ratio_right),
                magnitude: v.ratio_left,
            }
        })
        .collect();
    let t = VotesTemplate {
        title: "votes — sorter archive",
        css: crate::SITE_CSS,
        vote_count: total,
        page,
        pages,
        rows,
    };
    Ok(Html(t.render().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?))
}

pub fn listen_addr() -> SocketAddr {
    let port = std::env::var("PORT")
        .ok()
        .and_then(|p| p.parse::<u16>().ok())
        .unwrap_or(8080);
    SocketAddr::from(([0, 0, 0, 0], port))
}
