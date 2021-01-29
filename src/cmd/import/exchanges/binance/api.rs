use argh::FromArgs;
use binance::{
    account::Account,
    api::Binance,
    model::TradeHistory,
};
use std::convert::TryFrom;
use crate::trades::{Trade, TradeKind, TradeRecord};
use chrono::NaiveDateTime;
use crate::money::{
    amount,
    Money,
};
use rust_decimal::Decimal;
use std::str::FromStr;
use crate::money::currencies::Currency;
use color_eyre::eyre;

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
        let trades = self.get_trade_history(&self.symbol)?;
        crate::utils::write_csv(trades,std::io::stdout())
    }

    fn get_trade_history(&self, symbol: &str) -> color_eyre::Result<Vec<TradeRecord>> {
        let account: Account = Binance::new(Some(self.api_key.clone()), Some(self.secret.clone()));
        let mut parts = symbol.split('/');
        let base_code = parts.next().ok_or(eyre::eyre!("Invalid symbol {}", symbol))?;
        let quote_code = parts.next().ok_or(eyre::eyre!("Invalid symbol {}", symbol))?;
        let base = crate::currencies::find(base_code)
            .ok_or(eyre::eyre!("failed to find base currency {}", base_code))?;
        let quote = crate::currencies::find(quote_code)
            .ok_or(eyre::eyre!("failed to find quote currency {}", quote_code))?;

        // the binance symbol has no separator e.g. ETHBTC
        let binance_symbol = symbol.replace('/', "");
        let trades =
            account.trade_history(binance_symbol)
                .unwrap() // todo: handle error
                .into_iter()
                .map(|trade| {
                    let trade = BinanceTrade { base: *base, quote: *quote, trade: trade.clone() };
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
