# whatsapp-rust-postgres-storage

PostgreSQL storage backend for `whatsapp-rust` using `sqlx 0.8`, replacing the default
SQLite/diesel backend.

## Why this exists

The default `whatsapp-rust` uses SQLite via `diesel`. When combined with `sqlx` (which also
pulls in `sqlx-sqlite`), both crates declare `links = "sqlite3"` — Cargo forbids two crates
with the same `links` key in one binary, producing a linker error at compile time.

This crate uses `sqlx 0.8` with PostgreSQL only (no `migrate` feature, which would re-introduce
`sqlx-sqlite`), eliminating the conflict entirely.

## Applying to the original repo

This work lives in a fork ([oonid/whatsapp-rust-sqlx](https://github.com/oonid/whatsapp-rust-sqlx)).
To carry the changes over to a fresh clone of the original repo:

**Create the patch** (run once, from this fork after committing):

```bash
# Single-commit patch with full metadata
git format-patch HEAD~1 --stdout > postgres-storage.patch
```

**Apply the patch** (run in the original repo):

```bash
# Verify it applies cleanly first (dry run)
git apply --check postgres-storage.patch

# Apply (stages all changes, does not commit)
git apply postgres-storage.patch

# Or apply and create the commit in one step (preserves author/message)
git am postgres-storage.patch
```

> `git apply` is a plain diff apply; `git am` additionally replays the commit message and
> authorship. Use `git am` if you want the history, `git apply` if you prefer to write your
> own commit message.

## Quick start

```bash
# 1. Start PostgreSQL
cd storages/postgres-storage
cp .env.example .env          # edit credentials if needed
source .env
docker compose up -d
cd ../..

# 2. Run the bot (QR pairing)
DATABASE_URL=postgres://wa:wa@localhost:5433/whatsapp \
  cargo run --no-default-features \
  --features postgres-storage,moka-cache,simd,tokio-transport,tokio-runtime,ureq-client,tokio-native,signal

# 3. Run tests
DATABASE_URL=postgres://wa:wa@localhost:5433/whatsapp \
  cargo test -p whatsapp-rust-postgres-storage
```

## Environment variables

| Variable | Default | Description |
|---|---|---|
| `DATABASE_URL` | `postgres://wa:wa@localhost:5433/whatsapp` | PostgreSQL connection URL |

## Files changed from upstream

| File | Change |
|---|---|
| `Cargo.toml` | Added `postgres-storage` feature flag and optional `whatsapp-rust-postgres-storage` dep |
| `src/store/mod.rs` | Re-exports `PostgresStore` under the `postgres-storage` feature |
| `src/main.rs` | Conditional backend init; reads `DATABASE_URL` env var |
| `storages/postgres-storage/` | New crate — full PostgreSQL backend (this directory) |
| `storages/postgres-storage/docker-compose.yml` | Postgres 17-alpine on port 5433 for local development |
| `storages/postgres-storage/.env.example` | Template for `DATABASE_URL` environment variable |

## Test coverage

Run with [`cargo-llvm-cov`](https://github.com/taiki-e/cargo-llvm-cov):

```bash
DATABASE_URL=postgres://wa:wa@localhost:5433/whatsapp \
  cargo llvm-cov --package whatsapp-rust-postgres-storage
```

Results (42 tests, `postgres_store.rs`):

| Metric | Covered | Missed | Total | % |
|---|---|---|---|---|
| Lines | 1354 | 112 | 1466 | **92.36%** |
| Regions | 2156 | 161 | 2317 | **93.05%** |
| Functions | 222 | 89 | 311 | 71.38% |

The function % is lower because LLVM counts each `|e| StoreError::Database(...)` error-handler
closure as a separate function — 48 of the 89 "missed functions" are those closures, which only
fire when the DB connection itself fails and are not reachable in integration tests. The
remaining missed lines are similarly unreachable without inserting corrupt data directly into
the database (wrong-length byte arrays, invalid bincode blobs).
