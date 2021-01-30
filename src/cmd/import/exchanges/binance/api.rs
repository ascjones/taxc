use crate::{
    money::{amount, currencies::Currency, Money},
    trades::{Trade, TradeKind, TradeRecord},
};
use argh::FromArgs;
use chrono::prelude::*;
use chrono::NaiveDateTime;
use color_eyre::eyre;
use hmac::{Hmac, Mac, NewMac};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::{convert::TryFrom, str::FromStr};

/// Import transactions from the binance API
#[derive(FromArgs, PartialEq, Debug)]
#[argh(subcommand, name = "binance")]
pub struct BinanceApiCommand {
    /// the api key
    #[argh(option)]
    api_key: String,
    /// the secret key
    /// !!! This will appear in your shell history so make sure this API key is restricted to your
    /// IP address. todo: make this more secure, encrypt with password? !!!
    #[argh(option)]
    secret: String,
    /// the symbol of the market for trades to download, must be in the format BASE-QUOTE e.g
    /// BTC-GBP
    /// todo: could make this an option and if None fetch all from binance::api::General::exchange_info()
    #[argh(option)]
    symbol: String,
}

impl BinanceApiCommand {
    pub fn exec(&self) -> color_eyre::Result<()> {
        let trades = self.get_trade_history(None)?;
        let trade_records = self.convert_trades(trades)?;
        crate::utils::write_csv(trade_records, std::io::stdout())
    }

    /// GET /api/v3/myTrades  (HMAC SHA256)
    ///
    /// [API Docs](https://github.com/binance/binance-spot-api-docs/blob/master/rest-api.md#account-trade-list-user_data)
    ///
    /// Get trades for a specific account and symbol.
    fn get_trade_history(&self, from_id: Option<u64>) -> color_eyre::Result<Vec<TradeHistory>> {
        let mut url =
            url::Url::from_str(&format!("{}/api/v3/myTrades", "https://api.binance.com"))?;

        let binance_symbol = self.symbol.replace("-", "");
        url.query_pairs_mut()
            .append_pair("symbol", &format!("{}", &binance_symbol));
        if let Some(from_id) = from_id {
            url.query_pairs_mut()
                .append_pair("fromId", &format!("{}", from_id));
        }
        url.query_pairs_mut()
            .append_pair("timestamp", &format!("{}", Utc::now().timestamp_millis()));

        let query_str = url.query().expect("missing query string");

        println!("{}", query_str);

        let mut signed_key = Hmac::<sha2::Sha256>::new_varkey(self.secret.as_bytes()).unwrap();
        signed_key.update(query_str.as_bytes());
        let signature = hex::encode(signed_key.finalize().into_bytes());

        let response = ureq::get(&url.to_string())
            .set("Content-Type", "application/x-www-form-urlencoded")
            .set("x-mbx-apikey", self.api_key.as_str())
            .query("signature", signature.as_str())
            .call()?;

        let trades: Vec<TradeHistory> = response.into_json()?;
        Ok(trades)
    }

    fn convert_trades(&self, trades: Vec<TradeHistory>) -> color_eyre::Result<Vec<TradeRecord>> {
        let mut parts = self.symbol.split('-');
        let base_code = parts
            .next()
            .ok_or(eyre::eyre!("Invalid symbol {}", self.symbol))?;
        let quote_code = parts
            .next()
            .ok_or(eyre::eyre!("Invalid symbol {}", self.symbol))?;
        let base = crate::currencies::find(base_code)
            .ok_or(eyre::eyre!("failed to find base currency {}", base_code))?;
        let quote = crate::currencies::find(quote_code)
            .ok_or(eyre::eyre!("failed to find quote currency {}", quote_code))?;

        let trades = trades
            .into_iter()
            .map(|trade| {
                let trade = BinanceTrade {
                    base: *base,
                    quote: *quote,
                    trade: trade.clone(),
                };
                Trade::try_from(&trade).map(|t| TradeRecord::from(&t))
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(trades)
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct TradeHistory {
    pub id: u64,
    pub price: Decimal,
    pub qty: Decimal,
    pub commission: Decimal,
    pub commission_asset: String,
    pub time: u64,
    pub is_buyer: bool,
    pub is_maker: bool,
    pub is_best_match: bool,
}

struct BinanceTrade {
    base: Currency,
    quote: Currency,
    trade: TradeHistory,
}

impl<'a> TryFrom<&'a BinanceTrade> for Trade<'a> {
    type Error = crate::cmd::import::exchanges::ExchangeError;

    fn try_from(value: &'a BinanceTrade) -> Result<Trade<'a>, Self::Error> {
        let trade = &value.trade;
        let seconds = trade.time as i64 / 1000;
        let nanos = (trade.time % 1000 * 1_000_000) as u32;
        let date_time = NaiveDateTime::from_timestamp(seconds, nanos);

        // base e.g. in ETH/BTC this is the ETH
        let base_amount = Money::from_decimal(trade.qty, &value.base);
        // quote e.g. in ETH/BTC this is the BTC
        let quote_amount = Money::from_decimal(trade.qty * trade.price, &value.quote);

        let (kind, buy, sell) = if trade.is_buyer {
            (TradeKind::Buy, base_amount, quote_amount)
        } else {
            (TradeKind::Sell, quote_amount, base_amount)
        };

        let fee = amount(&trade.commission_asset, trade.commission);

        Ok(Trade {
            date_time,
            kind,
            buy,
            sell,
            fee,
            rate: trade.price,
            exchange: Some("Binance".into()),
        })
    }
}
