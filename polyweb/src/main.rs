#[macro_use]
extern crate rocket;
use rocket::fs::FileServer;
use rocket::futures::future::join_all;
use rocket::serde::{Deserialize, Serialize};
use rocket::tokio::{self, fs};
use rocket::tokio::{
    task,
    time::{sleep, Duration},
};
use rocket_dyn_templates::{context, Template};
use std::collections::HashMap;
use std::env;

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

#[derive(Serialize, Deserialize)]
struct Entry {
    rank: u32,
    stat: String,
    name: String,
}

const BLACKLIST_FILE: &str = "blacklist.txt";
const HOF_BLACKLIST_FILE: &str = "hof_blacklist.txt";
const ALT_ACCOUNT_FILE: &str = "alt_accounts.txt";
const HOF_ALT_ACCOUNT_FILE: &str = "hof_alt_accounts.txt";
const GLOBAL_RANKINGS_FILE: &str = "poly_rankings.txt";
const HOF_RANKINGS_FILE: &str = "hof_rankings.txt";
const CUSTOM_TRACK_FILE: &str = "custom_tracks.txt";
const MAX_RANKINGS_AGE: Duration = Duration::from_secs(60 * 10);
const AUTOUPDATE_TIMER: Duration = Duration::from_secs(60 * 30);

#[get("/")]
async fn index() -> Template {
    let leaderboard = parse_leaderboard(GLOBAL_RANKINGS_FILE).await;
    Template::render("leaderboard", context! { leaderboard })
}

#[get("/hof")]
async fn hof() -> Template {
    let leaderboard = parse_hof_leaderboard(HOF_RANKINGS_FILE).await;
    Template::render("hof", context! { leaderboard })
}

#[get("/lb-custom/<track_id>")]
async fn custom_lb(track_id: &str) -> Template {
    let (name, leaderboard) = get_custom_leaderboard(track_id).await;
    Template::render(
        "track_leaderboard",
        context! { track_name: name, leaderboard },
    )
}

#[get("/lb-standard/<track_id>")]
async fn standard_lb(track_id: usize) -> Template {
    let leaderboard = get_standard_leaderboard(track_id).await;
    Template::render(
        "track_leaderboard",
        context! { track_name: format!("Track {} ", track_id), leaderboard },
    )
}

#[get("/policy")]
async fn policy() -> Template {
    let context: HashMap<String, String> = HashMap::new();
    Template::render("privacy_policy", context)
}

#[get("/tutorial")]
async fn tutorial() -> Template {
    let context: HashMap<String, String> = HashMap::new();
    Template::render("tutorial", context)
}

#[main]
async fn main() -> Result<(), rocket::Error> {
    let rocket = rocket::build()
        .mount(
            "/",
            routes![index, hof, tutorial, standard_lb, custom_lb, policy],
        )
        .mount("/static", FileServer::from("static"))
        .attach(Template::fairing());
    task::spawn(async {
        loop {
            if tokio::fs::try_exists(HOF_RANKINGS_FILE).await.unwrap() {
                let age = tokio::fs::metadata(HOF_RANKINGS_FILE)
                    .await
                    .unwrap()
                    .modified()
                    .unwrap()
                    .elapsed()
                    .unwrap();
                if age > MAX_RANKINGS_AGE {
                    hof_update().await.expect("Failed update");
                }
            } else {
                hof_update().await.expect("Failed update");
            }
            sleep(AUTOUPDATE_TIMER / 2).await;
            if tokio::fs::try_exists(GLOBAL_RANKINGS_FILE).await.unwrap() {
                let age = tokio::fs::metadata(GLOBAL_RANKINGS_FILE)
                    .await
                    .unwrap()
                    .modified()
                    .unwrap()
                    .elapsed()
                    .unwrap();
                if age > MAX_RANKINGS_AGE {
                    rankings_update().await.expect("Failed update");
                }
            } else {
                rankings_update().await.expect("Failed update");
            }
            sleep(AUTOUPDATE_TIMER / 2).await;
        }
    });
    rocket.launch().await?;
    Ok(())
}

async fn parse_leaderboard(file_path: &str) -> Vec<Entry> {
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

async fn parse_hof_leaderboard(file_path: &str) -> (Vec<Entry>, Vec<Entry>) {
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

async fn get_custom_leaderboard(track_id: &str) -> (String, Vec<Entry>) {
    let client = reqwest::Client::new();
    let track_ids: HashMap<String, String> = tokio::fs::read_to_string(CUSTOM_TRACK_FILE)
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
    let url;
    if track_ids.contains_key(&track_id.to_lowercase()) {
        url = format!(
            "https://vps.kodub.com:43273/leaderboard?version=0.4.0&trackId={}&skip=0&amount=500",
            track_ids.get(&track_id.to_lowercase()).unwrap()
        );
    } else {
        url = format!(
            "https://vps.kodub.com:43273/leaderboard?version=0.4.0&trackId={}&skip=0&amount=500",
            track_id
        );
    }
    let result = client.get(&url).send().await.unwrap().text().await.unwrap();
    let response: LeaderBoard = serde_json::from_str(&result).unwrap();
    let mut leaderboard = Vec::new();
    let blacklist: Vec<String> = tokio::fs::read_to_string(BLACKLIST_FILE)
        .await
        .unwrap()
        .lines()
        .map(|s| s.to_string())
        .collect();
    let alt_file: Vec<String> = tokio::fs::read_to_string(ALT_ACCOUNT_FILE)
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
                line.split(SPLIT_CHAR).nth(0).unwrap().to_string(),
            );
        }
    }
    let mut rank = 0;
    let mut has_time: Vec<String> = Vec::new();
    for entry in response.entries {
        let name;
        if alt_list.contains_key(&entry.name) {
            name = alt_list.get(&entry.name).unwrap().clone();
        } else {
            name = entry.name.clone();
        }
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
                        entry.frames as u32 / 1000
                    )
                }
            },
            name: name.clone(),
        });
        has_time.push(name);
    }
    let name = if track_ids.contains_key(&track_id.to_lowercase()) {
        format!("{} ", track_id)
    } else {
        String::new()
    };
    (name, leaderboard)
}

async fn get_standard_leaderboard(track_id: usize) -> Vec<Entry> {
    let client = reqwest::Client::new();
    let track_ids: Vec<String> = tokio::fs::read_to_string("official_tracks.txt")
        .await
        .unwrap()
        .lines()
        .map(|s| s.to_string())
        .collect();
    let url = format!(
        "https://vps.kodub.com:43273/leaderboard?version=0.4.0&trackId={}&skip=0&amount=500",
        track_ids[track_id - 1]
    );
    let result = client.get(&url).send().await.unwrap().text().await.unwrap();
    let response: LeaderBoard = serde_json::from_str(&result).unwrap();
    let mut leaderboard = Vec::new();
    let blacklist: Vec<String> = tokio::fs::read_to_string(BLACKLIST_FILE)
        .await
        .unwrap()
        .lines()
        .map(|s| s.to_string())
        .collect();
    let alt_file: Vec<String> = tokio::fs::read_to_string(ALT_ACCOUNT_FILE)
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
                line.split(SPLIT_CHAR).nth(0).unwrap().to_string(),
            );
        }
    }
    let mut rank = 0;
    let mut has_time: Vec<String> = Vec::new();
    for entry in response.entries {
        let name;
        if alt_list.contains_key(&entry.name) {
            name = alt_list.get(&entry.name).unwrap().clone();
        } else {
            name = entry.name.clone();
        }
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
                        entry.frames as u32 / 1000
                    )
                }
            },
            name: name.clone(),
        });
        has_time.push(name);
    }
    leaderboard
}

async fn rankings_update() -> Result<(), Error> {
    dotenv::dotenv().ok();
    let lb_size = env::var("LEADERBOARD_SIZE")
        .expect("Expected LEADERBOARD_SIZE in env!")
        .parse()
        .expect("LEADERBOARD_SIZE not a valid integer!");
    let client = reqwest::Client::new();
    let track_ids: Vec<String> = tokio::fs::read_to_string("official_tracks.txt")
        .await?
        .lines()
        .map(|s| s.to_string())
        .collect();
    let track_num = track_ids.len();
    let futures = track_ids.into_iter().map(|track_id| {
        let client = client.clone();
        let mut urls = Vec::new();
        for i in 0..lb_size {
            urls.push(format!("https://vps.kodub.com:43273/leaderboard?version=0.4.0&trackId={}&skip={}&amount=500",
            track_id,
            i * 500));
        }
        task::spawn(
            async move {
                let mut res = Vec::new();
                for i in 0..lb_size {
                    res.push(client.get(&urls[i]).send().await.unwrap().text().await.unwrap());
                }
                return Ok::<Vec<String>, reqwest::Error>(res);
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
    let blacklist: Vec<String> = tokio::fs::read_to_string(BLACKLIST_FILE)
        .await?
        .lines()
        .map(|s| s.to_string())
        .collect();
    let alt_file: Vec<String> = tokio::fs::read_to_string(ALT_ACCOUNT_FILE)
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
                line.split(SPLIT_CHAR).nth(0).unwrap().to_string(),
            );
        }
    }
    for leaderboard in leaderboards {
        let mut has_time: Vec<String> = Vec::new();
        for entry in leaderboard {
            let name;
            if alt_list.contains_key(&entry.name) {
                name = alt_list.get(&entry.name).unwrap().clone();
            } else {
                name = entry.name.clone();
            }
            if !has_time.contains(&name) && !blacklist.contains(&name) {
                player_times
                    .entry(name.clone())
                    .or_insert(Vec::new())
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
    tokio::fs::write(GLOBAL_RANKINGS_FILE, output.clone()).await?;
    Ok(())
}

async fn hof_update() -> Result<(), Error> {
    let client = reqwest::Client::new();
    let track_ids: Vec<String> = tokio::fs::read_to_string("hof_tracks.txt")
        .await?
        .lines()
        .map(|s| s.to_string())
        .collect();
    let track_num = track_ids.len() as u32;
    let futures = track_ids.into_iter().map(|track_id| {
        let client = client.clone();
        let url = format!(
            "https://vps.kodub.com:43273/leaderboard?version=0.4.0&trackId={}&skip=0&amount=100",
            track_id.splitn(2, " ").next().unwrap()
        );
        task::spawn(async move {
            let res = client.get(url).send().await.unwrap().text().await.unwrap();
            return Ok::<String, reqwest::Error>(res);
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
        let leaderboard: Vec<LeaderBoardEntry> =
            serde_json::from_str::<LeaderBoard>(&result)?.entries;
        leaderboards.push(leaderboard);
    }
    let mut player_rankings: HashMap<String, Vec<usize>> = HashMap::new();
    let blacklist: Vec<String> = tokio::fs::read_to_string(HOF_BLACKLIST_FILE)
        .await?
        .lines()
        .map(|s| s.to_string())
        .collect();
    let alt_file: Vec<String> = tokio::fs::read_to_string(HOF_ALT_ACCOUNT_FILE)
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
                line.split(SPLIT_CHAR).nth(0).unwrap().to_string(),
            );
        }
    }
    let point_values: Vec<u32> = fs::read_to_string("hof_points.txt")
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
            let name;
            if alt_list.contains_key(&entry.name) {
                name = alt_list.get(&entry.name).unwrap().clone();
            } else {
                name = entry.name.clone();
            }
            if !has_ranking.contains(&name) && !blacklist.contains(&name) {
                player_rankings
                    .entry(name.clone())
                    .or_insert(Vec::new())
                    .push(pos);
                has_ranking.push(name);
                pos += 1;
            }
        }
    }
    let mut sorted_leaderboard: Vec<(String, u32)> = player_rankings
        .clone()
        .into_iter()
        .map(|(name, rankings)| {
            let mut points = 0;
            for ranking in rankings {
                if ranking < point_values.len() {
                    points += point_values[ranking];
                }
            }
            (name, points)
        })
        .collect();
    sorted_leaderboard.sort_by_key(|(_, points)| *points);
    sorted_leaderboard.reverse();
    let mut final_leaderboard: Vec<(u32, u32, String)> = Vec::new();
    let mut points_prev = point_values[0] * track_num + 1;
    let mut rank_prev = 0;
    for (name, points) in sorted_leaderboard.clone() {
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
            if rank == 1 {
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
    tokio::fs::write(HOF_RANKINGS_FILE, output.clone()).await?;
    Ok(())
}
