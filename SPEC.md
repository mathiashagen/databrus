# databrus — Specification

A command-line tool, written in Rust, that finds the cheapest energy drinks in Norway. It fetches
current prices from Norwegian grocery chains and online stores and ranks them by price per liter,
with deposit (pant) shown separately. It flags current deals, keeps a local price history so users
can tell whether a deal is real, and outputs either a readable table or versioned JSON.

Status: draft v1 spec · 2026-09-29

---

## 1. Goals and non-goals

### Goals
- Answer "where is Monster Ultra White cheapest right now, per liter?" in one command.
- Cover the major Norwegian chains: Rema 1000, Kiwi, Meny, Spar, Joker, Coop (Extra, Obs, Mega, Prix), Bunnpris and Oda, plus Europris, Engrossnett and the online candy shops Havaristen and Fastcandy.
- Cover the major brands (Red Bull, Monster, Burn, Nocco, Battery and others) and store brands (Xtra, First Price, Coop-branded and so on).
- Rank fairly across single cans, multipacks and multi-buy offers.
- Build up local price history and use it to judge whether a deal is actually good.
- Offer a stable, versioned JSON output that scripts can rely on.

### Non-goals (v1)
- Prices for individual physical stores. Prices are **chain-level** (see §4.3).
- A GUI or web frontend.
- Caffeine-per-krone ranking. The catalog may carry caffeine data for the future, but it is not a v1 feature.
- Buying or ordering anything.

---

## 2. Language conventions

**All user-facing surfaces are Norwegian (bokmål).** That includes subcommands, flags, help text,
table headers, messages, config keys, JSON keys, the catalog file format (`katalog.toml`) and the
README.

**The source code is English**: identifiers, module and file names, comments, tests, the database
schema and commit messages. Where the two meet, the English Rust names carry explicit Norwegian
names (`serde(rename)` for JSON, config and catalog keys; `#[arg(long = …)]`, `#[command(name = …)]`
and `#[value(name = …)]` for the command line). Doc comments on clap items are the Norwegian help
text and stay Norwegian. Tests guard the boundary: the `--help` snapshot, a check that no English
command or flag name leaks, and key-set checks for the JSON and config contracts.

- **Command names, flag names, config keys and JSON keys use only ASCII.** `æ→ae`, `ø→o`, `å→a`
  (for example `--storrelse`, `overvak`). This keeps them easy to type on any keyboard layout and
  safe to use in shells.
- Human-readable output (table text, messages, help descriptions) uses proper Norwegian spelling
  with æøå.
- Numbers in human-readable output use Norwegian formatting: decimal comma, and a non-breaking
  space as the thousands separator (`24,90`, `1 249,00`). JSON always uses plain integers (see §9).
- Input accepts both a comma and a dot as the decimal separator (`0,5` and `0.5`).

---

## 3. CLI

Binary name: `databrus`.

### 3.1 Command overview

| Command | Purpose |
|---|---|
| `databrus [SØK...] [filtre]` | Default: search. Free text plus filters. |
| `databrus sok [SØK...] [filtre]` | Explicit search. Use it when the query collides with a subcommand name. |
| `databrus tilbud [filtre]` | Only current deals, sorted by deal quality. |
| `databrus historikk <PRODUKT> [--kjede ...] [--dager N]` | Price history chart per chain. |
| `databrus oppdater [--kilde ...]` | Fetch from all sources now (respects the TTL floor, §7.4). |
| `databrus butikker` | List known chains, their sources and data freshness. |
| `databrus produkter [filtre]` | List the canonical catalog (brand/flavor/size) without prices. |
| `databrus overvak legg-til <PRODUKT> --under <KR>` | Add a price alert. |
| `databrus overvak liste` / `fjern <ID>` | Manage alerts. |
| `databrus planlegg installer [--tid HH:MM]` / `fjern` / `status` | Install or remove a scheduled daily fetch. |
| `databrus eksporter [--format csv] [--fra DATO] [--til DATO]` | Export price history. |
| `databrus konfig vis` / `sti` / `sett <NOKKEL> <VERDI>` / `init` | Inspect or edit config. |
| `databrus fullforing <powershell\|bash\|zsh\|fish>` | Print shell completions. |

Subcommand names are reserved words. `databrus tilbud` always means the subcommand. To search for
a product whose name is a reserved word, use `databrus sok tilbud`.

### 3.2 Search filters (shared by search, `tilbud`, `produkter`, `eksporter`)

| Flag | Meaning |
|---|---|
| positional `SØK...` | Free text, fuzzy-matched against the canonical product name (brand + line + flavor). |
| `--merke <MERKE>` (repeatable) | Exact brand filter, tab-completable: `monster`, `red-bull`, `burn`, `nocco`, `battery`, `xtra`, ... |
| `--smak <SMAK>` (repeatable) | Exact flavor filter, matched against canonical flavor slugs and aliases. |
| `--storrelse <STR>` (repeatable) | Container size. Accepts `0,5`, `0.5`, `500ml`, `500`, `0,33l`. Values ≤ 5 are liters and larger values are ml. |
| `--butikk <KJEDE>` (repeatable, or a comma list) | Chain filter, tab-completable: `rema`, `kiwi`, `meny`, `spar`, `joker`, `coop-extra`, `coop-obs`, `coop-mega`, `coop-prix`, `bunnpris`, `oda`. `coop` expands to all Coop chains. |
| `--sukkerfri` / `--med-sukker` | Sugar-free filter. |
| `--beholder <boks\|flaske>` | Container type. |
| `--maks-pris <KR>` | Maximum effective unit price. |
| `--maks-literpris <KR>` | Maximum effective price per liter. |
| `--sorter <literpris\|pris\|rabatt\|navn>` | Default `literpris`. `rabatt` is the % below the 90-day median. |
| `--enkeltvis` | Rank by the single-unit price and ignore multi-buy offers (§5.2). |
| `--alle` | Show every row (the default is the top 20). Also shows sold-out and very stale rows. |
| `--gruppert` | *(reserved, not in v1)* |

**Why repeated flags and not comma lists for size:** a comma is the Norwegian decimal separator,
so `--storrelse 0,33,0,5` is ambiguous. `--butikk` accepts comma lists because chain slugs contain
no commas.

### 3.3 Global flags

| Flag | Meaning |
|---|---|
| `--json` | Versioned JSON document on stdout (§9). |
| `--json-linjer` | NDJSON: one result object per line, with a header line first. |
| `--oppdater` | Force a fetch before answering (still subject to the TTL floor). |
| `--frakoblet` | Never touch the network. Local data only. |
| `--streng` | Exit with code 2 if any source failed or any data is stale (§8). |
| `--farge <auto\|alltid\|aldri>` | Color control. `auto` respects TTY detection and `NO_COLOR`. |
| `-v` / `-vv` | Diagnostic logging to stderr. |
| `--konfig <STI>` | Alternative config file. |

`--json` and `--json-linjer` can't be combined. In JSON modes all warnings go to stderr, so stdout
is always valid JSON.

### 3.4 Example session

```
$ databrus monster ultra --storrelse 0,5
Produkt                        Str.    Kjede       Pris   Kr/L   Pant  Tilbud        Vurdering  Trend
Monster Ultra White            0,5 l   Kiwi       15,90  31,80  +2,00  KAMPANJE 3stk  SUPERT     ▆▆▇▆▃▁
Monster Ultra White            0,5 l   Rema 1000  19,90  39,80  +2,00                –          ▅▅▅▅▅▅
Monster Ultra Paradise         0,5 l   Coop Extra 21,90  43,80  +2,00  MEDLEM 18,90   –          ▄▅▅▅▅▅
Monster Ultra White            0,5 l   Oda        22,40  44,80  +2,00                –          ▅▅▅▅▆▆
...
Viser 20 av 47 treff · priser hentet for 2 t siden · 1 kilde feilet (se over)
```

For a multi-buy deal, `Pris` is the *effective* unit price, and the badge shows the quantity
required (`KAMPANJE 3stk`). See §5.2.

---

## 4. Data sources

### 4.1 Source architecture

Every source implements a common trait:

```rust
#[async_trait]
trait Kilde {
    fn id(&self) -> KildeId;                 // "kassalapp", "oda", "rema", "coop"
    fn kjeder(&self) -> &[Kjede];            // chains this source can provide
    async fn hent(&self, ctx: &HenteKontekst) -> Result<Vec<RaaOppforing>, KildeFeil>;
}
```

`RaaOppforing` (raw listing) carries: source, chain, source product ID, GTIN/EAN if known, raw
name, raw size text, pack size, shelf price (øre), member price (øre, with program) if any, offer
details (type, parameters, valid from/to) if any, availability if known, and the source's own
timestamp if it gives one.

Sources run concurrently with tokio. The politeness limits in §7.4 apply per host.

### 4.2 Sources in v1

| Source | Role | Chains | Notes |
|---|---|---|---|
| **Kassalapp API** (`kassal.app/api/v1`) | Primary base prices | See §4.5 | Needs a free API key (§10.2). Fetches the whole energy drink category (`category_id=111`) page by page (about 11 requests). The category already has one row per store, so EAN lookups aren't needed for fetching. Rate limit 60 req/min (verified, §4.5). |
| **Oda** | Direct adapter (implemented) | Oda | Public JSON search API, one search for "energidrikk" (~79 products, 2 requests). Gives price, availability and campaigns: `mix_and_match` "3 for 2" → `n_for_m`, `fixed_price_bundle` "5 for 109 kr" → `n_for_sum`, `price_discount` → `fastpris` with the undiscounted price as shelf price. No EAN: matched through source links and names (§4.6, §6.3). On live data (2026-09-29) 49 of 79 match; the rest are mostly sodas, sports drinks and a few new products. |
| ~~Rema 1000 offers~~ | – | Rema 1000 | **Not available**: prices and offers exist only in the REMA app (§4.6). |
| ~~Coop offers~~ | – | Coop Extra/Obs/Mega/Prix | **Not available**: Coop's online store is closed, and weekly offers are flyer images (§4.6). |

Each direct adapter can be turned off in config (`[kilder.oda] aktiv = false`). The Rema and
Coop placeholders are off by default, since they have no source (§4.6). An adapter that
fails its self-check (unexpected schema) disables itself for that run and warns. It never crashes
the tool.

### 4.3 Merge rules

For each `(chain, product)`:

1. **Base price**: the freshest observation among all sources. When sources tie, a direct adapter beats Kassalapp.
2. **Offers**: an active offer from a direct adapter is layered on top of the base price. If two sources report different offers, prefer the direct adapter and log the conflict at `-v`.
3. **Chain level**: when a source reports multiple stores of the same chain with different prices, use the **mode** (most common price). If there is no mode, use the median. Record the min and max in the listing (`prisspenn`) so JSON consumers can see the spread.

### 4.4 Chains

A canonical chain list with slugs, display names and parent groups:

| Slug | Display | Group |
|---|---|---|
| `rema` | Rema 1000 | Reitan |
| `kiwi` | Kiwi | NorgesGruppen |
| `meny` | Meny | NorgesGruppen |
| `spar` | Spar | NorgesGruppen |
| `joker` | Joker | NorgesGruppen |
| `coop-extra` | Coop Extra | Coop |
| `coop-obs` | Coop Obs | Coop |
| `coop-mega` | Coop Mega | Coop |
| `coop-prix` | Coop Prix | Coop |
| `bunnpris` | Bunnpris | – |
| `oda` | Oda | – |
| `europris` | Europris | – |
| `engrossnett` | Engrossnett | – |
| `havaristen` | Havaristen | – |
| `fastcandy` | Fastcandy | – |

Engrossnett is an online wholesaler that mostly sells large trays (e.g. 24-packs). Its rows rely
on pack parsing (§6.3) to get a fair per-can price. Havaristen and Fastcandy are small online candy
shops.

The mapping from each source's store/chain codes to these slugs lives in code, with a test for each source.

### 4.5 Kassalapp findings (verified 2026-09-29)

Measured against the full energy drink category (1 011 listings, 242 unique EANs):

| Kassalapp code | Chain | Listings | Fresh (≤ 14 days) | Typical age |
|---|---|---|---|---|
| `MENY_NO` | `meny` | 215 | 61 | fresh rows updated daily |
| `SPAR_NO` | `spar` | 155 | 50 | fresh rows updated daily |
| `JOKER_NO` | `joker` | 149 | 43 | fresh rows updated daily |
| `BUNNPRIS` | `bunnpris` | 59 | 37 | fresh |
| `EUROPRIS_NO` | `europris` | 60 | 15 | mixed |
| `ENGROSSNETT_NO` | `engrossnett` | 31 | 6 | ~1 month |
| `ODA_NO` | `oda` | 59 | 1 | ~1 year |
| `KIWI` | `kiwi` | 48 | 0 | ~2.4 years |
| `REMA_1000` | `rema` | 28 | 0 | ~2.4 years |
| `COOP_NO` | *(not mapped)* | 79 | 0 | ~2.4 years |
| `HAVARISTEN`, `FASTCANDY` | `havaristen`, `fastcandy` | 6 | 0 | > 1 year |

Consequences:
- **Kiwi, Rema, Coop and Oda have no usable Kassalapp data.** With the 14-day cutoff (§8) they
  show nothing until a direct source exists. Finding direct sources for them is a research task
  right after M1 (§15). Oda already has a planned adapter (§4.2).
- **`COOP_NO` is one code for all Coop chains**, so it can't be mapped to Extra/Obs/Mega/Prix. It
  is ignored until the Coop adapter exists.
- **No offer or member-price data.** Kassalapp gives only the current shelf price. From Kassalapp,
  deals can only be detected as `PRISFALL` from history (§7.8), never as `KAMPANJE`.
- **Volume is often missing** (`weight_unit` is null on about two thirds of rows), so volume and
  pack size are parsed from the product name (§6.3).
- **Brand names are inconsistent** (`Red bull`, `Red Bull`, `RED BULL`); matching normalizes them.
- **Each store price has its own timestamp** (`current_price.date` in EAN lookups, `updated_at` in
  searches). That timestamp is the observation time, not the fetch time, so a stale row never looks
  fresh just because it was fetched today.
- `/products` pages with `size` ≤ 100 and has only `next` links, no total count. The whole category
  is about 11 requests.

---

### 4.6 Direct source research (2026-09-29)

Goal: current prices for the chains Kassalapp has no fresh data for (§4.5). Only public,
read-only sources were considered; app APIs were not reverse-engineered, and nothing behind a
login or a customer agreement was used.

| Chain | Finding | Verdict |
|---|---|---|
| **Oda** | `https://oda.com/api/v1/search/mixed/?q=energidrikk&type=product&page=N&size=50` returns ~79 products in 2 requests. Per product: `gross_price`, `gross_unit_price`, `availability.is_available`, `discount` (e.g. `discount_type = "mix_and_match"`, `undiscounted_gross_price`) and `promotion.title` ("3 for 2"). The detail endpoint `/api/v1/products/{id}/` adds `bottle_deposit` (pant, not included in the price). **No EAN.** | **Feasible.** |
| **Kiwi** | No online store (`"webshop": false` in the site config); the old product page and `/tilbud` return 404, and the sitemap has only editorial pages. Offers exist only in the app and the printed flyer. | No public source. |
| **Rema 1000** | rema.no is a marketing site with no product catalog or offers; prices and offers are only in the REMA app. | No public source. |
| **Coop** | Coop's online store (`matlevering.coop.no`) no longer resolves. coop.no has no product catalog; the weekly offers (`/extra/tilbud`, `obs.no/kampanjer/denne-ukens-tilbud`) are flyers, not structured data. | No public source. |
| Flyer services (Tjek: etilbudsavis/mattilbud) | Would cover Kiwi, Rema and Coop weekly offers as structured data, but the API is **customer-only** and the terms limit use to agreed services. | Only with an agreement (services@tjek.com). |

**Oda's rules for automated clients** (from its `robots.txt`): the User-Agent must contain
"bot", the program name and a company name or contact email, and clients must back off on 429
and 5xx and respect `Retry-After`. The Oda adapter therefore uses
`databrus-bot/<versjon> (+https://github.com/mathiashagen/databrus)`.

**Matching Oda without EANs**: Kassalapp's (stale) Oda rows link Oda product ids to EANs
(`https://oda.com/no/products/23300-…` ↔ `5060166693732`); 59 energy drinks have both. Oda
listings whose id is known this way get a verified EAN match; the rest go through name matching
(§6.3), with Oda's `brand` as the brand and `name_extra` ("0,5 l") as the size.

**Consequence:** after Oda, Kiwi, Rema and Coop have no automated source that is both public
and allowed. The options are a customer agreement with Tjek, prices entered by the user, or
waiting for Kassalapp to cover them again.

## 5. Pricing model

All money is stored and computed as **integer øre**, and all volumes as **integer ml**. Floats are
used only for display and statistics output.

### 5.1 Terms

- **Hyllepris** (shelf price): the ordinary price of one sellable unit. That unit may be a single can or a multipack.
- **Enhet** (unit): one container (can or bottle).
- **Effektiv enhetspris** (effective unit price): the price per container when the best applicable offer is used.
- **Literpris** (per-liter price): effective unit price ÷ volume. **Pant is excluded.**
- **Minsteantall** (minimum quantity): how many containers you have to buy to get the effective price.

### 5.2 Offer types and effective price

| Offer type | Example | Effective unit price | Min qty |
|---|---|---|---|
| Fixed price | "Nå 15,90" | offer price | 1 |
| Multi-buy N for M | "3 for 2" | `pris × M / N` | N |
| N for sum | "2 for 50 kr" | `sum / N` | N |
| Percentage | "30 % rabatt" | `pris × (100 − p) / 100` | 1 |
| Multipack | 4-pack for 69,90 | `pakkepris / 4` | 4 |
| Nth item discount | "3. stk gratis", "2. til halv pris" | total for the cycle / cycle length | cycle length |

- The default ranking uses the effective price, and the table shows the minimum quantity when it is greater than 1.
- `--enkeltvis` ranks by the price of buying exactly one container (multipacks are still shown, divided per unit, and marked).
- A multipack and a single can of the same product at the same chain are **separate listings**, each with its own effective price. In the default (non-`--alle`) view, only the cheapest per-liter listing per `(product, chain)` is shown.
- Round only when displaying (half-up to whole øre). Per-liter is computed as `enhetspris_ore * 1000 / volum_ml`, rounded half-up to whole øre per liter.

### 5.3 Member prices

- Store the member price separately, together with its program: `coop` (Coop medlem), `trumf` (NorgesGruppen), `ae` (Rema Æ), `kiwi-pluss`.
- Config `medlemskap = ["coop", "trumf"]` lists the programs the user belongs to.
- **Ranking**: use the member price only if the user belongs to that program. Otherwise rank by the ordinary price and show the member price as a `MEDLEM xx,xx` marker in the Tilbud column.
- JSON always includes both prices and a flag saying which one was used for ranking.

### 5.4 Pant (deposit)

- Never included in `Pris` or `Kr/L`. Shown in its own `Pant` column per unit (`+2,00`).
- Rates are configurable, with defaults based on container volume: **≤ 500 ml → 2,00 kr**, **> 500 ml → 3,00 kr**. (Verify against Infinitum's current rates at implementation time. Rates are data in config/catalog, not hard-coded logic.)
- JSON gives pant per unit and for the minimum quantity (`pant_ore`, `pant_minsteantall_ore`).
- If the source reports a pant amount that differs from the computed one, prefer the source's value and log it at `-v`.

### 5.5 Sanity bounds

An observation is flagged `mistenkelig` (suspicious) and **left out of history statistics and
ranking** (it is shown only with `--alle`) if it meets any of these:
- unit price < 5 kr or > 150 kr per container
- per-liter < 10 kr or > 400 kr
- price changed by > 60 % since the last observation with no offer to explain it

---

## 6. Product catalog and matching

### 6.1 Canonical product

```
Produkt {
  id: slug,               // "monster-ultra-white-500-boks"
  merke: slug,            // "monster"
  linje: Option<String>,  // "Ultra"
  smak: slug,             // "white"
  smak_alias: [String],   // ["zero ultra", "ultra zero"]
  sukkerfri: bool,
  volum_ml: u32,
  beholder: Boks | Flaske,
  gtin: [String],         // one product can have several EANs (single, regional variants)
  koffein_mg_per_100ml: Option<u16>,   // stored, not used in v1
  egenmerke: bool,        // store brand
}
```

### 6.2 Catalog file

- Bundled into the binary (`include_str!("../data/katalog.toml")`). Generated from Kassalapp's energy drink category and curated by hand (2026-09-29): 172 products, 24 multipacks and 187 EANs across Red Bull, Monster, Burn, Nocco, Battery, Tørst, Explo, Cult and smaller brands. Sports drinks, protein drinks, powders and pallets that Kassalapp files under energy drinks are left out. Sugar-free comes from the sugar content (< 0,5 g per 100 ml), not from the name. With this catalog, 679 of 810 Kassalapp listings match by EAN; the rest are the left-out products and Engrossnett trays without EANs.
- A user override file at `<konfigmappe>/katalog.toml` is merged over the bundled one (matched by `id`). This lets users fix or add entries without a new release.
- **Source links** (`[[kildekobling]]` with `kilde`, `id`, `gtin`) give an EAN to listings from sources that report none, such as Oda (§4.6). A link to an EAN outside the catalog keeps the listing unmatched rather than name-matched.
- Multipack GTINs map to `(produkt_id, antall)`. **The pack size of a listing comes from the listing's own name**, not from the GTIN: stores reuse GTINs across pack sizes (Engrossnett sells 24-trays under the single-can GTIN, and a 4-pack GTIN sometimes appears on a single can). The catalog's `antall` is only a cross-check, and a mismatch is logged at `-v`.

### 6.3 Matching pipeline

For each raw listing:
1. **GTIN match** against the catalog (including multipack GTINs) gives a verified match.
2. **Name match**, only for listings **without an EAN**. A listing with an EAN the catalog does not know is left unmatched: it is almost always a product deliberately left out (sports or protein drinks), and a wrong match is worse than none. The rules are strict and word-based rather than a fuzzy score:
   - The name is normalized (lowercase, æøå folded), sizes and pack sizes are dropped, and filler words ("boks", "energidrikk", "energy", "drink", "og", "flaske", …) are ignored. The size may also come from the source's size field, including free text such as "Blåbær, 250 ml".
   - The **volume must be equal** and the **brand must be present** (in the name, or as the source's brand field). A listing that says "flaske" only matches bottles.
   - One of the product's word sets must be fully present: its name without the brand, a hand-written alias (`smak_alias`), or – weaker – its flavor slug.
   - **Every remaining word must be explained** by the product's name, aliases or flavor. Sugar-free words ("zero", "sukkerfri", "u/sukker") are only accepted on sugar-free products.
   - The best match wins: name or alias before flavor, then more matched words. **A tie means no match.**
   - A name match gets `verifisert = false` and is shown with a `?` in the table. On the live data (2026-09-29) all 11 listings without an EAN (Engrossnett trays) match correctly, bringing the total to 690 of 810.
3. **No match**: the listing is stored, keyed by `(source, source product ID)`, and history is still recorded. `databrus produkter --ukjente` lists these so they can be added to the catalog. **Unmatched listings are not shown in searches**: with the curated catalog, almost all of them are products left out on purpose (sports and protein drinks). Listings only reach search results through a catalog match, verified (EAN) or not (fuzzy, shown with `?`).

Unknown brands are included if the source categorizes them as energy drinks, or if the name contains "energy"/"energi". This keeps new store brands from disappearing.

### 6.4 Free-text search

Fuzzy-match the query against `merke + linje + smak + aliases` using `nucleo-matcher` (or an
equivalent). Typed flags are then applied as exact filters. Query normalization folds æøå, so
`sukkerfri`, `zero` and `sugarfree` in free text turn on the sugar-free filter.

---

## 7. Local storage, fetching and history

### 7.1 Locations (via the `directories` crate)

| What | Windows | Linux | macOS |
|---|---|---|---|
| Config | `%APPDATA%\databrus\konfig.toml` | `~/.config/databrus/konfig.toml` | `~/Library/Application Support/databrus/konfig.toml` |
| Database | `%LOCALAPPDATA%\databrus\databrus.sqlite` | `~/.local/share/databrus/databrus.sqlite` | same as config dir |

### 7.2 Schema (SQLite via `rusqlite` with the `bundled` feature, migrations via `rusqlite_migration`)

```sql
product         (id TEXT PK, name, brand, line, flavor, sugar_free, volume_ml, container,
                 store_brand, ad_hoc BOOL)
listing         (id INTEGER PK, source, chain, source_product_id, gtin, product_id FK, pack_size,
                 raw_name, verified BOOL, first_seen, last_seen,
                 UNIQUE(source, chain, source_product_id))
price_interval  (id INTEGER PK, listing_id FK, shelf_price_ore, member_price_ore,
                 membership_program, offer_json, available BOOL, suspicious BOOL,
                 valid_from TIMESTAMP, last_seen TIMESTAMP)
fetch_log       (id INTEGER PK, source, started, finished, status, error_message, listing_count)
alert           (id INTEGER PK, product_id, chain NULL, threshold_ore, threshold_kind, created,
                 last_triggered_interval)
```

Chain, source, container and membership values are stored as their public slugs (`coop-extra`,
`kassalapp`, `boks`, `kiwi-pluss`), and `offer_json` uses the same Norwegian keys as the JSON output.

- **Change-only history**: when a fetch sees the same `(shelf price, member price, offer, available)` as the latest interval, it only updates `last_seen` (never backwards in time). Any change closes the old interval implicitly by inserting a new row with `valid_from` = the observation time.
- **Gaps**: if `sist_sett` of the latest interval is older than 3 days when a new observation arrives, the interval is treated as ending at `sist_sett`, and the gap is left as *unknown*. It is not assumed that the price stayed the same. Statistics weight prices by the time they were known to be valid.
- All timestamps are stored in UTC. Day boundaries (such as "30 days") use Europe/Oslo.
- The DB is opened in WAL mode, so a scheduled fetch and an interactive search can run at the same time.

### 7.3 Fetch model (cache with TTL)

- Default TTL is **6 h** per source (`ttl_timer` in config).
- Search, `tilbud` and similar commands check how fresh each relevant source is. Stale sources are fetched **before** results are shown, with a spinner on stderr when it's a TTY.
- `--oppdater` forces a fetch (subject to the floor below). `--frakoblet` never fetches.
- `oppdater` fetches all enabled sources. The scheduled job runs this.

### 7.4 Politeness

- An honest `User-Agent: databrus-bot/<versjon> (+<repo-url>)`. It says it is a bot, names the program and gives a contact, as Oda's policy requires (§4.6).
- At most **2 concurrent requests per host**, plus each source's own documented rate limit (token bucket).
- Respect `Retry-After`. Use exponential backoff with jitter on 429 and 5xx, up to 3 retries.
- **TTL floor of 15 minutes** per source that even `--oppdater` can't bypass. A skipped fetch prints an informational message on stderr.
- Timeouts: 10 s connect, 30 s total per request.

### 7.5 Scheduled collection (`planlegg`)

- `planlegg installer [--tid 07:00]` installs a daily `databrus oppdater --stille` job:
  - **Windows** (implemented): Task Scheduler via `schtasks.exe /create /xml` (task name `databrus-oppdater`). Runs as the current user while they are logged on, so no password is stored. Runs as soon as possible after a missed start, and also on battery. The first start is the next occurrence of `--tid`, so installing never triggers an immediate run. A `--konfig` path is passed on to the task as an absolute path.
  - **Linux**: a systemd user timer (`~/.config/systemd/user/databrus-oppdater.{service,timer}`, `OnCalendar` at `--tid`, `Persistent=true` so a missed run is caught up) when `systemctl --user` works, otherwise a crontab line marked `# databrus-oppdater`, with the output appended to `planlagt.log` in the data directory. Installing one removes the other. The rest of the crontab is kept as it is, and a crontab that can't be read (other than "no crontab") is an error rather than overwritten. A user timer only runs while the user is logged in unless lingering is enabled (`loginctl enable-linger`); `installer` says so. cron does not catch up missed runs.
  - **macOS**: a launch agent `~/Library/LaunchAgents/io.github.mathiashagen.databrus-oppdater.plist` (`StartCalendarInterval`, output to `planlagt.log`), loaded with `launchctl bootstrap gui/<uid>` so desktop notifications can show. launchd runs a start missed during sleep on wake.
  - Other systems get a clear error suggesting a cron line of their own.
  - `installer` warns when the Kassalapp key or `DATABRUS_DATA_DIR` is only set in the shell's environment, since the scheduled job may not see it.
- `planlegg status` shows whether it is installed, its time and command, and the last fetch per source with its result (from `fetch_log`).
- `planlegg fjern` removes it.
- `--stille` means no output except errors. Alerts (§7.7) are evaluated after every `oppdater`.

### 7.6 Deal verdict ("vurdering")

Computed per **listing** from the history of the ranked price (the effective price,
per-liter). A row is judged against the history of the listing it shows. History is not
combined across the listings of a `(product, chain)` (say a single can and a 4-pack): a
listing's history starts when a source first reports it, so a 4-pack first seen today would
look like a drop from the single can's price even if it has been on the shelf all along.

Reference values:
- **L30**: the lowest price in the 30 days *before the current price began* (the EU Omnibus principle)
- **M90**: the time-weighted median over the last 90 days, excluding the current interval
- **ATL**: the all-time low in local history

| Verdict | Rule (checked in order) |
|---|---|
| `UKJENT` | Less than 14 days of known coverage in the last 90 days. |
| `LURERI` | The row is marked as a deal (§7.8), **and** the current price ≥ L30, **and** the price just before the deal was ≥ 5 % above L30 (the price was raised before the "sale"). |
| `SUPERT` | The current price ≤ ATL × 1,02, **or** ≥ 20 % below M90. |
| `BRA` | ≥ 10 % below M90 **and** < L30. |
| `MIDDELS` | Everything else. |

Thresholds are configurable under `[vurdering]`. In the table, `–` means that no verdict is shown
because the row is not a deal and the price is at or above M90. `historikk` and JSON always show
the underlying numbers (`l30`, `m90`, `atl`, `dekning_dager`).

### 7.7 Price alerts (`overvak`)

- `overvak legg-til "monster ultra white" --under 18 [--kjede kiwi] [--literpris]`. The threshold is on the effective unit price, or on the per-liter price with `--literpris`, and is strict (`--under 18` means below 18,00). The product is resolved like in `historikk` (§7.9): by id or text, an exact name first, then products with prices. If the match is still ambiguous, the command errors and lists the candidates with their ids. It prints the current best price.
- Checked after every fetch that got new data: `oppdater` (manual or scheduled), and the automatic TTL fetch during a search or `tilbud`.
- The price an alert compares is the best current price for its product (at its chain, if it has one), as search would show it: fresh, available and not suspicious, whatever `standard_butikker` says.
- When an alert triggers: a line on stderr (`varsel: …`, also with `--stille`, since the scheduled job has no other way to report it), plus a desktop notification (`notify-rust`: Windows toast, macOS, Linux D-Bus) unless `[varsler] skrivebord = false`. A notification that fails to show is logged, not an error. The same alert re-fires only when the price interval changes (so there's no notification spam every day for an unchanged price).
- `overvak liste` shows alerts with their current best price and status (`UNDER`). With `--json` the payload key is `varsler`: each alert (`id`, `produkt`, `kjede`, `grense_ore`, `grensetype`, `opprettet`) plus `produktnavn`, `beste_pris_ore`, `beste_kjede` and `under_grensen`. `overvak fjern <ID>` removes one; an unknown id is a usage error.

### 7.8 Deal detection ("tilbud")

A listing is a deal if **either** of these holds:
- **`KAMPANJE`**: the source marks an active offer (valid today in Europe/Oslo time). Expired offers are dropped, and future offers are shown only with `tilbud --kommende`.
- **`PRISFALL`**: no source offer, but the effective price is ≥ 10 % below M90 for that chain. This requires coverage (not `UKJENT`).

Member-only offers the user can't use show the `MEDLEM` marker but don't count as a deal for
ranking unless the user has that membership.

`tilbud` sorts by verdict (`SUPERT`, then `BRA`, then `MIDDELS`, then `UKJENT`, then `LURERI`), then by
per-liter price. `--sorter` changes the order within each verdict. Deals are picked before the
cheapest listing per `(product, chain)`, so a single can on offer shows up even when the
4-pack is cheaper per liter. With `--kommende`, an offer that has not started is priced as on its
first day and marked `KAMPANJE fra 2.10.`.

### 7.9 History views

- **Sparkline** (`Trend` column): per-liter price for the row's listing over the last 90 days, in 6 slots (the time-weighted average in each), drawn with `▁▂▃▄▅▆▇█`. The scale spans at least 5 % of the price, so small changes don't look like big swings and a flat price is `▅▅▅▅▅▅`. Blank when coverage is below 14 days. The column is dropped below 100 columns, and when no row has enough history. Not part of the JSON.
- **`historikk <PRODUKT>`**: a Unicode (braille) line chart of per-liter price over time, one series per chain (colored, with a legend), plus a summary table per chain: now, L30, M90, ATL, verdict, days of coverage. `--dager` (default 90), `--kjede` to filter. Uses a small in-house braille renderer; prices are drawn as steps and gaps stay empty. Each chain shows the listing search would show today, or, when it has no current price, the listing seen most recently. The product is given by id or text. An exact name wins, then products with prices; if several still match, the command fails and lists them with their ids.
- **`eksporter`**: CSV on stdout. Plain RFC 4180 by default (commas, decimal dots, CRLF, times in Norwegian time with the UTC offset). With `--excel`: a UTF-8 BOM, `;` as the delimiter, decimal commas and times as `2026-09-30 07:00:00`. One row per stored price interval of matched listings, oldest first per listing: `produkt_id`, `produkt`, `volum_ml`, `kjede`, `kilde`, `antall_i_pakke`, `gyldig_fra`, `sist_sett`, `hyllepris_kr`, `medlemspris_kr`, `medlemsprogram`, `tilbud` (in words, e.g. `3 for 2`), `tilbud_gyldig_fra`, `tilbud_gyldig_til`, `effektiv_enhetspris_kr`, `literpris_kr` (computed as in search), `pant_kr`, `tilgjengelig`, `mistenkelig`. The product and chain filters apply. `--fra`/`--til` keep intervals seen on those days (inclusive, Europe/Oslo). `--json` is a usage error.

---

## 8. Error handling, freshness and exit codes

- **Degrade and warn**: show what's available. Each failed source gets one warning line on stderr in Norwegian, for example `advarsel: Coop-tilbud feilet (HTTP 503) – viser data fra 2 dager siden`.
- **Freshness markers**: rows older than 24 h show their age (`3d`) after the chain name, dimmed. Rows older than **14 days** are hidden unless `--alle` is set.
- **Sold out** (from Oda, or any source that reports availability): marked `utsolgt`, dimmed, and sorted last. Hidden unless `--alle` is set.
- **No API key**: Kassalapp is skipped with a one-time setup hint (§10.2), and direct adapters still run.
- **First run with an empty DB and `--frakoblet`**: a clear message that there is no local data, exit 3.
- **Corrupt or locked DB**: the error message names the file path. Never delete it automatically.

| Exit code | Meaning |
|---|---|
| 0 | Success, including zero hits (a message is printed when there are no hits). |
| 1 | Usage error or fatal error. |
| 2 | `--streng` set and at least one source failed or served stale data. |
| 3 | No data available at all (offline or all sources failed with an empty DB). |

---

## 9. JSON output

### 9.1 `--json`

```json
{
  "skjemaversjon": 1,
  "generert": "2026-09-29T11:42:07Z",
  "kilder": [
    {"id": "kassalapp", "status": "ok", "hentet": "2026-09-29T09:40:00Z"},
    {"id": "coop", "status": "feilet", "hentet": "2026-09-27T06:00:00Z", "feil": "HTTP 503"}
  ],
  "sporring": {"tekst": "monster ultra", "filtre": {"storrelse_ml": [500]}, "sorter": "literpris"},
  "resultater": [
    {
      "produkt": {
        "id": "monster-ultra-white-500-boks",
        "navn": "Monster Ultra White",
        "merke": "monster", "smak": "white", "sukkerfri": true,
        "volum_ml": 500, "beholder": "boks", "egenmerke": false, "verifisert": true
      },
      "kjede": "kiwi",
      "kilde": "kassalapp",
      "antall_i_pakke": 1,
      "hyllepris_ore": 2490,
      "medlemspris_ore": null,
      "medlemsprogram": null,
      "effektiv_enhetspris_ore": 1590,
      "literpris_ore": 3180,
      "minsteantall": 3,
      "brukt_pris": "tilbud",
      "pant_ore": 200,
      "pant_minsteantall_ore": 600,
      "tilbud": {"type": "n_for_sum", "n": 3, "sum_ore": 4770,
                 "gyldig_fra": "2026-09-28", "gyldig_til": "2026-10-04", "kilde_merket": true},
      "tilbudsmerke": "KAMPANJE",
      "vurdering": {"verdi": "SUPERT", "l30_ore": 2290, "m90_ore": 2490, "atl_ore": 1690,
                    "dekning_dager": 88},
      "prisspenn": {"min_ore": 1590, "maks_ore": 1590},
      "tilgjengelig": true,
      "sist_sett": "2026-09-29T09:40:00Z",
      "alder_timer": 2
    }
  ]
}
```

Rules:
- Money is always integer øre with an `_ore` suffix, volume is always integer ml, and timestamps are RFC 3339 UTC. Dates without a time (offer validity) are `YYYY-MM-DD` in Oslo time.
- Enum values are lowercase ASCII slugs, except `vurdering.verdi` and `tilbudsmerke`, which are uppercase.
- **Compatibility**: adding fields is non-breaking. Removing, renaming or changing the meaning of a field bumps `skjemaversjon`.
- The JSON schema for `databrus --json` is published in the repo as `schema/v{skjemaversjon}.json` (JSON Schema 2020-12, generated with `schemars` from the Rust types, for the *serialize* contract: a field that is always written is required even when it can be `null`). Its `$id` is the raw GitHub URL of the file.

### 9.2 `--json-linjer`

The first line is a header object (`{"type":"hode","skjemaversjon":1,"generert":...,"kilder":[...]}`).
Each following line is `{"type":"resultat", ...same object as above...}`.

### 9.3 Other commands

`tilbud`, `historikk`, `butikker`, `produkter` and `overvak liste` all support `--json` with the
same envelope (`skjemaversjon`, `generert`) and a command-specific payload key. `tilbud` is the
exception: its rows are search results, so it prints the search document unchanged
(`sporring`, `resultater`) and the published schema covers it. For `historikk`
the content is `produkt`, `dager` and `serier`: per chain `kjede`, the current `literpris_ore`
(`null` without a current price), `vurdering` (as in search) and `intervaller`
(`fra`, `til`, `literpris_ore`).

---

## 10. Configuration

### 10.1 `konfig.toml`

```toml
medlemskap = ["coop", "trumf"]     # coop | trumf | ae | kiwi-pluss
standard_butikker = []             # empty = all chains
standard_antall = 20
farge = "auto"

[henting]
ttl_timer = 6
min_intervall_minutter = 15        # floor; values below 15 are rejected

[pant]
liten_ore = 200                    # ≤ grense_ml
stor_ore = 300
grense_ml = 500

[vurdering]
min_dekning_dager = 14
supert_under_median_prosent = 20
bra_under_median_prosent = 10
atl_toleranse_prosent = 2
lureri_prisokning_prosent = 5
prisfall_prosent = 10

[varsler]
skrivebord = true      # also show alerts as desktop notifications

[kilder.kassalapp]
aktiv = true
api_nokkel = ""                    # KASSALAPP_API_KEY overrides this

[kilder.oda]
aktiv = true
[kilder.rema]
aktiv = false                      # no public source (§4.6)
[kilder.coop]
aktiv = false                      # no public source (§4.6)
```

- `konfig init` writes a commented default file.
- `konfig sett` validates the key and value.
- Unknown keys cause a warning, not an error.

### 10.2 Kassalapp API key

- Order of precedence: the `KASSALAPP_API_KEY` env var, then `kilder.kassalapp.api_nokkel`.
- If neither is set, print this once per run on stderr: how to get a free key at kassal.app, and both ways to set it.
- The key is never logged, not even at `-vv`, and HTTP debug logging redacts `Authorization`.

---

## 11. Table rendering

- `comfy-table` (or `tabled`) with Unicode borders off by default: a clean column layout like the §3.4 example.
- Columns: **Produkt**, **Str.**, **Kjede** (+ age), **Pris**, **Kr/L**, **Pant**, **Tilbud**, **Vurdering**, **Trend**. Vurdering and Trend are added in M2, when there is history to fill them; until then they are left out rather than shown empty.
- Adapts to width: below 100 columns, drop Trend. Below 80, drop Pant and shorten Tilbud. Product names are truncated with `…`.
- Colors (`owo-colors` + `anstream`): cheapest row bold; `SUPERT` green, `BRA` cyan, `LURERI` red; `KAMPANJE` yellow; stale and sold-out rows dimmed; `?` (unverified match) dimmed.
- A footer line summarizes: rows shown / total, data age, source failures.
- When stdout is not a TTY and `--json` is not set: plain table, no colors, no spinner.

---

## 12. Technical stack

| Concern | Choice |
|---|---|
| Language | Rust stable, edition 2024, MSRV pinned in `Cargo.toml` |
| Async / HTTP | `tokio`, `reqwest` with `rustls-tls` (no OpenSSL), `governor` for rate limiting |
| CLI | `clap` v4 derive, `clap_complete` for PowerShell/bash/zsh/fish |
| Storage | `rusqlite` (`bundled`), `rusqlite_migration` |
| Serialization | `serde`, `serde_json`, `toml`, `schemars` |
| Time | `jiff` (or `chrono` + `chrono-tz`) with Europe/Oslo |
| Matching | `nucleo-matcher` (or `strsim`) |
| Output | `comfy-table`, `owo-colors`, `anstream`, `indicatif` (spinner) |
| Charts | `textplots` or an in-house braille renderer |
| Notifications | `notify-rust` |
| Paths | `directories` |
| Errors / logs | `thiserror` (library), `anyhow` (binary), `tracing` + `tracing-subscriber` |

### 12.1 Module layout

```
src/
  main.rs            // parse args, run, map the result to an exit code
  lib.rs             // Context and command dispatch
  cli/               // clap definitions (Norwegian names), completions
  commands/          // one module per command: search, deals, update, stores, products, …
  sources/           // Source trait, kassalapp, oda, rema, coop, http (politeness, retry)
  catalog/           // catalog loading/merging, GTIN index, name parsing, matching
  pricing/           // øre math, offer → effective price, deposit, sanity bounds
  db/                // SQLite, migrations, change-only writes, queries
  history/           // L30/M90/ATL, verdict, deal detection
  search/            // filter + ranking pipeline
  output/            // table, JSON/NDJSON, Norwegian number formatting
  alerts/            // price alerts
  schedule/          // schtasks / systemd / cron / launchd
  config/            // config load/validate, file locations
data/katalog.toml
migrations/
schema/v1.json
tests/fixtures/<source>/...
```

---

## 13. Testing

- **Unit tests**: øre math and every offer type (§5.2), including rounding edge cases; size and pack parsing (`4x0,5l`, `50cl`, `0.33 L`, `24-pk`); verdict rules against synthetic histories (including gaps and `LURERI` patterns); change-only history writes.
- **Adapter tests against recorded fixtures**: real responses saved under `tests/fixtures/`, served with `wiremock`. No test touches the network. Each adapter also has a "schema drift" fixture that must produce a clean `KildeFeil`, not a panic.
- **Snapshot tests** (`insta`): table output at several widths with and without color, JSON output, `historikk` chart.
- **Property tests** (`proptest`): per-liter ranking is monotonic, and effective price ≤ shelf price for every valid offer.
- **Live smoke tests** behind `--features live-tests`, run nightly in CI with the API key as a secret. A failure opens an issue and doesn't block releases.
- **JSON schema check** (a normal test, so it runs in CI): the schema is regenerated and compared with the committed `schema/v{skjemaversjon}.json`.
  - A **breaking** change fails until `skjemaversjon` is bumped, which starts a new file: a field removed or renamed, a type changed or widened (e.g. now nullable), a field that may now be missing, an enum value removed, or a variant of a tagged enum removed.
  - An **additive** change (new fields, new enum values such as a new chain, new offer types) only requires regenerating the file with `DATABRUS_UPDATE_SCHEMA=1 cargo test published_schema`.
  - Real output is validated against the schema: a unit test with offers, member prices, packs and a failed source, and the end-to-end test against actual `databrus --json` output.

---

## 14. Distribution

- `cargo install databrus`.
- GitHub Actions with `cargo-dist`: release binaries for Windows (x86_64 MSVC), macOS (arm64 + x86_64) and Linux (x86_64 musl), plus a PowerShell and a shell installer.
- CI on every PR: `fmt`, `clippy -D warnings`, and tests on Windows/Linux/macOS.

---

## 15. Milestones

1. **M1 – Core search**: config, Kassalapp adapter, catalog + GTIN matching, SQLite with change-only history, pricing math, search with filters, table + JSON output, TTL fetching, degrade/warn.
   - **Right after M1 – source research** (done, §4.6): only Oda has a usable public source.
2. **M2 – History**: verdict engine, deal detection, `tilbud`, sparklines, `historikk` chart, `eksporter`.
   - Done (2026-09-30). History is judged per listing rather than per `(product, chain)` (§7.6).
3. **M3 – Direct adapters**: Oda (§4.6), merge rules. Rema and Coop offer adapters are dropped for lack of a public source; revisit if Tjek or the chains offer access.
4. **M4 – Automation and release**: `planlegg` for all OSes, `overvak` + notifications, completions, cargo-dist releases, published JSON schema.
   - Done: `planlegg` on Windows, Linux and macOS, `overvak` with notifications, the published JSON schema. Left: cargo-dist releases.

---

## 16. Risks and open questions

- **Undocumented endpoints**: Oda's JSON API is public but undocumented and can change without warning. Mitigations: fixtures, schema-drift detection, per-source kill switch, and Kassalapp as the baseline.
- **Kiwi, Rema and Coop have no public price source** (§4.6). These are among the cheapest chains, so this is the biggest limit on the tool's usefulness.
- **Terms of use**: the tool is for personal use with conservative request rates. Review each source's terms before publishing to crates.io, and drop any adapter whose terms forbid automated access.
- **Kassalapp freshness and chain coverage** decide the quality of the baseline. Record `hentet` per source, so staleness is always visible. **Confirmed problem (§4.5):** Kassalapp has no fresh Kiwi, Rema, Coop or Oda prices. Direct sources for those chains are the biggest open question for the tool's usefulness.
- **Pant rates** must be confirmed against Infinitum at implementation time. They are configurable data.
- **Cold start**: verdicts show `UKJENT` for the first ~14 days. A possible v1.x improvement is to seed history from Kassalapp's per-product price history (not in v1 scope).
- **Chain-level simplification**: a price shown for Coop Extra may not match every Coop Extra store. The spread (`prisspenn`) is surfaced in JSON. Store-level pricing is a possible future extension.
