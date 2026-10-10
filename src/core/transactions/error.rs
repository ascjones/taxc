#[derive(Debug, thiserror::Error, PartialEq, Eq)]
#[non_exhaustive]
pub enum TransactionError {
    #[error("duplicate transaction id: {0}")]
    DuplicateTransactionId(String),
    #[error("linked transaction not found: {id} -> {linked_id}")]
    LinkedTransactionNotFound { id: String, linked_id: String },
    #[error("linked transaction type mismatch: {id} -> {linked_id}")]
    LinkedTransactionTypeMismatch { id: String, linked_id: String },
    #[error("linked transaction is not reciprocal: {id} -> {linked_id}")]
    LinkedTransactionNotReciprocal { id: String, linked_id: String },
    #[error(
        "linked transfer moves different assets: {id} ({asset}) -> {linked_id} ({linked_asset})"
    )]
    LinkedTransactionAssetMismatch {
        id: String,
        asset: String,
        linked_id: String,
        linked_asset: String,
    },
    #[error("linked deposit {deposit_id} receives more than withdrawal {withdrawal_id} sent")]
    LinkedDepositExceedsWithdrawal {
        withdrawal_id: String,
        deposit_id: String,
    },
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
    #[error("quantity must be positive for {asset}: {id}")]
    NonPositiveQuantity { id: String, asset: String },
    #[error("{field} must be positive: {id}")]
    NonPositiveAmount { id: String, field: String },
    #[error("cost_fraction must be greater than 0 and less than 1: {id}")]
    InvalidCostFraction { id: String },
    #[error("demerger new holding must be a different asset from the original: {id}")]
    DemergerSameAsset { id: String },
    #[error("{tx_type} cannot apply to GBP: {id}")]
    SterlingNotAllowed { id: String, tx_type: String },
    #[error("Fee transaction requires a fee with a positive amount: {id}")]
    FeeRequired { id: String },
    #[error("fee not allowed on {tx_type}: {id}")]
    FeeNotAllowed { id: String, tx_type: String },
    #[error("valuation not allowed on {tx_type}: {id}")]
    ValuationNotAllowed { id: String, tx_type: String },
    #[error("valuation cannot be negative: {id}")]
    NegativeValuation { id: String },
    #[error("fee amount cannot be negative: {id}")]
    NegativeFeeAmount { id: String },
    #[error("undefined asset symbol: {symbol}")]
    UndefinedAsset { symbol: String },
    #[error("duplicate asset symbol: {symbol}")]
    DuplicateAsset { symbol: String },
}
