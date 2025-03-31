pub mod db;
pub mod schema;

use std::{collections::HashMap, env, time::Duration};

use dotenvy::dotenv;
use futures::future::join_all;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tokio::{fs, task, time::sleep};

const BLACKLIST_FILE: &str = "data/blacklist.txt";
const ALT_ACCOUNT_FILE: &str = "data/alt_accounts.txt";
const RANKINGS_FILE: &str = "data/poly_rankings.txt";
const TRACK_FILE: &str = "lists/official_tracks.txt";
const BETA_RANKINGS_FILE: &str = "data/0.5_poly_rankings.txt";
const BETA_TRACK_FILE: &str = "lists/0.5_official_tracks.txt";

type Error = Box<dyn std::error::Error + Send + Sync>;
#[derive(Deserialize, Serialize)]
struct LeaderBoardEntry {
    name: String,
    frames: f64,
}

#[derive(Deserialize, Serialize)]
struct LeaderBoard {
    entries: Vec<LeaderBoardEntry>,
}

pub async fn global_rankings_update(
    entry_requirement: Option<usize>,
    beta: bool,
) -> Result<(), Error> {
    dotenv().ok();
    let mut lb_size = entry_requirement.unwrap_or_else(|| {
        env::var("LEADERBOARD_SIZE")
            .expect("Expected LEADERBOARD_SIZE in env!")
            .parse()
            .expect("LEADERBOARD_SIZE not a valid integer!")
    });
    if beta {
        lb_size = 5;
    }
    let client = Client::new();
    let official_tracks_file = if beta { BETA_TRACK_FILE } else { TRACK_FILE };
    let track_ids: Vec<String> = fs::read_to_string(official_tracks_file)
        .await?
        .lines()
        .map(|s| s.split(" ").next().unwrap().to_string())
        .collect();
    let track_num = track_ids.len();
    let futures = track_ids.clone().into_iter().map(|track_id| {
        let client = client.clone();
        let mut urls = Vec::new();
        for i in 0..lb_size {
            urls.push(format!(
                "https://vps.kodub.com:{}/leaderboard?version={}&trackId={}&skip={}&amount=500",
                if beta { 43274 } else { 43273 },
                if beta { "0.5.0-beta5" } else { "0.4.2" },
                track_id,
                i * 500,
            ));
        }
        task::spawn(async move {
            let mut res = Vec::new();
            for url in urls {
                sleep(Duration::from_millis(500)).await;
                res.push(client.get(url).send().await.unwrap().text().await.unwrap());
            }
            Ok::<Vec<String>, reqwest::Error>(res)
        })
    });
    let results: Vec<Vec<String>> = join_all(futures)
        .await
        .into_iter()
        .map(|res| res.unwrap())
        .filter_map(|res| res.ok())
        .collect();
    let mut leaderboards: Vec<Vec<LeaderBoardEntry>> = Vec::new();
    for result in results {
        let mut leaderboard: Vec<LeaderBoardEntry> = Vec::new();
        for res in result {
            leaderboard.append(&mut serde_json::from_str::<LeaderBoard>(&res)?.entries);
        }
        leaderboards.push(leaderboard);
    }
    let mut player_times: HashMap<String, Vec<f64>> = HashMap::new();
    let blacklist: Vec<String> = fs::read_to_string(BLACKLIST_FILE)
        .await?
        .lines()
        .map(|s| s.to_string())
        .collect();
    let alt_file: Vec<String> = fs::read_to_string(ALT_ACCOUNT_FILE)
        .await?
        .lines()
        .map(|s| s.to_string())
        .collect();
    let mut alt_list: HashMap<String, String> = HashMap::new();
    for line in alt_file {
        const SPLIT_CHAR: &str = "<|>";
        for entry in line.split(SPLIT_CHAR).skip(1) {
            alt_list.insert(
                entry.to_string(),
                line.split(SPLIT_CHAR).next().unwrap().to_string(),
            );
        }
    }
    for leaderboard in leaderboards {
        let mut has_time: Vec<String> = Vec::new();
        for entry in leaderboard {
            let name: String = if alt_list.contains_key(&entry.name) {
                alt_list.get(&entry.name).unwrap().clone()
            } else {
                entry.name.clone()
            };
            if !has_time.contains(&name) && !blacklist.contains(&name) {
                player_times
                    .entry(name.clone())
                    .or_default()
                    .push(entry.frames);
                has_time.push(name);
            }
        }
    }
    let mut sorted_leaderboard: Vec<(String, u32)> = player_times
        .into_iter()
        .filter(|(_, times)| times.len() == track_num)
        .map(|(name, times)| (name, times.iter().sum::<f64>() as u32))
        .collect();
    sorted_leaderboard.sort_by_key(|(_, frames)| *frames);
    let leaderboard: Vec<(usize, String, u32)> = sorted_leaderboard
        .into_iter()
        .enumerate()
        .map(|(i, (name, frames))| (i, name, frames))
        .collect();
    let mut output = String::new();
    for entry in leaderboard {
        output.push_str(
            format!(
                "{:>3} - {:>2}:{:0>2}.{:0>3.3} - {}\n",
                entry.0 + 1,
                entry.2 / 60000,
                entry.2 % 60000 / 1000,
                entry.2 % 1000,
                entry.1
            )
            .as_str(),
        );
    }
    if beta {
        fs::write(BETA_RANKINGS_FILE, output.clone()).await?
    } else {
        fs::write(RANKINGS_FILE, output.clone()).await?;
    }
    Ok(())
}
