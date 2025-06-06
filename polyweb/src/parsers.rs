use std::collections::HashMap;

use chrono::DateTime;

use polymanager::{
    PolyLeaderBoard, PolyLeaderBoardEntry, ALT_ACCOUNT_FILE, BLACKLIST_FILE, CUSTOM_TRACK_FILE,
    HISTORY_FILE_LOCATION, TRACK_FILE, VERSION,
};
use reqwest::Client;
use rocket::form::validate::Contains;
use rocket::serde::{Deserialize, Serialize};
use rocket::tokio::fs;

#[derive(Deserialize, Serialize)]
#[serde(crate = "rocket::serde")]
struct LeaderBoardEntry {
    name: String,
    frames: u32,
}

#[derive(Deserialize, Serialize)]
#[serde(crate = "rocket::serde")]
struct LeaderBoard {
    entries: Vec<LeaderBoardEntry>,
}

#[derive(Deserialize, Clone)]
#[serde(crate = "rocket::serde")]
#[serde(rename_all = "camelCase")]
struct FileRecord {
    name: String,
    frames: u32,
    timestamp: i64,
    recording: String,
}

#[allow(clippy::missing_panics_doc)]
pub async fn parse_leaderboard(file_path: &str) -> PolyLeaderBoard {
    let contents = fs::read_to_string(file_path)
        .await
        .expect("Failed to read file");
    serde_json::from_str(&contents).expect("Invalid leaderboard file")
}

#[allow(clippy::missing_panics_doc)]
pub async fn parse_leaderboard_with_records(file_path: &str) -> (PolyLeaderBoard, PolyLeaderBoard) {
    let contents = fs::read_to_string(file_path)
        .await
        .expect("Failed to read file");
    let mut lines = contents.lines();
    let leaderboard: PolyLeaderBoard =
        serde_json::from_str(lines.next().expect("Couldn't find leaderboard"))
            .expect("Invalid leaderboard");
    let record_leaderboard: PolyLeaderBoard =
        serde_json::from_str(lines.next().expect("Couldn't find leaderboard"))
            .expect("Invalid leaderboard");
    (leaderboard, record_leaderboard)
}

#[allow(clippy::missing_panics_doc)]
#[allow(clippy::too_many_lines)]
pub async fn get_custom_leaderboard(track_id: &str) -> (String, PolyLeaderBoard) {
    let client = Client::new();
    let track_ids: HashMap<String, String> = fs::read_to_string(CUSTOM_TRACK_FILE)
        .await
        .expect("Failed to read file")
        .lines()
        .map(std::string::ToString::to_string)
        .map(|s| {
            let parts = s.split_once(' ').expect("Invalid track ids file");
            (parts.1.to_string(), parts.0.to_string())
        })
        .collect();
    let mut real_track_id = String::new();
    for track in track_ids.clone().into_keys() {
        if track.to_lowercase() == track_id.to_lowercase() {
            real_track_id = track;
            break;
        }
    }
    let url = if real_track_id.is_empty() {
        format!(
            "https://vps.kodub.com:43273/leaderboard?version={VERSION}&trackId={track_id}&skip=0&amount=500",
        )
    } else {
        format!(
            "https://vps.kodub.com:43273/leaderboard?version={VERSION}&trackId={}&skip=0&amount=500",
            track_ids.get(&real_track_id).expect("Couldn't find track id")
        )
    };
    let result = client
        .get(&url)
        .send()
        .await
        .expect("Failed to send request")
        .text()
        .await
        .expect("Failed to get request text");
    let response: LeaderBoard = serde_json::from_str(&result).expect("Invalid leaderboard");
    let mut leaderboard = PolyLeaderBoard::default();
    let blacklist: Vec<String> = fs::read_to_string(BLACKLIST_FILE)
        .await
        .expect("Failed to open file")
        .lines()
        .map(std::string::ToString::to_string)
        .collect();
    let alt_file: Vec<String> = fs::read_to_string(ALT_ACCOUNT_FILE)
        .await
        .expect("Failed to open file")
        .lines()
        .map(std::string::ToString::to_string)
        .collect();
    let mut alt_list: HashMap<String, String> = HashMap::new();
    for line in alt_file {
        const SPLIT_CHAR: &str = "<|>";
        for entry in line.split(SPLIT_CHAR).skip(1) {
            alt_list.insert(
                entry.to_string(),
                line.split(SPLIT_CHAR)
                    .next()
                    .expect("Invalid alt list file")
                    .to_string(),
            );
        }
    }
    let mut rank = 0;
    let mut has_time: Vec<String> = Vec::new();
    for entry in response.entries {
        let name = if alt_list.contains_key(&entry.name) {
            alt_list
                .get(&entry.name)
                .expect("Check for entry earlier")
                .clone()
        } else {
            entry.name.clone()
        };
        if has_time.contains(&name) || blacklist.contains(&name) {
            continue;
        }
        rank += 1;
        leaderboard.push_entry(PolyLeaderBoardEntry::new(
            rank,
            name.clone(),
            if entry.frames < 60000 {
                (f64::from(entry.frames) / 1000.0).to_string()
            } else {
                format!(
                    "{}:{:0>2}.{:0>3}",
                    entry.frames / 60000,
                    entry.frames % 60000 / 1000,
                    entry.frames % 1000
                )
            },
        ));
        has_time.push(name);
    }
    let name = if track_ids.contains_key(&real_track_id) {
        format!("{real_track_id} ")
    } else {
        String::new()
    };
    (name, leaderboard)
}

#[allow(clippy::missing_panics_doc)]
pub async fn get_standard_leaderboard(track_id: &str) -> PolyLeaderBoard {
    let client = Client::new();
    let tracks = fs::read_to_string(TRACK_FILE)
        .await
        .expect("Failed to read file");
    let track_ids: HashMap<&str, String> = tracks
        .lines()
        .map(|s| {
            let parts = s.split_once(' ').expect("Invalid track ids file");
            (parts.1, parts.0.to_string())
        })
        .collect();
    let url = format!(
        "https://vps.kodub.com:43273/leaderboard?version={}&trackId={}&skip=0&amount=500",
        VERSION,
        track_ids.get(track_id).expect("Couldn't find track id")
    );
    let result = client
        .get(&url)
        .send()
        .await
        .expect("Failed to send request")
        .text()
        .await
        .expect("Failed to get request text");
    let response: LeaderBoard = serde_json::from_str(&result).expect("Invalid leaderboard");
    let mut leaderboard = PolyLeaderBoard::default();
    let blacklist: Vec<String> = fs::read_to_string(BLACKLIST_FILE)
        .await
        .expect("Failed to read file")
        .lines()
        .map(std::string::ToString::to_string)
        .collect();
    let alt_file: Vec<String> = fs::read_to_string(ALT_ACCOUNT_FILE)
        .await
        .expect("Failed to read file")
        .lines()
        .map(std::string::ToString::to_string)
        .collect();
    let mut alt_list: HashMap<String, String> = HashMap::new();
    for line in alt_file {
        const SPLIT_CHAR: &str = "<|>";
        for entry in line.split(SPLIT_CHAR).skip(1) {
            alt_list.insert(
                entry.to_string(),
                line.split(SPLIT_CHAR)
                    .next()
                    .expect("Invalid alt list file")
                    .to_string(),
            );
        }
    }
    let mut rank = 0;
    let mut has_time: Vec<String> = Vec::new();
    for entry in response.entries {
        let name = alt_list.get(&entry.name).unwrap_or(&entry.name).clone();
        if has_time.contains(&name) || blacklist.contains(&name) {
            continue;
        }
        rank += 1;
        leaderboard.push_entry(PolyLeaderBoardEntry::new(
            rank,
            name.clone(),
            if entry.frames < 60000 {
                (f64::from(entry.frames) / 1000.0).to_string()
            } else {
                format!(
                    "{}:{}.{}",
                    entry.frames / 60000,
                    entry.frames % 60000 / 1000,
                    entry.frames % 1000
                )
            },
        ));
        has_time.push(name);
    }
    leaderboard
}

#[allow(clippy::missing_panics_doc)]
pub async fn parse_history(track_id: &str) -> Vec<(String, String, String, String)> {
    let records = fs::read_to_string(format!("{HISTORY_FILE_LOCATION}HISTORY_{track_id}.txt"))
        .await
        .expect("Couldn't read from record file");
    let history = records
        .lines()
        .map(|s| serde_json::from_str(s).expect("Failed to deserialize history"));
    history
        .map(|record: FileRecord| {
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
                    format!("{:.3}", f64::from(record.frames) / 1000.0)
                },
                DateTime::from_timestamp(record.timestamp, 0)
                    .expect("Should be a valid timestamp")
                    .format("%Y/%m/%d %H:%M:%S")
                    .to_string(),
                record.recording,
            )
        })
        .collect()
}
