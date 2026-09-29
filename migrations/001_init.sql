-- Første skjema (SPEC §7.2).
-- Alle tidspunkt lagres som unix-sekunder (UTC). Alle beløp i øre, alle volum i ml.

CREATE TABLE produkt (
    id          TEXT PRIMARY KEY,
    navn        TEXT NOT NULL,
    merke       TEXT NOT NULL,
    linje       TEXT,
    smak        TEXT NOT NULL,
    sukkerfri   INTEGER NOT NULL,
    volum_ml    INTEGER NOT NULL CHECK (volum_ml > 0),
    beholder    TEXT NOT NULL CHECK (beholder IN ('boks', 'flaske')),
    egenmerke   INTEGER NOT NULL DEFAULT 0,
    ad_hoc      INTEGER NOT NULL DEFAULT 0
) STRICT;

CREATE TABLE oppforing (
    id                INTEGER PRIMARY KEY,
    kilde             TEXT NOT NULL,
    kjede             TEXT NOT NULL,
    kilde_produkt_id  TEXT NOT NULL,
    gtin              TEXT,
    produkt_id        TEXT REFERENCES produkt (id),
    antall            INTEGER NOT NULL DEFAULT 1 CHECK (antall > 0),
    raanavn           TEXT NOT NULL,
    verifisert        INTEGER NOT NULL DEFAULT 0,
    forst_sett        INTEGER NOT NULL,
    sist_sett         INTEGER NOT NULL,
    UNIQUE (kilde, kjede, kilde_produkt_id)
) STRICT;

CREATE INDEX oppforing_produkt ON oppforing (produkt_id);
CREATE INDEX oppforing_gtin ON oppforing (gtin);

-- Endringsbasert historikk: en ny rad bare når prisen (eller tilbudet) endrer seg.
-- Ellers oppdateres bare sist_sett på den nyeste raden.
CREATE TABLE prisintervall (
    id               INTEGER PRIMARY KEY,
    oppforing_id     INTEGER NOT NULL REFERENCES oppforing (id) ON DELETE CASCADE,
    hyllepris_ore    INTEGER NOT NULL,
    medlemspris_ore  INTEGER,
    medlemsprogram   TEXT,
    tilbud_json      TEXT,
    tilgjengelig     INTEGER,
    mistenkelig      INTEGER NOT NULL DEFAULT 0,
    gyldig_fra       INTEGER NOT NULL,
    sist_sett        INTEGER NOT NULL
) STRICT;

CREATE INDEX prisintervall_oppforing ON prisintervall (oppforing_id, gyldig_fra);

CREATE TABLE henting (
    id                  INTEGER PRIMARY KEY,
    kilde               TEXT NOT NULL,
    startet             INTEGER NOT NULL,
    fullfort            INTEGER NOT NULL,
    status              TEXT NOT NULL CHECK (status IN ('ok', 'feilet')),
    feilmelding         TEXT,
    antall_oppforinger  INTEGER
) STRICT;

CREATE INDEX henting_kilde ON henting (kilde, startet);

-- sist_utlost_intervall hindrer at samme varsel sendes på nytt for en uendret pris (SPEC §7.7).
CREATE TABLE varsel (
    id                     INTEGER PRIMARY KEY,
    produkt_id             TEXT NOT NULL REFERENCES produkt (id),
    kjede                  TEXT,
    grense_ore             INTEGER NOT NULL,
    grensetype             TEXT NOT NULL CHECK (grensetype IN ('enhetspris', 'literpris')),
    opprettet              INTEGER NOT NULL,
    sist_utlost_intervall  INTEGER REFERENCES prisintervall (id)
) STRICT;
