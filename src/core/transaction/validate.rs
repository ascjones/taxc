use std::collections::{HashMap, HashSet};

use super::error::TransactionError;
use super::model::{Asset, AssetRegistry, Transaction, TransactionType};
use super::normalize::{is_gbp, normalize_currency};
use super::valuation::Valuation;
use crate::core::events::{AssetClass, Tag};
use crate::core::price::Price;

pub(super) fn validate_price_base(
    id: &str,
    price: &Price,
    expected_asset: &str,
) -> Result<(), TransactionError> {
    let price_base = normalize_currency(&price.base);
    let expected = normalize_currency(expected_asset);
    if price_base != expected {
        return Err(TransactionError::PriceBaseMismatch {
            id: id.to_string(),
            base: price.base.clone(),
            expected: expected_asset.to_string(),
        });
    }
    Ok(())
}

pub(super) fn asset_class_for(registry: &AssetRegistry, symbol: &str) -> AssetClass {
    if is_gbp(symbol) {
        return AssetClass::Fiat;
    }
    let normalized = normalize_currency(symbol);
    registry
        .get(normalized.as_str())
        .map(|asset| asset.asset_class.clone())
        .expect("asset validated")
}

pub(super) fn validate_assets(
    assets: &[Asset],
    transactions: &[Transaction],
) -> Result<AssetRegistry, TransactionError> {
    let mut registry: AssetRegistry = HashMap::new();

    for asset in assets {
        if is_gbp(&asset.symbol) {
            continue;
        }
        if registry.contains_key(asset.symbol.as_str()) {
            return Err(TransactionError::DuplicateAsset {
                symbol: asset.symbol.clone(),
            });
        }
        registry.insert(asset.symbol.clone(), asset.clone());
    }

    for tx in transactions {
        match &tx.details {
            TransactionType::Trade { sold, bought } => {
                validate_symbol(&registry, sold.asset.as_str())?;
                validate_symbol(&registry, bought.asset.as_str())?;
            }
            TransactionType::Deposit { amount, .. }
            | TransactionType::Withdrawal { amount, .. } => {
                validate_symbol(&registry, amount.asset.as_str())?;
            }
        }

        if let Some(fee) = &tx.fee {
            validate_symbol(&registry, fee.asset.as_str())?;
            if let Some(price) = &fee.price {
                validate_symbol(&registry, price.base.as_str())?;
            }
        }

        if let Some(price) = tx.valuation.as_ref().and_then(Valuation::price) {
            validate_symbol(&registry, price.base.as_str())?;
        }
    }

    Ok(registry)
}

fn validate_symbol(registry: &AssetRegistry, symbol: &str) -> Result<(), TransactionError> {
    if is_gbp(symbol) || registry.contains_key(symbol) {
        return Ok(());
    }
    Err(TransactionError::UndefinedAsset {
        symbol: symbol.to_string(),
    })
}

pub(super) fn validate_links(transactions: &[Transaction]) -> Result<(), TransactionError> {
    let mut seen = HashSet::new();
    let mut index: HashMap<&str, &Transaction> = HashMap::new();

    for tx in transactions {
        if !seen.insert(tx.id.clone()) {
            return Err(TransactionError::DuplicateTransactionId(tx.id.clone()));
        }
        index.insert(&tx.id, tx);
    }

    for tx in transactions {
        match &tx.details {
            TransactionType::Deposit {
                linked_withdrawal: Some(withdrawal_id),
                ..
            } if tx.tag == Tag::Unclassified => {
                let withdrawal = index.get(withdrawal_id.as_str()).ok_or_else(|| {
                    TransactionError::LinkedTransactionNotFound {
                        id: tx.id.clone(),
                        linked_id: withdrawal_id.clone(),
                    }
                })?;
                if !matches!(withdrawal.details, TransactionType::Withdrawal { .. }) {
                    return Err(TransactionError::LinkedTransactionTypeMismatch {
                        id: tx.id.clone(),
                        linked_id: withdrawal_id.clone(),
                    });
                }
                if !matches!(
                    withdrawal.details,
                    TransactionType::Withdrawal {
                        linked_deposit: Some(ref deposit_id),
                        ..
                    } if deposit_id == &tx.id
                ) {
                    return Err(TransactionError::LinkedTransactionNotReciprocal {
                        id: tx.id.clone(),
                        linked_id: withdrawal_id.clone(),
                    });
                }
            }
            TransactionType::Withdrawal {
                linked_deposit: Some(deposit_id),
                ..
            } if tx.tag == Tag::Unclassified => {
                let deposit = index.get(deposit_id.as_str()).ok_or_else(|| {
                    TransactionError::LinkedTransactionNotFound {
                        id: tx.id.clone(),
                        linked_id: deposit_id.clone(),
                    }
                })?;
                if !matches!(deposit.details, TransactionType::Deposit { .. }) {
                    return Err(TransactionError::LinkedTransactionTypeMismatch {
                        id: tx.id.clone(),
                        linked_id: deposit_id.clone(),
                    });
                }
                if !matches!(
                    deposit.details,
                    TransactionType::Deposit {
                        linked_withdrawal: Some(ref withdrawal_id),
                        ..
                    } if withdrawal_id == &tx.id
                ) {
                    return Err(TransactionError::LinkedTransactionNotReciprocal {
                        id: tx.id.clone(),
                        linked_id: deposit_id.clone(),
                    });
                }
            }
            _ => {}
        }
    }

    Ok(())
}
