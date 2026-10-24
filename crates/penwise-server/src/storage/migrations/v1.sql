-- Penwise server schema v1 (docs/contracts/m3-server.md "Storage schema").
-- Applied once by rusqlite_migration (PRAGMA user_version 0 → 1). Never edit after it ships: add v2.sql.
--
-- Conventions:
-- * ids are 16-byte ULID BLOBs (penwise-proto ids, `to_bytes`);
-- * HLC stamps are 24-byte BLOBs (`Hlc::to_bytes`: big-endian wall_ms<<16|counter, then device id), so
--   SQLite's memcmp BLOB order is the LWW order and `a < b` compares stamps directly;
-- * times are Unix ms INTEGERs; booleans are 0/1 INTEGERs;
-- * connection pragmas (WAL, synchronous=NORMAL, foreign_keys=ON, busy_timeout) are set on open, not here.

-- Server identity and small settings. Keys: 'server_id' (16-byte ULID BLOB), 'created_ms' (INTEGER).
CREATE TABLE server_meta (
    key   TEXT PRIMARY KEY,
    value ANY  NOT NULL
) STRICT, WITHOUT ROWID;

-- Paired clients. `kind` = proto ClientKind::code (0 tablet, 1 browser, 2 agent).
CREATE TABLE devices (
    id           BLOB    PRIMARY KEY CHECK (length(id) = 16),
    name         TEXT    NOT NULL CHECK (length(CAST(name AS BLOB)) BETWEEN 1 AND 64),
    kind         INTEGER NOT NULL CHECK (kind >= 0),
    created_ms   INTEGER NOT NULL,
    last_seen_ms INTEGER
) STRICT, WITHOUT ROWID;

-- Long-lived device tokens. Only SHA-256(token text) is stored; the token itself never touches disk.
-- A device may hold several tokens over time (re-pairing); revoking sets revoked_ms, rows are kept for audit.
CREATE TABLE tokens (
    hash         BLOB    PRIMARY KEY CHECK (length(hash) = 32),
    device       BLOB    NOT NULL REFERENCES devices (id),
    created_ms   INTEGER NOT NULL,
    last_used_ms INTEGER,
    revoked_ms   INTEGER
) STRICT, WITHOUT ROWID;
CREATE INDEX tokens_by_device ON tokens (device);

-- Append-only op log. `seq` is what clients track as last_seen_seq: AUTOINCREMENT guarantees it is never
-- reused, even after a delete. `op_id UNIQUE` makes outbox replay idempotent.
-- kind: proto AnyOp discriminant (0 item, 1 page, 2 notebook).
-- notebook: notebook ops and page creates; page: item and page ops; item: item ops (core Edit::target).
-- body: proto `encode_op` (version byte + postcard AnyOp; item edits inside are core PWED blobs).
CREATE TABLE ops (
    seq         INTEGER PRIMARY KEY AUTOINCREMENT,
    op_id       BLOB    NOT NULL UNIQUE CHECK (length(op_id) = 16),
    device      BLOB    NOT NULL REFERENCES devices (id),
    hlc         BLOB    NOT NULL CHECK (length(hlc) = 24),
    kind        INTEGER NOT NULL CHECK (kind >= 0),
    notebook    BLOB    CHECK (notebook IS NULL OR length(notebook) = 16),
    page        BLOB    CHECK (page IS NULL OR length(page) = 16),
    item        BLOB    CHECK (item IS NULL OR length(item) = 16),
    body        BLOB    NOT NULL,
    received_ms INTEGER NOT NULL
) STRICT;
CREATE INDEX ops_by_page ON ops (page, seq) WHERE page IS NOT NULL;
CREATE INDEX ops_by_item ON ops (item, seq) WHERE item IS NOT NULL;

-- Materialized notebooks: LWW registers (value + stamp) maintained in the same transaction as the append.
CREATE TABLE notebooks (
    id          BLOB    PRIMARY KEY CHECK (length(id) = 16),
    created_hlc BLOB    NOT NULL CHECK (length(created_hlc) = 24),
    title       TEXT    NOT NULL,
    title_hlc   BLOB    NOT NULL CHECK (length(title_hlc) = 24),
    deleted     INTEGER NOT NULL CHECK (deleted IN (0, 1)),
    deleted_hlc BLOB    NOT NULL CHECK (length(deleted_hlc) = 24),
    -- Largest hlc.wall_ms of any op on the notebook or its pages.
    updated_ms  INTEGER NOT NULL
) STRICT, WITHOUT ROWID;

-- Materialized pages. `notebook` and `kind`/`width`/`height` are immutable after create.
-- kind: 0 fixed (width/height page px, both set), 1 infinite (both NULL). background: proto Background::code.
CREATE TABLE pages (
    id             BLOB    PRIMARY KEY CHECK (length(id) = 16),
    notebook       BLOB    NOT NULL REFERENCES notebooks (id),
    created_hlc    BLOB    NOT NULL CHECK (length(created_hlc) = 24),
    kind           INTEGER NOT NULL CHECK (kind IN (0, 1)),
    width          REAL,
    height         REAL,
    position       INTEGER NOT NULL,
    position_hlc   BLOB    NOT NULL CHECK (length(position_hlc) = 24),
    background     INTEGER NOT NULL CHECK (background >= 0),
    background_hlc BLOB    NOT NULL CHECK (length(background_hlc) = 24),
    deleted        INTEGER NOT NULL CHECK (deleted IN (0, 1)),
    deleted_hlc    BLOB    NOT NULL CHECK (length(deleted_hlc) = 24),
    -- Largest hlc.wall_ms of any op on the page.
    updated_ms     INTEGER NOT NULL,
    CHECK ((kind = 0) = (width IS NOT NULL AND height IS NOT NULL)),
    CHECK (kind = 0 OR (width IS NULL AND height IS NULL))
) STRICT, WITHOUT ROWID;
CREATE INDEX pages_by_notebook ON pages (notebook, deleted, position, id);

-- Materialized items: current winning state of every item (tombstones included), one row per Edit::Add.
-- kind: core ItemKindTag::code. data: core PWIT v1 item blob holding the winning field values.
-- stamps: version byte 1 + postcard proto FieldStamps (per-field LWW stamps).
CREATE TABLE items (
    id          BLOB    PRIMARY KEY CHECK (length(id) = 16),
    page        BLOB    NOT NULL REFERENCES pages (id),
    kind        INTEGER NOT NULL CHECK (kind >= 0),
    deleted     INTEGER NOT NULL CHECK (deleted IN (0, 1)),
    data        BLOB    NOT NULL,
    stamps      BLOB    NOT NULL,
    added_seq   INTEGER NOT NULL REFERENCES ops (seq),
    updated_seq INTEGER NOT NULL REFERENCES ops (seq)
) STRICT;
CREATE INDEX items_by_page ON items (page, deleted);
