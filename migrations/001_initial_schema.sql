-- Initial schema (SPEC §7.2).
-- All timestamps are unix seconds (UTC). All amounts in øre, all volumes in ml.
-- Chain, source, container and membership values are the public slugs ("coop-extra",
-- "kassalapp", "boks", "kiwi-pluss").

CREATE TABLE product (
    id          TEXT PRIMARY KEY,
    name        TEXT NOT NULL,
    brand       TEXT NOT NULL,
    line        TEXT,
    flavor      TEXT NOT NULL,
    sugar_free  INTEGER NOT NULL,
    volume_ml   INTEGER NOT NULL CHECK (volume_ml > 0),
    container   TEXT NOT NULL CHECK (container IN ('boks', 'flaske')),
    store_brand INTEGER NOT NULL DEFAULT 0,
    ad_hoc      INTEGER NOT NULL DEFAULT 0
) STRICT;

CREATE TABLE listing (
    id                 INTEGER PRIMARY KEY,
    source             TEXT NOT NULL,
    chain              TEXT NOT NULL,
    source_product_id  TEXT NOT NULL,
    gtin               TEXT,
    product_id         TEXT REFERENCES product (id),
    pack_size          INTEGER NOT NULL DEFAULT 1 CHECK (pack_size > 0),
    raw_name           TEXT NOT NULL,
    verified           INTEGER NOT NULL DEFAULT 0,
    first_seen         INTEGER NOT NULL,
    last_seen          INTEGER NOT NULL,
    UNIQUE (source, chain, source_product_id)
) STRICT;

CREATE INDEX listing_product ON listing (product_id);
CREATE INDEX listing_gtin ON listing (gtin);

-- Change-only history: a new row only when the price (or offer) changes. Otherwise only
-- last_seen on the newest row is updated.
CREATE TABLE price_interval (
    id                  INTEGER PRIMARY KEY,
    listing_id          INTEGER NOT NULL REFERENCES listing (id) ON DELETE CASCADE,
    shelf_price_ore     INTEGER NOT NULL,
    member_price_ore    INTEGER,
    membership_program  TEXT,
    offer_json          TEXT,
    available           INTEGER,
    suspicious          INTEGER NOT NULL DEFAULT 0,
    valid_from          INTEGER NOT NULL,
    last_seen           INTEGER NOT NULL
) STRICT;

CREATE INDEX price_interval_listing ON price_interval (listing_id, valid_from);

CREATE TABLE fetch_log (
    id             INTEGER PRIMARY KEY,
    source         TEXT NOT NULL,
    started        INTEGER NOT NULL,
    finished       INTEGER NOT NULL,
    status         TEXT NOT NULL CHECK (status IN ('ok', 'failed')),
    error_message  TEXT,
    listing_count  INTEGER
) STRICT;

CREATE INDEX fetch_log_source ON fetch_log (source, started);

-- last_triggered_interval keeps the same alert from firing again for an unchanged
-- price (SPEC §7.7).
CREATE TABLE alert (
    id                       INTEGER PRIMARY KEY,
    product_id               TEXT NOT NULL REFERENCES product (id),
    chain                    TEXT,
    threshold_ore            INTEGER NOT NULL,
    threshold_kind           TEXT NOT NULL CHECK (threshold_kind IN ('unit_price', 'liter_price')),
    created                  INTEGER NOT NULL,
    last_triggered_interval  INTEGER REFERENCES price_interval (id)
) STRICT;
