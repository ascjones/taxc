use std::collections::HashMap;

use rust_decimal::Decimal;

use super::error::TransactionError;
use super::normalize::{is_gbp, normalize_currency};
use super::valuation::Valuation;
use super::{Amount, Asset, AssetRegistry, Transaction, TransactionType};
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
        .map(|asset| asset.asset_class)
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

/// Reject zero/negative quantities and negative fee amounts.
pub(super) fn validate_amounts(transactions: &[Transaction]) -> Result<(), TransactionError> {
    fn check_positive(id: &str, amount: &Amount) -> Result<(), TransactionError> {
        if amount.quantity <= Decimal::ZERO {
            return Err(TransactionError::NonPositiveQuantity {
                id: id.to_string(),
                asset: amount.asset.clone(),
            });
        }
        Ok(())
    }

    for tx in transactions {
        match &tx.details {
            TransactionType::Trade { sold, bought } => {
                check_positive(&tx.id, sold)?;
                check_positive(&tx.id, bought)?;
            }
            TransactionType::Deposit { amount, .. }
            | TransactionType::Withdrawal { amount, .. } => {
                check_positive(&tx.id, amount)?;
            }
        }

        if let Some(fee) = &tx.fee {
            if fee.amount < Decimal::ZERO {
                return Err(TransactionError::NegativeFeeAmount { id: tx.id.clone() });
            }
        }
    }

    Ok(())
}

pub(super) fn validate_links(transactions: &[Transaction]) -> Result<(), TransactionError> {
    let mut index: HashMap<&str, &Transaction> = HashMap::new();
    for tx in transactions {
        if index.insert(tx.id.as_str(), tx).is_some() {
            return Err(TransactionError::DuplicateTransactionId(tx.id.clone()));
        }
    }

    for tx in transactions.iter().filter(|t| t.tag == Tag::Unclassified) {
        let Some((link, amount)) = transfer_link(tx) else {
            continue;
        };
        let other = index
            .get(link)
            .ok_or_else(|| TransactionError::LinkedTransactionNotFound {
                id: tx.id.clone(),
                linked_id: link.to_string(),
            })?;
        let opposite = matches!(
            (&tx.details, &other.details),
            (
                TransactionType::Deposit { .. },
                TransactionType::Withdrawal { .. }
            ) | (
                TransactionType::Withdrawal { .. },
                TransactionType::Deposit { .. }
            )
        );
        if !opposite {
            return Err(TransactionError::LinkedTransactionTypeMismatch {
                id: tx.id.clone(),
                linked_id: link.to_string(),
            });
        }
        let Some((_, other_amount)) = transfer_link(other).filter(|(back, _)| *back == tx.id)
        else {
            return Err(TransactionError::LinkedTransactionNotReciprocal {
                id: tx.id.clone(),
                linked_id: link.to_string(),
            });
        };

        if amount.asset != other_amount.asset {
            return Err(TransactionError::LinkedTransactionAssetMismatch {
                id: tx.id.clone(),
                asset: amount.asset.clone(),
                linked_id: link.to_string(),
                linked_asset: other_amount.asset.clone(),
            });
        }
        if let TransactionType::Withdrawal { .. } = tx.details {
            if other_amount.quantity > amount.quantity {
                return Err(TransactionError::LinkedDepositExceedsWithdrawal {
                    withdrawal_id: tx.id.clone(),
                    deposit_id: link.to_string(),
                });
            }
        }
    }

    Ok(())
}

/// The linked transaction id and moved amount of one leg of a transfer.
fn transfer_link(tx: &Transaction) -> Option<(&str, &Amount)> {
    match &tx.details {
        TransactionType::Deposit {
            amount,
            linked_withdrawal: Some(link),
        }
        | TransactionType::Withdrawal {
            amount,
            linked_deposit: Some(link),
        } => Some((link.as_str(), amount)),
        _ => None,
    }
}
