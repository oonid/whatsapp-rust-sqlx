-- Consolidated PostgreSQL schema for whatsapp-rust storage.
-- This is the final form of all SQLite migrations combined.

CREATE TABLE device (
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
    nct_salt BYTEA,
    server_has_prekeys BOOLEAN NOT NULL DEFAULT FALSE,
    server_cert_chain BYTEA,
    login_counter INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE identities (
    address TEXT NOT NULL,
    key BYTEA NOT NULL,
    device_id INTEGER NOT NULL DEFAULT 1,
    PRIMARY KEY (address, device_id)
);
CREATE INDEX idx_identities_device_id ON identities (device_id);

CREATE TABLE sessions (
    address TEXT NOT NULL,
    record BYTEA NOT NULL,
    device_id INTEGER NOT NULL DEFAULT 1,
    PRIMARY KEY (address, device_id)
);
CREATE INDEX idx_sessions_device_id ON sessions (device_id);

CREATE TABLE prekeys (
    id INTEGER NOT NULL,
    key BYTEA NOT NULL,
    uploaded BOOLEAN NOT NULL DEFAULT FALSE,
    device_id INTEGER NOT NULL DEFAULT 1,
    PRIMARY KEY (id, device_id)
);
CREATE INDEX idx_prekeys_device_id ON prekeys (device_id);

CREATE TABLE sender_keys (
    address TEXT NOT NULL,
    record BYTEA NOT NULL,
    device_id INTEGER NOT NULL DEFAULT 1,
    PRIMARY KEY (address, device_id)
);
CREATE INDEX idx_sender_keys_device_id ON sender_keys (device_id);

CREATE TABLE signed_prekeys (
    id INTEGER NOT NULL,
    record BYTEA NOT NULL,
    device_id INTEGER NOT NULL DEFAULT 1,
    PRIMARY KEY (id, device_id)
);
CREATE INDEX idx_signed_prekeys_device_id ON signed_prekeys (device_id);

CREATE TABLE app_state_keys (
    key_id BYTEA NOT NULL,
    key_data BYTEA NOT NULL,
    device_id INTEGER NOT NULL DEFAULT 1,
    inserted_seq BIGSERIAL,
    PRIMARY KEY (key_id, device_id)
);
CREATE INDEX idx_app_state_keys_device_id ON app_state_keys (device_id);

CREATE TABLE app_state_versions (
    name TEXT NOT NULL,
    state_data BYTEA NOT NULL,
    device_id INTEGER NOT NULL DEFAULT 1,
    PRIMARY KEY (name, device_id)
);
CREATE INDEX idx_app_state_versions_device_id ON app_state_versions (device_id);

CREATE TABLE app_state_mutation_macs (
    name TEXT NOT NULL,
    version BIGINT NOT NULL,
    index_mac BYTEA NOT NULL,
    value_mac BYTEA NOT NULL,
    device_id INTEGER NOT NULL DEFAULT 1,
    PRIMARY KEY (name, index_mac, device_id)
);
CREATE INDEX idx_app_state_mutation_macs_device_id ON app_state_mutation_macs (device_id);

CREATE TABLE lid_pn_mapping (
    lid TEXT NOT NULL,
    phone_number TEXT NOT NULL,
    created_at BIGINT NOT NULL,
    learning_source TEXT NOT NULL,
    updated_at BIGINT NOT NULL,
    device_id INTEGER NOT NULL DEFAULT 1,
    PRIMARY KEY (lid, device_id)
);
CREATE INDEX idx_lid_pn_mapping_phone ON lid_pn_mapping (phone_number, device_id);

CREATE TABLE base_keys (
    address TEXT NOT NULL,
    message_id TEXT NOT NULL,
    base_key BYTEA NOT NULL,
    device_id INTEGER NOT NULL DEFAULT 1,
    created_at BIGINT NOT NULL DEFAULT EXTRACT(EPOCH FROM NOW())::BIGINT,
    PRIMARY KEY (address, message_id, device_id)
);
CREATE INDEX idx_base_keys_device ON base_keys (device_id);

CREATE TABLE device_registry (
    user_id TEXT NOT NULL,
    devices_json TEXT NOT NULL,
    timestamp BIGINT NOT NULL,
    phash TEXT,
    device_id INTEGER NOT NULL DEFAULT 1,
    updated_at BIGINT NOT NULL DEFAULT EXTRACT(EPOCH FROM NOW())::BIGINT,
    raw_id INTEGER,
    PRIMARY KEY (user_id, device_id)
);
CREATE INDEX idx_device_registry_timestamp ON device_registry (timestamp);
CREATE INDEX idx_device_registry_device ON device_registry (device_id);
CREATE INDEX idx_device_registry_updated_at ON device_registry (updated_at);

CREATE TABLE sender_key_devices (
    group_jid TEXT NOT NULL,
    device_jid TEXT NOT NULL,
    has_key BOOLEAN NOT NULL DEFAULT FALSE,
    device_id INTEGER NOT NULL DEFAULT 1,
    updated_at BIGINT NOT NULL DEFAULT EXTRACT(EPOCH FROM NOW())::BIGINT,
    PRIMARY KEY (group_jid, device_jid, device_id)
);
CREATE INDEX idx_sender_key_devices_group ON sender_key_devices (group_jid, device_id);

CREATE TABLE tc_tokens (
    jid TEXT NOT NULL,
    token BYTEA NOT NULL,
    token_timestamp BIGINT NOT NULL,
    sender_timestamp BIGINT,
    device_id INTEGER NOT NULL DEFAULT 1,
    updated_at BIGINT NOT NULL DEFAULT EXTRACT(EPOCH FROM NOW())::BIGINT,
    PRIMARY KEY (jid, device_id)
);
CREATE INDEX idx_tc_tokens_timestamp ON tc_tokens (token_timestamp, device_id);

CREATE TABLE sent_messages (
    chat_jid TEXT NOT NULL,
    message_id TEXT NOT NULL,
    payload BYTEA NOT NULL,
    device_id INTEGER NOT NULL DEFAULT 1,
    created_at BIGINT NOT NULL DEFAULT EXTRACT(EPOCH FROM NOW())::BIGINT,
    PRIMARY KEY (chat_jid, message_id, device_id)
);
CREATE INDEX idx_sent_messages_created ON sent_messages (created_at, device_id);
