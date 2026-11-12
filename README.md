# sorter archive

A static index of sorter's public data — categories, users, votes, and
the consensus rankings — with no React, no backend, no JavaScript.

Scores come from the random-walk algorithm in
[`sorter2`'s ranking code](https://github.com/sorterisntonline/sorter2)
(ported to `scripts/generate.py`). Data is taken from the production
dump in [`db_dump`](https://github.com/sorterisntonline/db_dump).

Regenerate:

```sh
# restore the dump into a scratch postgres, then:
DATABASE_URL=postgres://... python3 scripts/generate.py
```

Deploy: push to `main`; GitHub Actions runs `flyctl deploy`.
Serve: `nginx:alpine` on a smallest Fly VM (`shared-cpu-1x`, 256MB).
