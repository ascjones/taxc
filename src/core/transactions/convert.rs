use rust_decimal::Decimal;

use super::error::TransactionError;
use super::normalize::{is_gbp, normalize_currency};
use super::validate::{asset_class_for, validate_price_base};
use super::valuation::Valuation;
use super::{AssetRegistry, Fee, Transaction, TransactionType};
use crate::core::events::{EventType, Tag, TaxableEvent};
use crate::core::price::Price;

impl Transaction {
    pub fn to_taxable_events(
        &self,
        registry: &AssetRegistry,
        exclude_unlinked: bool,
    ) -> Result<Vec<TaxableEvent>, TransactionError> {
        let Transaction {
            id,
            datetime,
            description,
            valuation,
            fee,
            tag,
            details,
            ..
        } = self;

        let mut event_index = 1usize;
        let mut next_event_id = || {
            let event_id = event_index;
            event_index += 1;
            event_id
        };

        match details {
            TransactionType::Trade { sold, bought } => {
                if !matches!(tag, Tag::Unclassified | Tag::Trade) {
                    return Err(TransactionError::InvalidTagForType {
                        id: id.clone(),
                        tag: tag_name(*tag).to_string(),
                        tx_type: "trade".to_string(),
                    });
                }

                let value_gbp = if is_gbp(&sold.asset) || is_gbp(&bought.asset) {
                    match valuation.as_ref() {
                        None => {
                            if is_gbp(&sold.asset) {
                                sold.quantity
                            } else {
                                bought.quantity
                            }
                        }
                        Some(_) => {
                            return Err(TransactionError::GbpTradeValuationNotAllowed {
                                id: id.clone(),
                            });
                        }
                    }
                } else {
                    match valuation.as_ref() {
                        Some(Valuation::Price(price)) => {
                            validate_price_base(id, price, &bought.asset)?;
                            price.to_gbp(bought.quantity)?
                        }
                        Some(Valuation::ValueGbp(value_gbp)) => *value_gbp,
                        None => {
                            return Err(TransactionError::MissingTradeValuation { id: id.clone() });
                        }
                    }
                };

                let mut events = Vec::new();

                let has_disposal = !is_gbp(&sold.asset);
                let has_acquisition = !is_gbp(&bought.asset);

                let tx_price = valuation.as_ref().and_then(Valuation::price);

                // Fee uses trade price if fee asset matches bought asset
                let fee_gbp = match fee {
                    Some(f) => Some(fee_to_gbp_with_context(f, Some(&bought.asset), tx_price)?),
                    None => None,
                };

                if has_disposal {
                    events.push(TaxableEvent {
                        id: next_event_id(),
                        source_transaction_id: id.clone(),
                        event_type: EventType::Disposal,
                        tag: Tag::Trade,
                        datetime: *datetime,
                        asset: normalize_currency(&sold.asset),
                        asset_class: asset_class_for(registry, &sold.asset),
                        quantity: sold.quantity,
                        value_gbp,
                        fee_gbp,
                        description: description.clone(),
                    });
                }

                if has_acquisition {
                    let acquisition_fee = if !has_disposal { fee_gbp } else { None };
                    events.push(TaxableEvent {
                        id: next_event_id(),
                        source_transaction_id: id.clone(),
                        event_type: EventType::Acquisition,
                        tag: Tag::Trade,
                        datetime: *datetime,
                        asset: normalize_currency(&bought.asset),
                        asset_class: asset_class_for(registry, &bought.asset),
                        quantity: bought.quantity,
                        value_gbp,
                        fee_gbp: acquisition_fee,
                        description: description.clone(),
                    });
                }

                Ok(events)
            }

            TransactionType::Deposit {
                amount,
                linked_withdrawal,
            } => {
                if *tag != Tag::Unclassified {
                    if linked_withdrawal.is_some() {
                        return Err(TransactionError::TaggedDepositLinked { id: id.clone() });
                    }

                    let value_gbp = match tag {
                        Tag::Dividend | Tag::Interest if is_gbp(&amount.asset) => {
                            if valuation.is_some() {
                                return Err(TransactionError::GbpIncomeValuationNotAllowed {
                                    id: id.clone(),
                                    tag: tag_name(*tag).to_string(),
                                });
                            }
                            amount.quantity
                        }
                        Tag::StakingReward
                        | Tag::Salary
                        | Tag::OtherIncome
                        | Tag::AirdropIncome
                        | Tag::Dividend
                        | Tag::Interest
                        | Tag::Gift => valuation_to_gbp_required(
                            id,
                            *tag,
                            "deposit",
                            valuation.as_ref(),
                            &amount.asset,
                            amount.quantity,
                        )?,
                        Tag::Airdrop => {
                            if valuation.is_some() {
                                return Err(TransactionError::AirdropValuationNotAllowed {
                                    id: id.clone(),
                                });
                            }
                            Decimal::ZERO
                        }
                        Tag::Trade | Tag::Unclassified => {
                            return Err(TransactionError::InvalidTagForType {
                                id: id.clone(),
                                tag: tag_name(*tag).to_string(),
                                tx_type: "deposit".to_string(),
                            });
                        }
                    };

                    // For airdrops, GBP income, and direct GBP valuations there is no price context.
                    let tx_price = valuation.as_ref().and_then(Valuation::price);
                    let (priced_asset, tx_price) = if *tag == Tag::Airdrop || tx_price.is_none() {
                        (None, None)
                    } else {
                        (Some(amount.asset.as_str()), tx_price)
                    };
                    let fee_gbp = match fee {
                        Some(f) => Some(fee_to_gbp_with_context(f, priced_asset, tx_price)?),
                        None => None,
                    };

                    return Ok(vec![TaxableEvent {
                        id: next_event_id(),
                        source_transaction_id: id.clone(),
                        event_type: EventType::Acquisition,
                        tag: *tag,
                        datetime: *datetime,
                        asset: normalize_currency(&amount.asset),
                        asset_class: asset_class_for(registry, &amount.asset),
                        quantity: amount.quantity,
                        value_gbp,
                        fee_gbp,
                        description: description.clone(),
                    }]);
                }

                if linked_withdrawal.is_some() || is_gbp(&amount.asset) {
                    return Ok(vec![]);
                }

                if exclude_unlinked {
                    log::warn!(
                        "Skipping unlinked deposit: id={} asset={}",
                        id,
                        amount.asset
                    );
                    return Ok(vec![]);
                }

                let tx_price = valuation.as_ref().and_then(Valuation::price);
                let fee_gbp = match fee {
                    Some(f) => Some(fee_to_gbp_with_context(
                        f,
                        tx_price.map(|_| amount.asset.as_str()),
                        tx_price,
                    )?),
                    None => None,
                };

                let value_gbp = valuation_to_gbp_optional(
                    id,
                    valuation.as_ref(),
                    &amount.asset,
                    amount.quantity,
                )?;

                log::warn!(
                    "Unlinked deposit treated as acquisition: id={} asset={}",
                    id,
                    amount.asset
                );
                Ok(vec![TaxableEvent {
                    id: next_event_id(),
                    source_transaction_id: id.clone(),
                    event_type: EventType::Acquisition,
                    tag: Tag::Unclassified,
                    datetime: *datetime,
                    asset: normalize_currency(&amount.asset),
                    asset_class: asset_class_for(registry, &amount.asset),
                    quantity: amount.quantity,
                    value_gbp,
                    fee_gbp,
                    description: description.clone(),
                }])
            }

            TransactionType::Withdrawal {
                amount,
                linked_deposit,
            } => {
                if *tag != Tag::Unclassified {
                    if linked_deposit.is_some() {
                        return Err(TransactionError::TaggedWithdrawalLinked { id: id.clone() });
                    }

                    if *tag != Tag::Gift {
                        return Err(TransactionError::InvalidTagForType {
                            id: id.clone(),
                            tag: tag_name(*tag).to_string(),
                            tx_type: "withdrawal".to_string(),
                        });
                    }

                    let value_gbp = valuation_to_gbp_required(
                        id,
                        *tag,
                        "withdrawal",
                        valuation.as_ref(),
                        &amount.asset,
                        amount.quantity,
                    )?;
                    let tx_price = valuation.as_ref().and_then(Valuation::price);
                    let fee_gbp = match fee {
                        Some(f) => Some(fee_to_gbp_with_context(f, Some(&amount.asset), tx_price)?),
                        None => None,
                    };

                    return Ok(vec![TaxableEvent {
                        id: next_event_id(),
                        source_transaction_id: id.clone(),
                        event_type: EventType::Disposal,
                        tag: Tag::Gift,
                        datetime: *datetime,
                        asset: normalize_currency(&amount.asset),
                        asset_class: asset_class_for(registry, &amount.asset),
                        quantity: amount.quantity,
                        value_gbp,
                        fee_gbp,
                        description: description.clone(),
                    }]);
                }

                if linked_deposit.is_some() || is_gbp(&amount.asset) {
                    return Ok(vec![]);
                }

                if exclude_unlinked {
                    log::warn!(
                        "Skipping unlinked withdrawal: id={} asset={}",
                        id,
                        amount.asset
                    );
                    return Ok(vec![]);
                }

                let tx_price = valuation.as_ref().and_then(Valuation::price);
                let fee_gbp = match fee {
                    Some(f) => Some(fee_to_gbp_with_context(
                        f,
                        tx_price.map(|_| amount.asset.as_str()),
                        tx_price,
                    )?),
                    None => None,
                };

                let value_gbp = valuation_to_gbp_optional(
                    id,
                    valuation.as_ref(),
                    &amount.asset,
                    amount.quantity,
                )?;

                log::warn!(
                    "Unlinked withdrawal treated as disposal: id={} asset={}",
                    id,
                    amount.asset
                );
                Ok(vec![TaxableEvent {
                    id: next_event_id(),
                    source_transaction_id: id.clone(),
                    event_type: EventType::Disposal,
                    tag: Tag::Unclassified,
                    datetime: *datetime,
                    asset: normalize_currency(&amount.asset),
                    asset_class: asset_class_for(registry, &amount.asset),
                    quantity: amount.quantity,
                    value_gbp,
                    fee_gbp,
                    description: description.clone(),
                }])
            }
        }
    }
}

fn fee_to_gbp_with_context(
    fee: &Fee,
    priced_asset: Option<&str>,
    tx_price: Option<&Price>,
) -> Result<Decimal, TransactionError> {
    // GBP fees need no conversion.
    if is_gbp(&fee.asset) {
        return Ok(fee.amount);
    }

    // Explicit fee price takes precedence.
    if let Some(price) = &fee.price {
        return price.to_gbp(fee.amount);
    }

    // Use transaction price if fee asset matches the priced asset.
    if let (Some(asset), Some(price)) = (priced_asset, tx_price) {
        let fee_asset_normalized = normalize_currency(&fee.asset);
        if fee_asset_normalized == normalize_currency(asset) {
            return price.to_gbp(fee.amount);
        }
    }

    // Fee asset doesn't match or no price available; require explicit price.
    Err(TransactionError::MissingFeePrice {
        asset: fee.asset.clone(),
    })
}

fn tag_name(tag: Tag) -> &'static str {
    match tag {
        Tag::Unclassified => "Unclassified",
        Tag::Trade => "Trade",
        Tag::StakingReward => "StakingReward",
        Tag::Salary => "Salary",
        Tag::OtherIncome => "OtherIncome",
        Tag::Airdrop => "Airdrop",
        Tag::AirdropIncome => "AirdropIncome",
        Tag::Dividend => "Dividend",
        Tag::Interest => "Interest",
        Tag::Gift => "Gift",
    }
}

fn valuation_to_gbp_required(
    id: &str,
    tag: Tag,
    tx_type: &str,
    valuation: Option<&Valuation>,
    expected_asset: &str,
    quantity: Decimal,
) -> Result<Decimal, TransactionError> {
    match valuation {
        Some(Valuation::Price(price)) => {
            validate_price_base(id, price, expected_asset)?;
            price.to_gbp(quantity)
        }
        Some(Valuation::ValueGbp(value_gbp)) => Ok(*value_gbp),
        None => Err(TransactionError::MissingTaggedValuation {
            id: id.to_string(),
            tag: tag_name(tag).to_string(),
            tx_type: tx_type.to_string(),
        }),
    }
}

fn valuation_to_gbp_optional(
    id: &str,
    valuation: Option<&Valuation>,
    expected_asset: &str,
    quantity: Decimal,
) -> Result<Decimal, TransactionError> {
    match valuation {
        Some(Valuation::Price(price)) => {
            validate_price_base(id, price, expected_asset)?;
            price.to_gbp(quantity)
        }
        Some(Valuation::ValueGbp(value_gbp)) => Ok(*value_gbp),
        None => Ok(Decimal::ZERO),
    }
}
