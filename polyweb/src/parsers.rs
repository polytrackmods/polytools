use std::collections::HashMap;

use chrono::DateTime;

use facet::Facet;
use polycore::{
    HISTORY_FILE_LOCATION, PolyLeaderBoard, PolyLeaderBoardEntry, TRACK_FILE, VERSION,
    check_blacklist, get_alt, send_to_networker,
};
use reqwest::Client;
use rocket::form::validate::Contains;
use rocket::tokio::fs;

#[derive(Facet)]
struct LeaderBoardEntry {
    name: String,
    frames: u32,
}

#[derive(Facet)]
struct LeaderBoard {
    entries: Vec<LeaderBoardEntry>,
}

#[derive(Facet, Clone)]
struct FileRecord {
    name: String,
    frames: u32,
    timestamp: i64,
    recording: String,
}

pub(crate) async fn parse_leaderboard(file_path: &str) -> PolyLeaderBoard {
    let contents = fs::read_to_string(file_path)
        .await
        .expect("Failed to read file");
    facet_json::from_str(&contents).expect("Invalid leaderboard file")
}

pub(crate) async fn parse_leaderboard_with_records(
    file_path: &str,
) -> (PolyLeaderBoard, PolyLeaderBoard) {
    let contents = fs::read_to_string(file_path)
        .await
        .expect("Failed to read file");
    let mut lines = contents.lines();
    let leaderboard: PolyLeaderBoard =
        facet_json::from_str(lines.next().expect("Couldn't find leaderboard"))
            .expect("Invalid leaderboard");
    let record_leaderboard: PolyLeaderBoard =
        facet_json::from_str(lines.next().expect("Couldn't find leaderboard"))
            .expect("Invalid leaderboard");
    (leaderboard, record_leaderboard)
}

pub(crate) async fn get_standard_leaderboard(track_id: &str) -> PolyLeaderBoard {
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
        "https://vps.kodub.com/leaderboard?version={}&trackId={}&skip=0&amount=500",
        VERSION,
        track_ids.get(track_id).expect("Couldn't find track id")
    );
    let result = send_to_networker(&client, &url)
        .await
        .expect("Failed to complete request");
    let response: LeaderBoard = facet_json::from_str(&result).expect("Invalid leaderboard");
    let mut leaderboard = PolyLeaderBoard::default();
    let mut rank = 0;
    let mut has_time: Vec<String> = Vec::new();
    for entry in response.entries {
        let name = get_alt(&entry.name)
            .await
            .expect("should be able to get alt");
        if !has_time.contains(&name)
            && check_blacklist(&name)
                .await
                .expect("should be able to get blacklist")
        {
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
    }
    leaderboard
}

pub(crate) async fn parse_history(track_id: &str) -> Vec<(String, String, String, String)> {
    let records = fs::read_to_string(format!("{HISTORY_FILE_LOCATION}HISTORY_{track_id}.txt"))
        .await
        .expect("Couldn't read from record file");
    let history = records
        .lines()
        .map(|s| facet_json::from_str(s).expect("Failed to deserialize history"));
    history
        .map(|record: FileRecord| {
            (
                record.name,
                if record.frames > 60000 {
                    format!(
                        "{}:{:0>2}.{:0>3}",
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
