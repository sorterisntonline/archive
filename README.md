# sorter archive

Static-to-dynamic index of sorter's public data — categories, users,
votes, and rank-centrality scores — served by a small axum app from a
durable (redb) projection of the production dump.

The ranking code is a verbatim copy of sorter2's (`src/ranking.rs`,
`src/reducer.rs`) and only the largest connected component of the vote
graph receives a consensus score; everything outside it is unranked.

Data: restore the dump in
[`db_dump`](https://github.com/sorterisntonline/db_dump) into Postgres,
then run:

```sh
ARCHIVE_IMPORT=1 cargo run --bin archive   # only once
cargo run --bin archive
```

Serve: axum + askama templates on `:8080`. Deploy: push to `main`;
GitHub Actions runs `flyctl deploy` on `shared-cpu-1x` 256MB.
