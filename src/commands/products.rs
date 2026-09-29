//! `databrus produkter` – the product catalog, filtered, without prices.

use serde::Serialize;

use crate::Context;
use crate::cli::ProductsArgs;
use crate::db::UnmatchedListing;
use crate::error::{AppError, ExitStatus};
use crate::model::Product;
use crate::output::json::{self, Envelope, Header};
use crate::output::{OutputFormat, format, table};
use crate::search::SearchFilter;

#[derive(Serialize)]
struct ProductsContent<'a> {
    #[serde(rename = "produkter")]
    products: Vec<&'a Product>,
}

#[derive(Serialize)]
struct UnmatchedContent {
    #[serde(rename = "ukjente")]
    unmatched: Vec<UnmatchedListing>,
}

pub fn run(args: &ProductsArgs, ctx: &Context) -> Result<ExitStatus, AppError> {
    if args.unknown {
        return unmatched(ctx);
    }

    let catalog = ctx.catalog()?;
    let filter = SearchFilter::from_args(&args.filters, &ctx.config);
    let mut products: Vec<&Product> = catalog
        .products
        .iter()
        .filter(|p| filter.product_matches(p))
        .collect();
    products.sort_by(|a, b| a.name.cmp(&b.name).then(a.volume.cmp(&b.volume)));

    match ctx.output_format() {
        OutputFormat::Json => json::write(&Envelope::new(ProductsContent { products }))?,
        OutputFormat::JsonLines => json::write_lines(&Header::new(), &products)?,
        OutputFormat::Table if products.is_empty() => {
            eprintln!("ingen produkter passer filtrene");
        }
        OutputFormat::Table => {
            let mut t = table::new(&["Produkt", "Str.", "Beholder", "Sukkerfri", "EAN-er", "Id"]);
            for p in &products {
                t.add_row(vec![
                    p.name.clone(),
                    format::liters(p.volume),
                    p.container.slug().to_owned(),
                    if p.sugar_free { "ja" } else { "nei" }.to_owned(),
                    p.gtin.len().to_string(),
                    p.id.0.clone(),
                ]);
            }
            table::write(&t)?;
        }
    }
    Ok(ExitStatus::Ok)
}

fn unmatched(ctx: &Context) -> Result<ExitStatus, AppError> {
    let db = ctx.open_database()?;
    let unmatched = db.unmatched_listings()?;
    match ctx.output_format() {
        OutputFormat::Json => json::write(&Envelope::new(UnmatchedContent { unmatched }))?,
        OutputFormat::JsonLines => json::write_lines(&Header::new(), &unmatched)?,
        OutputFormat::Table if unmatched.is_empty() => {
            eprintln!("ingen umatchede oppføringer");
        }
        OutputFormat::Table => {
            let mut t = table::new(&["Navn", "Kjede", "Kilde", "EAN"]);
            for u in &unmatched {
                t.add_row(vec![
                    u.raw_name.clone(),
                    u.chain.clone(),
                    u.source.clone(),
                    u.gtin.clone().unwrap_or_else(|| "–".into()),
                ]);
            }
            table::write(&t)?;
        }
    }
    Ok(ExitStatus::Ok)
}
