pub mod binance;
pub mod bittrex;
pub mod coinbase;
pub mod poloniex;
pub mod uphold;

#[derive(Debug, derive_more::From, derive_more::Display)]
pub enum ExchangeError {
    UnsupportedExchange(String),
    DateParse(chrono::format::ParseError),
    InvalidRecord(&'static str),
    DecimalError(rust_decimal::Error),
}

impl std::error::Error for ExchangeError {}
