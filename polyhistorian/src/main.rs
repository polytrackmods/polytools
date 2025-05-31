use std::{cmp::Ordering, collections::HashMap, time::Duration};

use chrono::{DateTime, Utc};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tokio::{
    fs::{self, OpenOptions},
    io::AsyncWriteExt,
    time::sleep,
};

use filenamify::filenamify;

use polymanager::{COMMUNITY_TRACK_FILE, HISTORY_FILE_LOCATION, TRACK_FILE, get_datetime};

type Error = Box<dyn std::error::Error + Send + Sync>;

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
    timestamp: i64,
    recording: String,
}
impl FileRecord {
    const fn new() -> Self {
        Self {
            id: 0,
            user_id: String::new(),
            name: String::new(),
            car_colors: String::new(),
            frames: 0,
            timestamp: 0,
            recording: String::new(),
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
        let timestamp = self.timestamp;
        let date = DateTime::from_timestamp(timestamp, 0)
            .expect("Should always be a valid timestamp")
            .format("%Y/%m/%d %H:%M:%S")
            .to_string();
        println!(
            "{}New {} Record\n{} | {:>2.3} ({:0>1.3}) | {}",
            " ".repeat(22),
            track,
            date,
            f64::from(self.frames) / 1000.0,
            (f64::from(prior_frames) - f64::from(self.frames)) / -1000.0,
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
        if other.frames == 0 {
            if self.frames == 0 {
                Some(Ordering::Equal)
            } else {
                Some(Ordering::Less)
            }
        } else if self.frames == 0 {
            Some(Ordering::Greater)
        } else {
            Some(self.frames.cmp(&other.frames))
        }
    }
}

impl Record {
    async fn to_file(&self) -> FileRecord {
        let now = Utc::now();
        let timestamp = now.timestamp();
        let recording = self.get_recording().await;
        FileRecord {
            id: self.id,
            user_id: self.user_id.clone(),
            name: self.name.clone(),
            car_colors: self.car_colors.clone(),
            frames: self.frames,
            timestamp,
            recording,
        }
    }
    async fn get_recording(&self) -> String {
        let client = Client::new();
        if let Ok(send_result) = client
            .get(format!(
                "https://vps.kodub.com:43273/recordings?version=0.5.0&recordingIds={}",
                self.id
            ))
            .send()
            .await
        {
            send_result.text().await.map_or_else(
                |_| String::new(),
                |response| {
                    serde_json::from_str::<Vec<Recording>>(&response).map_or_else(
                        |_| String::new(),
                        |recordings| {
                            if recordings.is_empty() {
                                String::new()
                            } else {
                                recordings
                                    .first()
                                    .expect("Checked for empty")
                                    .recording
                                    .trim_matches('"')
                                    .to_string()
                            }
                        },
                    )
                },
            )
        } else {
            String::new()
        }
    }
}

#[derive(Deserialize)]
struct Recording {
    recording: String,
}

#[tokio::main]
async fn main() -> Result<(), Error> {
    let client = Client::new();
    let track_ids = fs::read_to_string(TRACK_FILE)
        .await
        .expect("Couldn't read from track file");
    let mut tracks: Vec<(&str, &str)> = track_ids
        .lines()
        .map(|l| l.split_once(' ').expect("Invalid track ids file"))
        .collect();
    let track_ids = fs::read_to_string(COMMUNITY_TRACK_FILE)
        .await
        .expect("Couldn't read from track file");
    tracks.append(
        &mut track_ids
            .lines()
            .map(|l| l.split_once(' ').expect("Invalid track ids file"))
            .collect(),
    );
    let mut prior_records: HashMap<&str, FileRecord> = HashMap::new();
    if !fs::try_exists(HISTORY_FILE_LOCATION).await.unwrap_or(false) {
        fs::create_dir(HISTORY_FILE_LOCATION)
            .await
            .expect("Couldn't create directory");
    }
    for (_, name) in tracks.clone() {
        let path = format!("{}HISTORY_{}.txt", HISTORY_FILE_LOCATION, filenamify(name));
        if fs::try_exists(path.clone()).await.unwrap_or(false) {
            let text = fs::read_to_string(path).await.expect("Couldn't read file");
            let line = text.lines().last().expect("Should have a last record");
            let record: FileRecord = serde_json::from_str(line).expect("Error deserializing line");
            prior_records.insert(name, record);
        } else {
            fs::write(path, "").await.expect("Couldn't create file");
            prior_records.insert(name, FileRecord::new());
        }
    }
    loop {
        println!("Checking records! ({})", get_datetime());
        for (id, name) in tracks.clone() {
            let url = format!(
                "https://vps.kodub.com:43273/leaderboard?version=0.5.0&skip=0&onlyVerified=true&amount=5&trackId={id}"
            );
            let mut response_text = String::new();
            while response_text.is_empty() {
                response_text = if let Ok(response) = client.get(&url).send().await {
                    if let Ok(text) = response.text().await {
                        text
                    } else {
                        sleep(Duration::from_millis(500)).await;
                        String::new()
                    }
                } else {
                    sleep(Duration::from_secs(60 * 5)).await;
                    continue;
                };
            }
            if let Ok(new_lb) = serde_json::from_str::<LeaderBoard>(&response_text) {
                if let Some(new_record) = new_lb.entries.first() {
                    if *new_record
                        < prior_records
                            .get(name)
                            .expect("Inserted earlier")
                            .clone()
                            .to_record()
                    {
                        let path =
                            format!("{}HISTORY_{}.txt", HISTORY_FILE_LOCATION, filenamify(name));
                        let mut file = OpenOptions::new()
                            .write(true)
                            .append(true)
                            .open(path)
                            .await
                            .expect("Failed to open file");
                        let new_record = new_record.clone().to_file().await;
                        file.write_all(
                            format!(
                                "{}\n",
                                serde_json::to_string(&new_record).expect("Failed serializing")
                            )
                            .as_bytes(),
                        )
                        .await
                        .expect("Failed writing to file");
                        new_record.print(
                            name,
                            prior_records.get(name).expect("Inserted earlier").frames,
                        );
                        prior_records.entry(name).and_modify(|r| *r = new_record);
                    }
                }
            }
        }
        sleep(Duration::from_secs(60 * 5)).await;
    }
}
