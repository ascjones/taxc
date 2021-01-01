use lazy_static::lazy_static;
use steel_cent::{
    currency::{
        self,
        Currency,
    },
    formatting::{
        self,
        FormatPart,
        FormatSpec,
        ParseError,
    },
    Money,
};

lazy_static! {
    pub static ref BTC: Currency = Currency::new("BTC", 0, 8);
    pub static ref ETH: Currency = Currency::new("ETH", 0, 8);
    pub static ref ETC: Currency = Currency::new("ETC", 0, 2);
    pub static ref XRP: Currency = Currency::new("XRP", 0, 2);
    pub static ref REP: Currency = Currency::new("REP", 0, 2);
    pub static ref DGD: Currency = Currency::new("DGD", 0, 2);
    pub static ref UKG: Currency = Currency::new("UKG", 0, 2);
    pub static ref OMG: Currency = Currency::new("OMG", 0, 2);
    pub static ref DOT: Currency = Currency::new("DOT", 0, 2);
    pub static ref ATM: Currency = Currency::new("ATM", 0, 2);
}

lazy_static! {
    pub static ref CRYPTO_PARSER: formatting::Parser = formatting::generic_style()
        .parser()
        .with_short_symbol(*BTC, "BTC".to_string())
        .with_short_symbol(*ETH, "ETH".to_string())
        .with_short_symbol(*ETC, "ETC".to_string())
        .with_short_symbol(*XRP, "XRP".to_string())
        .with_short_symbol(*REP, "REP".to_string())
        .with_short_symbol(*DGD, "DGD".to_string())
        .with_short_symbol(*UKG, "UKG".to_string())
        .with_short_symbol(*OMG, "OMG".to_string())
        .with_short_symbol(*DOT, "DOT".to_string())
        .with_short_symbol(*ATM, "ATM".to_string());
}

lazy_static! {
    pub static ref STYLE_NO_SYMBOL: FormatSpec = FormatSpec::new(
        ',',
        '.',
        vec![FormatPart::OptionalMinus, FormatPart::Amount]
    );
}

// todo: make this return Result instead of panicking
pub fn amount(currency: &str, amount: f64) -> Money {
    let money = if currency == "BTC" || currency == "ETH" {
        format!("{:.8} {}", amount, currency)
    } else {
        format!("{:.2} {}", amount, currency)
    };
    parse_money(&money).expect(&format!("{} is invalid money", money))
}

pub fn parse_money_parts(currency: &str, amount: &str) -> Result<Money, ParseError> {
    parse_money(&format!("{} {}", amount, currency))
}

pub fn parse_money(money: &str) -> Result<Money, ParseError> {
    let parser = &CRYPTO_PARSER;
    parser.parse(money)
}

pub fn get_currency(code: &str) -> Option<Currency> {
    match code {
        "BTC" => Some(*BTC),
        "ETH" => Some(*ETH),
        _ => currency::with_code(code),
    }
}

pub fn display_amount(amt: &Money) -> String {
    STYLE_NO_SYMBOL.display_for(amt).to_string()
}
