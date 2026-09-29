# databrus

Finn de billigste energidrikkene i Norge – fra kommandolinjen.

`databrus` henter priser fra norske dagligvarekjeder og nettbutikker, rangerer dem etter
literpris (pant vises for seg), fremhever tilbud og lagrer prishistorikk lokalt, slik at du kan se
om et tilbud faktisk er bra.

> **Status:** skjelett. Kommandolinje, konfigurasjon, katalog og database er på plass;
> ekte prisinnhenting kommer i milepæl M1. Se [SPEC.md](SPEC.md) for full spesifikasjon.

## Kom i gang

```console
$ cargo install --path .
$ databrus konfig init          # skriv standardkonfigurasjon
$ databrus butikker             # kjente kjeder og kilder
$ databrus produkter --merke monster
$ databrus monster ultra --storrelse 0,5
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
