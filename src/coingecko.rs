use crate::currencies::Currency;
use chrono::NaiveDateTime;
use color_eyre::eyre;
use rust_decimal::Decimal;

#[derive(Debug)]
pub struct CoingeckoPrice {
    pub timestamp: NaiveDateTime,
    pub price: Decimal,
}

/// Download all available daily prices for the given coin in the quote currency from the coingecko
/// API
pub fn fetch_daily_prices(coin: &str, quote_currency: &Currency) -> eyre::Result<Vec<CoingeckoPrice>> {
    let url = format!(
        "https://api.coingecko.com/api/v3/coins/{}/market_chart",
        coin
    );
    let response = ureq::get(&url)
        .query("vs_currency", quote_currency.code)
        .query("interval", "daily")
        .query("days", "max")
        .call()?;

    let coingecko_prices: Vec<(i64, Decimal)> = response.into_json()?;
    log::info!("{} {} prices fetched", coingecko_prices.len(), coin);

    Ok(coingecko_prices
        .into_iter()
        .map(|(timestamp, price)| {
            let unix_time_secs = timestamp / 1000;
            CoingeckoPrice {
                timestamp: NaiveDateTime::from_timestamp(unix_time_secs, 0).into(),
                price,
            }
        })
        .collect()
    )
}
