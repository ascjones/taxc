use super::valuation::Valuation;
use super::{Asset, Transaction, TransactionType};
use crate::core::price::Price;

fn normalize_price(price: &mut Price) {
    price.base = normalize_currency(&price.base);
    if let Some(quote) = price.quote.as_mut() {
        *quote = normalize_currency(quote);
    }
}

pub(super) fn normalize_transactions(transactions: &mut [Transaction]) {
    for tx in transactions {
        // Normalize valuation price object at transaction level.
        if let Some(Valuation::Price(p)) = tx.valuation.as_mut() {
            normalize_price(p);
        }

        // Normalize fee at transaction level
        if let Some(f) = tx.fee.as_mut() {
            f.asset = normalize_currency(&f.asset);
            if let Some(fp) = f.price.as_mut() {
                normalize_price(fp);
            }
        }

        // Normalize type-specific fields
        match &mut tx.details {
            TransactionType::Trade { sold, bought } => {
                sold.asset = normalize_currency(&sold.asset);
                bought.asset = normalize_currency(&bought.asset);
            }
            TransactionType::Deposit { amount, .. } => {
                amount.asset = normalize_currency(&amount.asset);
            }
            TransactionType::Withdrawal { amount, .. } => {
                amount.asset = normalize_currency(&amount.asset);
            }
        }
    }
}

pub(super) fn normalize_assets(assets: &mut [Asset]) {
    for asset in assets {
        asset.symbol = normalize_currency(&asset.symbol);
    }
}

pub(super) fn normalize_currency(s: &str) -> String {
    s.trim().to_uppercase()
}

pub(super) fn is_gbp(s: &str) -> bool {
    s.eq_ignore_ascii_case("GBP")
}
