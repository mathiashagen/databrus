# databrus

Finn de billigste energidrikkene i Norge – fra kommandolinjen.

`databrus` henter priser fra norske dagligvarekjeder og nettbutikker, rangerer dem etter
literpris (pant vises for seg), fremhever tilbud og lagrer prishistorikk lokalt, slik at du kan se
om et tilbud faktisk er bra.

> **Status:** under utvikling (milepæl M1). Søk virker med priser fra Kassalapp for Meny,
> Spar, Joker, Bunnpris, Europris og Engrossnett. Kiwi, Rema, Coop og Oda mangler ferske
> priser inntil de får egne kilder, og tilbudsvurdering og historikk kommer i M2.
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
