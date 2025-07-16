pub mod db;
pub mod schema;

use std::{collections::HashMap, io::Read as _, time::Duration};

use anyhow::{Result, anyhow};
use chrono::{DateTime, Datelike as _, Utc};
use dotenvy::dotenv;
use flate2::read::ZlibDecoder;
use futures::future::join_all;
use regex::Regex;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};
use tokio::{fs, task, time::sleep};

pub const BLACKLIST_FILE: &str = "data/blacklist.txt";
pub const ALT_ACCOUNT_FILE: &str = "data/alt_accounts.txt";
pub const RANKINGS_FILE: &str = "data/poly_rankings.txt";
const LB_SIZE: u32 = 20;
pub const TRACK_FILE: &str = "lists/official_tracks.txt";
pub const HOF_CODE_FILE: &str = "lists/hof_codes.txt";
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
pub const VERSION: &str = "0.5.1";
pub const HISTORY_FILE_LOCATION: &str = "histories/";
pub const REQUEST_RETRY_COUNT: u32 = 10;
pub const ET_CODE_FILE: &str = "data/et_codes.txt";
pub const ET_TRACK_FILE: &str = "data/et_tracks.txt";
pub const ET_RANKINGS_FILE: &str = "data/et_rankings.txt";

pub const UPDATE_LB_COUNT: u64 = 4;
pub const UPDATE_CYCLE_LEN: Duration = Duration::from_secs(UPDATE_LB_COUNT * 15 * 60);

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

#[derive(Serialize, Deserialize, Default)]
struct BlackListFile {
    #[serde(with = "serde_regex")]
    regexes: Vec<Regex>,
}
#[derive(Serialize, Deserialize, Default)]
struct AltListFile {
    entries: Vec<AltListEntry>,
}
#[derive(Serialize, Deserialize, Default)]
struct AltListEntry {
    name: String,
    #[serde(with = "serde_regex")]
    alts: Vec<Regex>,
}

#[allow(clippy::missing_errors_doc)]
pub async fn check_blacklist(list_file: &str, name: &str) -> Result<bool> {
    let content = fs::read_to_string(list_file).await?;
    let blacklist_file: BlackListFile = serde_json::from_str(&content)?;
    for regex in blacklist_file.regexes {
        if regex.is_match(name) {
            return Ok(false);
        }
    }
    Ok(true)
}
#[allow(clippy::missing_errors_doc)]
pub async fn get_alt(list_file: &str, name: &str) -> Result<String> {
    let content = fs::read_to_string(list_file).await?;
    let altlist_file: AltListFile = serde_json::from_str(&content)?;
    for entry in altlist_file.entries {
        if name == entry.name {
            return Ok(name.to_string());
        }
        for regex in entry.alts {
            if regex.is_match(name) {
                return Ok(entry.name);
            }
        }
    }
    Ok(name.to_string())
}
#[allow(clippy::missing_errors_doc)]
pub async fn read_blacklist(list_file: &str) -> Result<String> {
    let content = fs::read_to_string(list_file).await?;
    let blacklist_file: BlackListFile = serde_json::from_str(&content).unwrap_or_default();
    Ok(blacklist_file
        .regexes
        .iter()
        .map(std::string::ToString::to_string)
        .collect::<Vec<_>>()
        .join("\n"))
}
#[allow(clippy::missing_errors_doc)]
#[allow(clippy::missing_panics_doc)]
pub async fn write_blacklist(list_file: &str, regexes: String) -> Result<()> {
    let blacklist_file: BlackListFile = BlackListFile {
        regexes: regexes
            .lines()
            .map(|r| Regex::new(r).expect("invalid RegEx"))
            .collect(),
    };
    let content = serde_json::to_string(&blacklist_file)?;
    fs::write(list_file, content).await?;
    Ok(())
}
#[allow(clippy::missing_errors_doc)]
pub async fn read_altlist(list_file: &str) -> Result<String> {
    let content = fs::read_to_string(list_file).await?;
    Ok(serde_json::to_string_pretty(
        &serde_json::from_str::<AltListFile>(&content).unwrap_or_default(),
    )?)
}
#[allow(clippy::missing_errors_doc)]
pub async fn write_altlist(list_file: &str, content: String) -> Result<()> {
    let content = serde_json::to_string(&serde_json::from_str::<AltListFile>(&content)?)?;
    fs::write(list_file, content).await?;
    Ok(())
}

#[allow(clippy::missing_errors_doc)]
#[allow(clippy::missing_panics_doc)]
pub async fn global_rankings_update() -> Result<()> {
    dotenv().ok();
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
    let leaderboards = tracks_leaderboards(track_ids, LB_SIZE).await?;
    let mut player_times: HashMap<String, Vec<u32>> = HashMap::new();
    for leaderboard in leaderboards {
        let mut has_time: Vec<String> = Vec::new();
        for entry in leaderboard {
            let name = get_alt(ALT_ACCOUNT_FILE, &entry.name).await?;
            if !has_time.contains(&name) && check_blacklist(BLACKLIST_FILE, &name).await? {
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
                        "{}:{:0>2}.{:0>3}",
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
pub async fn hof_update() -> Result<()> {
    let track_ids: Vec<String> = fs::read_to_string(HOF_TRACK_FILE)
        .await?
        .lines()
        .map(|track_id| {
            track_id
                .split_once(' ')
                .expect("invalid lb file format")
                .0
                .to_string()
        })
        .collect();
    let track_num = u32::try_from(track_ids.len()).expect("Shouldn't have that many track IDs");
    let leaderboards = tracks_leaderboards(track_ids, 1).await?;
    let mut player_rankings: HashMap<String, Vec<usize>> = HashMap::new();
    let mut time_rankings: HashMap<String, Vec<u32>> = HashMap::new();
    let point_values: Vec<u32> = fs::read_to_string(HOF_POINTS_FILE)
        .await?
        .lines()
        .map(|s| s.to_string().parse().expect("Invalid point value file"))
        .collect();
    for leaderboard in leaderboards {
        let mut has_ranking: Vec<String> = Vec::new();
        let mut pos = 0;
        for entry in leaderboard {
            let name = get_alt(HOF_ALT_ACCOUNT_FILE, &entry.name).await?;
            if !has_ranking.contains(&name) && check_blacklist(HOF_BLACKLIST_FILE, &name).await? {
                has_ranking.push(name.clone());
                time_rankings
                    .entry(name.clone())
                    .or_default()
                    .push(entry.frames);
                if pos + 1 > point_values.len() {
                    continue;
                }
                player_rankings.entry(name).or_default().push(pos);
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
                        "{}:{:0>2}.{:0>3}",
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
pub async fn community_update() -> Result<()> {
    let track_ids: Vec<String> = fs::read_to_string(COMMUNITY_TRACK_FILE)
        .await?
        .lines()
        .map(|track_id| {
            track_id
                .split_once(' ')
                .expect("invalid lb file format")
                .0
                .to_string()
        })
        .collect();
    let track_num = u32::try_from(track_ids.len()).expect("Shouldn't have that many tracks");
    let leaderboards = tracks_leaderboards(track_ids, COMMUNITY_LB_SIZE).await?;
    let mut player_rankings: HashMap<String, Vec<usize>> = HashMap::new();
    let mut time_rankings: HashMap<String, Vec<u32>> = HashMap::new();
    for leaderboard in leaderboards {
        let mut has_ranking: Vec<String> = Vec::new();
        let mut pos = 0;
        for entry in leaderboard {
            if pos + 1 > COMMUNITY_LB_SIZE as usize * 500 {
                break;
            }
            let name = get_alt(ALT_ACCOUNT_FILE, &entry.name).await?;
            if !has_ranking.contains(&name) && check_blacklist(BLACKLIST_FILE, &name).await? {
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
                        "{}:{:0>2}.{:0>3}",
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

async fn tracks_leaderboards(
    track_ids: Vec<String>,
    lb_size: u32,
) -> Result<Vec<Vec<LeaderBoardEntry>>> {
    let client = Client::new();
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
                sleep(Duration::from_millis(1000)).await;
                let mut response = client.get(&url).send().await?.text().await?;
                while response.is_empty() && att < REQUEST_RETRY_COUNT {
                    att += 1;
                    sleep(Duration::from_millis(5000)).await;
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
    Ok(leaderboards)
}

#[must_use]
pub fn export_to_id(track_code: &str) -> Option<String> {
    let track_data = decode_track_code(track_code)?;
    let id = hash_vec(&track_data);
    Some(id)
}
const DECODE_VALUES: [i32; 123] = [
    -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1,
    -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1,
    52, 53, 54, 55, 56, 57, 58, 59, 60, 61, -1, -1, -1, -1, -1, -1, -1, 0, 1, 2, 3, 4, 5, 6, 7, 8,
    9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, -1, -1, -1, -1, -1, -1, 26,
    27, 28, 29, 30, 31, 32, 33, 34, 35, 36, 37, 38, 39, 40, 41, 42, 43, 44, 45, 46, 47, 48, 49, 50,
    51,
];
fn decode_track_code(track_code: &str) -> Option<Vec<u8>> {
    let track_code = track_code.get(10..)?;
    let td_start = track_code.find("4p")?;
    let track_data = track_code.get(td_start..)?;
    let step1 = decode(track_data)?;
    let step2 = decompress(&step1)?;
    let step2_str = String::from_utf8(step2).ok()?;
    let step3 = decode(&step2_str)?;
    let step4 = decompress(&step3)?;
    let name_len = *step4.first()? as usize;
    let author_len = *step4.get(1 + name_len)? as usize;
    let track_data = step4.get((name_len + author_len + 2)..)?.to_vec();
    Some(track_data)
}
fn decompress(data: &[u8]) -> Option<Vec<u8>> {
    let mut decoder = ZlibDecoder::new(data);
    let mut decompressed_data = Vec::new();
    decoder.read_to_end(&mut decompressed_data).ok()?;
    Some(decompressed_data)
}
fn hash_vec(track_data: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(track_data);
    let result = hasher.finalize();
    hex::encode(result)
}
fn decode(input: &str) -> Option<Vec<u8>> {
    let mut out_pos = 0;
    let mut bytes_out: Vec<u8> = Vec::new();
    for (i, ch) in input.chars().enumerate() {
        let char_code = ch as usize;
        let char_value = match DECODE_VALUES.get(char_code) {
            None => return None,
            Some(v) => match v {
                -1 => return None,
                _ => u8::try_from(*v).expect("Value should be u8"),
            },
        };
        let value_len = if (char_value & 30) == 30 { 5 } else { 6 };
        decode_chars(
            &mut bytes_out,
            out_pos,
            value_len,
            char_value,
            i == input.len() - 1,
        );
        out_pos += value_len;
    }
    Some(bytes_out)
}
fn decode_chars(
    bytes: &mut Vec<u8>,
    bit_index: usize,
    value_len: usize,
    char_value: u8,
    is_last: bool,
) {
    let byte_index = bit_index / 8;
    while byte_index >= bytes.len() {
        bytes.push(0);
    }
    let offset = bit_index - 8 * byte_index;
    bytes[byte_index] |= char_value << offset;
    if offset > 8 - value_len && !is_last {
        let byte_index_next = byte_index + 1;
        if byte_index_next >= bytes.len() {
            bytes.push(0);
        }
        bytes[byte_index_next] |= char_value >> (8 - offset);
    }
}

#[must_use]
#[allow(clippy::missing_panics_doc)]
pub fn recent_et_period(current_time: DateTime<Utc>) -> DateTime<Utc> {
    let candidate = current_time
        .date_naive()
        .and_hms_opt(20, 0, 0)
        .expect("should be valid");
    let mut candidate = candidate.and_utc();
    if candidate >= current_time {
        candidate -= Duration::from_secs(86400);
    }
    while candidate.weekday() != chrono::Weekday::Sun {
        candidate -= Duration::from_secs(86400);
    }
    candidate
}

#[allow(clippy::missing_panics_doc)]
#[allow(clippy::missing_errors_doc)]
#[allow(clippy::too_many_lines)]
#[allow(clippy::cognitive_complexity)]
pub async fn et_rankings_update() -> Result<()> {
    let track_ids: Vec<String> = fs::read_to_string(ET_TRACK_FILE)
        .await?
        .lines()
        .map(|track_id| {
            track_id
                .split_once(' ')
                .expect("invalid lb file format")
                .0
                .to_string()
        })
        .collect();
    let track_num = u32::try_from(track_ids.len()).expect("Shouldn't have that many track IDs");
    let leaderboards = tracks_leaderboards(track_ids, 1).await?;
    let mut player_rankings: HashMap<String, Vec<usize>> = HashMap::new();
    let mut time_rankings: HashMap<String, Vec<u32>> = HashMap::new();
    // TODO: change to actual point values
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
            let name = get_alt(ALT_ACCOUNT_FILE, &entry.name).await?;
            if !has_ranking.contains(&name) && check_blacklist(BLACKLIST_FILE, &name).await? {
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
    fs::write(ET_RANKINGS_FILE, output.clone()).await?;
    Ok(())
}
