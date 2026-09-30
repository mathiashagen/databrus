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

## Kom i gang

```console
$ cargo install --path .
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

Snapshot-tester bruker [insta](https://insta.rs); se over endringer med `cargo insta review`.

JSON-utdataene er beskrevet av et versjonert skjema i [`schema/v1.json`](schema/v1.json).
Skjemaet genereres fra koden, og en test sjekker at filen er oppdatert:

- Nye felt eller nye verdier (f.eks. en ny kjede): kjør
  `DATABRUS_UPDATE_SCHEMA=1 cargo test published_schema` og commit filen.
- Brytende endringer (fjernede eller omdøpte felt, endrede typer): testen feiler til
  `SCHEMA_VERSION` er økt, og da lages `schema/v2.json`.
