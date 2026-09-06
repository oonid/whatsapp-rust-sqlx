# WhatsApp-Rust

Rust implementation of the WhatsApp protocol: QR pairing, E2E encrypted messaging (1-on-1 + group), media, VoIP, connection management.

Ground truth for protocol behavior is WhatsApp Web itself: query the structured [whatspec](https://github.com/oxidezap/whatspec) IR first, drop to the raw bundle in `docs/captured-js/` when it can't answer, and treat **whatsmeow** (Go) and **Baileys** (TypeScript) as second opinions. See `agent_docs/wa_web_reference.md`.

## Crates

- **wacore** — platform-agnostic core: binary protocol, crypto, IQ types, state traits. Also builds for wasm32 and ESP32, so no Tokio here.
- **waproto** — prost-generated protobufs from `whatsapp.proto`. No feature logic.
- **whatsapp-rust** — Tokio runtime, SQLite persistence (Diesel), high-level API.
- **whatspec-codegen** (`tools/`) — build tooling, never published and outside `default-members`. Regenerates every whatspec-derived file in one pass from a pinned IR commit. Nothing links it.

## Build & verify

```bash
cargo fmt --all
cargo nextest run -p <touched crate> --lib              # fast local loop
cargo clippy --workspace --all-targets -- -D warnings   # what CI enforces
```

CI runs tests through [cargo-nextest](https://nexte.st) (`--profile ci`, config in `.config/nextest.toml`); install it from a [pre-built binary](https://nexte.st/docs/installation/pre-built-binaries/) to reproduce a CI failure locally. `cargo test` still works — with one gap in the other direction: nextest cannot run **doctests**, so CI runs `cargo test --doc` as its own step and a doc example you add is only covered there.

Workspace clippy takes minutes — pushing and letting CI parallelize the matrix is usually faster. E2E tests (`cargo nextest run --profile e2e -p e2e-tests`) need the mock server running; see `agent_docs/e2e_testing.md`.

Touching `unsafe` — the `Yokeable`/`StableDeref` impls in `wacore-binary`'s `node.rs`, the `set_len` in `zlib_pool.rs` — means CI's Miri gate (`.github/workflows/miri.yml`) is what proves it, since neither clippy nor a native test observes an aliasing violation or an uninit read. Locally: `rustup component add miri rust-src && cargo miri test -p wacore-binary --lib`. Interpretation is ~100× native, so a fixture that only makes sense at hundreds of KB (zlib window refill, buffer growth) belongs behind `#[cfg_attr(miri, ignore)]` with a small twin that keeps the `unsafe` covered.

## Gotchas

Things that look correct and are not:

- **Device state.** Never mutate `Device` directly, not even in tests — a write-lock mutation bypasses the cached snapshot. Mutate through `DeviceCommand` + `PersistenceManager::process_command()`; read through `get_device_snapshot()`, which returns a cached `Arc<Device>` (sync, refcount-cheap, safe per message) — hold it and borrow fields instead of cloning them. `get_device_arc()` exists only for store adapters that need `&mut Device` trait access.
- **Locks.** `session_locks` serializes Signal encrypt/decrypt per protocol address; `chat_lanes` (`ChatLane::enqueue_lock` in `src/client.rs`) serializes *incoming* processing per chat. Outgoing sends are deliberately not per-chat locked — WA Web doesn't lock them either.
- **Wire-tagged enums.** Every protocol enum derives `WireEnum`, and its `#[wire = ...]` attribute is the single source of truth for the wire value. Do not also derive `serde::Serialize`/`Deserialize` or add `#[serde(rename_all)]` — the derive owns both. In tagged mode it generates a sibling `<Name>Tag`; parsers must dispatch on `<Name>Tag::try_from(node.tag.as_ref())` rather than string literals, so renaming a tag stays a one-attribute change. Modes and attributes: `agent_docs/protocol_architecture.md`.
- **Event payloads are a frozen API.** Sealed with `#[non_exhaustive]` + `#[derive(bon::Builder)]` and constructed via `Type::builder()…build()`; a maybe-absent field is `Option<T>`, never an empty-string or zero sentinel. The full stability policy is the `Event` doc comment in `wacore/src/types/events.rs`.
- **Generated files are generated, not edited.** `wacore/src/iq/abprops.rs`, `wacore/src/iq/mex_operations.rs`, `wacore/appstate/src/schemas.rs`, `wacore/src/types/wire_enums.rs`, `wacore/src/iq/targets.rs`, `wacore/src/stanza/wire_tags.rs`, `wacore/binary/src/tokens.json`, `waproto/src/whatsapp.proto` and `wacore/src/version/generated.rs` all come out of `cargo run -p whatspec-codegen`, together, from one pinned whatspec commit. An action or flag the protocol carries but the bundle no longer builds goes in a hand-written sibling (`wacore/appstate/src/schemas_unlisted.rs`, `props::stale`), never in the generated file. `wire_enums.rs` binds only the catalog entries listed in the emitter's `WANTED`, because 88 of the 403 have a synthetic name and names repeat across modules; the variants themselves always come from the bundle. A candidate is found by its variant set but decided by its module: two enums agreeing on every value are not the same enum unless the module owns the wire format we parse. `targets.rs` binds the same way and covers `w:g2` only, the one namespace where a request's target is not implied by its namespace. `wire_tags.rs` takes its stanza tags from the union of the `notif`, `srvreq` and `stanza` documents, because the dispatcher table alone omits `iq` and `ack`, which this repository handles; it drops `privacy`, which is the type of an outgoing stanza and never arrives under that tag, so adding it would invite a handler that can never fire.
- **`whatsapp.proto` is not the whole persisted schema.** It comes from whatspec and is regenerated wholesale, so fields we persist but upstream does not declare live in `LOCAL_FIELDS` in `waproto/build.rs`, spliced into the descriptor at build time, and whole retained messages in `LOCAL_BLOCKS` in the codegen's proto emitter. Never hand-edit the `.proto` or `.desc` to add one — the next sync would drop it.
- **Blocking work** — `ureq`, heavy CPU — belongs in `tokio::task::spawn_blocking`; it shares a runtime with the read loop.
- **let-chains**, never nested `if let`. Clippy's `collapsible_if` is denied in CI.
- **No real PII in tests**, including vectors derived from production captures. Regenerate them from fictitious JIDs and numbers.
- **Errors**: `thiserror` for typed errors, `anyhow` where several failure kinds meet. No `.unwrap()` outside tests.

## Adding a feature

Find the wire format before designing anything — see `agent_docs/feature_implementation.md`. IQ requests go through `client.execute(Spec::new(&jid)).await?`, and `IqSpec` constructors take `&Jid` so callers need not clone. Public surface is `pub use` in `src/features/*.rs`, re-exported from `src/features/mod.rs` and `src/lib.rs`.

Comments carry the *why* of a decision, at the single point where it is made. Repeating a rationale at call sites is how it goes stale.

## Detailed docs

Read the one that covers what you are touching:

| Doc | Read it when |
| --- | --- |
| `agent_docs/wa_web_reference.md` | Confirming any protocol behavior, limit, enum value, or stanza shape against real WA Web |
| `agent_docs/protocol_architecture.md` | Building or parsing stanzas: `ProtocolNode`, `IqSpec`, derive macros, node helpers |
| `agent_docs/noise_handshake.md` | Connection setup: XX/IK/fallback selection, server cert cache, failure classification |
| `agent_docs/feature_implementation.md` | Starting a feature and needing its wire format from captured WA Web JS |
| `agent_docs/subsystem_boundary.md` | Adding a feature gate, adding a `Client` field only one subsystem reads, or proposing that a subsystem leave the core |
| `agent_docs/signal_durability.md` | Any code that reads, mutates, persists, or sends Signal state |
| `agent_docs/e2e_testing.md` | Writing or fixing tests under `tests/e2e/` |
| `agent_docs/observability.md` | Adding a cache, counter, or anything reported by `memory_report()` / `stats()` |
| `agent_docs/plugin_architecture.md` | Touching the `plugins` / `client-lifecycle` feature surface |
| `agent_docs/voip_audio_codecs.md` | VoIP media: codec profiles, negotiation, encoded audio API |
| `agent_docs/wam_telemetry.md` | WAM: the generated event catalog, the buffer codec, and what a client may honestly report |
| `agent_docs/binary_size_ci.md` | A size gate failed, or a change adds dependencies or generic instantiations |
| `agent_docs/build_flags.md` | Recommending codegen flags, or asked why a `target-feature` is not a default |
| `agent_docs/debugging.md` | Decoding raw binary-protocol bytes by hand |

## Fork-specific — oonid/whatsapp-rust-sqlx

This repo is a **fork of `oxidezap/whatsapp-rust`** carrying one feature upstream
does not have — a **PostgreSQL storage backend** — plus two send-path bug fixes.
It is vendored into the `sapa-rs` superproject at `vendor/whatsapp-rust-sqlx` and
consumed by the `wa` crate there.

### Branches and remotes

| Ref | Meaning |
|---|---|
| `postgres-storage` | **our branch.** Local commits rebased on top of an upstream commit |
| `main` | a mirror of upstream, kept for reference only — never develop here |
| `origin` | `git@github.com:oonid/whatsapp-rust-sqlx` (SSH) |
| `upstream` | `https://github.com/oxidezap/whatsapp-rust` |

**All `push` / `fetch` / `clone` belong to the user.** Agents do local git only —
commit, branch, worktree, rebase, merge. Present remote commands for the user to run.

### The local commits

Oldest first, on top of an upstream base:

```
chore: append fork-specific guidelines to AGENTS.md
fix(send): hash participant list in display format (Baileys-compat)
fix(send): TC token issuance causing perpetual 463 MissingTcToken
feat(postgres): rebuild postgres-storage for 0.7.0 (P2)
fix(postgres): drop stale SCHEMA_STMTS, repair test mocks
test(postgres): give the suite its own database and provision it once
feat(postgres): implement pending-inbound store/delete with batched variants (P3a)
feat(postgres): group metadata, batched mutation MACs, resource report (P3b/P3c)
docs: correct the TC-token patch note — upstream fixed half of it
<upstream base>
```

**The two `fix(send)` patches are the ones to watch on an upstream rebase** — upstream
keeps rewriting those exact files. Re-apply them by intent, not by blindly taking a side.
(They applied cleanly on the rebase onto `3cf0d648`; that is luck, not a guarantee, and
"applied cleanly" is not "still correct" — the checks below are what prove it.)

- **phash** (`wacore/src/messages.rs`) — `participant_list_hash` must hash JIDs in
  **Display** form (`write!(arena, "{jid}")`), not upstream's ad-format `:0`. WhatsApp
  Business clients reject the ad-format hash; Baileys uses Display form.
- **TC token** (`src/send/tctoken_lifecycle.rs`) — treat a **cold A/B-prop cache as
  enabled**. `PRIVACY_TOKEN_SENDING_ON_ALL_1_ON_1_MESSAGES` carries
  `default: AbDefault::Bool(false)`, `is_enabled` falls back to that default until the
  server pushes the prop, and the cstoken fallback does not rescue it because
  `WA_NCT_TOKEN_SEND_ENABLED` defaults `false` too — so a fresh start attaches no token
  at all and earns a perpetual `463 MissingTcToken`. Read the prop with `.get(..)` and
  default to `true` when absent; an explicit server `"0"` must still disable it.

  This patch **used to have a second half** — dropping an `is_self` guard that fired when
  the bot's LID collided with the admin's chat LID. **Upstream fixed that**: `is_own_identity`
  now compares namespaces via `is_same_chat_as` rather than `is_same_user_as`. Do not
  re-apply that half on a future rebase; check whether the guard is still user-equality
  before assuming it needs touching.

### The postgres backend

- **The schema is the `SCHEMA_STMTS: &[&str]` const** in
  `storages/postgres-storage/src/postgres_store.rs`, run idempotently at every startup
  and version-tracked in a `_wa_migrations` table. **A `migrations/*.sql` directory is
  never read** — adding files there is a silent no-op that fails at runtime.
  Add columns and tables with `CREATE ... IF NOT EXISTS` plus
  `ALTER TABLE ... ADD COLUMN IF NOT EXISTS`, because live databases (`wa_state`) are
  already provisioned. Bump the `_wa_migrations` version when you do.
- **Mirror `storages/sqlite-storage/`.** It is upstream's reference implementation and
  stays current; when the store traits change, read it first and follow its structure.
- **The postgres backend has its own suite: 63 tests** in
  `storages/postgres-storage/src/`, needing a live Postgres (see Build and test). The
  `wa` crate's suite in the sapa-rs superproject remains the integration gate; this one
  is the fast local check.
- **Two upstream fields are not yet persisted here**, and the sqlite backend does
  persist them: `CachedServerCertChain.signature_verified` (its proto field 3) and
  `HashState.bootstrapped` (its proto field 5). Ours decode to `false`, which is the
  safe direction upstream documents for rows written before a field existed — but it
  costs one extra XX handshake per restart, and a re-bootstrap for version-0
  collections. `wire::tests::server_cert_chain_does_not_yet_persist_signature_verified`
  asserts the gap so it cannot rot silently. Closing it means adding both fields to
  `storages/postgres-storage/proto/wire.proto` and regenerating the descriptor — which
  needs `protoc`, and needs a postgres arm added to
  `scripts/regenerate-wire-desc.sh`, which today only handles sqlite.

### Build and test

```bash
cargo check -p whatsapp-rust --features postgres-storage

# this backend's own suite — needs a live Postgres whose role can CREATE DATABASE
# (it provisions wa_pgstore_test itself)
TEST_DATABASE_URL=postgres://postgres:postgres@localhost:5434/postgres \
  cargo test -p whatsapp-rust-postgres-storage

# the two fork patches, specifically
cargo test -p wacore --lib parse_message_info_tests   # phash vectors
cargo test -p whatsapp-rust --lib tc_token            # cold A/B-prop cache

# from the sapa-rs superproject — the real gate for this backend
DATABASE_URL=postgres://postgres:postgres@localhost:5434/postgres cargo test -p wa
```

`cargo build --workspace` fails without ALSA system libraries — that is the `voip-cli`
example, unrelated to this backend. Build the two packages directly instead.

Postgres runs in the `sapa-rs-postgres` container on **5434** (not 5432). If host
connections start failing after long uptime (`ConnectionReset` / `PoolTimedOut`),
`docker restart sapa-rs-postgres` — the data persists.

Never kill services with `pkill -f <path>`; it matches the launching shell too. Use
`pkill -x <exact-binary-name>`.

### Conventions (Fork)
- Commit messages carry a detailed body explaining the motivation. **No `Co-Authored-By` trailer.**
