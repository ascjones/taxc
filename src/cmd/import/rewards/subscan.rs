use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct RewardSlashResponse {
    data: RewardSlashData
}

#[derive(Debug, Deserialize)]
struct RewardSlashData {
    count: u32,
    list: Vec<RewardSlashEvent>,
}

#[derive(Debug, Deserialize)]
pub struct RewardSlashEvent {
    pub block_num: u32,
    pub block_timestamp: u64,
    pub module_id: String,
    pub event_id: String,
    pub amount: u128,
}

const ROWS_PER_PAGE: u8 = 20;

pub fn fetch_reward_slash(network: &str, address: &str) -> color_eyre::Result<Vec<RewardSlashEvent>> {
    let url = format!(
        "https://{}.subscan.io/api/scan/account/reward_slash",
        address
    );

    let fetch_rewards = |row, page| -> color_eyre::Result<RewardSlashResponse>{
        let response = ureq::post(&url)
            .send_json(ureq::json! {
                "address": address,
                "page": page,
                "row": row,
            })?;

        let rewards = response.into_json()?;
        Ok(rewards)
    };

    let response = fetch_rewards(0, 1)?;
    let count = response.data.count;
    log::info!("Fetching {} total rewards", count);
    let last_page_num = count / ROWS_PER_PAGE;
    let mut rewards = Vec::new();

    for page in 0..last_page_num {
        log::info!("Fetching page {}/{}", page, last_page_num);
        let response = fetch_rewards(page, ROWS_PER_PAGE)?;
        rewards.extend_from_slice(&response.data.list);
    }
    Ok(rewards)
}
