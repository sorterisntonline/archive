#!/usr/bin/env python3
"""Static archive generator: reads the sorter PRODUCTION dump (restored in a
scratch Postgres) and emits a pure-HTML site into public/.

Ranking: faithful port of the random-walk scoring in
~/programming/sorter2/server/src/ranking.rs (compute_scores_from_edges):
pairwise-normalize edges on each vote (magnitude 0-100 -> share), then walk
with stay-probability (d_max - w)/d_max until convergence, renormalize.
"""
import html
import os
import sys
from collections import defaultdict

import psycopg2
import psycopg2.extras

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
OUT = os.path.join(ROOT, "public")


def fetch():
    url = os.environ.get("DATABASE_URL", "postgres://postgres:temp@localhost:15455/sorter")
    conn = psycopg2.connect(url)
    cur = conn.cursor(cursor_factory=psycopg2.extras.RealDictCursor)
    cur.execute("SELECT * FROM users ORDER BY created_at")
    users = cur.fetchall()
    cur.execute("SELECT * FROM items ORDER BY created_at")
    items = cur.fetchall()
    cur.execute("SELECT * FROM tags ORDER BY title")
    tags = cur.fetchall()
    cur.execute("SELECT * FROM items_in_tags")
    item_tags = cur.fetchall()
    cur.execute("SELECT * FROM votes ORDER BY created_at")
    votes = cur.fetchall()
    conn.close()
    return users, items, tags, item_tags, votes


def compute_scores(votes, item_ids):
    """Port of sorter2 ranking.rs compute_scores_from_edges."""
    index = {iid: i for i, iid in enumerate(item_ids)}
    n = len(item_ids)
    raw = defaultdict(float)
    for v in votes:
        li, ri, m = v["left_item_id"], v["right_item_id"], v["magnitude"]
        if li not in index or ri not in index:
            continue
        m = max(0, min(100, m))
        # sorter2 convention: winner takes its ratio share, loser the rest.
        # magnitude = left's share; edges flow loser -> winner.
        w_left = m / 100.0
        w_right = 1.0 - w_left
        a, b = index[li], index[ri]
        if w_left > 0:
            raw[(b, a)] += w_left
        if w_right > 0:
            raw[(a, b)] += w_right
    # Pairwise-normalize (each pair's shares sum to 1).
    normalized = {}
    for (i, j) in list(raw.keys()):
        if (i, j) in normalized:
            continue
        w_ij = raw.get((i, j), 0.0)
        w_ji = raw.get((j, i), 0.0)
        total = w_ij + w_ji
        if total <= 0:
            continue
        normalized[(i, j)] = w_ij / total
        if w_ji > 0:
            normalized[(j, i)] = w_ji / total
    out_edges = [[] for _ in range(n)]
    neighbors = [set() for _ in range(n)]
    for (src, dst), w in normalized.items():
        out_edges[src].append((dst, w))
        neighbors[src].add(dst)
        neighbors[dst].add(src)
    d_max = max((len(s) for s in neighbors), default=0)
    if d_max == 0:
        return [1.0 / n] * n
    scores = [1.0 / n] * n
    nxt = [0.0] * n
    for _ in range(10_000):
        for i in range(n):
            ws = sum(w for _, w in out_edges[i])
            stay = (d_max - ws) / d_max
            nxt[i] += scores[i] * stay
            for dst, w in out_edges[i]:
                nxt[dst] += scores[i] * (w / d_max)
        diff = sum(abs(a - b) for a, b in zip(scores, nxt))
        scores, nxt = nxt, [0.0] * n
        if diff < 1e-8:
            break
    s = sum(scores)
    if s > 0:
        scores = [x / s for x in scores]
    return scores


CSS = """
:root{--paper:#fdfdfc;--ink:#101216;--mut:#5c6068;--acc:#2b4eff;--line:#e2e4e8;--win:#0e8f66}
*{box-sizing:border-box}
html{color-scheme:light}
body{margin:0;background:var(--paper);color:var(--ink);font:400 17px/1.55 -apple-system,"Segoe UI",Helvetica,Arial,sans-serif;-webkit-font-smoothing:antialiased}
a{color:var(--acc);text-decoration:none}a:hover{text-decoration:underline}
.wrap{max-width:1060px;margin:0 auto;padding:28px 22px 90px}
nav{display:flex;gap:18px;border-bottom:1px solid var(--line);padding:6px 0 14px;font-size:15px}
nav b{font-weight:700}
nav span{margin-left:auto;color:var(--mut);font-size:13px}
h1,h2,h3{font-family:Iowan Old Style,Palatino,Palatino Linotype,Palladio,Georgia,serif;font-weight:600;letter-spacing:-.01em;margin:0}
.hero{padding:52px 0 10px}
.hero .kicker{font-size:15px;color:var(--mut)}
.duet{display:flex;align-items:stretch;gap:0;margin-top:18px;border:1px solid var(--line)}
.duet .side{flex:1;padding:26px 28px}
.duet .side .rk{font-size:13px;color:var(--mut);font-variant-numeric:tabular-nums}
.duet .side .t{font-family:Iowan Old Style,Palatino,Georgia,serif;font-size:30px;line-height:1.2;margin:8px 0}
.duet .vs{flex:0 0 auto;align-self:center;padding:0 20px;font-family:Iowan Old Style,Georgia,serif;font-size:22px;color:var(--acc)}
table{width:100%;border-collapse:collapse;margin:10px 0 30px}
th{text-align:left;font-size:13px;color:var(--mut);font-weight:500;padding:8px 10px;border-bottom:1px solid var(--line)}
td{padding:9px 10px;border-bottom:1px solid var(--line);vertical-align:top}
.num{font-variant-numeric:tabular-nums;text-align:right}
.mut{color:var(--mut)}.small{font-size:14px}
h2.sec{margin:38px 0 6px;font-size:26px}
.lede{max-width:62ch;color:var(--mut);font-size:17px}
.stat{display:flex;gap:30px;flex-wrap:wrap;margin:22px 0 8px}
.stat b{font-family:Iowan Old Style,Georgia,serif;font-size:34px;font-weight:600;display:block}
.stat span{color:var(--mut);font-size:14px}
.hist{display:flex;align-items:flex-end;gap:3px;height:110px;margin:14px 0 4px}
.hist div{flex:1;background:var(--acc);min-width:2px;opacity:.75}
.hist div.hot{opacity:1;background:var(--win)}
.taglist{columns:2;margin:6px 0 20px}
.taglist li{margin:0 0 6px 18px}
footer{border-top:1px solid var(--line);margin-top:60px;padding-top:16px;color:var(--mut);font-size:14px}
@media(prefers-reduced-motion:reduce){*{transition:none!important}}
@media(max-width:640px){.duet{flex-direction:column}.taglist{columns:1}}
"""


def esc(x):
    return html.escape(str(x or ""), quote=True)


def page(title, body):
    return f"""<!DOCTYPE html>
<html lang="en"><head><meta charset="utf-8">
<title>{esc(title)}</title><meta name="viewport" content="width=device-width,initial-scale=1">
<style>{CSS}</style></head><body><div class="wrap">
<nav><a href="/"><b>sorter</b> archive</a><a href="/tags.html">categories</a><a href="/users.html">users</a><a href="/votes.html">votes</a><a href="/rankings.html">rankings</a><span>a static index, built from the production database</span></nav>
{body}
<footer>Generated from a production database dump. Scores computed with a rank-centrality random walk ported from sorter2.</footer>
</div></body></html>"""


def main():
    users, items, tags, item_tags, votes = fetch()
    os.makedirs(OUT, exist_ok=True)

    item_by_id = {i["id"]: i for i in items}
    user_by_name = {u["user_name"]: u for u in users}
    tag_by_id = {t["id"]: t for t in tags}

    # --- rankings (sorter2 random walk over the vote graph) ---
    item_ids = sorted(item_by_id)
    scores = compute_scores(votes, item_ids)
    ranked = sorted(
        ((scores[item_ids.index(i["id"])], i) for i in items),
        key=lambda x: -x[0],
    )
    rank_of = {i["id"]: r + 1 for r, (_, i) in enumerate(ranked)}

    # --- aggregates ---
    votes_by_user = defaultdict(int)
    for v in votes:
        votes_by_user[v["owner"]] += 1
    items_by_owner = defaultdict(list)
    for i in items:
        items_by_owner[i["owner"]].append(i)
    it2tags = defaultdict(list)
    tag2items = defaultdict(list)
    for it in item_tags:
        it2tags[it["item_id"]].append(it["tag_id"])
        tag2items[it["tag_id"]].append(it["item_id"])
    tag_counts = {tid: len(v) for tid, v in tag2items.items()}

    # --- daily vote histogram (last 60 days with votes) ---
    per_day = defaultdict(int)
    for v in votes:
        per_day[str(v["created_at"])[:10]] += 1
    days = sorted(per_day)
    top_day = max(per_day.values()) if per_day else 1
    hist = "".join(
        f'<div title="{esc(d)}: {per_day[d]}" style="height:{int(100*per_day[d]/top_day)}%"></div>'
        for d in days[-60:]
    )

    # --- index ---
    (s1, top1), (s2, top2) = ranked[0], ranked[1]
    tags_li = "".join(
        f'<li><a href="tags/{esc(t["slug"])}.html">{esc(t["title"])}</a> '
        f'<span class="mut small">({tag_counts.get(t["id"], 0)})</span></li>'
        for t in tags
    )
    users_rows = "".join(
        f'<tr><td><a href="users/{esc(u["user_name"])}.html">{esc(u["user_name"])}</a></td>'
        f'<td class="num">{votes_by_user.get(u["user_name"], 0)}</td>'
        f'<td class="num">{len(items_by_owner.get(u["user_name"], []))}</td></tr>'
        for u in sorted(
            users, key=lambda u: -votes_by_user.get(u["user_name"], 0)
        )[:20]
    )
    index = f"""
<h1 class="hero"><span class="kicker">Sorter's public record, frozen as a static index.</span></h1>
<div class="duet">
 <div class="side"><div class="rk">rank {rpos(ranked, top1)}</div><div class="t">{esc(top1['title'])}</div><div class="mut small">by {esc(top1['owner'])} · score {s1:.4f}</div></div>
 <div class="vs">vs</div>
 <div class="side"><div class="rk">rank {rpos(ranked, top2)}</div><div class="t">{esc(top2['title'])}</div><div class="mut small">by {esc(top2['owner'])} · score {s2:.4f}</div></div>
</div>
<p class="lede" style="margin-top:26px">Sorter was a site where people submitted things and settled which deserved to sit higher by voting head-to-head. This archive keeps the data: every category, voter, and vote, plus the consensus order computed by the site's own random walk. Nothing here loads a script; every page is written out in advance.</p>
<div class="stat">
 <div><b>{len(users)}</b><span>registered users</span></div>
 <div><b>{len(items)}</b><span>items posted</span></div>
 <div><b>{len(votes):,}</b><span>votes cast</span></div>
 <div><b>{len(tags)}</b><span>categories</span></div>
</div>
<h2 class="sec">Categories</h2>
<ul class="taglist">{tags_li}</ul>
<h2 class="sec">Most active voters</h2>
<table><tr><th>voter</th><th class="num">votes</th><th class="num">items posted</th></tr>{users_rows}</table>
<h2 class="sec">Votes by day</h2>
<div class="hist">{hist}</div>
<p class="mut small">One bar per day with votes, oldest to newest. Peak day: {esc(max(per_day, key=per_day.get) if per_day else '-')} ({top_day:,} votes).</p>
"""
    (open(os.path.join(OUT, "index.html"), "w").write(
        page("sorter archive", index)))

    # --- rankings ---
    rows = "".join(
        f'<tr><td class="num">{r}</td><td>{esc(i["title"])}</td>'
        f'<td class="mut">{esc(i["owner"])}</td><td class="num">{s:.4f}</td></tr>'
        for r, (s, i) in enumerate(ranked[:100], 1)
    )
    (open(os.path.join(OUT, "rankings.html"), "w").write(page(
        "rankings — sorter archive",
        f"<h2 class='sec'>Rankings</h2><p class='lede'>Top 100 of {len(items):,} items by "
        "the site's random-walk score (sorter2's algorithm, ported).</p>"
        f"<table><tr><th class='num'>rank</th><th>item</th><th>posted by</th>"
        f"<th class='num'>score</th></tr>{rows}</table>")))

    # --- categories index + tag pages ---
    cat_rows = "".join(
        f'<tr><td><a href="tags/{esc(t["slug"])}.html">{esc(t["title"])}</a></td>'
        f'<td class="num">{tag_counts.get(t["id"], 0)}</td>'
        f'<td class="mut">{esc(t["owner"])}</td></tr>'
        for t in tags
    )
    (open(os.path.join(OUT, "tags.html"), "w").write(page(
        "categories — sorter archive",
        f"<h2 class='sec'>Categories</h2><table><tr><th>category</th>"
        f"<th class='num'>items</th><th>owner</th></tr>{cat_rows}</table>")))
    for t in tags:
        its = [item_by_id[i] for i in tag2items.get(t["id"], [])
               if i in item_by_id]
        its.sort(key=lambda i: -scores[item_ids.index(i["id"])])
        trs = "".join(
            f'<tr><td class="num">{rank_of[i["id"]]}</td>'
            f'<td>{esc(i["title"])}</td><td class="mut">{esc(i["owner"])}</td></tr>'
            for i in its[:200])
        d = os.path.join(OUT, "tags")
        os.makedirs(d, exist_ok=True)
        (open(os.path.join(d, f"{t['slug']}.html"), "w").write(page(
            f"{t['title']} — sorter archive",
            f"<h2 class='sec'>{esc(t['title'])}</h2>"
            f"<p class='lede'>{len(its)} items, best first.</p>"
            f"<table><tr><th class='num'>rank</th><th>item</th>"
            f"<th>posted by</th></tr>{trs}</table>")))

    # --- users index + profiles ---
    urows = "".join(
        f'<tr><td><a href="users/{esc(u["user_name"])}.html">{esc(u["user_name"])}</a></td>'
        f'<td class="num">{votes_by_user.get(u["user_name"], 0)}</td>'
        f'<td class="num">{len(items_by_owner.get(u["user_name"], []))}</td></tr>'
        for u in sorted(users, key=lambda u: -votes_by_user.get(u["user_name"], 0)))
    (open(os.path.join(OUT, "users.html"), "w").write(page(
        "users — sorter archive",
        "<h2 class='sec'>Users</h2>"
        f"<table><tr><th>voter</th><th class='num'>votes</th>"
        f"<th class='num'>items posted</th></tr>{urows}</table>")))
    for u in users:
        name = u["user_name"]
        mine = sorted(items_by_owner.get(name, []),
                      key=lambda i: -scores[item_ids.index(i["id"])])
        trs = "".join(
            f'<tr><td class="num">{rank_of[i["id"]]}</td>'
            f'<td>{esc(i["title"])}</td></tr>' for i in mine[:100])
        d = os.path.join(OUT, "users")
        os.makedirs(d, exist_ok=True)
        (open(os.path.join(d, f"{name}.html"), "w").write(page(
            f"{name} — sorter archive",
            f"<h2 class='sec'>{esc(name)}</h2>"
            f"<p class='lede'>{votes_by_user.get(name, 0)} votes cast, "
            f"{len(mine)} items posted. Top items:</p>"
            f"<table><tr><th class='num'>rank</th><th>item</th></tr>{trs}</table>")))

    # --- votes ---
    recent = votes[-200:]
    vr = "".join(
        f'<tr><td>{esc(v["created_at"])[:16]}</td><td>{esc(v["owner"])}</td>'
        f'<td>{esc(item_by_id.get(v["left_item_id"], {}).get("title", ""))} vs '
        f'{esc(item_by_id.get(v["right_item_id"], {}).get("title", ""))}</td>'
        f'<td class="num">{v["magnitude"]}</td></tr>' for v in recent)
    (open(os.path.join(OUT, "votes.html"), "w").write(page(
        "votes — sorter archive",
        f"<h2 class='sec'>Votes</h2><p class='lede'>{len(votes):,} votes total; "
        "the 200 most recent:</p>"
        f"<table><tr><th>when</th><th>voter</th><th>pair</th>"
        f"<th class='num'>magnitude</th></tr>{vr}</table>")))

    print(f"wrote {OUT}: users={len(users)} items={len(items)} "
          f"votes={len(votes)} tags={len(tags)}")


def rpos(ranked, item):
    for r, (_, i) in enumerate(ranked, 1):
        if i["id"] == item["id"]:
            return r
    return "-"


if __name__ == "__main__":
    main()
