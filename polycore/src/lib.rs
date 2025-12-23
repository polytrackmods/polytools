use std::{collections::HashMap, time::Duration};

use anyhow::{Error, Result, anyhow};
use chrono::{DateTime, Datelike as _, Utc};
use facet::Facet;
use futures::future::join_all;
use regex::Regex;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tokio::{fs, task, time::sleep};

pub const BLACKLIST_FILE: &str = "data/blacklist.txt";
pub const ALT_ACCOUNT_FILE: &str = "data/alt_accounts.txt";
pub const RANKINGS_FILE: &str = "data/poly_rankings.txt";
pub const POINT_RANKINGS_FILE: &str = "data/point_poly_rankings.txt";
const LB_SIZE: u32 = 20;
pub const TRACK_FILE: &str = "lists/official_tracks.txt";
pub const HOF_CODE_FILE: &str = "lists/hof_codes.txt";
pub const HOF_TRACK_FILE: &str = "lists/hof_tracks.txt";
pub const HOF_ALL_TRACK_FILE: &str = "lists/hof_tracks_all.txt";
pub const HOF_POINTS_FILE: &str = "lists/hof_points.txt";
pub const HOF_RANKINGS_FILE: &str = "data/hof_rankings.txt";
pub const HOF_TIME_RANKINGS_FILE: &str = "data/hof_time_rankings.txt";
pub const COMMUNITY_TRACK_FILE: &str = "lists/community_tracks.txt";
pub const COMMUNITY_RANKINGS_FILE: &str = "data/community_rankings.txt";
pub const COMMUNITY_TIME_RANKINGS_FILE: &str = "data/community_time_rankings.txt";
const COMMUNITY_LB_SIZE: u32 = 20;
pub const VERSION: &str = "0.5.2";
pub const HISTORY_FILE_LOCATION: &str = "histories/";
pub const REQUEST_RETRY_COUNT: u32 = 5;
pub const ET_CODE_FILE: &str = "data/et_codes.txt";
pub const ET_TRACK_FILE: &str = "data/et_tracks.txt";
pub const ET_RANKINGS_FILE: &str = "data/et_rankings.txt";

pub const UPDATE_LB_COUNT: u64 = 4;
pub const UPDATE_CYCLE_LEN: Duration = Duration::from_secs(UPDATE_LB_COUNT * 10 * 60);

const UPDATE_LOCK_FILE: &str = "data/update.lock";
const MAX_LOCK_TIME: Duration = Duration::from_secs(300);

#[derive(thiserror::Error, Debug)]
enum PolyError {
    #[error("Currently updating something, please wait a bit")]
    BusyUpdating,
}

#[derive(Facet)]
#[facet(rename_all = "camelCase")]
pub struct LeaderBoardEntry {
    pub name: String,
    pub frames: u32,
    pub user_id: String,
}

#[derive(Facet)]
struct LeaderBoard {
    entries: Vec<LeaderBoardEntry>,
}

#[derive(Deserialize, Serialize, Default, Facet)]
pub struct PolyLeaderBoard {
    pub total: usize,
    pub entries: Vec<PolyLeaderBoardEntry>,
}

#[derive(Deserialize, Serialize, Facet)]
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
pub async fn check_blacklist(name: &str) -> Result<bool> {
    let content = fs::read_to_string(BLACKLIST_FILE).await?;
    let blacklist_file: BlackListFile = serde_json::from_str(&content)?;
    for regex in blacklist_file.regexes {
        if regex.is_match(name) {
            return Ok(false);
        }
    }
    Ok(true)
}
#[allow(clippy::missing_errors_doc)]
pub async fn get_alt(name: &str) -> Result<String> {
    let content = fs::read_to_string(ALT_ACCOUNT_FILE).await?;
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
pub async fn read_blacklist() -> Result<String> {
    let content = fs::read_to_string(BLACKLIST_FILE).await?;
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
pub async fn write_blacklist(regexes: String) -> Result<()> {
    let blacklist_file: BlackListFile = BlackListFile {
        regexes: regexes
            .lines()
            .map(|r| Regex::new(r).expect("invalid RegEx"))
            .collect(),
    };
    let content = serde_json::to_string(&blacklist_file)?;
    fs::write(BLACKLIST_FILE, content).await?;
    Ok(())
}
#[allow(clippy::missing_errors_doc)]
pub async fn read_altlist() -> Result<String> {
    let content = fs::read_to_string(ALT_ACCOUNT_FILE).await?;
    Ok(serde_json::to_string_pretty(
        &serde_json::from_str::<AltListFile>(&content).unwrap_or_default(),
    )?)
}
#[allow(clippy::missing_errors_doc)]
pub async fn write_altlist(content: String) -> Result<()> {
    let content = serde_json::to_string(&serde_json::from_str::<AltListFile>(&content)?)?;
    fs::write(ALT_ACCOUNT_FILE, content).await?;
    Ok(())
}

/* #[allow(clippy::missing_errors_doc)]
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
            let name = get_alt(&entry.name).await?;
            if !has_time.contains(&name) && check_blacklist(&name).await? {
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
    let leaderboard = PolyLeaderBoard {
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
    let output = facet_json::to_string(&leaderboard);
    fs::write(RANKINGS_FILE, output).await?;
    tracing::info!("Updated Global LB!");
    Ok(())
} */

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
            let name = get_alt(&entry.name).await?;
            if !has_ranking.contains(&name) && check_blacklist(&name).await? {
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
            (name.clone(), points, tiebreakers)
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
    for (rank, (name, points, _)) in sorted_leaderboard.clone().into_iter().enumerate() {
        final_leaderboard.push_entry(PolyLeaderBoardEntry::new(
            rank + 1,
            name,
            points.to_string(),
        ));
    }
    let mut output = facet_json::to_string(&final_leaderboard);
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
    output.push_str(&facet_json::to_string(&final_player_records));
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
    let time_output = facet_json::to_string(&time_leaderboard);
    fs::write(HOF_TIME_RANKINGS_FILE, time_output).await?;
    tracing::info!("Updated HOF LB!");
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
            let name = get_alt(&entry.name).await?;
            if !has_ranking.contains(&name) && check_blacklist(&name).await? {
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
            (name.clone(), points as u32, tiebreakers)
        })
        .collect();
    sorted_leaderboard.sort_by(|a, b| {
        let (_, points_a, tiebreakers_a) = a;
        let (_, points_b, tiebreakers_b) = b;
        points_b
            .cmp(points_a)
            .then_with(|| tiebreakers_b.cmp(tiebreakers_a))
    });
    let final_leaderboard_entries: Vec<_> = sorted_leaderboard
        .into_iter()
        .enumerate()
        .map(|(rank, (name, points, _))| {
            PolyLeaderBoardEntry::new(rank + 1, name, points.to_string())
        })
        .collect();
    let final_leaderboard = PolyLeaderBoard {
        total: final_leaderboard_entries.len(),
        entries: final_leaderboard_entries,
    };
    let mut output = facet_json::to_string(&final_leaderboard);
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
    for (name, records) in &player_records {
        if *records < records_prev {
            records_prev = *records;
            rank_prev += 1;
        }
        final_player_records.push_entry(PolyLeaderBoardEntry::new(
            rank_prev,
            name.clone(),
            records_prev.to_string(),
        ));
    }
    output.push('\n');
    output.push_str(&facet_json::to_string(&final_player_records));
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
    let time_output = facet_json::to_string(&time_leaderboard);
    fs::write(COMMUNITY_TIME_RANKINGS_FILE, time_output).await?;
    tracing::info!("Updated CT LB!");
    Ok(())
}

#[allow(clippy::missing_panics_doc)]
#[allow(clippy::missing_errors_doc)]
#[allow(clippy::too_many_lines)]
#[allow(clippy::cognitive_complexity)]
#[allow(clippy::cast_precision_loss)]
#[allow(clippy::cast_possible_truncation)]
#[allow(clippy::cast_sign_loss)]
pub async fn global_rankings_update() -> Result<()> {
    let track_ids: Vec<String> = fs::read_to_string(TRACK_FILE)
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
    let leaderboards = tracks_leaderboards(track_ids, LB_SIZE).await?;
    let mut point_rankings: HashMap<String, Vec<usize>> = HashMap::new();
    let mut time_rankings: HashMap<String, Vec<u32>> = HashMap::new();
    for leaderboard in leaderboards {
        let mut has_ranking: Vec<String> = Vec::new();
        let mut pos = 0;
        for entry in leaderboard {
            let name = get_alt(&entry.name).await?;
            if !has_ranking.contains(&name) && check_blacklist(&name).await? {
                point_rankings.entry(name.clone()).or_default().push(pos);
                time_rankings
                    .entry(name.clone())
                    .or_default()
                    .push(entry.frames);
                has_ranking.push(name);
                pos += 1;
            }
        }
    }
    let mut sorted_point_leaderboard: Vec<(String, u32, Vec<u32>)> = point_rankings
        .iter()
        .map(|(name, rankings)| {
            let mut tiebreakers = vec![0; LB_SIZE as usize * 500];
            let mut points = 0.0;
            for ranking in rankings {
                points += 100.0 / (*ranking as f64 + 1.0).sqrt();
                *tiebreakers.get_mut(*ranking).unwrap_or(&mut 0) += 1;
            }
            (name.clone(), points as u32, tiebreakers)
        })
        .collect();
    sorted_point_leaderboard.sort_by(|a, b| {
        let (_, points_a, tiebreakers_a) = a;
        let (_, points_b, tiebreakers_b) = b;
        points_b
            .cmp(points_a)
            .then_with(|| tiebreakers_b.cmp(tiebreakers_a))
    });
    let final_point_leaderboard_entries: Vec<_> = sorted_point_leaderboard
        .into_iter()
        .enumerate()
        .map(|(rank, (name, points, _))| {
            PolyLeaderBoardEntry::new(rank + 1, name, points.to_string())
        })
        .collect();
    let final_point_leaderboard = PolyLeaderBoard {
        total: final_point_leaderboard_entries.len(),
        entries: final_point_leaderboard_entries,
    };
    let mut output = facet_json::to_string(&final_point_leaderboard);
    let mut player_records: HashMap<String, u32> = HashMap::new();
    for (name, rankings) in point_rankings {
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
    for (name, records) in &player_records {
        if *records < records_prev {
            records_prev = *records;
            rank_prev += 1;
        }
        final_player_records.push_entry(PolyLeaderBoardEntry::new(
            rank_prev,
            name.clone(),
            records_prev.to_string(),
        ));
    }
    output.push('\n');
    output.push_str(&facet_json::to_string(&final_player_records));
    fs::write(POINT_RANKINGS_FILE, output).await?;
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
    let time_output = facet_json::to_string(&time_leaderboard);
    fs::write(RANKINGS_FILE, time_output).await?;
    tracing::info!("Updated Global LB!");
    Ok(())
}

#[derive(Serialize)]
struct UrlRequest {
    url: String,
}
impl UrlRequest {
    fn new(url: &str) -> Self {
        Self {
            url: url.to_string(),
        }
    }
}
const POLYNETWORKER_URL: &str = "http://127.0.0.1:3000/submit";

#[allow(clippy::missing_errors_doc)]
pub async fn send_to_networker(client: &Client, url: &str) -> Result<String> {
    Ok(client
        .post(POLYNETWORKER_URL)
        .json(&UrlRequest::new(url))
        .send()
        .await?
        .text()
        .await?)
}

pub async fn tracks_leaderboards(
    track_ids: Vec<String>,
    lb_size: u32,
) -> Result<Vec<Vec<LeaderBoardEntry>>> {
    let client = Client::new();
    let futures = track_ids.iter().map(|track_id| {
        let client = client.clone();
        let mut urls = Vec::new();
        for i in 0..lb_size {
            urls.push(format!(
                "https://vps.kodub.com/leaderboard?version={VERSION}&trackId={track_id}&skip={}&amount=500",
                i * 500,
            ));
        }
        task::spawn(async move {
            let mut res = Vec::new();
            for url in &urls {
                let mut att = 0;
                let mut response = String::new();
                while response.is_empty() && att <= REQUEST_RETRY_COUNT {
                    response = send_to_networker(&client, url).await?;
                    sleep(Duration::from_millis(500)).await;
                    att += 1;
                }
                res.push(response);
            }
            Ok::<Vec<String>, Error>(res)
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
                &mut facet_json::from_str::<LeaderBoard>(&res)
                    .map_err(|_| anyhow!("Probably got rate limited, please try again later"))?
                    .entries,
            );
        }
        leaderboards.push(leaderboard);
    }
    Ok(leaderboards)
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
            let name = get_alt(&entry.name).await?;
            if !has_ranking.contains(&name) && check_blacklist(&name).await? {
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
    for (rank, (name, points, _)) in sorted_leaderboard.clone().into_iter().enumerate() {
        final_leaderboard.push_entry(PolyLeaderBoardEntry::new(
            rank + 1,
            name,
            points.to_string(),
        ));
    }
    let mut output = facet_json::to_string(&final_leaderboard);
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
    output.push_str(&facet_json::to_string(&final_player_records));
    fs::write(ET_RANKINGS_FILE, output.clone()).await?;
    tracing::info!("Updated ET Rankings!");
    Ok(())
}

#[allow(clippy::missing_panics_doc)]
pub async fn read_track_file(file: &str) -> Vec<(String, String)> {
    fs::read_to_string(file)
        .await
        .expect("Failed to read file")
        .lines()
        .map(|l| l.split_once(' ').expect("failed to split tracks in file"))
        .map(|(a, b)| (a.to_string(), b.to_string()))
        .collect()
}
