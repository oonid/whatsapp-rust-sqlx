pub static SCHEMA_STMTS: &[&str] = &[
    "CREATE TABLE IF NOT EXISTS _wa_migrations (
        id INTEGER NOT NULL PRIMARY KEY,
        applied_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
    )",
    "CREATE TABLE IF NOT EXISTS device (
        id INTEGER NOT NULL PRIMARY KEY,
        lid TEXT NOT NULL DEFAULT '',
        pn TEXT NOT NULL DEFAULT '',
        registration_id INTEGER NOT NULL DEFAULT 0,
        noise_key BYTEA NOT NULL,
        identity_key BYTEA NOT NULL,
        signed_pre_key BYTEA NOT NULL,
        signed_pre_key_id INTEGER NOT NULL DEFAULT 0,
        signed_pre_key_signature BYTEA NOT NULL,
        adv_secret_key BYTEA NOT NULL,
        account BYTEA,
        push_name TEXT NOT NULL DEFAULT '',
        app_version_primary INTEGER NOT NULL DEFAULT 0,
        app_version_secondary INTEGER NOT NULL DEFAULT 0,
        app_version_tertiary BIGINT NOT NULL DEFAULT 0,
        app_version_last_fetched_ms BIGINT NOT NULL DEFAULT 0,
        edge_routing_info BYTEA,
        props_hash TEXT,
        next_pre_key_id INTEGER NOT NULL DEFAULT 0,
        first_unupload_pre_key_id INTEGER NOT NULL DEFAULT 0,
        nct_salt BYTEA,
        server_has_prekeys BOOLEAN NOT NULL DEFAULT FALSE,
        server_cert_chain BYTEA,
        login_counter INTEGER NOT NULL DEFAULT 0,
        lid_migrated BOOLEAN NOT NULL DEFAULT FALSE,
        last_signed_pre_key_rotation_ms BIGINT NOT NULL DEFAULT 0,
        read_receipts_disabled BOOLEAN NOT NULL DEFAULT FALSE,
        server_client_expiration TEXT
    )",
    // Column names mirror storages/sqlite-storage/src/schema.rs deliberately: the two
    // backends are diffed against each other on every upstream sync, and a rename is a
    // gratuitous difference to re-read each time. `updated_at` is what makes the cache
    // ageable — without it a stale group snapshot is indistinguishable from a fresh one.
    "CREATE TABLE IF NOT EXISTS group_metadata (
        group_jid TEXT NOT NULL,
        info BYTEA NOT NULL,
        device_id INTEGER NOT NULL DEFAULT 1,
        updated_at BIGINT NOT NULL DEFAULT 0,
        PRIMARY KEY (group_jid, device_id)
    )",
    // Same mirroring rule as group_metadata above. `inserted_at` is the drain order:
    // the offline queue is replayed oldest-first, and without a stored arrival time the
    // rows come back in whatever order Postgres finds them.
    "CREATE TABLE IF NOT EXISTS pending_inbound_messages (
        chat TEXT NOT NULL,
        sender TEXT NOT NULL,
        id TEXT NOT NULL,
        message BYTEA NOT NULL,
        device_id INTEGER NOT NULL DEFAULT 1,
        inserted_at BIGINT NOT NULL DEFAULT 0,
        PRIMARY KEY (chat, sender, id, device_id)
    )",
    "CREATE TABLE IF NOT EXISTS identities (
        address TEXT NOT NULL,
        key BYTEA NOT NULL,
        device_id INTEGER NOT NULL DEFAULT 1,
        PRIMARY KEY (address, device_id)
    )",
    "CREATE INDEX IF NOT EXISTS idx_identities_device_id ON identities (device_id)",
    "CREATE TABLE IF NOT EXISTS sessions (
        address TEXT NOT NULL,
        record BYTEA NOT NULL,
        device_id INTEGER NOT NULL DEFAULT 1,
        PRIMARY KEY (address, device_id)
    )",
    "CREATE INDEX IF NOT EXISTS idx_sessions_device_id ON sessions (device_id)",
    "CREATE TABLE IF NOT EXISTS prekeys (
        id INTEGER NOT NULL,
        key BYTEA NOT NULL,
        uploaded BOOLEAN NOT NULL DEFAULT FALSE,
        device_id INTEGER NOT NULL DEFAULT 1,
        PRIMARY KEY (id, device_id)
    )",
    "CREATE INDEX IF NOT EXISTS idx_prekeys_device_id ON prekeys (device_id)",
    "CREATE TABLE IF NOT EXISTS sender_keys (
        address TEXT NOT NULL,
        record BYTEA NOT NULL,
        device_id INTEGER NOT NULL DEFAULT 1,
        PRIMARY KEY (address, device_id)
    )",
    "CREATE INDEX IF NOT EXISTS idx_sender_keys_device_id ON sender_keys (device_id)",
    "CREATE TABLE IF NOT EXISTS signed_prekeys (
        id INTEGER NOT NULL,
        record BYTEA NOT NULL,
        device_id INTEGER NOT NULL DEFAULT 1,
        PRIMARY KEY (id, device_id)
    )",
    "CREATE INDEX IF NOT EXISTS idx_signed_prekeys_device_id ON signed_prekeys (device_id)",
    "CREATE TABLE IF NOT EXISTS app_state_keys (
        key_id BYTEA NOT NULL,
        key_data BYTEA NOT NULL,
        device_id INTEGER NOT NULL DEFAULT 1,
        inserted_seq BIGSERIAL,
        PRIMARY KEY (key_id, device_id)
    )",
    "CREATE INDEX IF NOT EXISTS idx_app_state_keys_device_id ON app_state_keys (device_id)",
    "CREATE TABLE IF NOT EXISTS app_state_versions (
        name TEXT NOT NULL,
        state_data BYTEA NOT NULL,
        device_id INTEGER NOT NULL DEFAULT 1,
        PRIMARY KEY (name, device_id)
    )",
    "CREATE INDEX IF NOT EXISTS idx_app_state_versions_device_id ON app_state_versions (device_id)",
    "CREATE TABLE IF NOT EXISTS app_state_mutation_macs (
        name TEXT NOT NULL,
        version BIGINT NOT NULL,
        index_mac BYTEA NOT NULL,
        value_mac BYTEA NOT NULL,
        device_id INTEGER NOT NULL DEFAULT 1,
        PRIMARY KEY (name, index_mac, device_id)
    )",
    "CREATE INDEX IF NOT EXISTS idx_app_state_mutation_macs_device_id ON app_state_mutation_macs (device_id)",
    "CREATE TABLE IF NOT EXISTS lid_pn_mapping (
        lid TEXT NOT NULL,
        phone_number TEXT NOT NULL,
        created_at BIGINT NOT NULL,
        learning_source TEXT NOT NULL,
        updated_at BIGINT NOT NULL,
        device_id INTEGER NOT NULL DEFAULT 1,
        PRIMARY KEY (lid, device_id)
    )",
    "CREATE INDEX IF NOT EXISTS idx_lid_pn_mapping_phone ON lid_pn_mapping (phone_number, device_id)",
    "CREATE TABLE IF NOT EXISTS base_keys (
        address TEXT NOT NULL,
        message_id TEXT NOT NULL,
        base_key BYTEA NOT NULL,
        device_id INTEGER NOT NULL DEFAULT 1,
        created_at BIGINT NOT NULL DEFAULT EXTRACT(EPOCH FROM NOW())::BIGINT,
        PRIMARY KEY (address, message_id, device_id)
    )",
    "CREATE INDEX IF NOT EXISTS idx_base_keys_device ON base_keys (device_id)",
    "CREATE TABLE IF NOT EXISTS device_registry (
        user_id TEXT NOT NULL,
        devices_json TEXT NOT NULL,
        timestamp BIGINT NOT NULL,
        phash TEXT,
        device_id INTEGER NOT NULL DEFAULT 1,
        updated_at BIGINT NOT NULL DEFAULT EXTRACT(EPOCH FROM NOW())::BIGINT,
        raw_id INTEGER,
        PRIMARY KEY (user_id, device_id)
    )",
    "CREATE INDEX IF NOT EXISTS idx_device_registry_timestamp ON device_registry (timestamp)",
    "CREATE INDEX IF NOT EXISTS idx_device_registry_device ON device_registry (device_id)",
    "CREATE INDEX IF NOT EXISTS idx_device_registry_updated_at ON device_registry (updated_at)",
    "CREATE TABLE IF NOT EXISTS sender_key_devices (
        group_jid TEXT NOT NULL,
        device_jid TEXT NOT NULL,
        has_key BOOLEAN NOT NULL DEFAULT FALSE,
        device_id INTEGER NOT NULL DEFAULT 1,
        updated_at BIGINT NOT NULL DEFAULT EXTRACT(EPOCH FROM NOW())::BIGINT,
        PRIMARY KEY (group_jid, device_jid, device_id)
    )",
    "CREATE INDEX IF NOT EXISTS idx_sender_key_devices_group ON sender_key_devices (group_jid, device_id)",
    "CREATE TABLE IF NOT EXISTS tc_tokens (
        jid TEXT NOT NULL,
        token BYTEA NOT NULL,
        token_timestamp BIGINT NOT NULL,
        sender_timestamp BIGINT,
        device_id INTEGER NOT NULL DEFAULT 1,
        updated_at BIGINT NOT NULL DEFAULT EXTRACT(EPOCH FROM NOW())::BIGINT,
        PRIMARY KEY (jid, device_id)
    )",
    "CREATE INDEX IF NOT EXISTS idx_tc_tokens_timestamp ON tc_tokens (token_timestamp, device_id)",
    "CREATE TABLE IF NOT EXISTS sent_messages (
        chat_jid TEXT NOT NULL,
        message_id TEXT NOT NULL,
        payload BYTEA NOT NULL,
        device_id INTEGER NOT NULL DEFAULT 1,
        created_at BIGINT NOT NULL DEFAULT EXTRACT(EPOCH FROM NOW())::BIGINT,
        PRIMARY KEY (chat_jid, message_id, device_id)
    )",
    "CREATE INDEX IF NOT EXISTS idx_sent_messages_created ON sent_messages (created_at, device_id)",
    "INSERT INTO _wa_migrations (id) VALUES (1) ON CONFLICT DO NOTHING",
    "ALTER TABLE device ADD COLUMN IF NOT EXISTS first_unupload_pre_key_id INTEGER NOT NULL DEFAULT 0",
    "CREATE TABLE IF NOT EXISTS msg_secrets (
        chat TEXT NOT NULL,
        sender TEXT NOT NULL,
        msg_id TEXT NOT NULL,
        secret BYTEA NOT NULL,
        device_id INTEGER NOT NULL DEFAULT 1,
        created_at BIGINT NOT NULL DEFAULT 0,
        expires_at BIGINT NOT NULL DEFAULT 0,
        message_ts BIGINT NOT NULL DEFAULT 0,
        PRIMARY KEY (chat, sender, msg_id, device_id)
    )",
    "CREATE INDEX IF NOT EXISTS idx_msg_secrets_expires ON msg_secrets (device_id, expires_at)",
    "INSERT INTO _wa_migrations (id) VALUES (2) ON CONFLICT DO NOTHING",
    "ALTER TABLE device ADD COLUMN IF NOT EXISTS lid_migrated BOOLEAN NOT NULL DEFAULT FALSE",
    "ALTER TABLE device ADD COLUMN IF NOT EXISTS last_signed_pre_key_rotation_ms BIGINT NOT NULL DEFAULT 0",
    "ALTER TABLE device ADD COLUMN IF NOT EXISTS read_receipts_disabled BOOLEAN NOT NULL DEFAULT FALSE",
    "ALTER TABLE device ADD COLUMN IF NOT EXISTS server_client_expiration TEXT",
    "INSERT INTO _wa_migrations (id) VALUES (3) ON CONFLICT DO NOTHING",
];
