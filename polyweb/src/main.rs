#[macro_use]
extern crate rocket;
use polymanager::{
    community_update, global_rankings_update, hof_update, ALT_ACCOUNT_FILE, BETA_RANKINGS_FILE,
    BLACKLIST_FILE, COMMUNITY_RANKINGS_FILE, CUSTOM_TRACK_FILE, HOF_RANKINGS_FILE, RANKINGS_FILE,
    TRACK_FILE,
};
use reqwest::Client;
use rocket::form::validate::Contains;
use rocket::fs::FileServer;
use rocket::serde::{Deserialize, Serialize};
use rocket::tokio::{
    fs, task,
    time::{sleep, Duration},
};
use rocket_dyn_templates::{context, Template};
use std::collections::HashMap;

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
struct Entry {
    rank: u32,
    stat: String,
    name: String,
}

const AUTOUPDATE_TIMER: Duration = Duration::from_secs(60 * 30);

#[get("/")]
async fn index() -> Template {
    let leaderboard = parse_leaderboard(RANKINGS_FILE).await;
    Template::render("leaderboard", context! { leaderboard })
}

#[get("/beta")]
async fn beta() -> Template {
    let leaderboard = parse_leaderboard(BETA_RANKINGS_FILE).await;
    Template::render("leaderboard", context! { leaderboard })
}

#[get("/community")]
async fn community() -> Template {
    let leaderboard = parse_community_leaderboard(COMMUNITY_RANKINGS_FILE).await;
    Template::render("community", context! { leaderboard })
}

#[get("/hof")]
async fn hof() -> Template {
    let leaderboard = parse_hof_leaderboard(HOF_RANKINGS_FILE).await;
    Template::render("hof", context! { leaderboard })
}

#[get("/lb-custom")]
async fn custom_lb_home() -> Template {
    let tracks: Vec<String> = fs::read_to_string(CUSTOM_TRACK_FILE)
        .await
        .unwrap()
        .lines()
        .map(|s| s.to_string())
        .map(|s| s.split_once(" ").unwrap().1.to_string())
        .collect();
    Template::render("lb_custom_home", context! { tracks })
}

#[get("/lb-standard")]
async fn standard_lb_home() -> Template {
    let track_num = fs::read_to_string(TRACK_FILE)
        .await
        .unwrap()
        .lines()
        .count() as u32;
    let numbers: Vec<String> = (1..=track_num).map(|num| format!("{:0>2}", num)).collect();
    Template::render("lb_standard_home", context! { numbers })
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
            routes![
                index,
                hof,
                beta,
                community,
                tutorial,
                standard_lb_home,
                standard_lb,
                custom_lb_home,
                custom_lb,
                policy
            ],
        )
        .mount("/static", FileServer::from("static"))
        .attach(Template::fairing());
    task::spawn(async {
        loop {
            community_update().await.expect("Failed update");
            sleep(AUTOUPDATE_TIMER / 3).await;
            hof_update().await.expect("Failed update");
            sleep(AUTOUPDATE_TIMER / 3).await;
            global_rankings_update(None, false)
                .await
                .expect("Failed update");
            sleep(AUTOUPDATE_TIMER / 3).await;
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

async fn parse_community_leaderboard(file_path: &str) -> (Vec<Entry>, Vec<Entry>) {
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
            "https://vps.kodub.com:43273/leaderboard?version=0.4.0&trackId={}&skip=0&amount=500",
            track_ids.get(&real_track_id).unwrap()
        )
    } else {
        format!(
            "https://vps.kodub.com:43273/leaderboard?version=0.4.0&trackId={}&skip=0&amount=500",
            track_id
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

async fn get_standard_leaderboard(track_id: usize) -> Vec<Entry> {
    let client = Client::new();
    let track_ids: Vec<String> = fs::read_to_string(TRACK_FILE)
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
