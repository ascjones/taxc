use rust_decimal_macros::dec;
use rusty_money::define_currency_set;

pub type Money<'a> = rusty_money::Money<'a, currencies::Currency>;

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
        },
        ATOM: {
            code: "ATOM",
            exponent: 6,
            locale: EnUs,
            minor_units: 1_000_000,
            name: "Cosmos ATOM",
            symbol: "ATOM",
            symbol_first: false,
        },
        XRP: {
            code: "XRP",
            exponent: 6,
            locale: EnUs,
            minor_units: 1_000_000,
            name: "Ripple",
            symbol: "XRP",
            symbol_first: false,
        },
        REP: {
            code: "REP",
            exponent: 18,
            locale: EnUs,
            minor_units: 1_000_000_000_000_000_000,
            name: "Augur",
            symbol: "REP",
            symbol_first: false,
        },
        DGD: {
            code: "DGD",
            exponent: 18,
            locale: EnUs,
            minor_units: 1_000_000_000_000_000_000,
            name: "Digix DAO",
            symbol: "DGD",
            symbol_first: false,
        },
        UKG: {
            code: "UKG",
            exponent: 18,
            locale: EnUs,
            minor_units: 1_000_000_000_000_000_000,
            name: "Unikoin Gold",
            symbol: "UKG",
            symbol_first: false,
        },
        OMG: {
            code: "OMG",
            exponent: 18,
            locale: EnUs,
            minor_units: 1_000_000_000_000_000_000,
            name: "OMG Network",
            symbol: "OMG",
            symbol_first: false,
        },
        DOT: {
            code: "DOT",
            exponent: 18,
            locale: EnUs,
            minor_units: 1_000_000_000_000_000_000,
            name: "Polkadot",
            symbol: "DOT",
            symbol_first: false,
        }
    }
);

// todo: make this return Result instead of panicking
pub fn amount<'a>(currency: &str, amount: rust_decimal::Decimal) -> crate::Money<'a> {
    let currency = currencies::find(currency).unwrap();
    rusty_money::Money::from_decimal(amount, currency)
}

pub fn zero(currency: &currencies::Currency) -> rusty_money::Money<currencies::Currency> {
    rusty_money::Money::from_decimal(dec!(0), currency)
}

pub fn parse_money_parts<'a>(
    currency: &str,
    amount: &str,
) -> Result<crate::Money<'a>, rusty_money::MoneyError> {
    let currency = currencies::find(currency).unwrap();
    rusty_money::Money::from_str(amount, currency)
}

pub fn display_amount(amt: &crate::Money) -> String {
    let params = rusty_money::Params {
        ..Default::default()
    };
    rusty_money::Formatter::money(&amt, params)
}
