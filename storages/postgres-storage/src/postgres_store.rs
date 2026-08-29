use async_trait::async_trait;
use bytes::Bytes;
use sqlx::{PgPool, Row};
use wacore::appstate::hash::HashState;
use wacore::appstate::processor::AppStateMutationMAC;
use wacore::libsignal::protocol::{KeyPair, PrivateKey, PublicKey};
use wacore::store::Device as CoreDevice;
use wacore::store::error::{Result, StoreError};
use wacore::store::traits::*;

// --------------------------------------------------------------------------
// Migration runner (no sqlx "migrate" feature — avoids sqlx-sqlite dep)
// --------------------------------------------------------------------------

/// Each element is a single DDL statement executed in order on first startup.
/// Use IF NOT EXISTS so re-running is safe (idempotent).
static SCHEMA_STMTS: &[&str] = &[
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
        login_counter INTEGER NOT NULL DEFAULT 0
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
    // Record that schema v1 is applied
    "INSERT INTO _wa_migrations (id) VALUES (1) ON CONFLICT DO NOTHING",
    // --- v2: prekey upload watermark (#833) + message-secret store ---
    // ALTER IF NOT EXISTS (beside the CREATE column above) so already-provisioned
    // databases pick up the new column too.
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
];

async fn run_migrations(pool: &PgPool) -> Result<()> {
    // Advisory lock serialises concurrent startup (e.g. multiple test threads).
    // pg_advisory_xact_lock is released automatically at transaction end.
    let mut tx = pool
        .begin()
        .await
        .map_err(|e| StoreError::Connection(Box::new(e)))?;
    sqlx::query("SELECT pg_advisory_xact_lock(8675309)")
        .execute(&mut *tx)
        .await
        .map_err(|e| StoreError::Migration(Box::new(e)))?;
    for stmt in SCHEMA_STMTS {
        sqlx::query(stmt)
            .execute(&mut *tx)
            .await
            .map_err(|e| StoreError::Migration(Box::new(e)))?;
    }
    tx.commit()
        .await
        .map_err(|e| StoreError::Migration(Box::new(e)))?;
    Ok(())
}

// --------------------------------------------------------------------------
// Store struct
// --------------------------------------------------------------------------

#[derive(Clone)]
pub struct PostgresStore {
    pool: PgPool,
    device_id: i32,
}

impl PostgresStore {
    pub async fn new(database_url: &str) -> Result<Self> {
        let pool = PgPool::connect(database_url)
            .await
            .map_err(|e| StoreError::Connection(Box::new(e)))?;
        run_migrations(&pool).await?;
        Ok(Self { pool, device_id: 1 })
    }

    pub async fn new_for_device(database_url: &str, device_id: i32) -> Result<Self> {
        let mut store = Self::new(database_url).await?;
        store.device_id = device_id;
        Ok(store)
    }

    pub fn device_id(&self) -> i32 {
        self.device_id
    }

    fn serialize_keypair(key_pair: &KeyPair) -> Result<Vec<u8>> {
        let mut bytes = Vec::with_capacity(64);
        bytes.extend_from_slice(key_pair.private_key.serialize());
        bytes.extend_from_slice(key_pair.public_key.public_key_bytes());
        Ok(bytes)
    }

    fn deserialize_keypair(bytes: &[u8]) -> Result<KeyPair> {
        if bytes.len() != 64 {
            return Err(StoreError::Validation(format!(
                "invalid KeyPair length: {}",
                bytes.len()
            )));
        }
        let private_key = PrivateKey::deserialize(&bytes[0..32])
            .map_err(|e| StoreError::Serialization(Box::new(e)))?;
        let public_key = PublicKey::from_djb_public_key_bytes(&bytes[32..64])
            .map_err(|e| StoreError::Serialization(Box::new(e)))?;
        Ok(KeyPair::new(public_key, private_key))
    }

    pub async fn device_exists(&self, device_id: i32) -> Result<bool> {
        let row = sqlx::query("SELECT 1 FROM device WHERE id = $1")
            .bind(device_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| StoreError::Database(Box::new(e)))?;
        Ok(row.is_some())
    }

    pub async fn load_device_data_for_device(&self, device_id: i32) -> Result<Option<CoreDevice>> {
        let row = sqlx::query(
            "SELECT id, lid, pn, registration_id, noise_key, identity_key, signed_pre_key,
                    signed_pre_key_id, signed_pre_key_signature, adv_secret_key, account,
                    push_name, app_version_primary, app_version_secondary, app_version_tertiary,
                    app_version_last_fetched_ms, edge_routing_info, props_hash, next_pre_key_id,
                    first_unupload_pre_key_id, nct_salt, server_has_prekeys, server_cert_chain,
                    login_counter, lid_migrated, last_signed_pre_key_rotation_ms, read_receipts_disabled, server_client_expiration
             FROM device WHERE id = $1",
        )
        .bind(device_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| StoreError::Database(Box::new(e)))?;

        let Some(row) = row else {
            return Ok(None);
        };

        let lid_str: String = row.get("lid");
        let pn_str: String = row.get("pn");
        let pn = if !pn_str.is_empty() {
            pn_str.parse().ok()
        } else {
            None
        };
        let lid = if !lid_str.is_empty() {
            lid_str.parse().ok()
        } else {
            None
        };

        let noise_key = Self::deserialize_keypair(row.get::<Vec<u8>, _>("noise_key").as_slice())?;
        let identity_key =
            Self::deserialize_keypair(row.get::<Vec<u8>, _>("identity_key").as_slice())?;
        let signed_pre_key =
            Self::deserialize_keypair(row.get::<Vec<u8>, _>("signed_pre_key").as_slice())?;

        let sig_bytes: Vec<u8> = row.get("signed_pre_key_signature");
        let signed_pre_key_signature: [u8; 64] = sig_bytes.try_into().map_err(|_| {
            StoreError::Validation("invalid signed_pre_key_signature length".into())
        })?;

        let adv_bytes: Vec<u8> = row.get("adv_secret_key");
        let adv_secret_key: [u8; 32] = adv_bytes
            .try_into()
            .map_err(|_| StoreError::Validation("invalid adv_secret_key length".into()))?;

        let account_bytes: Option<Vec<u8>> = row.get("account");
        let account = account_bytes
            .map(|data| {
                wacore::store::device::account_serde::from_bytes(&data)
                    .map_err(|e| StoreError::Serialization(Box::new(e)))
            })
            .transpose()?;

        let server_cert_chain_bytes: Option<Vec<u8>> = row.get("server_cert_chain");
        let server_cert_chain = server_cert_chain_bytes.and_then(|bytes| {
            match crate::wire::decode_server_cert_chain(&bytes) {
                Ok(chain) => Some(chain),
                Err(e) => {
                    log::warn!(
                        "device {} server_cert_chain blob ({} bytes) failed to decode: {e}; \
                         dropping cache, next connect will use XX",
                        device_id,
                        bytes.len(),
                    );
                    None
                }
            }
        });

        Ok(Some(CoreDevice {
            pn,
            lid,
            registration_id: row.get::<i32, _>("registration_id") as u32,
            noise_key,
            identity_key,
            signed_pre_key,
            signed_pre_key_id: row.get::<i32, _>("signed_pre_key_id") as u32,
            signed_pre_key_signature,
            adv_secret_key,
            account: account.map(std::sync::Arc::new),
            push_name: row.get("push_name"),
            app_version_primary: row.get::<i32, _>("app_version_primary") as u32,
            app_version_secondary: row.get::<i32, _>("app_version_secondary") as u32,
            app_version_tertiary: row
                .get::<i64, _>("app_version_tertiary")
                .try_into()
                .unwrap_or(0),
            app_version_last_fetched_ms: row.get("app_version_last_fetched_ms"),
            device_props: std::sync::Arc::new(wacore::store::device::DEVICE_PROPS.clone()),
            client_profile: wacore::client_profile::ClientProfile::web(),
            edge_routing_info: row.get("edge_routing_info"),
            props_hash: row.get("props_hash"),
            next_pre_key_id: row.get::<i32, _>("next_pre_key_id") as u32,
            first_unupload_pre_key_id: row.get::<i32, _>("first_unupload_pre_key_id") as u32,
            server_has_prekeys: row.get("server_has_prekeys"),
            lid_migrated: row.get("lid_migrated"),
            last_signed_pre_key_rotation_ms: row.get::<i64, _>("last_signed_pre_key_rotation_ms"),
            read_receipts_disabled: row.get("read_receipts_disabled"),
            server_client_expiration: row.get::<Option<String>, _>("server_client_expiration").and_then(|s| serde_json::from_str(&s).ok()),
            nct_salt: row.get("nct_salt"),
            nct_salt_sync_seen: false,
            server_cert_chain,
            login_counter: row.get("login_counter"),
        }))
    }

    pub async fn save_device_data_for_device(
        &self,
        device_id: i32,
        device_data: &CoreDevice,
    ) -> Result<()> {
        let noise_key = Self::serialize_keypair(&device_data.noise_key)?;
        let identity_key = Self::serialize_keypair(&device_data.identity_key)?;
        let signed_pre_key = Self::serialize_keypair(&device_data.signed_pre_key)?;
        let account_data = device_data
            .account
            .as_ref()
            .map(|a| wacore::store::device::account_serde::to_bytes(a));
        let server_cert_chain = device_data
            .server_cert_chain
            .as_ref()
            .map(crate::wire::encode_server_cert_chain);
        let lid = device_data
            .lid
            .as_ref()
            .map(|j| j.to_string())
            .unwrap_or_default();
        let pn = device_data
            .pn
            .as_ref()
            .map(|j| j.to_string())
            .unwrap_or_default();

        sqlx::query(
            "INSERT INTO device (id, lid, pn, registration_id, noise_key, identity_key,
                                  signed_pre_key, signed_pre_key_id, signed_pre_key_signature,
                                  adv_secret_key, account, push_name, app_version_primary,
                                  app_version_secondary, app_version_tertiary,
                                  app_version_last_fetched_ms, edge_routing_info, props_hash,
                                  next_pre_key_id, server_has_prekeys, nct_salt,
                                  server_cert_chain, login_counter, first_unupload_pre_key_id, lid_migrated, last_signed_pre_key_rotation_ms, read_receipts_disabled, server_client_expiration)
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18,$19,$20,$21,$22,$23,$24),$25,$26,$27,$28)
             ON CONFLICT (id) DO UPDATE SET
                lid = EXCLUDED.lid, pn = EXCLUDED.pn,
                registration_id = EXCLUDED.registration_id,
                noise_key = EXCLUDED.noise_key, identity_key = EXCLUDED.identity_key,
                signed_pre_key = EXCLUDED.signed_pre_key,
                signed_pre_key_id = EXCLUDED.signed_pre_key_id,
                signed_pre_key_signature = EXCLUDED.signed_pre_key_signature,
                adv_secret_key = EXCLUDED.adv_secret_key, account = EXCLUDED.account,
                push_name = EXCLUDED.push_name,
                app_version_primary = EXCLUDED.app_version_primary,
                app_version_secondary = EXCLUDED.app_version_secondary,
                app_version_tertiary = EXCLUDED.app_version_tertiary,
                app_version_last_fetched_ms = EXCLUDED.app_version_last_fetched_ms,
                edge_routing_info = EXCLUDED.edge_routing_info,
                props_hash = EXCLUDED.props_hash,
                next_pre_key_id = EXCLUDED.next_pre_key_id,
                server_has_prekeys = EXCLUDED.server_has_prekeys,
                nct_salt = EXCLUDED.nct_salt, server_cert_chain = EXCLUDED.server_cert_chain,
                login_counter, lid_migrated, last_signed_pre_key_rotation_ms, read_receipts_disabled, server_client_expiration = EXCLUDED.login_counter,
                first_unupload_pre_key_id = EXCLUDED.first_unupload_pre_key_id, lid_migrated = EXCLUDED.lid_migrated, last_signed_pre_key_rotation_ms = EXCLUDED.last_signed_pre_key_rotation_ms, read_receipts_disabled = EXCLUDED.read_receipts_disabled, server_client_expiration = EXCLUDED.server_client_expiration",
        )
        .bind(device_id)
        .bind(&lid)
        .bind(&pn)
        .bind(device_data.registration_id as i32)
        .bind(&noise_key)
        .bind(&identity_key)
        .bind(&signed_pre_key)
        .bind(device_data.signed_pre_key_id as i32)
        .bind(device_data.signed_pre_key_signature.as_slice())
        .bind(device_data.adv_secret_key.as_slice())
        .bind(account_data.as_deref())
        .bind(&device_data.push_name)
        .bind(device_data.app_version_primary as i32)
        .bind(device_data.app_version_secondary as i32)
        .bind(device_data.app_version_tertiary as i64)
        .bind(device_data.app_version_last_fetched_ms)
        .bind(device_data.edge_routing_info.as_deref())
        .bind(device_data.props_hash.as_deref())
        .bind(device_data.next_pre_key_id as i32)
        .bind(device_data.server_has_prekeys)
        .bind(device_data.nct_salt.as_deref())
        .bind(server_cert_chain.as_deref())
        .bind(device_data.login_counter)
        .bind(device_data.first_unupload_pre_key_id as i32)
        .bind(device_data.lid_migrated)
        .bind(device_data.last_signed_pre_key_rotation_ms)
        .bind(device_data.read_receipts_disabled)
        .bind(device_data.server_client_expiration.as_ref().and_then(|x| serde_json::to_string(x).ok()))
        .execute(&self.pool)
        .await
        .map_err(|e| StoreError::Database(Box::new(e)))?;
        Ok(())
    }

    pub async fn create_new_device(&self) -> Result<i32> {
        let new_device = CoreDevice::new();
        self.save_device_data_for_device(self.device_id, &new_device)
            .await?;
        Ok(self.device_id)
    }

    pub async fn put_identity_for_device(
        &self,
        address: &str,
        key: [u8; 32],
        device_id: i32,
    ) -> Result<()> {
        sqlx::query(
            "INSERT INTO identities (address, key, device_id) VALUES ($1, $2, $3)
             ON CONFLICT (address, device_id) DO UPDATE SET key = EXCLUDED.key",
        )
        .bind(address)
        .bind(key.as_slice())
        .bind(device_id)
        .execute(&self.pool)
        .await
        .map_err(|e| StoreError::Database(Box::new(e)))?;
        Ok(())
    }

    pub async fn delete_identity_for_device(&self, address: &str, device_id: i32) -> Result<()> {
        sqlx::query("DELETE FROM identities WHERE address = $1 AND device_id = $2")
            .bind(address)
            .bind(device_id)
            .execute(&self.pool)
            .await
            .map_err(|e| StoreError::Database(Box::new(e)))?;
        Ok(())
    }

    pub async fn load_identity_for_device(
        &self,
        address: &str,
        device_id: i32,
    ) -> Result<Option<Vec<u8>>> {
        let row = sqlx::query("SELECT key FROM identities WHERE address = $1 AND device_id = $2")
            .bind(address)
            .bind(device_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| StoreError::Database(Box::new(e)))?;
        Ok(row.map(|r| r.get("key")))
    }

    pub async fn get_session_for_device(
        &self,
        address: &str,
        device_id: i32,
    ) -> Result<Option<Vec<u8>>> {
        let row = sqlx::query("SELECT record FROM sessions WHERE address = $1 AND device_id = $2")
            .bind(address)
            .bind(device_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| StoreError::Database(Box::new(e)))?;
        Ok(row.map(|r| r.get("record")))
    }

    pub async fn put_session_for_device(
        &self,
        address: &str,
        session: &[u8],
        device_id: i32,
    ) -> Result<()> {
        sqlx::query(
            "INSERT INTO sessions (address, record, device_id) VALUES ($1, $2, $3)
             ON CONFLICT (address, device_id) DO UPDATE SET record = EXCLUDED.record",
        )
        .bind(address)
        .bind(session)
        .bind(device_id)
        .execute(&self.pool)
        .await
        .map_err(|e| StoreError::Database(Box::new(e)))?;
        Ok(())
    }

    pub async fn delete_session_for_device(&self, address: &str, device_id: i32) -> Result<()> {
        sqlx::query("DELETE FROM sessions WHERE address = $1 AND device_id = $2")
            .bind(address)
            .bind(device_id)
            .execute(&self.pool)
            .await
            .map_err(|e| StoreError::Database(Box::new(e)))?;
        Ok(())
    }

    pub async fn put_sender_key_for_device(
        &self,
        address: &str,
        record: &[u8],
        device_id: i32,
    ) -> Result<()> {
        sqlx::query(
            "INSERT INTO sender_keys (address, record, device_id) VALUES ($1, $2, $3)
             ON CONFLICT (address, device_id) DO UPDATE SET record = EXCLUDED.record",
        )
        .bind(address)
        .bind(record)
        .bind(device_id)
        .execute(&self.pool)
        .await
        .map_err(|e| StoreError::Database(Box::new(e)))?;
        Ok(())
    }

    pub async fn get_sender_key_for_device(
        &self,
        address: &str,
        device_id: i32,
    ) -> Result<Option<Vec<u8>>> {
        let row =
            sqlx::query("SELECT record FROM sender_keys WHERE address = $1 AND device_id = $2")
                .bind(address)
                .bind(device_id)
                .fetch_optional(&self.pool)
                .await
                .map_err(|e| StoreError::Database(Box::new(e)))?;
        Ok(row.map(|r| r.get("record")))
    }

    pub async fn delete_sender_key_for_device(&self, address: &str, device_id: i32) -> Result<()> {
        sqlx::query("DELETE FROM sender_keys WHERE address = $1 AND device_id = $2")
            .bind(address)
            .bind(device_id)
            .execute(&self.pool)
            .await
            .map_err(|e| StoreError::Database(Box::new(e)))?;
        Ok(())
    }

    pub async fn get_app_state_sync_key_for_device(
        &self,
        key_id: &[u8],
        device_id: i32,
    ) -> Result<Option<AppStateSyncKey>> {
        let row =
            sqlx::query("SELECT key_data FROM app_state_keys WHERE key_id = $1 AND device_id = $2")
                .bind(key_id)
                .bind(device_id)
                .fetch_optional(&self.pool)
                .await
                .map_err(|e| StoreError::Database(Box::new(e)))?;

        match row {
            None => Ok(None),
            Some(r) => {
                let data: Vec<u8> = r.get("key_data");
                let key = crate::wire::decode_app_state_sync_key(&data)
                    .map_err(|e| StoreError::Serialization(Box::new(e)))?;
                Ok(Some(key))
            }
        }
    }

    pub async fn set_app_state_sync_key_for_device(
        &self,
        key_id: &[u8],
        key: AppStateSyncKey,
        device_id: i32,
    ) -> Result<()> {
        let data = crate::wire::encode_app_state_sync_key(&key);
        sqlx::query(
            "INSERT INTO app_state_keys (key_id, key_data, device_id) VALUES ($1, $2, $3)
             ON CONFLICT (key_id, device_id) DO UPDATE SET key_data = EXCLUDED.key_data",
        )
        .bind(key_id)
        .bind(&data)
        .bind(device_id)
        .execute(&self.pool)
        .await
        .map_err(|e| StoreError::Database(Box::new(e)))?;
        Ok(())
    }

    pub async fn get_latest_app_state_sync_key_id_for_device(
        &self,
        device_id: i32,
    ) -> Result<Option<Vec<u8>>> {
        let row = sqlx::query(
            "SELECT key_id FROM app_state_keys WHERE device_id = $1 ORDER BY inserted_seq DESC LIMIT 1",
        )
        .bind(device_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| StoreError::Database(Box::new(e)))?;
        Ok(row.map(|r| r.get("key_id")))
    }

    pub async fn get_app_state_version_for_device(
        &self,
        name: &str,
        device_id: i32,
    ) -> Result<HashState> {
        let row = sqlx::query(
            "SELECT state_data FROM app_state_versions WHERE name = $1 AND device_id = $2",
        )
        .bind(name)
        .bind(device_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| StoreError::Database(Box::new(e)))?;

        match row {
            None => Ok(HashState::default()),
            Some(r) => {
                let data: Vec<u8> = r.get("state_data");
                let state = crate::wire::decode_hash_state(&data).map_err(|e| StoreError::Serialization(Box::new(e)))?;
                Ok(state)
            }
        }
    }

    pub async fn set_app_state_version_for_device(
        &self,
        name: &str,
        state: HashState,
        device_id: i32,
    ) -> Result<()> {
        let data = crate::wire::encode_hash_state(&state);
        sqlx::query(
            "INSERT INTO app_state_versions (name, state_data, device_id) VALUES ($1, $2, $3)
             ON CONFLICT (name, device_id) DO UPDATE SET state_data = EXCLUDED.state_data",
        )
        .bind(name)
        .bind(&data)
        .bind(device_id)
        .execute(&self.pool)
        .await
        .map_err(|e| StoreError::Database(Box::new(e)))?;
        Ok(())
    }

    pub async fn put_app_state_mutation_macs_for_device(
        &self,
        name: &str,
        version: u64,
        mutations: &[AppStateMutationMAC],
        device_id: i32,
    ) -> Result<()> {
        if mutations.is_empty() {
            return Ok(());
        }
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| StoreError::Database(Box::new(e)))?;
        let version = version as i64;
        for m in mutations {
            sqlx::query(
                "INSERT INTO app_state_mutation_macs (name, version, index_mac, value_mac, device_id)
                 VALUES ($1, $2, $3, $4, $5)
                 ON CONFLICT (name, index_mac, device_id)
                 DO UPDATE SET version = EXCLUDED.version, value_mac = EXCLUDED.value_mac",
            )
            .bind(name)
            .bind(version)
            .bind(m.index_mac.as_slice())
            .bind(m.value_mac.as_slice())
            .bind(device_id)
            .execute(&mut *tx)
            .await
            .map_err(|e| StoreError::Database(Box::new(e)))?;
        }
        tx.commit()
            .await
            .map_err(|e| StoreError::Database(Box::new(e)))?;
        Ok(())
    }

    pub async fn delete_app_state_mutation_macs_for_device(
        &self,
        name: &str,
        index_macs: &[Vec<u8>],
        device_id: i32,
    ) -> Result<()> {
        if index_macs.is_empty() {
            return Ok(());
        }
        sqlx::query(
            "DELETE FROM app_state_mutation_macs
             WHERE name = $1 AND device_id = $2 AND index_mac = ANY($3::bytea[])",
        )
        .bind(name)
        .bind(device_id)
        .bind(index_macs)
        .execute(&self.pool)
        .await
        .map_err(|e| StoreError::Database(Box::new(e)))?;
        Ok(())
    }

    pub async fn get_app_state_mutation_mac_for_device(
        &self,
        name: &str,
        index_mac: &[u8],
        device_id: i32,
    ) -> Result<Option<Vec<u8>>> {
        let row = sqlx::query(
            "SELECT value_mac FROM app_state_mutation_macs
             WHERE name = $1 AND index_mac = $2 AND device_id = $3",
        )
        .bind(name)
        .bind(index_mac)
        .bind(device_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| StoreError::Database(Box::new(e)))?;
        Ok(row.map(|r| r.get("value_mac")))
    }
}

// --------------------------------------------------------------------------
// Trait impls
// --------------------------------------------------------------------------

#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
impl SignalStore for PostgresStore {
    async fn put_identity(&self, address: &str, key: [u8; 32]) -> Result<()> {
        self.put_identity_for_device(address, key, self.device_id)
            .await
    }

    async fn load_identity(&self, address: &str) -> Result<Option<[u8; 32]>> {
        let blob = self
            .load_identity_for_device(address, self.device_id)
            .await?;
        match blob {
            None => Ok(None),
            Some(v) => Ok(Some(v.try_into().map_err(|v: Vec<u8>| {
                StoreError::Validation(format!(
                    "identity key for '{}' has invalid length {} (expected 32)",
                    address,
                    v.len()
                ))
            })?)),
        }
    }

    async fn delete_identity(&self, address: &str) -> Result<()> {
        self.delete_identity_for_device(address, self.device_id)
            .await
    }

    async fn get_session(&self, address: &str) -> Result<Option<Bytes>> {
        Ok(self
            .get_session_for_device(address, self.device_id)
            .await?
            .map(Bytes::from))
    }

    async fn has_session(&self, address: &str) -> Result<bool> {
        let row = sqlx::query("SELECT 1 FROM sessions WHERE address = $1 AND device_id = $2")
            .bind(address)
            .bind(self.device_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| StoreError::Database(Box::new(e)))?;
        Ok(row.is_some())
    }

    async fn put_session(&self, address: &str, session: &[u8]) -> Result<()> {
        self.put_session_for_device(address, session, self.device_id)
            .await
    }

    async fn delete_session(&self, address: &str) -> Result<()> {
        self.delete_session_for_device(address, self.device_id)
            .await
    }

    async fn store_prekey(&self, id: u32, record: &[u8], uploaded: bool) -> Result<()> {
        sqlx::query(
            "INSERT INTO prekeys (id, key, uploaded, device_id) VALUES ($1, $2, $3, $4)
             ON CONFLICT (id, device_id) DO UPDATE SET key = EXCLUDED.key, uploaded = EXCLUDED.uploaded",
        )
        .bind(id as i32)
        .bind(record)
        .bind(uploaded)
        .bind(self.device_id)
        .execute(&self.pool)
        .await
        .map_err(|e| StoreError::Database(Box::new(e)))?;
        Ok(())
    }

    async fn mark_prekeys_uploaded(&self, ids: &[u32]) -> Result<()> {
        if ids.is_empty() {
            return Ok(());
        }
        // UPDATE-only (no upsert): a key consumed between the upload snapshot and
        // this call must stay deleted, never be resurrected.
        let ids: Vec<i32> = ids.iter().map(|&id| id as i32).collect();
        sqlx::query(
            "UPDATE prekeys SET uploaded = TRUE WHERE id = ANY($1::int[]) AND device_id = $2",
        )
        .bind(&ids)
        .bind(self.device_id)
        .execute(&self.pool)
        .await
        .map_err(|e| StoreError::Database(Box::new(e)))?;
        Ok(())
    }

    async fn store_prekeys_batch(&self, keys: &[(u32, Bytes)], uploaded: bool) -> Result<()> {
        if keys.is_empty() {
            return Ok(());
        }
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| StoreError::Database(Box::new(e)))?;
        for (id, record) in keys {
            sqlx::query(
                "INSERT INTO prekeys (id, key, uploaded, device_id) VALUES ($1, $2, $3, $4)
                 ON CONFLICT (id, device_id) DO UPDATE SET key = EXCLUDED.key, uploaded = EXCLUDED.uploaded",
            )
            .bind(*id as i32)
            .bind(record.as_ref())
            .bind(uploaded)
            .bind(self.device_id)
            .execute(&mut *tx)
            .await
            .map_err(|e| StoreError::Database(Box::new(e)))?;
        }
        tx.commit()
            .await
            .map_err(|e| StoreError::Database(Box::new(e)))?;
        Ok(())
    }

    async fn load_prekey(&self, id: u32) -> Result<Option<Bytes>> {
        let row = sqlx::query("SELECT key FROM prekeys WHERE id = $1 AND device_id = $2")
            .bind(id as i32)
            .bind(self.device_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| StoreError::Database(Box::new(e)))?;
        Ok(row.map(|r| Bytes::from(r.get::<Vec<u8>, _>("key"))))
    }

    async fn load_prekeys_batch(&self, ids: &[u32]) -> Result<Vec<(u32, Bytes)>> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        let ids_i32: Vec<i32> = ids.iter().map(|&id| id as i32).collect();
        let rows = sqlx::query(
            "SELECT id, key FROM prekeys WHERE id = ANY($1::int4[]) AND device_id = $2",
        )
        .bind(&ids_i32)
        .bind(self.device_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| StoreError::Database(Box::new(e)))?;
        Ok(rows
            .into_iter()
            .map(|r| {
                (
                    r.get::<i32, _>("id") as u32,
                    Bytes::from(r.get::<Vec<u8>, _>("key")),
                )
            })
            .collect())
    }

    async fn remove_prekey(&self, id: u32) -> Result<()> {
        sqlx::query("DELETE FROM prekeys WHERE id = $1 AND device_id = $2")
            .bind(id as i32)
            .bind(self.device_id)
            .execute(&self.pool)
            .await
            .map_err(|e| StoreError::Database(Box::new(e)))?;
        Ok(())
    }

    async fn get_max_prekey_id(&self) -> Result<u32> {
        let row = sqlx::query("SELECT MAX(id) AS max_id FROM prekeys WHERE device_id = $1")
            .bind(self.device_id)
            .fetch_one(&self.pool)
            .await
            .map_err(|e| StoreError::Database(Box::new(e)))?;
        let max: Option<i32> = row.get("max_id");
        Ok(max.unwrap_or(0) as u32)
    }

    async fn store_signed_prekey(&self, id: u32, record: &[u8]) -> Result<()> {
        sqlx::query(
            "INSERT INTO signed_prekeys (id, record, device_id) VALUES ($1, $2, $3)
             ON CONFLICT (id, device_id) DO UPDATE SET record = EXCLUDED.record",
        )
        .bind(id as i32)
        .bind(record)
        .bind(self.device_id)
        .execute(&self.pool)
        .await
        .map_err(|e| StoreError::Database(Box::new(e)))?;
        Ok(())
    }

    async fn load_signed_prekey(&self, id: u32) -> Result<Option<Vec<u8>>> {
        let row = sqlx::query("SELECT record FROM signed_prekeys WHERE id = $1 AND device_id = $2")
            .bind(id as i32)
            .bind(self.device_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| StoreError::Database(Box::new(e)))?;
        Ok(row.map(|r| r.get("record")))
    }

    async fn load_all_signed_prekeys(&self) -> Result<Vec<(u32, Vec<u8>)>> {
        let rows = sqlx::query("SELECT id, record FROM signed_prekeys WHERE device_id = $1")
            .bind(self.device_id)
            .fetch_all(&self.pool)
            .await
            .map_err(|e| StoreError::Database(Box::new(e)))?;
        Ok(rows
            .into_iter()
            .map(|r| (r.get::<i32, _>("id") as u32, r.get("record")))
            .collect())
    }

    async fn remove_signed_prekey(&self, id: u32) -> Result<()> {
        sqlx::query("DELETE FROM signed_prekeys WHERE id = $1 AND device_id = $2")
            .bind(id as i32)
            .bind(self.device_id)
            .execute(&self.pool)
            .await
            .map_err(|e| StoreError::Database(Box::new(e)))?;
        Ok(())
    }

    async fn put_sender_key(&self, address: &str, record: &[u8]) -> Result<()> {
        self.put_sender_key_for_device(address, record, self.device_id)
            .await
    }

    async fn get_sender_key(&self, address: &str) -> Result<Option<Vec<u8>>> {
        self.get_sender_key_for_device(address, self.device_id)
            .await
    }

    async fn delete_sender_key(&self, address: &str) -> Result<()> {
        self.delete_sender_key_for_device(address, self.device_id)
            .await
    }
}

#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
impl AppSyncStore for PostgresStore {
    async fn get_sync_key(&self, key_id: &[u8]) -> Result<Option<AppStateSyncKey>> {
        self.get_app_state_sync_key_for_device(key_id, self.device_id)
            .await
    }

    async fn set_sync_key(&self, key_id: &[u8], key: AppStateSyncKey) -> Result<()> {
        self.set_app_state_sync_key_for_device(key_id, key, self.device_id)
            .await
    }

    async fn get_version(&self, name: &str) -> Result<HashState> {
        self.get_app_state_version_for_device(name, self.device_id)
            .await
    }

    async fn set_version(&self, name: &str, state: HashState) -> Result<()> {
        self.set_app_state_version_for_device(name, state, self.device_id)
            .await
    }

    async fn put_mutation_macs(
        &self,
        name: &str,
        version: u64,
        mutations: &[AppStateMutationMAC],
    ) -> Result<()> {
        self.put_app_state_mutation_macs_for_device(name, version, mutations, self.device_id)
            .await
    }

    async fn get_mutation_mac(&self, name: &str, index_mac: &[u8]) -> Result<Option<Vec<u8>>> {
        self.get_app_state_mutation_mac_for_device(name, index_mac, self.device_id)
            .await
    }

    async fn delete_mutation_macs(&self, name: &str, index_macs: &[Vec<u8>]) -> Result<()> {
        self.delete_app_state_mutation_macs_for_device(name, index_macs, self.device_id)
            .await
    }

    async fn clear_mutation_macs(&self, name: &str) -> Result<()> {
        sqlx::query("DELETE FROM app_state_mutation_macs WHERE name = $1 AND device_id = $2")
            .bind(name)
            .bind(self.device_id)
            .execute(&self.pool)
            .await
            .map_err(|e| StoreError::Database(Box::new(e)))?;
        Ok(())
    }

    async fn get_latest_sync_key_id(&self) -> Result<Option<Vec<u8>>> {
        self.get_latest_app_state_sync_key_id_for_device(self.device_id)
            .await
    }
}

#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
impl ProtocolStore for PostgresStore {
    async fn get_sender_key_devices(&self, group_jid: &str) -> Result<Vec<(String, bool)>> {
        let rows = sqlx::query(
            "SELECT device_jid, has_key FROM sender_key_devices
             WHERE group_jid = $1 AND device_id = $2",
        )
        .bind(group_jid)
        .bind(self.device_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| StoreError::Database(Box::new(e)))?;
        Ok(rows
            .into_iter()
            .map(|r| (r.get("device_jid"), r.get("has_key")))
            .collect())
    }

    async fn set_sender_key_status(&self, group_jid: &str, entries: &[(&str, bool)]) -> Result<()> {
        if entries.is_empty() {
            return Ok(());
        }
        let now = wacore::time::now_secs();
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| StoreError::Database(Box::new(e)))?;
        for (device_jid, has_key) in entries {
            sqlx::query(
                "INSERT INTO sender_key_devices (group_jid, device_jid, has_key, device_id, updated_at)
                 VALUES ($1, $2, $3, $4, $5)
                 ON CONFLICT (group_jid, device_jid, device_id)
                 DO UPDATE SET has_key = EXCLUDED.has_key, updated_at = EXCLUDED.updated_at",
            )
            .bind(group_jid)
            .bind(*device_jid)
            .bind(*has_key)
            .bind(self.device_id)
            .bind(now)
            .execute(&mut *tx)
            .await
            .map_err(|e| StoreError::Database(Box::new(e)))?;
        }
        tx.commit()
            .await
            .map_err(|e| StoreError::Database(Box::new(e)))?;
        Ok(())
    }

    async fn clear_sender_key_devices(&self, group_jid: &str) -> Result<()> {
        sqlx::query("DELETE FROM sender_key_devices WHERE group_jid = $1 AND device_id = $2")
            .bind(group_jid)
            .bind(self.device_id)
            .execute(&self.pool)
            .await
            .map_err(|e| StoreError::Database(Box::new(e)))?;
        Ok(())
    }

    async fn clear_all_sender_key_devices(&self) -> Result<()> {
        sqlx::query("DELETE FROM sender_key_devices WHERE device_id = $1")
            .bind(self.device_id)
            .execute(&self.pool)
            .await
            .map_err(|e| StoreError::Database(Box::new(e)))?;
        Ok(())
    }

    async fn delete_sender_key_device_rows(&self, device_jids: &[&str]) -> Result<()> {
        if device_jids.is_empty() {
            return Ok(());
        }
        let owned: Vec<String> = device_jids.iter().map(|s| s.to_string()).collect();
        sqlx::query(
            "DELETE FROM sender_key_devices
             WHERE device_jid = ANY($1::text[]) AND device_id = $2",
        )
        .bind(&owned)
        .bind(self.device_id)
        .execute(&self.pool)
        .await
        .map_err(|e| StoreError::Database(Box::new(e)))?;
        Ok(())
    }

    async fn get_lid_mapping(&self, lid: &str) -> Result<Option<LidPnMappingEntry>> {
        let row = sqlx::query(
            "SELECT lid, phone_number, created_at, learning_source, updated_at
             FROM lid_pn_mapping WHERE lid = $1 AND device_id = $2",
        )
        .bind(lid)
        .bind(self.device_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| StoreError::Database(Box::new(e)))?;
        Ok(row.map(|r| LidPnMappingEntry {
            lid: r.get("lid"),
            phone_number: r.get("phone_number"),
            created_at: r.get("created_at"),
            updated_at: r.get("updated_at"),
            learning_source: r.get("learning_source"),
        }))
    }

    async fn get_pn_mapping(&self, phone: &str) -> Result<Option<LidPnMappingEntry>> {
        let row = sqlx::query(
            "SELECT lid, phone_number, created_at, learning_source, updated_at
             FROM lid_pn_mapping WHERE phone_number = $1 AND device_id = $2
             ORDER BY updated_at DESC LIMIT 1",
        )
        .bind(phone)
        .bind(self.device_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| StoreError::Database(Box::new(e)))?;
        Ok(row.map(|r| LidPnMappingEntry {
            lid: r.get("lid"),
            phone_number: r.get("phone_number"),
            created_at: r.get("created_at"),
            updated_at: r.get("updated_at"),
            learning_source: r.get("learning_source"),
        }))
    }

    async fn put_lid_mapping(&self, entry: &LidPnMappingEntry) -> Result<()> {
        self.put_lid_mappings(std::slice::from_ref(entry)).await
    }

    async fn put_lid_mappings(&self, entries: &[LidPnMappingEntry]) -> Result<()> {
        if entries.is_empty() {
            return Ok(());
        }
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| StoreError::Database(Box::new(e)))?;
        for entry in entries {
            sqlx::query(
                "INSERT INTO lid_pn_mapping
                    (lid, phone_number, created_at, learning_source, updated_at, device_id)
                 VALUES ($1, $2, $3, $4, $5, $6)
                 ON CONFLICT (lid, device_id) DO UPDATE SET
                    phone_number = EXCLUDED.phone_number,
                    learning_source = EXCLUDED.learning_source,
                    updated_at = EXCLUDED.updated_at",
            )
            .bind(&entry.lid)
            .bind(&entry.phone_number)
            .bind(entry.created_at)
            .bind(&entry.learning_source)
            .bind(entry.updated_at)
            .bind(self.device_id)
            .execute(&mut *tx)
            .await
            .map_err(|e| StoreError::Database(Box::new(e)))?;
        }
        tx.commit()
            .await
            .map_err(|e| StoreError::Database(Box::new(e)))?;
        Ok(())
    }

    async fn get_all_lid_mappings(&self) -> Result<Vec<LidPnMappingEntry>> {
        let rows = sqlx::query(
            "SELECT lid, phone_number, created_at, learning_source, updated_at
             FROM lid_pn_mapping WHERE device_id = $1",
        )
        .bind(self.device_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| StoreError::Database(Box::new(e)))?;
        Ok(rows
            .into_iter()
            .map(|r| LidPnMappingEntry {
                lid: r.get("lid"),
                phone_number: r.get("phone_number"),
                created_at: r.get("created_at"),
                updated_at: r.get("updated_at"),
                learning_source: r.get("learning_source"),
            })
            .collect())
    }

    async fn save_base_key(&self, address: &str, message_id: &str, base_key: &[u8]) -> Result<()> {
        let now = wacore::time::now_secs();
        sqlx::query(
            "INSERT INTO base_keys (address, message_id, base_key, device_id, created_at)
             VALUES ($1, $2, $3, $4, $5)
             ON CONFLICT (address, message_id, device_id) DO UPDATE SET base_key = EXCLUDED.base_key",
        )
        .bind(address)
        .bind(message_id)
        .bind(base_key)
        .bind(self.device_id)
        .bind(now)
        .execute(&self.pool)
        .await
        .map_err(|e| StoreError::Database(Box::new(e)))?;
        Ok(())
    }

    async fn has_same_base_key(
        &self,
        address: &str,
        message_id: &str,
        current_base_key: &[u8],
    ) -> Result<bool> {
        let row = sqlx::query(
            "SELECT base_key FROM base_keys
             WHERE address = $1 AND message_id = $2 AND device_id = $3",
        )
        .bind(address)
        .bind(message_id)
        .bind(self.device_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| StoreError::Database(Box::new(e)))?;
        Ok(row.map(|r| r.get::<Vec<u8>, _>("base_key")).as_deref() == Some(current_base_key))
    }

    async fn delete_base_key(&self, address: &str, message_id: &str) -> Result<()> {
        sqlx::query(
            "DELETE FROM base_keys WHERE address = $1 AND message_id = $2 AND device_id = $3",
        )
        .bind(address)
        .bind(message_id)
        .bind(self.device_id)
        .execute(&self.pool)
        .await
        .map_err(|e| StoreError::Database(Box::new(e)))?;
        Ok(())
    }

    async fn update_device_list(&self, record: DeviceListRecord) -> Result<()> {
        let devices_json = serde_json::to_string(&record.devices)
            .map_err(|e| StoreError::Serialization(Box::new(e)))?;
        let now = wacore::time::now_secs();
        sqlx::query(
            "INSERT INTO device_registry
                (user_id, devices_json, timestamp, phash, device_id, updated_at, raw_id)
             VALUES ($1, $2, $3, $4, $5, $6, $7)
             ON CONFLICT (user_id, device_id) DO UPDATE SET
                devices_json = EXCLUDED.devices_json,
                timestamp = EXCLUDED.timestamp, phash = EXCLUDED.phash,
                updated_at = EXCLUDED.updated_at, raw_id = EXCLUDED.raw_id",
        )
        .bind(&*record.user)
        .bind(&devices_json)
        .bind(record.timestamp)
        .bind(&record.phash)
        .bind(self.device_id)
        .bind(now)
        .bind(record.raw_id.map(|v| v as i32))
        .execute(&self.pool)
        .await
        .map_err(|e| StoreError::Database(Box::new(e)))?;
        Ok(())
    }

    async fn update_device_lists(&self, records: Vec<DeviceListRecord>) -> Result<()> {
        if records.is_empty() {
            return Ok(());
        }
        let now = wacore::time::now_secs();
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| StoreError::Database(Box::new(e)))?;
        for record in &records {
            let devices_json = serde_json::to_string(&record.devices)
                .map_err(|e| StoreError::Serialization(Box::new(e)))?;
            sqlx::query(
                "INSERT INTO device_registry
                    (user_id, devices_json, timestamp, phash, device_id, updated_at, raw_id)
                 VALUES ($1, $2, $3, $4, $5, $6, $7)
                 ON CONFLICT (user_id, device_id) DO UPDATE SET
                    devices_json = EXCLUDED.devices_json,
                    timestamp = EXCLUDED.timestamp, phash = EXCLUDED.phash,
                    updated_at = EXCLUDED.updated_at, raw_id = EXCLUDED.raw_id",
            )
            .bind(&*record.user)
            .bind(&devices_json)
            .bind(record.timestamp)
            .bind(&record.phash)
            .bind(self.device_id)
            .bind(now)
            .bind(record.raw_id.map(|v| v as i32))
            .execute(&mut *tx)
            .await
            .map_err(|e| StoreError::Database(Box::new(e)))?;
        }
        tx.commit()
            .await
            .map_err(|e| StoreError::Database(Box::new(e)))?;
        Ok(())
    }

    async fn get_devices(&self, user: &str) -> Result<Option<DeviceListRecord>> {
        let row = sqlx::query(
            "SELECT user_id, devices_json, timestamp, phash, raw_id
             FROM device_registry WHERE user_id = $1 AND device_id = $2",
        )
        .bind(user)
        .bind(self.device_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| StoreError::Database(Box::new(e)))?;

        match row {
            None => Ok(None),
            Some(r) => {
                let devices_json: String = r.get("devices_json");
                let devices: Vec<DeviceInfo> = serde_json::from_str(&devices_json)
                    .map_err(|e| StoreError::Serialization(Box::new(e)))?;
                Ok(Some(DeviceListRecord {
                    user: r.get::<String, _>("user_id").into(),
                    devices: devices.into_boxed_slice(),
                    timestamp: r.get("timestamp"),
                    phash: r.get("phash"),
                    raw_id: r.get::<Option<i32>, _>("raw_id").map(|v| v as u32),
                }))
            }
        }
    }

    async fn delete_devices(&self, user: &str) -> Result<()> {
        sqlx::query("DELETE FROM device_registry WHERE user_id = $1 AND device_id = $2")
            .bind(user)
            .bind(self.device_id)
            .execute(&self.pool)
            .await
            .map_err(|e| StoreError::Database(Box::new(e)))?;
        Ok(())
    }

    async fn get_tc_token(&self, jid: &str) -> Result<Option<TcTokenEntry>> {
        let row = sqlx::query(
            "SELECT token, token_timestamp, sender_timestamp
             FROM tc_tokens WHERE jid = $1 AND device_id = $2",
        )
        .bind(jid)
        .bind(self.device_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| StoreError::Database(Box::new(e)))?;
        Ok(row.map(|r| TcTokenEntry {
            token: r.get("token"),
            token_timestamp: r.get("token_timestamp"),
            sender_timestamp: r.get("sender_timestamp"),
        }))
    }

    async fn put_tc_token(&self, jid: &str, entry: &TcTokenEntry) -> Result<()> {
        let now = wacore::time::now_secs();
        sqlx::query(
            "INSERT INTO tc_tokens (jid, token, token_timestamp, sender_timestamp, device_id, updated_at)
             VALUES ($1, $2, $3, $4, $5, $6)
             ON CONFLICT (jid, device_id) DO UPDATE SET
                token = EXCLUDED.token,
                token_timestamp = EXCLUDED.token_timestamp,
                sender_timestamp = EXCLUDED.sender_timestamp,
                updated_at = EXCLUDED.updated_at",
        )
        .bind(jid)
        .bind(&entry.token)
        .bind(entry.token_timestamp)
        .bind(entry.sender_timestamp)
        .bind(self.device_id)
        .bind(now)
        .execute(&self.pool)
        .await
        .map_err(|e| StoreError::Database(Box::new(e)))?;
        Ok(())
    }

    async fn delete_tc_token(&self, jid: &str) -> Result<()> {
        sqlx::query("DELETE FROM tc_tokens WHERE jid = $1 AND device_id = $2")
            .bind(jid)
            .bind(self.device_id)
            .execute(&self.pool)
            .await
            .map_err(|e| StoreError::Database(Box::new(e)))?;
        Ok(())
    }

    async fn get_all_tc_token_jids(&self) -> Result<Vec<String>> {
        let rows = sqlx::query("SELECT jid FROM tc_tokens WHERE device_id = $1")
            .bind(self.device_id)
            .fetch_all(&self.pool)
            .await
            .map_err(|e| StoreError::Database(Box::new(e)))?;
        Ok(rows.into_iter().map(|r| r.get("jid")).collect())
    }

    async fn delete_expired_tc_tokens(&self, cutoff_timestamp: i64, _sender_cutoff: i64) -> Result<u32> {
        let result =
            sqlx::query("DELETE FROM tc_tokens WHERE token_timestamp < $1 AND device_id = $2")
                .bind(cutoff_timestamp)
                .bind(self.device_id)
                .execute(&self.pool)
                .await
                .map_err(|e| StoreError::Database(Box::new(e)))?;
        Ok(result.rows_affected() as u32)
    }

    async fn store_sent_message(
        &self,
        chat_jid: &str,
        message_id: &str,
        payload: &[u8],
    ) -> Result<()> {
        let now = wacore::time::now_secs();
        sqlx::query(
            "INSERT INTO sent_messages (chat_jid, message_id, payload, device_id, created_at)
             VALUES ($1, $2, $3, $4, $5)
             ON CONFLICT (chat_jid, message_id, device_id) DO UPDATE SET
                payload = EXCLUDED.payload, created_at = EXCLUDED.created_at",
        )
        .bind(chat_jid)
        .bind(message_id)
        .bind(payload)
        .bind(self.device_id)
        .bind(now)
        .execute(&self.pool)
        .await
        .map_err(|e| StoreError::Database(Box::new(e)))?;
        Ok(())
    }

    async fn take_sent_message(&self, chat_jid: &str, message_id: &str) -> Result<Option<Vec<u8>>> {
        let row = sqlx::query(
            "DELETE FROM sent_messages
             WHERE chat_jid = $1 AND message_id = $2 AND device_id = $3
             RETURNING payload",
        )
        .bind(chat_jid)
        .bind(message_id)
        .bind(self.device_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| StoreError::Database(Box::new(e)))?;
        Ok(row.map(|r| r.get::<Vec<u8>, _>("payload")))
    }

    async fn delete_expired_sent_messages(&self, cutoff_timestamp: i64) -> Result<u32> {
        let result =
            sqlx::query("DELETE FROM sent_messages WHERE created_at < $1 AND device_id = $2")
                .bind(cutoff_timestamp)
                .bind(self.device_id)
                .execute(&self.pool)
                .await
                .map_err(|e| StoreError::Database(Box::new(e)))?;
        Ok(result.rows_affected() as u32)
    }
}

#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
impl MsgSecretStore for PostgresStore {
    async fn put_msg_secrets(&self, entries: Vec<MsgSecretEntry>) -> Result<usize> {
        if entries.is_empty() {
            return Ok(0);
        }
        let now = wacore::time::now_secs();
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| StoreError::Database(Box::new(e)))?;
        let mut stored = 0usize;
        for entry in &entries {
            // On conflict, merge deadlines (0 = never wins, else later) and parent
            // ts (later non-zero wins) so a redelivery/edit never shortens a window.
            sqlx::query(
                "INSERT INTO msg_secrets
                     (chat, sender, msg_id, secret, device_id, created_at, expires_at, message_ts)
                 VALUES ($1,$2,$3,$4,$5,$6,$7,$8)
                 ON CONFLICT (chat, sender, msg_id, device_id) DO UPDATE SET
                     secret = EXCLUDED.secret,
                     created_at = EXCLUDED.created_at,
                     expires_at = CASE
                         WHEN msg_secrets.expires_at = 0 OR EXCLUDED.expires_at = 0 THEN 0
                         ELSE GREATEST(msg_secrets.expires_at, EXCLUDED.expires_at)
                     END,
                     message_ts = GREATEST(msg_secrets.message_ts, EXCLUDED.message_ts)",
            )
            .bind(&*entry.chat)
            .bind(&*entry.sender)
            .bind(&*entry.msg_id)
            .bind(entry.secret.as_slice())
            .bind(self.device_id)
            .bind(now)
            .bind(entry.expires_at)
            .bind(entry.message_ts)
            .execute(&mut *tx)
            .await
            .map_err(|e| StoreError::Database(Box::new(e)))?;
            stored += 1;
        }
        tx.commit()
            .await
            .map_err(|e| StoreError::Database(Box::new(e)))?;
        Ok(stored)
    }

    async fn get_msg_secret(
        &self,
        chat: &str,
        sender: &str,
        msg_id: &str,
    ) -> Result<Option<Vec<u8>>> {
        let row: Option<Vec<u8>> = sqlx::query_scalar(
            "SELECT secret FROM msg_secrets
             WHERE chat = $1 AND sender = $2 AND msg_id = $3 AND device_id = $4",
        )
        .bind(chat)
        .bind(sender)
        .bind(msg_id)
        .bind(self.device_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| StoreError::Database(Box::new(e)))?;
        Ok(row)
    }

    async fn get_msg_secret_with_ts(
        &self,
        chat: &str,
        sender: &str,
        msg_id: &str,
    ) -> Result<Option<(Vec<u8>, i64)>> {
        let row: Option<(Vec<u8>, i64)> = sqlx::query_as(
            "SELECT secret, message_ts FROM msg_secrets
             WHERE chat = $1 AND sender = $2 AND msg_id = $3 AND device_id = $4",
        )
        .bind(chat)
        .bind(sender)
        .bind(msg_id)
        .bind(self.device_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| StoreError::Database(Box::new(e)))?;
        Ok(row)
    }

    async fn delete_expired_msg_secrets(&self, cutoff_timestamp: i64) -> Result<u32> {
        // Rows with expires_at = 0 (never) are kept.
        let result = sqlx::query(
            "DELETE FROM msg_secrets
             WHERE device_id = $1 AND expires_at <> 0 AND expires_at <= $2",
        )
        .bind(self.device_id)
        .bind(cutoff_timestamp)
        .execute(&self.pool)
        .await
        .map_err(|e| StoreError::Database(Box::new(e)))?;
        Ok(result.rows_affected() as u32)
    }
}

#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
impl DeviceStore for PostgresStore {
    async fn save(&self, device: &CoreDevice) -> Result<()> {
        self.save_device_data_for_device(self.device_id, device)
            .await
    }

    async fn load(&self) -> Result<Option<CoreDevice>> {
        self.load_device_data_for_device(self.device_id).await
    }

    async fn exists(&self) -> Result<bool> {
        self.device_exists(self.device_id).await
    }

    async fn create(&self) -> Result<i32> {
        self.create_new_device().await
    }
    // snapshot_db: no-op (default from DeviceStore trait — no file to copy in PostgreSQL)
}

#[cfg(test)]
impl PostgresStore {
    async fn wipe_for_test(&self) {
        let did = self.device_id;
        let pool = &self.pool;
        for stmt in &[
            "DELETE FROM device WHERE id = $1",
            "DELETE FROM identities WHERE device_id = $1",
            "DELETE FROM sessions WHERE device_id = $1",
            "DELETE FROM prekeys WHERE device_id = $1",
            "DELETE FROM sender_keys WHERE device_id = $1",
            "DELETE FROM signed_prekeys WHERE device_id = $1",
            "DELETE FROM app_state_keys WHERE device_id = $1",
            "DELETE FROM app_state_versions WHERE device_id = $1",
            "DELETE FROM app_state_mutation_macs WHERE device_id = $1",
            "DELETE FROM lid_pn_mapping WHERE device_id = $1",
            "DELETE FROM base_keys WHERE device_id = $1",
            "DELETE FROM device_registry WHERE device_id = $1",
            "DELETE FROM sender_key_devices WHERE device_id = $1",
            "DELETE FROM tc_tokens WHERE device_id = $1",
            "DELETE FROM sent_messages WHERE device_id = $1",
        ] {
            sqlx::query(stmt).bind(did).execute(pool).await.ok();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use portable_atomic::AtomicI32;
    use std::sync::atomic::Ordering;

    static DEVICE_COUNTER: AtomicI32 = AtomicI32::new(1000);

    fn test_db_url() -> String {
        std::env::var("TEST_DATABASE_URL")
            .or_else(|_| std::env::var("DATABASE_URL"))
            .unwrap_or_else(|_| "postgres://wa:wa@localhost:5433/whatsapp".to_string())
    }

    async fn create_test_store() -> PostgresStore {
        let device_id = DEVICE_COUNTER.fetch_add(1, Ordering::Relaxed);
        let store = PostgresStore::new_for_device(&test_db_url(), device_id)
            .await
            .expect("Failed to connect to test Postgres — is docker compose up?");
        store.wipe_for_test().await;
        store
    }

    #[tokio::test]
    async fn test_device_create_and_load() {
        let store = create_test_store().await;
        assert!(!store.device_exists(store.device_id()).await.unwrap());
        let id = store.create_new_device().await.unwrap();
        assert_eq!(id, store.device_id());
        assert!(store.device_exists(store.device_id()).await.unwrap());
        let loaded = store
            .load_device_data_for_device(store.device_id())
            .await
            .unwrap();
        assert!(loaded.is_some());
    }

    #[tokio::test]
    async fn test_device_save_and_load_roundtrip() {
        let store = create_test_store().await;
        store.create_new_device().await.unwrap();

        let mut device = store
            .load_device_data_for_device(store.device_id())
            .await
            .unwrap()
            .unwrap();
        device.push_name = "TestBot".to_string();
        device.next_pre_key_id = 42;
        store
            .save_device_data_for_device(store.device_id(), &device)
            .await
            .unwrap();

        let reloaded = store
            .load_device_data_for_device(store.device_id())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(reloaded.push_name, "TestBot");
        assert_eq!(reloaded.next_pre_key_id, 42);
    }

    #[tokio::test]
    async fn test_server_cert_chain_roundtrip() {
        use wacore::store::device::{CachedNoiseCert, CachedServerCertChain};
        let store = create_test_store().await;
        store.create_new_device().await.unwrap();

        let chain = CachedServerCertChain {
            intermediate: CachedNoiseCert {
                key: [0xAB; 32],
                not_before: 1_700_000_000,
                not_after: 1_900_000_000,
            },
            leaf: CachedNoiseCert {
                key: [0xCD; 32],
                not_before: 1_700_000_500,
                not_after: 1_899_999_500,
            },
        };

        let mut device = store
            .load_device_data_for_device(store.device_id())
            .await
            .unwrap()
            .unwrap();
        device.server_cert_chain = Some(chain.clone());
        store
            .save_device_data_for_device(store.device_id(), &device)
            .await
            .unwrap();

        let loaded = store
            .load_device_data_for_device(store.device_id())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(loaded.server_cert_chain.as_ref(), Some(&chain));

        let mut device = loaded;
        device.server_cert_chain = None;
        store
            .save_device_data_for_device(store.device_id(), &device)
            .await
            .unwrap();
        let reloaded = store
            .load_device_data_for_device(store.device_id())
            .await
            .unwrap()
            .unwrap();
        assert!(reloaded.server_cert_chain.is_none());
    }

    #[tokio::test]
    async fn test_identity_put_load_delete() {
        let store = create_test_store().await;
        let key = [0xAAu8; 32];
        store
            .put_identity("addr1@s.whatsapp.net", key)
            .await
            .unwrap();
        let loaded = store.load_identity("addr1@s.whatsapp.net").await.unwrap();
        assert_eq!(loaded, Some(key));
        store.delete_identity("addr1@s.whatsapp.net").await.unwrap();
        assert!(
            store
                .load_identity("addr1@s.whatsapp.net")
                .await
                .unwrap()
                .is_none()
        );
    }

    #[tokio::test]
    async fn test_session_put_get_delete() {
        let store = create_test_store().await;
        store
            .put_session("peer@s.whatsapp.net", &[1, 2, 3, 4])
            .await
            .unwrap();
        assert!(store.has_session("peer@s.whatsapp.net").await.unwrap());
        let record = store
            .get_session("peer@s.whatsapp.net")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(record.as_ref(), &[1, 2, 3, 4]);
        store.delete_session("peer@s.whatsapp.net").await.unwrap();
        assert!(!store.has_session("peer@s.whatsapp.net").await.unwrap());
    }

    #[tokio::test]
    async fn test_prekeys_store_load_remove() {
        let store = create_test_store().await;
        store.store_prekey(1, &[0xAA; 32], false).await.unwrap();
        store.store_prekey(2, &[0xBB; 32], true).await.unwrap();
        let k1 = store.load_prekey(1).await.unwrap().unwrap();
        assert_eq!(k1.as_ref(), &[0xAA; 32]);
        assert_eq!(store.get_max_prekey_id().await.unwrap(), 2);
        store.remove_prekey(1).await.unwrap();
        assert!(store.load_prekey(1).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn test_prekeys_batch() {
        use bytes::Bytes;
        let store = create_test_store().await;
        let keys: Vec<(u32, Bytes)> = vec![
            (10, Bytes::from(vec![0x10; 32])),
            (11, Bytes::from(vec![0x11; 32])),
            (12, Bytes::from(vec![0x12; 32])),
        ];
        store.store_prekeys_batch(&keys, true).await.unwrap();
        let loaded = store.load_prekeys_batch(&[10, 11, 12]).await.unwrap();
        assert_eq!(loaded.len(), 3);
    }

    #[tokio::test]
    async fn test_signed_prekey_store_load_remove() {
        let store = create_test_store().await;
        store.store_signed_prekey(1, &[0xCC; 64]).await.unwrap();
        let loaded = store.load_signed_prekey(1).await.unwrap().unwrap();
        assert_eq!(loaded, vec![0xCC; 64]);
        let all = store.load_all_signed_prekeys().await.unwrap();
        assert!(!all.is_empty());
        store.remove_signed_prekey(1).await.unwrap();
        assert!(store.load_signed_prekey(1).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn test_sender_key_put_get_delete() {
        let store = create_test_store().await;
        store
            .put_sender_key("group1@g.us:0@lid", &[0xDD; 64])
            .await
            .unwrap();
        let loaded = store
            .get_sender_key("group1@g.us:0@lid")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(loaded, vec![0xDD; 64]);
        store.delete_sender_key("group1@g.us:0@lid").await.unwrap();
        assert!(
            store
                .get_sender_key("group1@g.us:0@lid")
                .await
                .unwrap()
                .is_none()
        );
    }

    #[tokio::test]
    async fn test_device_registry_save_and_get() {
        let store = create_test_store().await;
        let record = DeviceListRecord {
            user: "1234567890".to_string().into(),
            devices: vec![
                DeviceInfo {
                    device_id: 0,
                    // key_index: None,
                },
                DeviceInfo {
                    device_id: 1,
                    // key_index: Some(42),
                },
            ],
            timestamp: 1234567890,
            phash: Some("2:abcdef".to_string().into()),
            raw_id: None,
        };
        store.update_device_list(record).await.unwrap();
        let loaded = store.get_devices("1234567890").await.unwrap().unwrap();
        assert_eq!(loaded.user, "1234567890");
        assert_eq!(loaded.devices.len(), 2);
        // assert_eq!(loaded.devices[1].key_index, Some(42));
        assert_eq!(loaded.phash, Some("2:abcdef".to_string()));
    }

    #[tokio::test]
    async fn test_device_registry_update_existing() {
        let store = create_test_store().await;
        let record1 = DeviceListRecord {
            user: "9876543210".to_string().into(),
            devices: vec![wacore::store::traits::DeviceInfo::new(0, None)].into(),
            timestamp: 1000,
            phash: Some("2:old".to_string().into()),
            raw_id: None,
        };
        store.update_device_list(record1).await.unwrap();
        let record2 = DeviceListRecord {
            user: "9876543210".to_string().into(),
            devices: vec![
                DeviceInfo {
                    device_id: 0,
                    // key_index: None,
                },
                DeviceInfo {
                    device_id: 2,
                    // key_index: None,
                },
            ],
            timestamp: 2000,
            phash: Some("2:new".to_string().into()),
            raw_id: None,
        };
        store.update_device_list(record2).await.unwrap();
        let loaded = store.get_devices("9876543210").await.unwrap().unwrap();
        assert_eq!(loaded.devices.len(), 2);
        assert_eq!(loaded.phash, Some("2:new".into()));
    }

    #[tokio::test]
    async fn test_device_registry_get_nonexistent() {
        let store = create_test_store().await;
        assert!(
            store
                .get_devices("nonexistent_user")
                .await
                .unwrap()
                .is_none()
        );
    }

    #[tokio::test]
    async fn test_sender_key_devices_set_and_get() {
        let store = create_test_store().await;
        let group = "group123@g.us";
        store
            .set_sender_key_status(group, &[("user1:5@lid", true), ("user2:3@lid", false)])
            .await
            .unwrap();
        let devices = store.get_sender_key_devices(group).await.unwrap();
        assert_eq!(devices.len(), 2);
        assert!(devices.contains(&("user1:5@lid".to_string(), true)));
        assert!(devices.contains(&("user2:3@lid".to_string(), false)));
    }

    #[tokio::test]
    async fn test_sender_key_devices_upsert_overwrites() {
        let store = create_test_store().await;
        let group = "group456@g.us";
        store
            .set_sender_key_status(group, &[("user1:5@lid", false)])
            .await
            .unwrap();
        store
            .set_sender_key_status(group, &[("user1:5@lid", true)])
            .await
            .unwrap();
        let devices = store.get_sender_key_devices(group).await.unwrap();
        assert_eq!(devices.len(), 1);
        assert_eq!(devices[0], ("user1:5@lid".to_string(), true));
    }

    #[tokio::test]
    async fn test_sender_key_devices_clear() {
        let store = create_test_store().await;
        let group = "group789@g.us";
        store
            .set_sender_key_status(group, &[("user1:5@lid", true), ("user2:3@lid", true)])
            .await
            .unwrap();
        store.clear_sender_key_devices(group).await.unwrap();
        assert!(
            store
                .get_sender_key_devices(group)
                .await
                .unwrap()
                .is_empty()
        );
    }

    #[tokio::test]
    async fn test_tc_token_put_get_delete() {
        let store = create_test_store().await;
        let entry = TcTokenEntry {
            token: vec![1, 2, 3, 4, 5],
            token_timestamp: 1707000000,
            sender_timestamp: Some(1707000100),
        };
        store.put_tc_token("user@lid", &entry).await.unwrap();
        let loaded = store.get_tc_token("user@lid").await.unwrap().unwrap();
        assert_eq!(loaded.token, vec![1, 2, 3, 4, 5]);
        assert_eq!(loaded.token_timestamp, 1707000000);
        assert_eq!(loaded.sender_timestamp, Some(1707000100));
        store.delete_tc_token("user@lid").await.unwrap();
        assert!(store.get_tc_token("user@lid").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn test_tc_token_upsert() {
        let store = create_test_store().await;
        let e1 = TcTokenEntry {
            token: vec![1, 2, 3],
            token_timestamp: 1000,
            sender_timestamp: None,
        };
        store.put_tc_token("user2@lid", &e1).await.unwrap();
        let e2 = TcTokenEntry {
            token: vec![4, 5, 6],
            token_timestamp: 2000,
            sender_timestamp: Some(1500),
        };
        store.put_tc_token("user2@lid", &e2).await.unwrap();
        let loaded = store.get_tc_token("user2@lid").await.unwrap().unwrap();
        assert_eq!(loaded.token, vec![4, 5, 6]);
        assert_eq!(loaded.sender_timestamp, Some(1500));
    }

    #[tokio::test]
    async fn test_tc_token_get_all_jids() {
        let store = create_test_store().await;
        let entry = TcTokenEntry {
            token: vec![1],
            token_timestamp: 1000,
            sender_timestamp: None,
        };
        store.put_tc_token("jid1@lid", &entry).await.unwrap();
        store.put_tc_token("jid2@lid", &entry).await.unwrap();
        store.put_tc_token("jid3@lid", &entry).await.unwrap();
        let mut jids = store.get_all_tc_token_jids().await.unwrap();
        jids.sort();
        assert_eq!(jids, vec!["jid1@lid", "jid2@lid", "jid3@lid"]);
    }

    #[tokio::test]
    async fn test_tc_token_delete_expired() {
        let store = create_test_store().await;
        let old = TcTokenEntry {
            token: vec![1],
            token_timestamp: 1000,
            sender_timestamp: None,
        };
        let recent = TcTokenEntry {
            token: vec![2],
            token_timestamp: 5000,
            sender_timestamp: None,
        };
        store.put_tc_token("old@lid", &old).await.unwrap();
        store.put_tc_token("recent@lid", &recent).await.unwrap();
        let deleted = store.delete_expired_tc_tokens(3000, 3000).await.unwrap();
        assert_eq!(deleted, 1);
        assert!(store.get_tc_token("old@lid").await.unwrap().is_none());
        assert!(store.get_tc_token("recent@lid").await.unwrap().is_some());
    }

    #[tokio::test]
    async fn test_sent_message_store_take() {
        let store = create_test_store().await;
        store
            .store_sent_message("chat@s.whatsapp.net", "msg001", &[0xDE, 0xAD])
            .await
            .unwrap();
        let payload = store
            .take_sent_message("chat@s.whatsapp.net", "msg001")
            .await
            .unwrap();
        assert_eq!(payload, Some(vec![0xDE, 0xAD]));
        // second take returns None (consumed)
        let again = store
            .take_sent_message("chat@s.whatsapp.net", "msg001")
            .await
            .unwrap();
        assert!(again.is_none());
    }

    #[tokio::test]
    async fn test_sent_message_delete_expired() {
        let store = create_test_store().await;
        // Insert with a backdated created_at by storing then updating directly
        store
            .store_sent_message("chat@s.whatsapp.net", "old_msg", &[1])
            .await
            .unwrap();
        store
            .store_sent_message("chat@s.whatsapp.net", "new_msg", &[2])
            .await
            .unwrap();
        // Force old_msg timestamp to 1 so it's always expired
        sqlx::query(
            "UPDATE sent_messages SET created_at = 1 WHERE message_id = $1 AND device_id = $2",
        )
        .bind("old_msg")
        .bind(store.device_id())
        .execute(&store.pool)
        .await
        .unwrap();
        let deleted = store.delete_expired_sent_messages(1000).await.unwrap();
        assert_eq!(deleted, 1);
        assert!(
            store
                .take_sent_message("chat@s.whatsapp.net", "old_msg")
                .await
                .unwrap()
                .is_none()
        );
        assert!(
            store
                .take_sent_message("chat@s.whatsapp.net", "new_msg")
                .await
                .unwrap()
                .is_some()
        );
    }

    #[tokio::test]
    async fn test_lid_mapping_put_get() {
        let store = create_test_store().await;
        let entry = LidPnMappingEntry {
            lid: "lid123@lid".to_string(),
            phone_number: "15551234567".to_string(),
            created_at: 1000,
            updated_at: 2000,
            learning_source: "contact".to_string(),
        };
        store.put_lid_mapping(&entry).await.unwrap();
        let by_lid = store.get_lid_mapping("lid123@lid").await.unwrap().unwrap();
        assert_eq!(by_lid.phone_number, "15551234567");
        let by_pn = store.get_pn_mapping("15551234567").await.unwrap().unwrap();
        assert_eq!(by_pn.lid, "lid123@lid");
    }

    #[tokio::test]
    async fn test_base_key_save_check_delete() {
        let store = create_test_store().await;
        let base_key = [0xBBu8; 32];
        store
            .save_base_key("peer@s.whatsapp.net", "msgXYZ", &base_key)
            .await
            .unwrap();
        assert!(
            store
                .has_same_base_key("peer@s.whatsapp.net", "msgXYZ", &base_key)
                .await
                .unwrap()
        );
        assert!(
            !store
                .has_same_base_key("peer@s.whatsapp.net", "msgXYZ", &[0xFF; 32])
                .await
                .unwrap()
        );
        store
            .delete_base_key("peer@s.whatsapp.net", "msgXYZ")
            .await
            .unwrap();
        assert!(
            !store
                .has_same_base_key("peer@s.whatsapp.net", "msgXYZ", &base_key)
                .await
                .unwrap()
        );
    }

    #[tokio::test]
    async fn test_app_state_sync_key_roundtrip() {
        let store = create_test_store().await;
        let key_id = vec![0x01u8, 0x02, 0x03];
        let key = AppStateSyncKey {
            key_data: vec![0xAA; 32],
            fingerprint: vec![0xBB; 6],
            timestamp: 12345,
        };
        store.set_sync_key(&key_id, key.clone()).await.unwrap();
        let loaded = store.get_sync_key(&key_id).await.unwrap().unwrap();
        assert_eq!(loaded.key_data, vec![0xAA; 32]);
        assert_eq!(loaded.timestamp, 12345);
        let latest = store.get_latest_sync_key_id().await.unwrap();
        assert!(latest.is_some());
    }

    #[tokio::test]
    async fn test_app_state_sync_key_nonexistent() {
        let store = create_test_store().await;
        let result = store.get_sync_key(&[0xFF, 0xFF]).await.unwrap();
        assert!(result.is_none());
        let latest = store.get_latest_sync_key_id().await.unwrap();
        assert!(latest.is_none());
    }

    #[tokio::test]
    async fn test_app_state_sync_key_upsert() {
        let store = create_test_store().await;
        let key_id = vec![0x10u8];
        store
            .set_sync_key(
                &key_id,
                AppStateSyncKey {
                    key_data: vec![1; 32],
                    fingerprint: vec![],
                    timestamp: 1,
                },
            )
            .await
            .unwrap();
        store
            .set_sync_key(
                &key_id,
                AppStateSyncKey {
                    key_data: vec![2; 32],
                    fingerprint: vec![],
                    timestamp: 2,
                },
            )
            .await
            .unwrap();
        let loaded = store.get_sync_key(&key_id).await.unwrap().unwrap();
        assert_eq!(loaded.key_data, vec![2; 32]);
        assert_eq!(loaded.timestamp, 2);
    }

    #[tokio::test]
    async fn test_app_state_version_roundtrip() {
        let store = create_test_store().await;
        // Default is zero/empty HashState
        let initial = store.get_version("critical_unblock_to_sync").await.unwrap();
        assert_eq!(initial.version, 0);
        assert_eq!(initial.hash, [0u8; 128]);

        let mut state = HashState::default();
        state.version = 42;
        state.hash = [0xAB; 128];
        store
            .set_version("critical_unblock_to_sync", state.clone())
            .await
            .unwrap();

        let loaded = store.get_version("critical_unblock_to_sync").await.unwrap();
        assert_eq!(loaded.version, 42);
        assert_eq!(loaded.hash, [0xAB; 128]);

        // Different name returns default
        let other = store
            .get_version("regular_high_level_contact_node")
            .await
            .unwrap();
        assert_eq!(other.version, 0);
        assert_eq!(other.hash, [0u8; 128]);
    }

    #[tokio::test]
    async fn test_app_state_version_upsert() {
        let store = create_test_store().await;
        let mut s1 = HashState::default();
        s1.version = 1;
        store.set_version("notify_privacy_info", s1).await.unwrap();

        let mut s2 = HashState::default();
        s2.version = 2;
        store.set_version("notify_privacy_info", s2).await.unwrap();

        let loaded = store.get_version("notify_privacy_info").await.unwrap();
        assert_eq!(loaded.version, 2);
    }

    #[tokio::test]
    async fn test_app_state_mutation_macs_put_get_delete() {
        use wacore::appstate::processor::AppStateMutationMAC;
        let store = create_test_store().await;
        let name = "critical_unblock_to_sync";
        let m1 = AppStateMutationMAC {
            index_mac: vec![0x01; 32],
            value_mac: vec![0xAA; 32],
        };
        let m2 = AppStateMutationMAC {
            index_mac: vec![0x02; 32],
            value_mac: vec![0xBB; 32],
        };

        store
            .put_mutation_macs(name, 1, &[m1.clone(), m2.clone()])
            .await
            .unwrap();

        let got = store
            .get_mutation_mac(name, &[0x01; 32])
            .await
            .unwrap()
            .unwrap();
        assert_eq!(got, vec![0xAA; 32]);
        let got2 = store
            .get_mutation_mac(name, &[0x02; 32])
            .await
            .unwrap()
            .unwrap();
        assert_eq!(got2, vec![0xBB; 32]);

        store
            .delete_mutation_macs(name, &[vec![0x01; 32]])
            .await
            .unwrap();
        assert!(
            store
                .get_mutation_mac(name, &[0x01; 32])
                .await
                .unwrap()
                .is_none()
        );
        // m2 unaffected
        assert!(
            store
                .get_mutation_mac(name, &[0x02; 32])
                .await
                .unwrap()
                .is_some()
        );
    }

    #[tokio::test]
    async fn test_app_state_mutation_macs_upsert() {
        use wacore::appstate::processor::AppStateMutationMAC;
        let store = create_test_store().await;
        let name = "regular_high_level_contact_node";
        let m = AppStateMutationMAC {
            index_mac: vec![0x11; 32],
            value_mac: vec![0x22; 32],
        };
        store.put_mutation_macs(name, 1, &[m]).await.unwrap();

        let m_updated = AppStateMutationMAC {
            index_mac: vec![0x11; 32],
            value_mac: vec![0x33; 32],
        };
        store
            .put_mutation_macs(name, 2, &[m_updated])
            .await
            .unwrap();

        let got = store
            .get_mutation_mac(name, &[0x11; 32])
            .await
            .unwrap()
            .unwrap();
        assert_eq!(got, vec![0x33; 32]);
    }

    #[tokio::test]
    async fn test_app_state_mutation_macs_empty_slice() {
        let store = create_test_store().await;
        // Empty slice should not error
        store.put_mutation_macs("any_name", 1, &[]).await.unwrap();
        store.delete_mutation_macs("any_name", &[]).await.unwrap();
    }

    #[tokio::test]
    async fn test_put_lid_mappings_batch() {
        let store = create_test_store().await;
        let entries = vec![
            LidPnMappingEntry {
                lid: "lid1@lid".to_string(),
                phone_number: "15551111111".to_string(),
                created_at: 1000,
                updated_at: 1000,
                learning_source: "usync".to_string(),
            },
            LidPnMappingEntry {
                lid: "lid2@lid".to_string(),
                phone_number: "15552222222".to_string(),
                created_at: 2000,
                updated_at: 2000,
                learning_source: "usync".to_string(),
            },
        ];
        store.put_lid_mappings(&entries).await.unwrap();

        let e1 = store.get_lid_mapping("lid1@lid").await.unwrap().unwrap();
        assert_eq!(e1.phone_number, "15551111111");
        let e2 = store.get_lid_mapping("lid2@lid").await.unwrap().unwrap();
        assert_eq!(e2.phone_number, "15552222222");
    }

    #[tokio::test]
    async fn test_get_all_lid_mappings() {
        let store = create_test_store().await;
        assert!(store.get_all_lid_mappings().await.unwrap().is_empty());

        let entries = vec![
            LidPnMappingEntry {
                lid: "l1@lid".to_string(),
                phone_number: "10000000001".to_string(),
                created_at: 1,
                updated_at: 1,
                learning_source: "contact".to_string(),
            },
            LidPnMappingEntry {
                lid: "l2@lid".to_string(),
                phone_number: "10000000002".to_string(),
                created_at: 2,
                updated_at: 2,
                learning_source: "contact".to_string(),
            },
        ];
        store.put_lid_mappings(&entries).await.unwrap();
        let all = store.get_all_lid_mappings().await.unwrap();
        assert_eq!(all.len(), 2);
    }

    #[tokio::test]
    async fn test_update_device_lists_batch() {
        let store = create_test_store().await;
        let records = vec![
            DeviceListRecord {
                user: "batch_user1".to_string().into(),
                devices: vec![wacore::store::traits::DeviceInfo::new(0, None)].into(),
                timestamp: 100,
                phash: None,
                raw_id: None,
            },
            DeviceListRecord {
                user: "batch_user2".to_string().into(),
                devices: vec![wacore::store::traits::DeviceInfo::new(0, None)].into(),
                timestamp: 200,
                phash: Some("2:xyz".to_string().into()),
                raw_id: Some(7),
            },
        ];
        store.update_device_lists(records).await.unwrap();

        let r1 = store.get_devices("batch_user1").await.unwrap().unwrap();
        assert_eq!(r1.devices.len(), 1);
        let r2 = store.get_devices("batch_user2").await.unwrap().unwrap();
        assert_eq!(r2.phash, Some("2:xyz".into()));
        assert_eq!(r2.raw_id, Some(7));
    }

    #[tokio::test]
    async fn test_device_registry_delete() {
        let store = create_test_store().await;
        let record = DeviceListRecord {
            user: "to_delete".to_string().into(),
            devices: vec![wacore::store::traits::DeviceInfo::new(0, None)].into(),
            timestamp: 1,
            phash: None,
            raw_id: None,
        };
        store.update_device_list(record).await.unwrap();
        assert!(store.get_devices("to_delete").await.unwrap().is_some());
        store.delete_devices("to_delete").await.unwrap();
        assert!(store.get_devices("to_delete").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn test_sender_key_devices_different_groups() {
        let store = create_test_store().await;
        let group1 = "groupA@g.us";
        let group2 = "groupB@g.us";
        store
            .set_sender_key_status(group1, &[("user:5@lid", true)])
            .await
            .unwrap();

        let g1 = store.get_sender_key_devices(group1).await.unwrap();
        assert_eq!(g1.len(), 1);
        let g2 = store.get_sender_key_devices(group2).await.unwrap();
        assert!(g2.is_empty());
    }

    #[tokio::test]
    async fn test_clear_all_sender_key_devices() {
        let store = create_test_store().await;
        store
            .set_sender_key_status("groupX@g.us", &[("u1:0@lid", true)])
            .await
            .unwrap();
        store
            .set_sender_key_status("groupY@g.us", &[("u2:0@lid", true)])
            .await
            .unwrap();
        store.clear_all_sender_key_devices().await.unwrap();
        assert!(
            store
                .get_sender_key_devices("groupX@g.us")
                .await
                .unwrap()
                .is_empty()
        );
        assert!(
            store
                .get_sender_key_devices("groupY@g.us")
                .await
                .unwrap()
                .is_empty()
        );
    }

    #[tokio::test]
    async fn test_delete_sender_key_device_rows() {
        let store = create_test_store().await;
        let group = "groupZ@g.us";
        store
            .set_sender_key_status(
                group,
                &[("u1:0@lid", true), ("u2:0@lid", false), ("u3:0@lid", true)],
            )
            .await
            .unwrap();
        store
            .delete_sender_key_device_rows(&["u1:0@lid", "u3:0@lid"])
            .await
            .unwrap();
        let remaining = store.get_sender_key_devices(group).await.unwrap();
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].0, "u2:0@lid");
    }

    #[tokio::test]
    async fn test_create_new_device_uses_configured_device_id() {
        let store = create_test_store().await;
        assert!(!store.device_exists(store.device_id()).await.unwrap());
        let returned_id = store.create_new_device().await.unwrap();
        assert_eq!(returned_id, store.device_id());
        assert!(store.device_exists(store.device_id()).await.unwrap());
    }

    #[tokio::test]
    async fn test_device_store_trait_save_load_create_exists() {
        let store = create_test_store().await;
        assert!(!store.exists().await.unwrap());
        store.create().await.unwrap();
        assert!(store.exists().await.unwrap());
        let loaded = store.load().await.unwrap().unwrap();
        let mut device = loaded;
        device.push_name = "TraitTest".to_string();
        store.save(&device).await.unwrap();
        let reloaded = store.load().await.unwrap().unwrap();
        assert_eq!(reloaded.push_name, "TraitTest");
    }

    #[tokio::test]
    async fn test_load_device_data_nonexistent_returns_none() {
        let store = create_test_store().await;
        // device row was wiped by wipe_for_test; load must return None, not an error
        let result = store
            .load_device_data_for_device(store.device_id())
            .await
            .unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_update_device_list_with_raw_id() {
        let store = create_test_store().await;
        let record = DeviceListRecord {
            user: "raw_id_user".to_string().into(),
            devices: vec![wacore::store::traits::DeviceInfo::new(0, None)].into(),
            timestamp: 999,
            phash: None,
            raw_id: Some(42),
        };
        store.update_device_list(record).await.unwrap();
        let loaded = store.get_devices("raw_id_user").await.unwrap().unwrap();
        assert_eq!(loaded.raw_id, Some(42));
    }
}
