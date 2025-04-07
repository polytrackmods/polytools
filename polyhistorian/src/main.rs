use std::{cmp::Ordering, collections::HashMap, time::Duration};

use chrono::{DateTime, Utc};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tokio::{
    fs::{self, OpenOptions},
    io::AsyncWriteExt,
    time::sleep,
};

use polymanager::{HISTORY_FILE_LOCATION, TRACK_FILE};

#[derive(Serialize, Deserialize)]
struct LeaderBoard {
    total: u32,
    entries: Vec<Record>,
}

#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct FileRecord {
    id: u64,
    user_id: String,
    name: String,
    car_colors: String,
    frames: u32,
    timestamp: String,
}
impl FileRecord {
    fn new() -> Self {
        Self {
            id: 0,
            user_id: String::new(),
            name: String::new(),
            car_colors: String::new(),
            frames: 0,
            timestamp: String::new(),
        }
    }
    fn to_record(&self) -> Record {
        Record {
            id: self.id,
            user_id: self.user_id.clone(),
            name: self.name.clone(),
            car_colors: self.car_colors.clone(),
            frames: self.frames,
        }
    }
    fn print(&self, track: &str, prior_frames: u32) {
        let timestamp = self.timestamp.clone().parse::<i64>().unwrap();
        let date = DateTime::from_timestamp(timestamp, 0)
            .unwrap()
            .format("%Y/%m/%d %H:%M:%S")
            .to_string();
        println!(
            "{}New {} Record\n{} | {:>2.3} ({:0>1.3}) | {}",
            " ".repeat(22),
            track,
            date,
            self.frames as f64 / 1000.0,
            (prior_frames as f64 - self.frames as f64) / -1000.0,
            self.name,
        );
    }
}

#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct Record {
    id: u64,
    user_id: String,
    name: String,
    car_colors: String,
    frames: u32,
}

impl PartialEq for Record {
    fn eq(&self, other: &Self) -> bool {
        self.user_id == other.user_id && self.id == other.id
    }
}

impl PartialOrd for Record {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.frames.cmp(&other.frames))
    }
}

impl Record {
    fn to_file(&self) -> FileRecord {
        let now = Utc::now();
        let timestamp = now.timestamp().to_string();
        FileRecord {
            id: self.id,
            user_id: self.user_id.clone(),
            name: self.name.clone(),
            car_colors: self.car_colors.clone(),
            frames: self.frames,
            timestamp,
        }
    }
}

#[tokio::main]
async fn main() {
    let client = Client::new();
    let track_ids = fs::read_to_string(TRACK_FILE)
        .await
        .expect("Couldn't read from track file");
    let tracks: Vec<(&str, &str)> = track_ids
        .lines()
        .map(|l| {
            let mut parts = l.splitn(2, " ");
            (parts.next().unwrap(), parts.next().unwrap())
        })
        .collect();
    let mut prior_records: HashMap<&str, FileRecord> = HashMap::new();
    if !fs::try_exists(HISTORY_FILE_LOCATION).await.unwrap_or(false) {
        fs::create_dir(HISTORY_FILE_LOCATION)
            .await
            .expect("Couldn't create directory");
    }
    for (_, name) in tracks.clone() {
        let path = format!("{}HISTORY_{}.txt", HISTORY_FILE_LOCATION, name);
        if !fs::try_exists(path.clone()).await.unwrap_or(false) {
            fs::write(path, "").await.expect("Couldn't create file");
            prior_records.insert(name, FileRecord::new());
        } else {
            let text = fs::read_to_string(path).await.expect("Couldn't read file");
            let line = text.lines().last().unwrap();
            let record: FileRecord = serde_json::from_str(line).expect("Error deserializing line");
            prior_records.insert(name, record);
        }
    }
    loop {
        for (id, name) in tracks.clone() {
            let url = format!(
                "https://vps.kodub.com:43273/leaderboard?version=0.5.0&skip=0&onlyVerified=true&amount=5&trackId={}",
                id
            );
            let response = client
                .get(&url)
                .send()
                .await
                .expect("Error sending request")
                .text()
                .await
                .unwrap();
            let new_lb: LeaderBoard =
                serde_json::from_str(&response).expect("Error deserializing request body");
            if let Some(new_record) = new_lb.entries.first() {
                if *new_record < prior_records.get(name).unwrap().clone().to_record() {
                    let path = format!("{}HISTORY_{}.txt", HISTORY_FILE_LOCATION, name);
                    let mut file = OpenOptions::new()
                        .write(true)
                        .append(true)
                        .open(path)
                        .await
                        .unwrap();
                    let new_record = new_record.clone().to_file();
                    file.write_all(
                        format!("{}\n", serde_json::to_string(&new_record).unwrap()).as_bytes(),
                    )
                    .await
                    .expect("Failed writing to file");
                    new_record.print(name, prior_records.get(name).unwrap().frames);
                    prior_records.entry(name).and_modify(|r| *r = new_record);
                }
            }
        }
        sleep(Duration::from_secs(60 * 10)).await;
    }
}
