use std::collections::HashMap;

use chrono::DateTime;

use polymanager::{
    ALT_ACCOUNT_FILE, BLACKLIST_FILE, CUSTOM_TRACK_FILE, HISTORY_FILE_LOCATION, TRACK_FILE, VERSION,
};
use reqwest::Client;
use rocket::form::validate::Contains;
use rocket::serde::{Deserialize, Serialize};
use rocket::tokio::fs;

#[derive(Deserialize, Serialize)]
#[serde(crate = "rocket::serde")]
struct LeaderBoardEntry {
    name: String,
    frames: f64,
}

#[derive(Deserialize, Serialize)]
#[serde(crate = "rocket::serde")]
struct LeaderBoard {
    entries: Vec<LeaderBoardEntry>,
}

#[derive(Serialize, Deserialize)]
#[serde(crate = "rocket::serde")]
pub struct Entry {
    rank: u32,
    stat: String,
    name: String,
}

#[derive(Deserialize, Clone)]
#[serde(crate = "rocket::serde")]
#[serde(rename_all = "camelCase")]
struct FileRecord {
    name: String,
    frames: u32,
    timestamp: String,
    recording: String,
}

pub async fn parse_leaderboard(file_path: &str) -> Vec<Entry> {
    let contents = fs::read_to_string(file_path)
        .await
        .expect("Failed to read file");
    contents
        .lines()
        .filter_map(|line| {
            let parts: Vec<&str> = line
                .trim_start()
                .splitn(3, " - ")
                .filter(|s| !s.is_empty())
                .collect();
            if parts.len() == 3 {
                Some(Entry {
                    rank: parts[0].parse().ok()?,
                    stat: parts[1].to_string(),
                    name: parts[2].to_string(),
                })
            } else {
                None
            }
        })
        .collect()
}

pub async fn parse_hof_leaderboard(file_path: &str) -> (Vec<Entry>, Vec<Entry>) {
    let contents = fs::read_to_string(file_path)
        .await
        .expect("Failed to read file");
    let leaderboard: Vec<Entry> = contents
        .lines()
        .filter_map(|line| {
            if line.starts_with("<|-|>") {
                return None;
            }
            let parts: Vec<&str> = line
                .trim_start()
                .splitn(3, " - ")
                .filter(|s| !s.is_empty())
                .collect();
            if parts.len() == 3 {
                Some(Entry {
                    rank: parts[0].parse().ok()?,
                    stat: parts[1].to_string(),
                    name: parts[2].to_string(),
                })
            } else {
                None
            }
        })
        .collect();
    let record_leaderboard: Vec<Entry> = contents
        .lines()
        .filter_map(|line| {
            if !line.starts_with("<|-|>") {
                return None;
            }
            let parts: Vec<&str> = line
                .trim_start_matches("<|-|>")
                .trim_start()
                .splitn(3, " - ")
                .filter(|s| !s.is_empty())
                .collect();
            if parts.len() == 3 {
                Some(Entry {
                    rank: parts[0].parse().ok()?,
                    stat: parts[1].to_string(),
                    name: parts[2].to_string(),
                })
            } else {
                None
            }
        })
        .collect();
    (leaderboard, record_leaderboard)
}

pub async fn parse_community_leaderboard(file_path: &str) -> (Vec<Entry>, Vec<Entry>) {
    let contents = fs::read_to_string(file_path)
        .await
        .expect("Failed to read file");
    let leaderboard: Vec<Entry> = contents
        .lines()
        .filter_map(|line| {
            if line.starts_with("<|-|>") {
                return None;
            }
            let parts: Vec<&str> = line
                .trim_start()
                .splitn(3, " - ")
                .filter(|s| !s.is_empty())
                .collect();
            if parts.len() == 3 {
                Some(Entry {
                    rank: parts[0].parse().ok()?,
                    stat: parts[1].to_string(),
                    name: parts[2].to_string(),
                })
            } else {
                None
            }
        })
        .collect();
    let record_leaderboard: Vec<Entry> = contents
        .lines()
        .filter_map(|line| {
            if !line.starts_with("<|-|>") {
                return None;
            }
            let parts: Vec<&str> = line
                .trim_start_matches("<|-|>")
                .trim_start()
                .splitn(3, " - ")
                .filter(|s| !s.is_empty())
                .collect();
            if parts.len() == 3 {
                Some(Entry {
                    rank: parts[0].parse().ok()?,
                    stat: parts[1].to_string(),
                    name: parts[2].to_string(),
                })
            } else {
                None
            }
        })
        .collect();
    (leaderboard, record_leaderboard)
}

pub async fn get_custom_leaderboard(track_id: &str) -> (String, Vec<Entry>) {
    let client = Client::new();
    let track_ids: HashMap<String, String> = fs::read_to_string(CUSTOM_TRACK_FILE)
        .await
        .unwrap()
        .lines()
        .map(|s| s.to_string())
        .map(|s| {
            let mut parts = s.splitn(2, " ");
            let output_reversed = (
                parts.next().unwrap().to_string(),
                parts.next().unwrap().to_string(),
            );
            (output_reversed.1, output_reversed.0)
        })
        .collect();
    let mut real_track_id = String::new();
    for track in track_ids.clone().into_keys() {
        if track.to_lowercase() == track_id.to_lowercase() {
            real_track_id = track;
            break;
        }
    }
    let url = if !real_track_id.is_empty() {
        format!(
            "https://vps.kodub.com:43273/leaderboard?version={}&trackId={}&skip=0&amount=500",
            VERSION,
            track_ids.get(&real_track_id).unwrap()
        )
    } else {
        format!(
            "https://vps.kodub.com:43273/leaderboard?version={}&trackId={}&skip=0&amount=500",
            VERSION, track_id
        )
    };
    let result = client.get(&url).send().await.unwrap().text().await.unwrap();
    let response: LeaderBoard = serde_json::from_str(&result).unwrap();
    let mut leaderboard = Vec::new();
    let blacklist: Vec<String> = fs::read_to_string(BLACKLIST_FILE)
        .await
        .unwrap()
        .lines()
        .map(|s| s.to_string())
        .collect();
    let alt_file: Vec<String> = fs::read_to_string(ALT_ACCOUNT_FILE)
        .await
        .unwrap()
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
    let mut rank = 0;
    let mut has_time: Vec<String> = Vec::new();
    for entry in response.entries {
        let name = if alt_list.contains_key(&entry.name) {
            alt_list.get(&entry.name).unwrap().clone()
        } else {
            entry.name.clone()
        };
        if has_time.contains(&name) || blacklist.contains(&name) {
            continue;
        }
        rank += 1;
        leaderboard.push(Entry {
            rank,
            stat: {
                if entry.frames < 60000.0 {
                    (entry.frames / 1000.0).to_string()
                } else {
                    format!(
                        "{}:{:0>2}.{:0>3}",
                        entry.frames as u32 / 60000,
                        entry.frames as u32 % 60000 / 1000,
                        entry.frames as u32 % 1000
                    )
                }
            },
            name: name.clone(),
        });
        has_time.push(name);
    }
    let name = if track_ids.contains_key(&real_track_id) {
        format!("{} ", real_track_id)
    } else {
        String::new()
    };
    (name, leaderboard)
}

pub async fn get_standard_leaderboard(track_id: &str) -> Vec<Entry> {
    let client = Client::new();
    let tracks = fs::read_to_string(TRACK_FILE).await.unwrap();
    let track_ids: HashMap<&str, String> = tracks
        .lines()
        .map(|s| {
            let mut parts = s.splitn(2, " ");
            (
                parts.clone().nth(1).unwrap(),
                parts.next().unwrap().to_string(),
            )
        })
        .collect();
    let url = format!(
        "https://vps.kodub.com:43273/leaderboard?version={}&trackId={}&skip=0&amount=500",
        VERSION,
        track_ids.get(track_id).unwrap()
    );
    let result = client.get(&url).send().await.unwrap().text().await.unwrap();
    let response: LeaderBoard = serde_json::from_str(&result).unwrap();
    let mut leaderboard = Vec::new();
    let blacklist: Vec<String> = fs::read_to_string(BLACKLIST_FILE)
        .await
        .unwrap()
        .lines()
        .map(|s| s.to_string())
        .collect();
    let alt_file: Vec<String> = fs::read_to_string(ALT_ACCOUNT_FILE)
        .await
        .unwrap()
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
    let mut rank = 0;
    let mut has_time: Vec<String> = Vec::new();
    for entry in response.entries {
        let name = if alt_list.contains_key(&entry.name) {
            alt_list.get(&entry.name).unwrap().clone()
        } else {
            entry.name.clone()
        };
        if has_time.contains(&name) || blacklist.contains(&name) {
            continue;
        }
        rank += 1;
        leaderboard.push(Entry {
            rank,
            stat: {
                if entry.frames < 60000.0 {
                    (entry.frames / 1000.0).to_string()
                } else {
                    format!(
                        "{}:{}.{}",
                        entry.frames as u32 / 60000,
                        entry.frames as u32 % 60000 / 1000,
                        entry.frames as u32 % 1000
                    )
                }
            },
            name: name.clone(),
        });
        has_time.push(name);
    }
    leaderboard
}

pub async fn parse_history(track_id: &str) -> Vec<(String, String, String, String)> {
    let records = fs::read_to_string(format!("{}HISTORY_{}.txt", HISTORY_FILE_LOCATION, track_id))
        .await
        .expect("Couldn't read from record file");
    let history: Vec<FileRecord> = records
        .lines()
        .map(|s| serde_json::from_str(s).unwrap())
        .collect();
    history
        .into_iter()
        .map(|record| {
            (
                record.name,
                if record.frames > 60000 {
                    format!(
                        "{}:{}.{:0>3}",
                        record.frames / 60000,
                        record.frames % 60000 / 1000,
                        record.frames % 1000
                    )
                } else {
                    (record.frames as f64 / 1000.0).to_string()
                },
                DateTime::from_timestamp(record.timestamp.parse::<i64>().unwrap(), 0)
                    .unwrap()
                    .format("%Y/%m/%d %H:%M:%S")
                    .to_string(),
                record.recording,
            )
        })
        .collect()
}
