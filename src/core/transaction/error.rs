#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum TransactionError {
    #[error("duplicate transaction id: {0}")]
    DuplicateTransactionId(String),
    #[error("linked transaction not found: {id} -> {linked_id}")]
    LinkedTransactionNotFound { id: String, linked_id: String },
    #[error("linked transaction type mismatch: {id} -> {linked_id}")]
    LinkedTransactionTypeMismatch { id: String, linked_id: String },
    #[error("linked transaction is not reciprocal: {id} -> {linked_id}")]
    LinkedTransactionNotReciprocal { id: String, linked_id: String },
    #[error("valuation required when neither side is GBP: {id}")]
    MissingTradeValuation { id: String },
    #[error("valuation required for {tag} {tx_type}: {id}")]
    MissingTaggedValuation {
        id: String,
        tag: String,
        tx_type: String,
    },
    #[error("tagged deposit cannot have linked_withdrawal: {id}")]
    TaggedDepositLinked { id: String },
    #[error("tagged withdrawal cannot have linked_deposit: {id}")]
    TaggedWithdrawalLinked { id: String },
    #[error("airdrop deposit must not include valuation: {id}")]
    AirdropValuationNotAllowed { id: String },
    #[error("GBP {tag} deposit must not include valuation: {id}")]
    GbpIncomeValuationNotAllowed { id: String, tag: String },
    #[error("valuation is not needed for GBP trades, value is derived from quantities: {id}")]
    GbpTradeValuationNotAllowed { id: String },
    #[error("{tag} tag not allowed on {tx_type}: {id}")]
    InvalidTagForType {
        id: String,
        tag: String,
        tx_type: String,
    },
    #[error("price base '{base}' does not match expected asset '{expected}': {id}")]
    PriceBaseMismatch {
        id: String,
        base: String,
        expected: String,
    },
    #[error("fee price required for non-GBP fee asset: {asset}")]
    MissingFeePrice { asset: String },
    #[error("invalid price configuration: {0}")]
    InvalidPrice(String),
    #[error("invalid datetime: {0}")]
    InvalidDatetime(String),
    #[error("undefined asset symbol: {symbol}")]
    UndefinedAsset { symbol: String },
    #[error("duplicate asset symbol: {symbol}")]
    DuplicateAsset { symbol: String },
}
