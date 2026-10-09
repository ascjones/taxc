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
            TransactionType::Demerger {
                original,
                new_holding,
                ..
            } => {
                validate_symbol(&registry, original.as_str())?;
                validate_symbol(&registry, new_holding.asset.as_str())?;
            }
            TransactionType::RightsIssue { new_shares, .. } => {
                validate_symbol(&registry, new_shares.asset.as_str())?;
            }
            TransactionType::SmallCapitalDistribution { asset, .. } => {
                validate_symbol(&registry, asset.as_str())?;
            }
            TransactionType::Fee {} => {}
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

/// Reject zero/negative quantities and amounts, negative fee amounts, and a
/// demerger fraction outside (0, 1) or into its own original asset.
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
    fn check_positive_gbp(id: &str, field: &str, value: Decimal) -> Result<(), TransactionError> {
        if value <= Decimal::ZERO {
            return Err(TransactionError::NonPositiveAmount {
                id: id.to_string(),
                field: field.to_string(),
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
            TransactionType::Demerger {
                original,
                new_holding,
                cost_fraction,
            } => {
                check_positive(&tx.id, new_holding)?;
                if *cost_fraction <= Decimal::ZERO || *cost_fraction >= Decimal::ONE {
                    return Err(TransactionError::InvalidCostFraction { id: tx.id.clone() });
                }
                if *original == new_holding.asset {
                    return Err(TransactionError::DemergerSameAsset { id: tx.id.clone() });
                }
            }
            TransactionType::RightsIssue {
                new_shares,
                consideration,
            } => {
                check_positive(&tx.id, new_shares)?;
                check_positive_gbp(&tx.id, "consideration", *consideration)?;
            }
            TransactionType::SmallCapitalDistribution { amount, .. } => {
                check_positive_gbp(&tx.id, "amount", *amount)?;
            }
            TransactionType::Fee {} => {}
        }

        if let Some(fee) = &tx.fee {
            if fee.amount < Decimal::ZERO {
                return Err(TransactionError::NegativeFeeAmount { id: tx.id.clone() });
            }
        }
    }

    Ok(())
}

/// The reorganisation and fee-only types take the default tag and no
/// valuation, never apply to sterling, and constrain the fee: a `Fee` needs a
/// positive one, and a demerger or small capital distribution allows none.
pub(super) fn validate_restricted_types(
    transactions: &[Transaction],
) -> Result<(), TransactionError> {
    for tx in transactions {
        let (tx_type, assets, fee_rule) = match &tx.details {
            TransactionType::Trade { .. }
            | TransactionType::Deposit { .. }
            | TransactionType::Withdrawal { .. } => continue,
            TransactionType::Demerger {
                original,
                new_holding,
                ..
            } => (
                "Demerger",
                vec![original.as_str(), new_holding.asset.as_str()],
                FeeRule::None,
            ),
            TransactionType::RightsIssue { new_shares, .. } => (
                "RightsIssue",
                vec![new_shares.asset.as_str()],
                FeeRule::Optional,
            ),
            TransactionType::SmallCapitalDistribution { asset, .. } => (
                "SmallCapitalDistribution",
                vec![asset.as_str()],
                FeeRule::None,
            ),
            TransactionType::Fee {} => ("Fee", vec![], FeeRule::Required),
        };
        let id = || tx.id.clone();
        let tx_type_string = || tx_type.to_string();

        if tx.tag != Tag::Unclassified {
            return Err(TransactionError::InvalidTagForType {
                id: id(),
                tag: format!("{:?}", tx.tag),
                tx_type: tx_type_string(),
            });
        }
        if tx.valuation.is_some() {
            return Err(TransactionError::ValuationNotAllowed {
                id: id(),
                tx_type: tx_type_string(),
            });
        }
        if assets.into_iter().any(is_gbp) {
            return Err(TransactionError::SterlingNotAllowed {
                id: id(),
                tx_type: tx_type_string(),
            });
        }
        match fee_rule {
            FeeRule::None if tx.fee.is_some() => {
                return Err(TransactionError::FeeNotAllowed {
                    id: id(),
                    tx_type: tx_type_string(),
                });
            }
            FeeRule::Required if !tx.fee.as_ref().is_some_and(|f| f.amount > Decimal::ZERO) => {
                return Err(TransactionError::FeeRequired { id: id() });
            }
            _ => {}
        }
    }
    Ok(())
}

enum FeeRule {
    None,
    Optional,
    Required,
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
