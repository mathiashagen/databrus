//! `databrus eksporter` – the price history as CSV, one row per price interval (SPEC §7.9).
//! Reads local data only.

use std::io::{BufWriter, Write};

use jiff::Timestamp;
use jiff::civil::Date;

use crate::Context;
use crate::cli::ExportArgs;
use crate::db::HistoricPrice;
use crate::error::{AppError, ExitStatus};
use crate::history::{oslo_date, oslo_time, start_of_day};
use crate::model::{Ore, Product};
use crate::output::{OutputFormat, format};
use crate::pricing;
use crate::search::SearchFilter;
use crate::search::ranking::compute_price;

const HEADERS: [&str; 19] = [
    "produkt_id",
    "produkt",
    "volum_ml",
    "kjede",
    "kilde",
    "antall_i_pakke",
    "gyldig_fra",
    "sist_sett",
    "hyllepris_kr",
    "medlemspris_kr",
    "medlemsprogram",
    "tilbud",
    "tilbud_gyldig_fra",
    "tilbud_gyldig_til",
    "effektiv_enhetspris_kr",
    "literpris_kr",
    "pant_kr",
    "tilgjengelig",
    "mistenkelig",
];

pub fn run(args: &ExportArgs, ctx: &Context) -> Result<ExitStatus, AppError> {
    if ctx.output_format() != OutputFormat::Table {
        return Err(AppError::Usage(
            "eksporter skriver CSV og tar ikke --json eller --json-linjer".into(),
        ));
    }
    if let (Some(from), Some(to)) = (args.from, args.to)
        && from > to
    {
        return Err(AppError::Usage(format!("--fra {from} er etter --til {to}")));
    }
    let catalog = ctx.catalog()?;
    let db = ctx.open_database()?;
    super::require_price_data(&db)?;

    let filter = SearchFilter::from_args(&args.filters, &ctx.config);
    let period = Period::new(args.from, args.to);
    let mut rows: Vec<(&Product, &HistoricPrice)> = Vec::new();
    let history = db.price_history()?;
    for entry in &history {
        let Some(product) = catalog.find(&entry.price.product) else {
            continue;
        };
        if filter.chain_matches(entry.price.chain)
            && filter.product_matches(product)
            && period.overlaps(entry.valid_from, entry.price.last_seen)
        {
            rows.push((product, entry));
        }
    }
    // Stable, so each listing's intervals stay oldest first.
    rows.sort_by(|(a, x), (b, y)| {
        (&a.name, a.volume, x.price.chain, x.price.listing_id).cmp(&(
            &b.name,
            b.volume,
            y.price.chain,
            y.price.listing_id,
        ))
    });

    let csv = Csv::new(args.excel);
    let mut out = BufWriter::new(std::io::stdout().lock());
    if args.excel {
        // A byte order mark, so Excel reads the file as UTF-8.
        out.write_all("\u{feff}".as_bytes())?;
    }
    csv.write_row(&mut out, HEADERS.iter().map(|h| (*h).to_owned()))?;
    for (product, entry) in rows {
        csv.write_row(&mut out, row(&csv, product, entry, ctx))?;
    }
    out.flush()?;
    Ok(ExitStatus::Ok)
}

fn row(csv: &Csv, product: &Product, entry: &HistoricPrice, ctx: &Context) -> Vec<String> {
    let price = &entry.price;
    let date = oslo_date(entry.valid_from);
    let (calc, _) = compute_price(price, false, &ctx.config.memberships, date);
    let liter_price = pricing::liter_price(calc.unit_price, product.volume);
    let yes_no = |value: bool| if value { "ja" } else { "nei" }.to_owned();
    let date_text = |d: Option<Date>| d.map(|d| d.to_string()).unwrap_or_default();
    vec![
        product.id.0.clone(),
        product.name.clone(),
        product.volume.get().to_string(),
        price.chain.slug().to_owned(),
        price.source.slug().to_owned(),
        price.pack_size.to_string(),
        csv.time(entry.valid_from),
        csv.time(price.last_seen),
        csv.kr(price.shelf_price),
        price
            .member_price
            .map(|m| csv.kr(m.price))
            .unwrap_or_default(),
        price
            .member_price
            .map(|m| m.program.slug().to_owned())
            .unwrap_or_default(),
        price
            .offer
            .as_ref()
            .map(|o| format::offer(&o.offer))
            .unwrap_or_default(),
        date_text(price.offer.as_ref().and_then(|o| o.valid_from)),
        date_text(price.offer.as_ref().and_then(|o| o.valid_to)),
        csv.kr(calc.unit_price),
        csv.kr(liter_price),
        csv.kr(pricing::deposit(product.volume, &ctx.config.deposit)),
        price.available.map(yes_no).unwrap_or_default(),
        yes_no(price.suspicious),
    ]
}

/// `--fra`/`--til` as a half-open period of Norwegian days.
struct Period {
    start: Option<Timestamp>,
    end: Option<Timestamp>,
}

impl Period {
    fn new(from: Option<Date>, to: Option<Date>) -> Self {
        Self {
            start: from.map(start_of_day),
            end: to.and_then(|d| d.tomorrow().ok()).map(start_of_day),
        }
    }

    /// Whether a price seen from `from` until `to` was seen within the period.
    fn overlaps(&self, from: Timestamp, to: Timestamp) -> bool {
        self.start.is_none_or(|start| to >= start) && self.end.is_none_or(|end| from < end)
    }
}

/// RFC 4180, or with `excel` the dialect Norwegian Excel reads: semicolons and decimal
/// commas.
struct Csv {
    excel: bool,
}

impl Csv {
    fn new(excel: bool) -> Self {
        Self { excel }
    }

    fn delimiter(&self) -> char {
        if self.excel { ';' } else { ',' }
    }

    /// Kroner with two decimals and no thousands separator.
    fn kr(&self, amount: Ore) -> String {
        let sign = if amount.0 < 0 { "-" } else { "" };
        let abs = amount.0.unsigned_abs();
        let decimal = if self.excel { ',' } else { '.' };
        format!("{sign}{}{decimal}{:02}", abs / 100, abs % 100)
    }

    /// Norwegian time: with the UTC offset, or for Excel in a form it reads as a date.
    fn time(&self, time: Timestamp) -> String {
        let zoned = oslo_time(time);
        if self.excel {
            zoned.strftime("%Y-%m-%d %H:%M:%S").to_string()
        } else {
            zoned.strftime("%Y-%m-%dT%H:%M:%S%:z").to_string()
        }
    }

    fn field(&self, text: &str) -> String {
        if text.contains([self.delimiter(), '"', '\n', '\r']) {
            format!("\"{}\"", text.replace('"', "\"\""))
        } else {
            text.to_owned()
        }
    }

    fn write_row(
        &self,
        out: &mut impl Write,
        fields: impl IntoIterator<Item = String>,
    ) -> Result<(), AppError> {
        let fields: Vec<String> = fields.into_iter().map(|f| self.field(&f)).collect();
        let delimiter = self.delimiter().to_string();
        write!(out, "{}\r\n", fields.join(&delimiter))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use jiff::civil::date;

    use super::*;

    #[test]
    fn plain_csv_quotes_only_when_needed() {
        let csv = Csv::new(false);
        assert_eq!(csv.field("Monster"), "Monster");
        assert_eq!(csv.field("3 for 2, nå"), "\"3 for 2, nå\"");
        assert_eq!(csv.field("si \"hei\""), "\"si \"\"hei\"\"\"");
        assert_eq!(csv.field("a;b"), "a;b");
        assert_eq!(csv.kr(Ore(124_900)), "1249.00");
    }

    #[test]
    fn excel_uses_semicolons_and_decimal_commas() {
        let csv = Csv::new(true);
        assert_eq!(csv.field("a;b"), "\"a;b\"");
        assert_eq!(csv.field("a,b"), "a,b");
        assert_eq!(csv.kr(Ore(2490)), "24,90");
        assert_eq!(csv.kr(Ore(-5)), "-0,05");
    }

    #[test]
    fn times_are_norwegian() {
        let summer: Timestamp = "2026-07-01T10:00:00Z".parse().unwrap();
        assert_eq!(Csv::new(false).time(summer), "2026-07-01T12:00:00+02:00");
        assert_eq!(Csv::new(true).time(summer), "2026-07-01 12:00:00");
    }

    #[test]
    fn rows_are_written_with_crlf() {
        let mut out = Vec::new();
        Csv::new(true)
            .write_row(&mut out, ["a".to_owned(), "b;c".to_owned()])
            .unwrap();
        assert_eq!(out, b"a;\"b;c\"\r\n");
    }

    #[test]
    fn period_includes_both_days() {
        let period = Period::new(Some(date(2026, 9, 1)), Some(date(2026, 9, 30)));
        let at = |text: &str| text.parse::<Timestamp>().unwrap();
        // Ended just after midnight on the first day.
        assert!(period.overlaps(at("2026-08-01T00:00:00Z"), at("2026-08-31T22:30:00Z")));
        // Ended the day before.
        assert!(!period.overlaps(at("2026-08-01T00:00:00Z"), at("2026-08-31T21:30:00Z")));
        // Started late on the last day.
        assert!(period.overlaps(at("2026-09-30T21:30:00Z"), at("2026-10-05T00:00:00Z")));
        assert!(!period.overlaps(at("2026-09-30T22:30:00Z"), at("2026-10-05T00:00:00Z")));
        assert!(
            Period::new(None, None)
                .overlaps(at("2020-01-01T00:00:00Z"), at("2020-01-01T00:00:00Z"))
        );
    }
}
