pub mod db;
pub mod schema;

use std::{collections::HashMap, env, time::Duration};

use chrono::Utc;
use dotenvy::dotenv;
use futures::future::join_all;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tokio::{fs, task, time::sleep};

pub const BLACKLIST_FILE: &str = "data/blacklist.txt";
pub const ALT_ACCOUNT_FILE: &str = "data/alt_accounts.txt";
pub const RANKINGS_FILE: &str = "data/poly_rankings.txt";
pub const TRACK_FILE: &str = "lists/official_tracks.txt";
pub const HOF_TRACK_FILE: &str = "lists/hof_tracks.txt";
pub const HOF_BLACKLIST_FILE: &str = "data/hof_blacklist.txt";
pub const HOF_ALT_ACCOUNT_FILE: &str = "data/hof_alt_accounts.txt";
const HOF_POINTS_FILE: &str = "lists/hof_points.txt";
pub const HOF_RANKINGS_FILE: &str = "data/hof_rankings.txt";
pub const COMMUNITY_TRACK_FILE: &str = "lists/community_tracks.txt";
pub const COMMUNITY_RANKINGS_FILE: &str = "data/community_rankings.txt";
const COMMUNITY_LB_SIZE: u32 = 20;
pub const CUSTOM_TRACK_FILE: &str = "data/custom_tracks.txt";
pub const VERSION: &str = "0.5.0";
pub const HISTORY_FILE_LOCATION: &str = "histories/";

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

pub async fn global_rankings_update() -> Result<(), Error> {
    dotenv().ok();
    let lb_size = env::var("LEADERBOARD_SIZE")
        .expect("Expected LEADERBOARD_SIZE in env!")
        .parse()
        .expect("LEADERBOARD_SIZE not a valid integer!");
    let client = Client::new();
    let official_tracks_file = TRACK_FILE;
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
                43273,
                VERSION,
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
            if !res.is_empty() {
                leaderboard.append(&mut serde_json::from_str::<LeaderBoard>(&res)?.entries);
            }
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
    fs::write(RANKINGS_FILE, output.clone()).await?;
    Ok(())
}

pub async fn hof_update() -> Result<(), Error> {
    let client = Client::new();
    let track_ids: Vec<String> = fs::read_to_string(HOF_TRACK_FILE)
        .await?
        .lines()
        .map(|s| s.to_string())
        .collect();
    let track_num = track_ids.len() as u32;
    let futures = track_ids.into_iter().map(|track_id| {
        let client = client.clone();
        let url = format!(
            "https://vps.kodub.com:43273/leaderboard?version={}&trackId={}&skip=0&amount=100",
            VERSION,
            track_id.split(" ").next().unwrap()
        );
        task::spawn(async move {
            let res = client.get(url).send().await.unwrap().text().await.unwrap();
            Ok::<String, reqwest::Error>(res)
        })
    });
    let results: Vec<String> = join_all(futures)
        .await
        .into_iter()
        .map(|res| res.unwrap())
        .filter_map(|res| res.ok())
        .collect();
    let mut leaderboards: Vec<Vec<LeaderBoardEntry>> = Vec::new();
    for result in results {
        if !result.is_empty() {
            let leaderboard: Vec<LeaderBoardEntry> =
                serde_json::from_str::<LeaderBoard>(&result)?.entries;
            leaderboards.push(leaderboard);
        }
    }
    let mut player_rankings: HashMap<String, Vec<usize>> = HashMap::new();
    let blacklist: Vec<String> = fs::read_to_string(HOF_BLACKLIST_FILE)
        .await?
        .lines()
        .map(|s| s.to_string())
        .collect();
    let alt_file: Vec<String> = fs::read_to_string(HOF_ALT_ACCOUNT_FILE)
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
    let point_values: Vec<u32> = fs::read_to_string(HOF_POINTS_FILE)
        .await?
        .lines()
        .map(|s| s.to_string().parse().unwrap())
        .collect();
    for leaderboard in leaderboards {
        let mut has_ranking: Vec<String> = Vec::new();
        let mut pos = 0;
        for entry in leaderboard {
            if pos + 1 > point_values.len() {
                break;
            }
            let name = if alt_list.contains_key(&entry.name) {
                alt_list.get(&entry.name).unwrap().clone()
            } else {
                entry.name.clone()
            };
            if !has_ranking.contains(&name) && !blacklist.contains(&name) {
                player_rankings.entry(name.clone()).or_default().push(pos);
                has_ranking.push(name);
                pos += 1;
            }
        }
    }
    let mut sorted_leaderboard: Vec<(String, u32, Vec<u32>)> = player_rankings
        .clone()
        .into_iter()
        .map(|(name, rankings)| {
            let mut tiebreakers = vec![0; point_values.len()];
            let mut points = 0;
            for ranking in rankings {
                if ranking < point_values.len() {
                    points += point_values.get(ranking).unwrap();
                    *tiebreakers.get_mut(ranking).unwrap() += 1;
                }
            }
            (name, points, tiebreakers)
        })
        .collect();
    sorted_leaderboard.sort_by(|a, b| {
        let (_, points_a, tiebreakers_a) = a;
        let (_, points_b, tiebreakers_b) = b;
        points_b
            .cmp(points_a)
            .then_with(|| tiebreakers_b.cmp(tiebreakers_a))
    });
    let mut final_leaderboard: Vec<(u32, u32, String)> = Vec::new();
    let mut points_prev = point_values[0] * track_num + 1;
    let mut rank_prev = 0;
    for (name, points, _) in sorted_leaderboard.clone() {
        if points < points_prev {
            points_prev = points;
            rank_prev += 1;
        }
        final_leaderboard.push((rank_prev, points_prev, name));
    }
    let mut output = String::new();
    for (rank, points, name) in final_leaderboard {
        output.push_str(format!("{:>3} - {} - {}\n", rank, points, name).as_str());
    }
    let mut player_records: HashMap<String, u32> = HashMap::new();
    for (name, rankings) in player_rankings {
        for rank in rankings {
            if rank == 0 {
                *player_records.entry(name.clone()).or_insert(0) += 1;
            }
        }
    }
    let mut player_records: Vec<(String, u32)> = player_records.into_iter().collect();
    player_records.sort_by_key(|(_, amt)| *amt);
    player_records.reverse();
    let mut final_player_records: Vec<(u32, u32, String)> = Vec::new();
    let mut records_prev = track_num + 1;
    let mut rank_prev = 0;
    for (name, records) in player_records.clone() {
        if records < records_prev {
            records_prev = records;
            rank_prev += 1;
        }
        final_player_records.push((rank_prev, records_prev, name));
    }
    for (rank, records, name) in final_player_records {
        output.push_str(format!("<|-|> {:>3} - {} - {}\n", rank, records, name).as_str());
    }
    fs::write(HOF_RANKINGS_FILE, output.clone()).await?;
    Ok(())
}

pub async fn community_update() -> Result<(), Error> {
    let client = Client::new();
    let track_ids: Vec<String> = fs::read_to_string(COMMUNITY_TRACK_FILE)
        .await?
        .lines()
        .map(|s| s.to_string())
        .collect();
    let track_num = track_ids.len() as u32;
    let futures = track_ids.into_iter().map(|track_id| {
        let client = client.clone();
        let mut urls = Vec::new();
        for i in 0..COMMUNITY_LB_SIZE {
            let url = format!(
                "https://vps.kodub.com:{}/leaderboard?version={}&trackId={}&skip={}&amount=500",
                43273,
                VERSION,
                track_id.split(" ").next().unwrap(),
                i * 500,
            );
            urls.push(url);
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
        let mut leaderboard = Vec::new();
        for result_part in result {
            if !result_part.is_empty() {
                let mut leaderboard_part: Vec<LeaderBoardEntry> =
                    serde_json::from_str::<LeaderBoard>(&result_part)?.entries;
                leaderboard.append(&mut leaderboard_part);
            }
        }
        leaderboards.push(leaderboard);
    }
    let mut player_rankings: HashMap<String, Vec<usize>> = HashMap::new();
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
        let mut has_ranking: Vec<String> = Vec::new();
        let mut pos = 0;
        for entry in leaderboard {
            if pos + 1 > COMMUNITY_LB_SIZE as usize * 500 {
                break;
            }
            let name = if alt_list.contains_key(&entry.name) {
                alt_list.get(&entry.name).unwrap().clone()
            } else {
                entry.name.clone()
            };
            if !has_ranking.contains(&name) && !blacklist.contains(&name) {
                player_rankings.entry(name.clone()).or_default().push(pos);
                has_ranking.push(name);
                pos += 1;
            }
        }
    }
    let mut sorted_leaderboard: Vec<(String, u32, Vec<u32>)> = player_rankings
        .clone()
        .into_iter()
        .map(|(name, rankings)| {
            let mut tiebreakers = vec![0; COMMUNITY_LB_SIZE as usize * 500];
            let mut points = 0;
            for ranking in rankings {
                points += (100.0 / (ranking as f64 + 1.0).sqrt()) as u32;
                *tiebreakers.get_mut(ranking).unwrap_or(&mut 0) += 1;
            }
            (name, points, tiebreakers)
        })
        .collect();
    sorted_leaderboard.sort_by(|a, b| {
        let (_, points_a, tiebreakers_a) = a;
        let (_, points_b, tiebreakers_b) = b;
        points_b
            .cmp(points_a)
            .then_with(|| tiebreakers_b.cmp(tiebreakers_a))
    });
    let mut final_leaderboard: Vec<(u32, u32, String)> = Vec::new();
    let mut points_prev = COMMUNITY_LB_SIZE * 500 * track_num + 1;
    let mut rank_prev = 0;
    for (name, points, _) in sorted_leaderboard.clone() {
        if points < points_prev {
            points_prev = points;
            rank_prev += 1;
        }
        final_leaderboard.push((rank_prev, points_prev, name));
    }
    let mut output = String::new();
    for (rank, points, name) in final_leaderboard {
        output.push_str(format!("{:>3} - {} - {}\n", rank, points, name).as_str());
    }
    let mut player_records: HashMap<String, u32> = HashMap::new();
    for (name, rankings) in player_rankings {
        for rank in rankings {
            if rank == 0 {
                *player_records.entry(name.clone()).or_insert(0) += 1;
            }
        }
    }
    let mut player_records: Vec<(String, u32)> = player_records.into_iter().collect();
    player_records.sort_by_key(|(_, amt)| *amt);
    player_records.reverse();
    let mut final_player_records: Vec<(u32, u32, String)> = Vec::new();
    let mut records_prev = track_num + 1;
    let mut rank_prev = 0;
    for (name, records) in player_records.clone() {
        if records < records_prev {
            records_prev = records;
            rank_prev += 1;
        }
        final_player_records.push((rank_prev, records_prev, name));
    }
    for (rank, records, name) in final_player_records {
        output.push_str(format!("<|-|> {:>3} - {} - {}\n", rank, records, name).as_str());
    }
    fs::write(COMMUNITY_RANKINGS_FILE, output.clone()).await?;
    Ok(())
}

pub fn get_datetime() -> String {
    let now = Utc::now();
    now.format("%Y/%m/%d %H:%M:%S").to_string()
}
