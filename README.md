# databrus

Finn de billigste energidrikkene i Norge – fra kommandolinjen.

`databrus` henter priser fra norske dagligvarekjeder og nettbutikker, rangerer dem etter
literpris (pant vises for seg), fremhever tilbud og lagrer prishistorikk lokalt, slik at du kan se
om et tilbud faktisk er bra.

> **Status:** under utvikling. Søk virker med ferske priser for Meny, Spar, Joker, Bunnpris,
> Europris og Engrossnett (via Kassalapp) og Oda (direkte, med kampanjer som «3 for 2»).
> Kiwi, Rema og Coop har ingen offentlig priskilde og vises ikke. Tilbud vurderes mot lokal
> prishistorikk, som trenger 14 dager før vurderingene blir annet enn `UKJENT` – installer
> daglig henting med `databrus planlegg installer` (Windows, Linux og macOS).
> Se [SPEC.md](SPEC.md) for full spesifikasjon.

## Installer

Ferdige programfiler for Windows, macOS (Apple Silicon og Intel) og Linux (x86_64) ligger
under [Releases](https://github.com/mathiashagen/databrus/releases) fra og med første
utgivelse. Installasjonsskriptene legger `databrus` i `~/.cargo/bin`:

```console
# macOS og Linux
$ curl --proto '=https' --tlsv1.2 -LsSf https://github.com/mathiashagen/databrus/releases/latest/download/databrus-installer.sh | sh

# Windows (PowerShell)
> powershell -ExecutionPolicy Bypass -c "irm https://github.com/mathiashagen/databrus/releases/latest/download/databrus-installer.ps1 | iex"
```

Eller bygg selv med `cargo install --path .` fra en klone av repoet.

## Kom i gang

```console
$ databrus konfig init          # skriv standardkonfigurasjon
$ databrus butikker             # kjente kjeder og kilder
$ databrus produkter --merke monster
$ databrus oppdater              # hent ferske priser (skjer også automatisk ved søk)
$ databrus monster ultra --storrelse 0,5
$ databrus --json red bull --sukkerfri
$ databrus tilbud                # dagens tilbud, beste vurdering først
$ databrus historikk monster ultra white   # prisgraf per kjede
$ databrus eksporter --excel > priser.csv  # all prishistorikk som CSV
$ databrus planlegg installer --tid 07:00   # hent priser hver dag
$ databrus overvak legg-til monster ultra white --under 18   # varsle når prisen er under 18 kr
```

Kassalapp-kilden trenger en gratis API-nøkkel fra <https://kassal.app/api>:

```console
$ export KASSALAPP_API_KEY=...
# eller
$ databrus konfig sett kilder.kassalapp.api_nokkel ...
```

## Utvikling

```console
$ cargo test
$ cargo clippy --all-targets -- -D warnings
```

### Ny versjon

Utgivelser bygges av [dist](https://github.com/axodotdev/cargo-dist)
(`.github/workflows/release.yml`). Øk `version` i `Cargo.toml`, commit, og push en tagg:

```console
$ git tag v0.2.0
$ git push origin v0.2.0
```

Etter endringer i `dist-workspace.toml`: kjør `dist generate` og commit resultatet.

Snapshot-tester bruker [insta](https://insta.rs); se over endringer med `cargo insta review`.

JSON-utdataene er beskrevet av et versjonert skjema i [`schema/v1.json`](schema/v1.json).
Skjemaet genereres fra koden, og en test sjekker at filen er oppdatert:

- Nye felt eller nye verdier (f.eks. en ny kjede): kjør
  `DATABRUS_UPDATE_SCHEMA=1 cargo test published_schema` og commit filen.
- Brytende endringer (fjernede eller omdøpte felt, endrede typer): testen feiler til
  `SCHEMA_VERSION` er økt, og da lages `schema/v2.json`.
