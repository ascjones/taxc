use argh::FromArgs;
use hmac::{Hmac, Mac, NewMac};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

const API_ENDPOINT: &'static str = "https://api.binance.com";

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
}

impl BinanceApiCommand {
    pub fn exec(&self) -> color_eyre::Result<()> {
        let symbols = self.get_symbols()?;
        for symbol in symbols {
            let trades = self.get_aggregated_trades(&symbol)?;
            crate::utils::write_csv(trades,std::io::stdout())?
        }
        Ok(())
    }

    fn get_symbols(&self) -> color_eyre::Result<Vec<String>> {
        // todo: fetch symols from binance API
        Ok(vec!["ATOMBTC".to_string()])
    }

    /// GET /api/v3/aggTrades
    ///
    /// [API Docs](https://github.com/binance/binance-spot-api-docs/blob/master/rest-api.md#compressedaggregate-trades-list)
    ///
    /// Get compressed, aggregate trades. Trades that fill at the time, from the same taker order,
    /// with the same price will have the quantity aggregated.
    fn get_aggregated_trades(&self, symbol: &str) -> color_eyre::Result<Vec<AggregatedTrade>> {
        let query_str = format!("symbol={}", symbol);
        let mut signed_key = Hmac::<sha2::Sha256>::new_varkey(self.secret.as_bytes()).unwrap();
        signed_key.update(query_str.as_bytes());
        let signature = hex::encode(signed_key.finalize().into_bytes());

        let url = format!("{}/api/v3/aggTrades", API_ENDPOINT);
        let response = ureq::get(&url)
            .set("Content-Type", "application/x-www-form-urlencoded")
            .set("x-mbx-apikey", self.api_key.as_str())
            .query("symbol", symbol)
            .query("signature", signature.as_str())
            .call()?;

        let trades: Vec<AggregatedTrade> = response.into_json()?;
        Ok(trades)
    }
}

/// Type returned from https://github.com/binance/binance-spot-api-docs/blob/master/rest-api.md#compressedaggregate-trades-list
///  ```json
/// {
///     "a": 26129,         // Aggregate tradeId
///     "p": "0.01633102",  // Price
///     "q": "4.70443515",  // Quantity
///     "f": 27781,         // First tradeId
///     "l": 27781,         // Last tradeId
///     "T": 1498793709153, // Timestamp
///     "m": true,          // Was the buyer the maker?
///     "M": true           // Was the trade the best price match?
/// }
/// ```
#[derive(Deserialize, Serialize)]
pub struct AggregatedTrade {
    #[serde(rename = "a")]
    id: u64,
    #[serde(rename = "p")]
    price: Decimal,
    #[serde(rename = "q")]
    quantity: Decimal,
    #[serde(rename = "f")]
    first_trade_id: u64,
    #[serde(rename = "l")]
    last_trade_id: u64,
    #[serde(rename = "t")]
    timestamp: u64,
    #[serde(rename = "m")]
    is_maker: u64,
    #[serde(rename = "M")]
    is_best_match: u64,
}

// Request must be signed
// fn sign_request(&self, endpoint: &str, request: &str) -> String {
//     let mut signed_key = Hmac::<Sha256>::new_varkey(self.secret_key.as_bytes()).unwrap();
//     signed_key.update(request.as_bytes());
//     let signature = hex_encode(signed_key.finalize().into_bytes());
//     let request_body: String = format!("{}&signature={}", request, signature);
//     let url: String = format!("{}{}?{}", self.host, endpoint, request_body);
//
//     url
// }
//
// fn build_headers(&self, content_type: bool) -> Result<HeaderMap> {
//     let mut custom_headers = HeaderMap::new();
//
//     custom_headers.insert(USER_AGENT, HeaderValue::from_static("binance-rs"));
//     if content_type {
//         custom_headers.insert(
//             CONTENT_TYPE,
//             HeaderValue::from_static("application/x-www-form-urlencoded"),
//         );
//     }
//     custom_headers.insert(
//         HeaderName::from_static("x-mbx-apikey"),
//         HeaderValue::from_str(self.api_key.as_str())?,
//     );
//
//     Ok(custom_headers)
// }
