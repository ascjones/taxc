pub mod binance;
pub mod bittrex;
pub mod coinbase;
pub mod poloniex;
pub mod uphold;

#[derive(Debug, derive_more::From, derive_more::Display)]
pub enum ExchangeError {
    DateParse(chrono::format::ParseError),
    InvalidRecord(&'static str),
}

impl std::error::Error for ExchangeError {}
