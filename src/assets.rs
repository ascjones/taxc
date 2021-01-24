use rusty_money::define_currency_set;
use rust_decimal_macros::dec;

define_currency_set!(
    currencies {
        EUR: {
            code: "EUR",
            exponent: 2,
            locale: EnUs,
            minor_units: 100,
            name: "Euro",
            symbol: "€",
            symbol_first: true,
        },
        GBP: {
            code: "GBP",
            exponent: 2,
            locale: EnUs,
            minor_units: 100,
            name: "British Pound",
            symbol: "£",
            symbol_first: true,
        },
        USD: {
            code: "USD",
            exponent: 2,
            locale: EnUs,
            minor_units: 100,
            name: "United States Dollar",
            symbol: "$",
            symbol_first: true,
        },

        BTC: {
            code: "BTC",
            exponent: 8,
            locale: EnUs,
            minor_units: 100_000_000,
            name: "Bitcoin",
            symbol: "₿",
            symbol_first: true,
        },
        ETH: {
            code: "ETH",
            exponent: 18,
            locale: EnUs,
            minor_units: 1_000_000_000_000_000_000,
            name: "Ethereum",
            symbol: "ETH",
            symbol_first: false,
        },
        ETC: {
            code: "ETC",
            exponent: 18,
            locale: EnUs,
            minor_units: 1_000_000_000_000_000_000,
            name: "Ethereum Classic",
            symbol: "ETC",
            symbol_first: false,
        }
    }
);

// lazy_static! {
//     pub static ref XRP: Currency = Currency::new("XRP", 0, 2);
//     pub static ref REP: Currency = Currency::new("REP", 0, 2);
//     pub static ref DGD: Currency = Currency::new("DGD", 0, 2);
//     pub static ref UKG: Currency = Currency::new("UKG", 0, 2);
//     pub static ref OMG: Currency = Currency::new("OMG", 0, 2);
//     pub static ref DOT: Currency = Currency::new("DOT", 0, 2);
//     pub static ref ATM: Currency = Currency::new("ATM", 0, 2);
// }

// todo: make this return Result instead of panicking
pub fn amount<'a>(currency: &str, amount: rust_decimal::Decimal) -> crate::Money<'a> {
    let currency = currencies::find(currency).unwrap();
    rusty_money::Money::from_decimal(amount, currency)
}

pub fn zero(currency: &currencies::Currency) -> rusty_money::Money<currencies::Currency> {
    rusty_money::Money::from_decimal(dec!(0), currency)
}

pub fn parse_money_parts<'a>(currency: &str, amount: &str) -> Result<crate::Money<'a>, rusty_money::MoneyError> {
    let currency = currencies::find(currency).unwrap();
    rusty_money::Money::from_str(amount, currency)
}

// pub fn parse_money(money: &str) -> Result<rusty_money::Money<currencies::Currency>, ParseError> {
//     let parser = &CRYPTO_PARSER;
//     parser.parse(money)
// }
//
// pub fn get_currency(code: &str) -> Option<&currencies::Currency> {
//     currencies::find(code)
// }

pub fn display_amount(amt: &crate::Money) -> String {
    let params = rusty_money::Params {
        ..Default::default()
    };
    rusty_money::Formatter::money(&amt, params)
}
