use rust_decimal::Decimal;

use super::error::TransactionError;
use super::model::{AssetRegistry, Fee, Transaction, TransactionType};
use super::normalize::{is_gbp, normalize_currency};
use super::validate::{asset_class_for, validate_price_base};
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
            price,
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
                    if price.is_some() {
                        return Err(TransactionError::GbpTradePriceNotAllowed { id: id.clone() });
                    }
                    if is_gbp(&sold.asset) {
                        sold.quantity
                    } else {
                        bought.quantity
                    }
                } else {
                    let p = price
                        .as_ref()
                        .ok_or_else(|| TransactionError::MissingTradePrice { id: id.clone() })?;
                    validate_price_base(id, p, &bought.asset)?;
                    p.to_gbp(bought.quantity)?
                };

                let mut events = Vec::new();

                let has_disposal = !is_gbp(&sold.asset);
                let has_acquisition = !is_gbp(&bought.asset);

                // Fee uses trade price if fee asset matches bought asset
                let fee_gbp = match fee {
                    Some(f) => Some(fee_to_gbp_with_context(
                        f,
                        Some(&bought.asset),
                        price.as_ref(),
                    )?),
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
                            if price.is_some() {
                                return Err(TransactionError::GbpIncomePriceNotAllowed {
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
                        | Tag::Interest => {
                            let tx_price =
                                require_tagged_price(id, *tag, "deposit", price.as_ref())?;
                            validate_price_base(id, tx_price, &amount.asset)?;
                            tx_price.to_gbp(amount.quantity)?
                        }
                        Tag::Gift => {
                            let tx_price =
                                require_tagged_price(id, *tag, "deposit", price.as_ref())?;
                            validate_price_base(id, tx_price, &amount.asset)?;
                            tx_price.to_gbp(amount.quantity)?
                        }
                        Tag::Airdrop => {
                            if price.is_some() {
                                return Err(TransactionError::AirdropPriceNotAllowed {
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

                    // For airdrops and GBP income, there is no price context for fee resolution.
                    let (priced_asset, tx_price) = if *tag == Tag::Airdrop || price.is_none() {
                        (None, None)
                    } else {
                        (Some(amount.asset.as_str()), price.as_ref())
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

                let fee_gbp = match fee {
                    Some(f) => Some(fee_to_gbp_with_context(
                        f,
                        price.as_ref().map(|_| amount.asset.as_str()),
                        price.as_ref(),
                    )?),
                    None => None,
                };

                let value_gbp = match price {
                    Some(p) => {
                        validate_price_base(id, p, &amount.asset)?;
                        p.to_gbp(amount.quantity)?
                    }
                    None => Decimal::ZERO,
                };

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

                    let tx_price = require_tagged_price(id, *tag, "withdrawal", price.as_ref())?;
                    validate_price_base(id, tx_price, &amount.asset)?;
                    let fee_gbp = match fee {
                        Some(f) => Some(fee_to_gbp_with_context(
                            f,
                            Some(&amount.asset),
                            Some(tx_price),
                        )?),
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
                        value_gbp: tx_price.to_gbp(amount.quantity)?,
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

                let fee_gbp = match fee {
                    Some(f) => Some(fee_to_gbp_with_context(
                        f,
                        price.as_ref().map(|_| amount.asset.as_str()),
                        price.as_ref(),
                    )?),
                    None => None,
                };

                let value_gbp = match price {
                    Some(p) => {
                        validate_price_base(id, p, &amount.asset)?;
                        p.to_gbp(amount.quantity)?
                    }
                    None => Decimal::ZERO,
                };

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

fn require_tagged_price<'a>(
    id: &str,
    tag: Tag,
    tx_type: &str,
    price: Option<&'a Price>,
) -> Result<&'a Price, TransactionError> {
    price.ok_or_else(|| TransactionError::MissingTaggedPrice {
        id: id.to_string(),
        tag: tag_name(tag).to_string(),
        tx_type: tx_type.to_string(),
    })
}
