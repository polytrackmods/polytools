pub mod db;
pub mod schema;

use std::{collections::HashMap, env, time::Duration};

use anyhow::{Error, anyhow};
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
pub const HOF_ALL_TRACK_FILE: &str = "lists/hof_tracks_all.txt";
pub const HOF_BLACKLIST_FILE: &str = "data/hof_blacklist.txt";
pub const HOF_ALT_ACCOUNT_FILE: &str = "data/hof_alt_accounts.txt";
const HOF_POINTS_FILE: &str = "lists/hof_points.txt";
pub const HOF_RANKINGS_FILE: &str = "data/hof_rankings.txt";
pub const HOF_TIME_RANKINGS_FILE: &str = "data/hof_time_rankings.txt";
pub const COMMUNITY_TRACK_FILE: &str = "lists/community_tracks.txt";
pub const COMMUNITY_RANKINGS_FILE: &str = "data/community_rankings.txt";
pub const COMMUNITY_TIME_RANKINGS_FILE: &str = "data/community_time_rankings.txt";
const COMMUNITY_LB_SIZE: u32 = 20;
pub const CUSTOM_TRACK_FILE: &str = "data/custom_tracks.txt";
pub const VERSION: &str = "0.5.0";
pub const HISTORY_FILE_LOCATION: &str = "histories/";
pub const REQUEST_RETRY_COUNT: u32 = 10;

const UPDATE_LOCK_FILE: &str = "data/update.lock";
const MAX_LOCK_TIME: Duration = Duration::from_secs(300);

#[derive(thiserror::Error, Debug)]
enum PolyError {
    #[error("Currently updating something, please wait a bit")]
    BusyUpdating,
}

#[derive(Deserialize, Serialize)]
struct LeaderBoardEntry {
    name: String,
    frames: u32,
}

#[derive(Deserialize, Serialize)]
struct LeaderBoard {
    entries: Vec<LeaderBoardEntry>,
}

#[derive(Deserialize, Serialize, Default)]
pub struct PolyLeaderBoard {
    pub total: usize,
    pub entries: Vec<PolyLeaderBoardEntry>,
}

#[derive(Deserialize, Serialize)]
pub struct PolyLeaderBoardEntry {
    pub rank: usize,
    pub name: String,
    pub stat: String,
}

impl PolyLeaderBoard {
    pub fn push_entry(&mut self, entry: PolyLeaderBoardEntry) {
        self.entries.push(entry);
        self.total += 1;
    }
}
impl PolyLeaderBoardEntry {
    #[must_use]
    pub const fn new(rank: usize, name: String, stat: String) -> Self {
        Self { rank, name, stat }
    }
}

#[allow(clippy::missing_errors_doc)]
#[allow(clippy::missing_panics_doc)]
#[allow(clippy::too_many_lines)]
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
        .map(|s| {
            s.split(' ')
                .next()
                .expect("Error in track file")
                .to_string()
        })
        .collect();
    let track_num = track_ids.len();
    let futures = track_ids.iter().map(|track_id| {
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
                let mut att = 0;
                sleep(Duration::from_millis(200)).await;
                let mut response = client.get(&url).send().await?.text().await?;
                while response.is_empty() && att < REQUEST_RETRY_COUNT {
                    att += 1;
                    sleep(Duration::from_millis(1000)).await;
                    response = client.get(&url).send().await?.text().await?;
                }
                res.push(response);
            }
            Ok::<Vec<String>, reqwest::Error>(res)
        })
    });
    if fs::try_exists(UPDATE_LOCK_FILE).await? {
        if fs::metadata(UPDATE_LOCK_FILE)
            .await?
            .modified()?
            .elapsed()?
            > MAX_LOCK_TIME
        {
            fs::remove_file(UPDATE_LOCK_FILE).await?;
        } else {
            return Err(PolyError::BusyUpdating.into());
        }
    }
    fs::write(UPDATE_LOCK_FILE, "").await?;
    let results: Vec<Vec<String>> = join_all(futures)
        .await
        .into_iter()
        .map(|res| res.expect("JoinError ig"))
        .filter_map(std::result::Result::ok)
        .collect();
    fs::remove_file(UPDATE_LOCK_FILE).await?;
    let mut leaderboards: Vec<Vec<LeaderBoardEntry>> = Vec::new();
    for result in results {
        let mut leaderboard: Vec<LeaderBoardEntry> = Vec::new();
        for res in result {
            leaderboard.append(
                &mut serde_json::from_str::<LeaderBoard>(&res)
                    .map_err(|_| anyhow!("Probably got rate limited, please try again later"))?
                    .entries,
            );
        }
        leaderboards.push(leaderboard);
    }
    let mut player_times: HashMap<String, Vec<u32>> = HashMap::new();
    let blacklist: Vec<String> = fs::read_to_string(BLACKLIST_FILE)
        .await?
        .lines()
        .map(std::string::ToString::to_string)
        .collect();
    let alt_file: Vec<String> = fs::read_to_string(ALT_ACCOUNT_FILE)
        .await?
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
                    .expect("Wrong alt list file format")
                    .to_string(),
            );
        }
    }
    for leaderboard in leaderboards {
        let mut has_time: Vec<String> = Vec::new();
        for entry in leaderboard {
            let name: String = alt_list.get(&entry.name).unwrap_or(&entry.name).clone();
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
        .map(|(name, times)| (name, times.iter().sum()))
        .collect();
    sorted_leaderboard.sort_by_key(|(_, frames)| *frames);
    let leaderboard: PolyLeaderBoard = PolyLeaderBoard {
        total: sorted_leaderboard.len(),
        entries: sorted_leaderboard
            .into_iter()
            .enumerate()
            .map(|(i, (name, frames))| {
                PolyLeaderBoardEntry::new(
                    i + 1,
                    name,
                    format!(
                        "{:>2}:{:0>2}.{:0>3}",
                        frames / 60000,
                        frames % 60000 / 1000,
                        frames % 1000
                    ),
                )
            })
            .collect(),
    };
    let output = serde_json::to_string(&leaderboard)?;
    fs::write(RANKINGS_FILE, output).await?;
    Ok(())
}

#[allow(clippy::missing_panics_doc)]
#[allow(clippy::missing_errors_doc)]
#[allow(clippy::too_many_lines)]
#[allow(clippy::cognitive_complexity)]
pub async fn hof_update() -> Result<(), Error> {
    let client = Client::new();
    let track_ids: Vec<String> = fs::read_to_string(HOF_TRACK_FILE)
        .await?
        .lines()
        .map(std::string::ToString::to_string)
        .collect();
    let track_num = u32::try_from(track_ids.len()).expect("Shouldn't have that many track IDs");
    let futures = track_ids.iter().map(|track_id| {
        let client = client.clone();
        let url = format!(
            "https://vps.kodub.com:43273/leaderboard?version={}&trackId={}&skip=0&amount=100",
            VERSION,
            track_id.split(' ').next().expect("Invalid track id file")
        );
        task::spawn(async move {
            let mut att = 0;
            let mut res = client.get(&url).send().await?.text().await?;
            while res.is_empty() && att < REQUEST_RETRY_COUNT {
                att += 1;
                sleep(Duration::from_millis(1000)).await;
                res = client.get(&url).send().await?.text().await?;
            }
            Ok::<String, reqwest::Error>(res)
        })
    });
    if fs::try_exists(UPDATE_LOCK_FILE).await? {
        if fs::metadata(UPDATE_LOCK_FILE)
            .await?
            .modified()?
            .elapsed()?
            > MAX_LOCK_TIME
        {
            fs::remove_file(UPDATE_LOCK_FILE).await?;
        } else {
            return Err(PolyError::BusyUpdating.into());
        }
    }
    fs::write(UPDATE_LOCK_FILE, "").await?;
    let results: Vec<String> = join_all(futures)
        .await
        .into_iter()
        .map(|res| res.expect("JoinError ig"))
        .filter_map(std::result::Result::ok)
        .collect();
    fs::remove_file(UPDATE_LOCK_FILE).await?;
    let mut leaderboards: Vec<Vec<LeaderBoardEntry>> = Vec::new();
    for result in results {
        if !result.is_empty() {
            let leaderboard: Vec<LeaderBoardEntry> = serde_json::from_str::<LeaderBoard>(&result)
                .map_err(|_| anyhow!("Probably got rate limited, please try again later"))?
                .entries;
            leaderboards.push(leaderboard);
        }
    }
    let mut player_rankings: HashMap<String, Vec<usize>> = HashMap::new();
    let mut time_rankings: HashMap<String, Vec<u32>> = HashMap::new();
    let blacklist: Vec<String> = fs::read_to_string(HOF_BLACKLIST_FILE)
        .await?
        .lines()
        .map(std::string::ToString::to_string)
        .collect();
    let alt_file: Vec<String> = fs::read_to_string(HOF_ALT_ACCOUNT_FILE)
        .await?
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
    let point_values: Vec<u32> = fs::read_to_string(HOF_POINTS_FILE)
        .await?
        .lines()
        .map(|s| s.to_string().parse().expect("Invalid point value file"))
        .collect();
    for leaderboard in leaderboards {
        let mut has_ranking: Vec<String> = Vec::new();
        let mut pos = 0;
        for entry in leaderboard {
            if pos + 1 > point_values.len() {
                break;
            }
            let name = alt_list.get(&entry.name).unwrap_or(&entry.name).clone();
            if !has_ranking.contains(&name) && !blacklist.contains(&name) {
                player_rankings.entry(name.clone()).or_default().push(pos);
                time_rankings
                    .entry(name.clone())
                    .or_default()
                    .push(entry.frames);
                has_ranking.push(name);
                pos += 1;
            }
        }
    }
    let mut sorted_leaderboard: Vec<(String, u32, Vec<u32>)> = player_rankings
        .iter()
        .map(|(name, rankings)| {
            let mut tiebreakers = vec![0; point_values.len()];
            let mut points = 0;
            for ranking in rankings {
                if *ranking < point_values.len() {
                    points += point_values[*ranking];
                    tiebreakers[*ranking] += 1;
                }
            }
            (name.to_string(), points, tiebreakers)
        })
        .collect();
    sorted_leaderboard.sort_by(|a, b| {
        let (_, points_a, tiebreakers_a) = a;
        let (_, points_b, tiebreakers_b) = b;
        points_b
            .cmp(points_a)
            .then_with(|| tiebreakers_b.cmp(tiebreakers_a))
    });
    let mut final_leaderboard = PolyLeaderBoard::default();
    let mut points_prev = point_values[0] * track_num + 1;
    let mut rank_prev = 0;
    for (name, points, _) in sorted_leaderboard.clone() {
        if points < points_prev {
            points_prev = points;
            rank_prev += 1;
        }
        final_leaderboard.push_entry(PolyLeaderBoardEntry::new(
            rank_prev,
            name,
            points_prev.to_string(),
        ));
    }
    let mut output = serde_json::to_string(&final_leaderboard)?;
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
    let mut final_player_records = PolyLeaderBoard::default();
    let mut records_prev = track_num + 1;
    let mut rank_prev = 0;
    for (name, records) in player_records.clone() {
        if records < records_prev {
            records_prev = records;
            rank_prev += 1;
        }
        final_player_records.push_entry(PolyLeaderBoardEntry::new(
            rank_prev,
            name,
            records_prev.to_string(),
        ));
    }
    output.push('\n');
    output.push_str(&serde_json::to_string(&final_player_records).expect("Failed to serialize"));
    fs::write(HOF_RANKINGS_FILE, output.clone()).await?;
    let mut sorted_times: Vec<(String, u32)> = time_rankings
        .into_iter()
        .filter(|(_, times)| times.len() == track_num as usize)
        .map(|(name, times)| (name, times.iter().sum()))
        .collect();
    sorted_times.sort_by_key(|(_, frames)| *frames);
    let time_leaderboard = PolyLeaderBoard {
        total: sorted_times.len(),
        entries: sorted_times
            .into_iter()
            .enumerate()
            .map(|(i, (name, frames))| {
                PolyLeaderBoardEntry::new(
                    i + 1,
                    name,
                    format!(
                        "{:>2}:{:0>2}.{:0>3}",
                        frames / 60000,
                        frames % 60000 / 1000,
                        frames % 1000
                    ),
                )
            })
            .collect(),
    };
    let time_output = serde_json::to_string(&time_leaderboard)?;
    fs::write(HOF_TIME_RANKINGS_FILE, time_output).await?;
    Ok(())
}

#[allow(clippy::missing_panics_doc)]
#[allow(clippy::missing_errors_doc)]
#[allow(clippy::too_many_lines)]
#[allow(clippy::cognitive_complexity)]
#[allow(clippy::cast_precision_loss)]
#[allow(clippy::cast_possible_truncation)]
#[allow(clippy::cast_sign_loss)]
pub async fn community_update() -> Result<(), Error> {
    let client = Client::new();
    let track_ids: Vec<String> = fs::read_to_string(COMMUNITY_TRACK_FILE)
        .await?
        .lines()
        .map(std::string::ToString::to_string)
        .collect();
    let track_num = u32::try_from(track_ids.len()).expect("Shouldn't have that many tracks");
    let futures = track_ids.iter().map(|track_id| {
        let client = client.clone();
        let mut urls = Vec::new();
        for i in 0..COMMUNITY_LB_SIZE {
            let url = format!(
                "https://vps.kodub.com:{}/leaderboard?version={}&trackId={}&skip={}&amount=500",
                43273,
                VERSION,
                track_id.split(' ').next().expect("Invalid track ids file"),
                i * 500,
            );
            urls.push(url);
        }
        task::spawn(async move {
            let mut res = Vec::new();
            for url in urls {
                let mut att = 0;
                sleep(Duration::from_millis(500)).await;
                let mut response = client.get(&url).send().await?.text().await?;
                while response.is_empty() && att < REQUEST_RETRY_COUNT {
                    att += 1;
                    sleep(Duration::from_millis(1000)).await;
                    response = client.get(&url).send().await?.text().await?;
                }
                res.push(response);
            }
            Ok::<Vec<String>, reqwest::Error>(res)
        })
    });
    if fs::try_exists(UPDATE_LOCK_FILE).await? {
        if fs::metadata(UPDATE_LOCK_FILE)
            .await?
            .modified()?
            .elapsed()?
            > MAX_LOCK_TIME
        {
            fs::remove_file(UPDATE_LOCK_FILE).await?;
        } else {
            return Err(PolyError::BusyUpdating.into());
        }
    }
    fs::write(UPDATE_LOCK_FILE, "").await?;
    let results: Vec<Vec<String>> = join_all(futures)
        .await
        .into_iter()
        .map(|res| res.expect("JoinError ig"))
        .filter_map(std::result::Result::ok)
        .collect();
    fs::remove_file(UPDATE_LOCK_FILE).await?;
    let mut leaderboards: Vec<Vec<LeaderBoardEntry>> = Vec::new();
    for result in results {
        let mut leaderboard = Vec::new();
        for result_part in result {
            if !result_part.is_empty() {
                let mut leaderboard_part: Vec<LeaderBoardEntry> =
                    serde_json::from_str::<LeaderBoard>(&result_part)
                        .map_err(|_| anyhow!("Probably got rate limited, please try again later"))?
                        .entries;
                leaderboard.append(&mut leaderboard_part);
            }
        }
        leaderboards.push(leaderboard);
    }
    let mut player_rankings: HashMap<String, Vec<usize>> = HashMap::new();
    let mut time_rankings: HashMap<String, Vec<u32>> = HashMap::new();
    let blacklist: Vec<String> = fs::read_to_string(BLACKLIST_FILE)
        .await?
        .lines()
        .map(std::string::ToString::to_string)
        .collect();
    let alt_file: Vec<String> = fs::read_to_string(ALT_ACCOUNT_FILE)
        .await?
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
    for leaderboard in leaderboards {
        let mut has_ranking: Vec<String> = Vec::new();
        let mut pos = 0;
        for entry in leaderboard {
            if pos + 1 > COMMUNITY_LB_SIZE as usize * 500 {
                break;
            }
            let name = alt_list.get(&entry.name).unwrap_or(&entry.name).clone();
            if !has_ranking.contains(&name) && !blacklist.contains(&name) {
                player_rankings.entry(name.clone()).or_default().push(pos);
                time_rankings
                    .entry(name.clone())
                    .or_default()
                    .push(entry.frames);
                has_ranking.push(name);
                pos += 1;
            }
        }
    }
    let mut sorted_leaderboard: Vec<(String, u32, Vec<u32>)> = player_rankings
        .iter()
        .map(|(name, rankings)| {
            let mut tiebreakers = vec![0; COMMUNITY_LB_SIZE as usize * 500];
            let mut points = 0.0;
            for ranking in rankings {
                points += 100.0 / (*ranking as f64 + 1.0).sqrt();
                *tiebreakers.get_mut(*ranking).unwrap_or(&mut 0) += 1;
            }
            (name.to_string(), points as u32, tiebreakers)
        })
        .collect();
    sorted_leaderboard.sort_by(|a, b| {
        let (_, points_a, tiebreakers_a) = a;
        let (_, points_b, tiebreakers_b) = b;
        points_b
            .cmp(points_a)
            .then_with(|| tiebreakers_b.cmp(tiebreakers_a))
    });
    let mut final_leaderboard = PolyLeaderBoard::default();
    let mut points_prev = COMMUNITY_LB_SIZE * 500 * track_num + 1;
    let mut rank_prev = 0;
    for (name, points, _) in sorted_leaderboard.clone() {
        if points < points_prev {
            points_prev = points;
            rank_prev += 1;
        }
        final_leaderboard.push_entry(PolyLeaderBoardEntry::new(
            rank_prev,
            name,
            points_prev.to_string(),
        ));
    }
    let mut output = serde_json::to_string(&final_leaderboard)?;
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
    let mut final_player_records = PolyLeaderBoard::default();
    let mut records_prev = track_num + 1;
    let mut rank_prev = 0;
    for (name, records) in player_records.clone() {
        if records < records_prev {
            records_prev = records;
            rank_prev += 1;
        }
        final_player_records.push_entry(PolyLeaderBoardEntry::new(
            rank_prev,
            name,
            records_prev.to_string(),
        ));
    }
    output.push('\n');
    output.push_str(&serde_json::to_string(&final_player_records).expect("Failed to serialize"));
    fs::write(COMMUNITY_RANKINGS_FILE, output).await?;
    let mut sorted_times: Vec<(String, u32)> = time_rankings
        .into_iter()
        .filter(|(_, times)| times.len() == track_num as usize)
        .map(|(name, times)| (name, times.iter().sum()))
        .collect();
    sorted_times.sort_by_key(|(_, frames)| *frames);
    let time_leaderboard = PolyLeaderBoard {
        total: sorted_times.len(),
        entries: sorted_times
            .into_iter()
            .enumerate()
            .map(|(i, (name, frames))| {
                PolyLeaderBoardEntry::new(
                    i + 1,
                    name,
                    format!(
                        "{:>2}:{:0>2}.{:0>3}",
                        frames / 60000,
                        frames % 60000 / 1000,
                        frames % 1000
                    ),
                )
            })
            .collect(),
    };
    let time_output = serde_json::to_string(&time_leaderboard)?;
    fs::write(COMMUNITY_TIME_RANKINGS_FILE, time_output).await?;
    Ok(())
}

#[must_use]
pub fn get_datetime() -> String {
    let now = Utc::now();
    now.format("%Y/%m/%d %H:%M:%S").to_string()
}
