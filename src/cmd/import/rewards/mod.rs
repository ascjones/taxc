use crate::{cmd::prices::Prices, currencies::GBP, trades, Money};
use argh::FromArgs;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use std::{fs::File, io, path::PathBuf};
use chrono::{NaiveDate, NaiveDateTime};

mod subscan;

#[derive(FromArgs, PartialEq, Debug)]
#[argh(subcommand, name = "rewards")]
/// Calculate polkadot staking rewards
pub struct ImportStakingRewardsCommand {
    /// the DOT address
    #[argh(positional)]
    address: String,
}

#[derive(Debug, serde::Serialize)]
struct Reward {
    block: u32,
    amount: Decimal,
    currency: String,
    price: Decimal,
    value: Decimal,
}

impl ImportStakingRewardsCommand {
    pub fn exec(&self) -> color_eyre::Result<()> {
        let prices = crate::coingecko::fetch_daily_prices("polkadot", GBP)?;
        let reward_slash_events = subscan::fetch_reward_slash("polkadot", &self.address)?;

        let rewards = reward_slash_events
            .iter()
            .map(|evt| {
                let timestamp = NaiveDateTime::from_timestamp(unix_time_secs, 0);
                prices
                    .iter()
                    .find(|p| p.timestamp.date() == timestamp.date())
                    .ok_or(Err(eyre::eyre!("No price found for reward on date {}", timestamp.date())))
                    .map(|price| {
                        let amount = evt.amount / dec!(10000000000);
                        Reward {
                            block: evt.block_num,
                            amount,
                            currency: "GBP".into(),
                            price: price.price,
                            value: amount * price.price,
                        }
                    });
            })
            .collec::<Vec<_>>();

        println!(rewards);
    }
}
