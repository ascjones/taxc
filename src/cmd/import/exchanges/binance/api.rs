use argh::FromArgs;

/// Import transactions from the binance API
#[derive(FromArgs, PartialEq, Debug)]
#[argh(subcommand, name = "api")]
pub struct BinanceApiCommand {
    /// the api key
    #[argh(option)]
    api_key: String,
    /// the secret key
    #[argh(option)]
    secret: String,
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
