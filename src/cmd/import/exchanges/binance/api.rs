use crate::{
    money::{amount, currencies::Currency, Money},
    trades::{Trade, TradeKind, TradeRecord},
};
use argh::FromArgs;
use binance::{account::Account, api::Binance, model::TradeHistory};
use chrono::NaiveDateTime;
use color_eyre::eyre;
use rust_decimal::Decimal;
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
    /// the symbol of the market for trades to download, must be in the format BASE/QUOTE e.g
    /// BTC/GBP
    /// todo: could make this an option and if None fetch all from binance::api::General::exchange_info()
    #[argh(option)]
    symbol: String,
}

impl BinanceApiCommand {
    pub fn exec(&self) -> color_eyre::Result<()> {
        let trades = self.get_trade_history()?;
        let trade_records = self.convert_trades(trades)?;
        crate::utils::write_csv(trade_records, std::io::stdout())
    }

    fn get_trade_history(&self) -> color_eyre::Result<Vec<TradeHistory>> {
        let account: Account = Binance::new(Some(self.api_key.clone()), Some(self.secret.clone()));

        // the binance symbol has no separator e.g. ETHBTC
        let binance_symbol = self.symbol.replace('/', "");
        let trades = account
            .trade_history(binance_symbol)
            .map_err(|e| eyre::eyre!("Binance error {}", e))?;

        Ok(trades)
    }

    fn convert_trades(&self, trades: Vec<TradeHistory>) -> color_eyre::Result<Vec<TradeRecord>> {
        let mut parts = self.symbol.split('/');
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
        let qty = Decimal::try_from(trade.qty)?;
        let rate = Decimal::try_from(trade.price)?;

        // base e.g. in ETH/BTC this is the ETH
        let base_amount = Money::from_decimal(qty, &value.base);
        // quote e.g. in ETH/BTC this is the BTC
        let quote_amount = Money::from_decimal(qty * rate, &value.quote);

        let (kind, buy, sell) = if trade.is_buyer {
            (TradeKind::Buy, base_amount, quote_amount)
        } else {
            (TradeKind::Sell, quote_amount, base_amount)
        };

        let fee_amount = Decimal::from_str(&trade.commission)?;
        let fee = amount(&trade.commission_asset, fee_amount);

        Ok(Trade {
            date_time,
            kind,
            buy,
            sell,
            fee,
            rate,
            exchange: Some("Binance".into()),
        })
    }
}
