use chrono::{DateTime, FixedOffset};
use rust_decimal::Decimal;

use super::error::TransactionError;
use super::normalize::{is_gbp, normalize_currency};
use super::validate::{asset_class_for, validate_price_base};
use super::valuation::Valuation;
use super::{Amount, AssetRegistry, Fee, Transaction, TransactionType};
use crate::core::events::{EventType, Tag, TaxableEvent};
use crate::core::price::Price;

/// Event ids are assigned globally by `transactions_to_events` after all
/// transactions are converted and sorted.
const UNASSIGNED_ID: usize = 0;

impl Transaction {
    pub fn to_taxable_events(
        &self,
        registry: &AssetRegistry,
        exclude_unlinked: bool,
    ) -> Result<Vec<TaxableEvent>, TransactionError> {
        let ctx = EventContext {
            id: &self.id,
            datetime: self.datetime,
            account: &self.account,
            description: &self.description,
            valuation: self.valuation.as_ref(),
            fee: self.fee.as_ref(),
            tag: self.tag,
            registry,
            exclude_unlinked,
        };

        match &self.details {
            TransactionType::Trade { sold, bought } => ctx.trade_events(sold, bought),
            TransactionType::Deposit {
                amount,
                linked_withdrawal,
            } => ctx.deposit_events(amount, linked_withdrawal.as_deref()),
            TransactionType::Withdrawal {
                amount,
                linked_deposit,
            } => ctx.withdrawal_events(amount, linked_deposit.as_deref()),
        }
    }
}

/// The transaction-level fields every event derived from one transaction
/// shares, so the per-type conversions below take one argument rather than
/// eight.
struct EventContext<'a> {
    id: &'a str,
    datetime: DateTime<FixedOffset>,
    account: &'a str,
    description: &'a Option<String>,
    valuation: Option<&'a Valuation>,
    fee: Option<&'a Fee>,
    tag: Tag,
    registry: &'a AssetRegistry,
    exclude_unlinked: bool,
}

impl EventContext<'_> {
    /// Build one event, filling in the fields shared by every event this
    /// transaction produces.
    fn event(
        &self,
        event_type: EventType,
        tag: Tag,
        asset: &str,
        quantity: Decimal,
        value_gbp: Decimal,
        fee_gbp: Option<Decimal>,
    ) -> TaxableEvent {
        TaxableEvent {
            id: UNASSIGNED_ID,
            source_transaction_id: self.id.to_string(),
            account: self.account.to_string(),
            event_type,
            tag,
            datetime: self.datetime,
            asset: normalize_currency(asset),
            asset_class: asset_class_for(self.registry, asset),
            quantity,
            value_gbp,
            fee_gbp,
            description: self.description.clone(),
        }
    }

    fn tx_price(&self) -> Option<&Price> {
        self.valuation.and_then(Valuation::price)
    }

    /// Convert the transaction fee to GBP, if there is one. `priced_asset`
    /// names the asset the transaction price refers to, when it applies.
    fn fee_gbp(&self, priced_asset: Option<&str>) -> Result<Option<Decimal>, TransactionError> {
        match self.fee {
            Some(f) => Ok(Some(fee_to_gbp_with_context(
                f,
                priced_asset,
                self.tx_price(),
            )?)),
            None => Ok(None),
        }
    }

    fn invalid_tag(&self, tx_type: &str) -> TransactionError {
        TransactionError::InvalidTagForType {
            id: self.id.to_string(),
            tag: tag_name(self.tag).to_string(),
            tx_type: tx_type.to_string(),
        }
    }

    fn trade_events(
        &self,
        sold: &Amount,
        bought: &Amount,
    ) -> Result<Vec<TaxableEvent>, TransactionError> {
        if !matches!(self.tag, Tag::Unclassified | Tag::Trade) {
            return Err(self.invalid_tag("trade"));
        }

        let value_gbp = if is_gbp(&sold.asset) || is_gbp(&bought.asset) {
            // One leg is GBP, so it is its own valuation.
            if self.valuation.is_some() {
                return Err(TransactionError::GbpTradeValuationNotAllowed {
                    id: self.id.to_string(),
                });
            }
            if is_gbp(&sold.asset) {
                sold.quantity
            } else {
                bought.quantity
            }
        } else {
            match self.valuation {
                Some(Valuation::Price(price)) => {
                    validate_price_base(self.id, price, &bought.asset)?;
                    price.to_gbp(bought.quantity)?
                }
                Some(Valuation::ValueGbp(value_gbp)) => *value_gbp,
                None => {
                    return Err(TransactionError::MissingTradeValuation {
                        id: self.id.to_string(),
                    })
                }
            }
        };

        let has_disposal = !is_gbp(&sold.asset);
        let has_acquisition = !is_gbp(&bought.asset);

        // Fee uses trade price if fee asset matches bought asset.
        let fee_gbp = self.fee_gbp(Some(&bought.asset))?;

        let mut events = Vec::new();
        if has_disposal {
            events.push(self.event(
                EventType::Disposal,
                Tag::Trade,
                &sold.asset,
                sold.quantity,
                value_gbp,
                fee_gbp,
            ));
        }
        if has_acquisition {
            // The fee is charged once; the disposal leg takes it if present.
            let acquisition_fee = if has_disposal { None } else { fee_gbp };
            events.push(self.event(
                EventType::Acquisition,
                Tag::Trade,
                &bought.asset,
                bought.quantity,
                value_gbp,
                acquisition_fee,
            ));
        }
        Ok(events)
    }

    fn deposit_events(
        &self,
        amount: &Amount,
        linked_withdrawal: Option<&str>,
    ) -> Result<Vec<TaxableEvent>, TransactionError> {
        if self.tag != Tag::Unclassified {
            if linked_withdrawal.is_some() {
                return Err(TransactionError::TaggedDepositLinked {
                    id: self.id.to_string(),
                });
            }
            return self.tagged_deposit(amount);
        }

        if linked_withdrawal.is_some() || is_gbp(&amount.asset) {
            return Ok(vec![]);
        }
        self.unlinked_event(amount, EventType::Acquisition, "deposit", "acquisition")
    }

    fn tagged_deposit(&self, amount: &Amount) -> Result<Vec<TaxableEvent>, TransactionError> {
        let value_gbp = match self.tag {
            // A GBP amount is its own valuation.
            Tag::Salary | Tag::OtherIncome | Tag::Dividend | Tag::Interest | Tag::Cashback
                if is_gbp(&amount.asset) =>
            {
                if self.valuation.is_some() {
                    return Err(TransactionError::GbpIncomeValuationNotAllowed {
                        id: self.id.to_string(),
                        tag: tag_name(self.tag).to_string(),
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
            | Tag::Gift
            | Tag::Cashback => valuation_to_gbp_required(
                self.id,
                self.tag,
                "deposit",
                self.valuation,
                &amount.asset,
                amount.quantity,
            )?,
            Tag::Airdrop => {
                if self.valuation.is_some() {
                    return Err(TransactionError::AirdropValuationNotAllowed {
                        id: self.id.to_string(),
                    });
                }
                Decimal::ZERO
            }
            Tag::Trade | Tag::Unclassified | Tag::NoGainNoLoss => {
                return Err(self.invalid_tag("deposit"))
            }
        };

        // Airdrops, GBP income and direct GBP valuations carry no price context.
        let priced_asset = if self.tag == Tag::Airdrop || self.tx_price().is_none() {
            None
        } else {
            Some(amount.asset.as_str())
        };
        let fee_gbp = self.fee_gbp(priced_asset)?;

        Ok(vec![self.event(
            EventType::Acquisition,
            self.tag,
            &amount.asset,
            amount.quantity,
            value_gbp,
            fee_gbp,
        )])
    }

    fn withdrawal_events(
        &self,
        amount: &Amount,
        linked_deposit: Option<&str>,
    ) -> Result<Vec<TaxableEvent>, TransactionError> {
        if self.tag != Tag::Unclassified {
            if linked_deposit.is_some() {
                return Err(TransactionError::TaggedWithdrawalLinked {
                    id: self.id.to_string(),
                });
            }
            return self.tagged_withdrawal(amount);
        }

        if linked_deposit.is_some() || is_gbp(&amount.asset) {
            return Ok(vec![]);
        }
        self.unlinked_event(amount, EventType::Disposal, "withdrawal", "disposal")
    }

    fn tagged_withdrawal(&self, amount: &Amount) -> Result<Vec<TaxableEvent>, TransactionError> {
        if !matches!(self.tag, Tag::Gift | Tag::NoGainNoLoss) {
            return Err(self.invalid_tag("withdrawal"));
        }

        // A no gain/no loss transfer may omit its valuation; a gift may not.
        let value_gbp = if self.tag == Tag::NoGainNoLoss {
            valuation_to_gbp_optional(self.id, self.valuation, &amount.asset, amount.quantity)?
        } else {
            valuation_to_gbp_required(
                self.id,
                self.tag,
                "withdrawal",
                self.valuation,
                &amount.asset,
                amount.quantity,
            )?
        };
        let fee_gbp = self.fee_gbp(Some(&amount.asset))?;

        Ok(vec![self.event(
            EventType::Disposal,
            self.tag,
            &amount.asset,
            amount.quantity,
            value_gbp,
            fee_gbp,
        )])
    }

    /// An untagged, unlinked deposit or withdrawal: either dropped, or kept
    /// as an unclassified acquisition/disposal for the user to review.
    fn unlinked_event(
        &self,
        amount: &Amount,
        event_type: EventType,
        tx_type: &str,
        treated_as: &str,
    ) -> Result<Vec<TaxableEvent>, TransactionError> {
        if self.exclude_unlinked {
            log::warn!(
                "Skipping unlinked {}: id={} asset={}",
                tx_type,
                self.id,
                amount.asset
            );
            return Ok(vec![]);
        }

        let priced_asset = self.tx_price().map(|_| amount.asset.as_str());
        let fee_gbp = self.fee_gbp(priced_asset)?;
        let value_gbp =
            valuation_to_gbp_optional(self.id, self.valuation, &amount.asset, amount.quantity)?;

        log::warn!(
            "Unlinked {} treated as {}: id={} asset={}",
            tx_type,
            treated_as,
            self.id,
            amount.asset
        );
        Ok(vec![self.event(
            event_type,
            Tag::Unclassified,
            &amount.asset,
            amount.quantity,
            value_gbp,
            fee_gbp,
        )])
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
        Tag::Cashback => "Cashback",
        Tag::NoGainNoLoss => "NoGainNoLoss",
    }
}

/// Resolve a valuation to GBP, or `None` when the transaction carries none.
/// The two wrappers below decide what an absent valuation means.
fn valuation_to_gbp(
    id: &str,
    valuation: Option<&Valuation>,
    expected_asset: &str,
    quantity: Decimal,
) -> Result<Option<Decimal>, TransactionError> {
    match valuation {
        Some(Valuation::Price(price)) => {
            validate_price_base(id, price, expected_asset)?;
            price.to_gbp(quantity).map(Some)
        }
        Some(Valuation::ValueGbp(value_gbp)) => Ok(Some(*value_gbp)),
        None => Ok(None),
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
    valuation_to_gbp(id, valuation, expected_asset, quantity)?.ok_or_else(|| {
        TransactionError::MissingTaggedValuation {
            id: id.to_string(),
            tag: tag_name(tag).to_string(),
            tx_type: tx_type.to_string(),
        }
    })
}

fn valuation_to_gbp_optional(
    id: &str,
    valuation: Option<&Valuation>,
    expected_asset: &str,
    quantity: Decimal,
) -> Result<Decimal, TransactionError> {
    Ok(valuation_to_gbp(id, valuation, expected_asset, quantity)?.unwrap_or(Decimal::ZERO))
}
