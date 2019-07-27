use chrono::NaiveDateTime;
use lazy_static::lazy_static;
use std::fmt;
use steel_cent::{
    currency::{self, Currency},
    formatting::{self, FormatPart, FormatSpec, ParseError},
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
    pub static ref CRYPTO_PARSER: formatting::Parser = formatting::uk_style()
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

pub type Year = i32;

#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub struct Account {
    pub name: String,
    pub kind: AccountKind,
}

impl Account {
    pub fn new(name: &str, kind: AccountKind) -> Self {
        Account {
            name: name.into(),
            kind,
        }
    }
}

impl fmt::Display for Account {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}:{}", self.name, self.kind)
    }
}

impl Account {
    pub fn parse(s: &str) -> Result<Account, String> {
        let mut parts = s.split(':');
        let name = parts.next().ok_or("expected name")?;
        let kind_str = parts.next().ok_or("expected kind")?;
        let kind = match kind_str {
            "Exchange" => Ok(AccountKind::Exchange),
            "Bank" => Ok(AccountKind::Bank),
            "Crypto" => {
                let network = parts
                    .next()
                    .ok_or("expected network for Crypto Account".into())
                    .and_then(|network| match network {
                        "Bitcoin" => Ok(Network::Bitcoin),
                        "Ethereum" => Ok(Network::Ethereum),
                        "EthereumClassic" => Ok(Network::EthereumClassic),
                        "Ripple" => Ok(Network::Ripple),
                        x => Err(format!("Unknown Crypto network {}", x)),
                    })?;
                let address = parts.next().map(Into::into);
                Ok(AccountKind::Crypto(network, address))
            }
            x => Err(format!("Unknown account kind {}", x)),
        }?;
        Ok(Account::new(name, kind))
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub enum AccountKind {
    Exchange,
    Bank,
    Crypto(Network, Option<Address>),
}

impl fmt::Display for AccountKind {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            AccountKind::Exchange => write!(f, "{}", "Exchange"),
            AccountKind::Bank => write!(f, "{}", "Bank"),
            AccountKind::Crypto(network, address) => {
                let network = match network {
                    Network::Bitcoin => "Bitcoin",
                    Network::Ethereum => "Ethereum",
                    Network::EthereumClassic => "EthereumClassic",
                    Network::Ripple => "Ripple",
                };
                write!(f, "Crypto:{}", network)?;
                match address {
                    Some(addr) => write!(f, ":{}", addr),
                    None => Ok(()),
                }
            }
        }
    }
}

pub type Address = String;

#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub enum Network {
    Ethereum,
    EthereumClassic,
    Bitcoin,
    Ripple,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Entry {
    pub account: Account,
    pub amount: Money,
}

impl Entry {
    pub fn new(account: Account, amount: Money) -> Self {
        Entry { account, amount }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Transaction {
    pub source_id: Option<String>,
    pub date_time: NaiveDateTime,
    pub debit: Entry,
    pub credit: Entry,
    pub fee: Money,
}

impl Transaction {
    pub fn new(
        source_id: Option<String>,
        date_time: NaiveDateTime,
        debit: Entry,
        credit: Entry,
        fee: Money,
    ) -> Self {
        Transaction {
            source_id,
            date_time,
            debit,
            credit,
            fee,
        }
    }
}

pub fn amount(currency: &str, amount: f64) -> Money {
    let money = if currency == "BTC" || currency == "ETH" {
        format!("{}{:.8}", currency, amount)
    } else {
        format!("{}{:.2}", currency, amount)
    };
    parse_money(&money).expect(&format!("{} is invalid money", money))
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
