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
    time: String,
    name: String,
}

const BLACKLIST_FILE: &str = "blacklist.txt";
const ALT_ACCOUNT_FILE: &str = "alt_accounts.txt";
const RANKINGS_FILE: &str = "poly_rankings.txt";
const MAX_RANKINGS_AGE: Duration = Duration::from_secs(60 * 10);
const AUTOUPDATE_TIMER: Duration = Duration::from_secs(60 * 30);

#[get("/")]
async fn index() -> Template {
    let leaderboard = parse_leaderboard(RANKINGS_FILE).await;
    Template::render("index", context! { leaderboard })
}

#[get("/tutorial")]
async fn tutorial() -> Template {
    let context: HashMap<String, String> = HashMap::new();
    Template::render("tutorial", context)
}

#[main]
async fn main() -> Result<(), rocket::Error> {
    let rocket = rocket::build()
        .mount("/", routes![index, tutorial])
        .mount("/static", FileServer::from("static"))
        .attach(Template::fairing());
    task::spawn(async {
        loop {
            if tokio::fs::try_exists(RANKINGS_FILE).await.unwrap() {
                let age = tokio::fs::metadata(RANKINGS_FILE)
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
            sleep(AUTOUPDATE_TIMER).await;
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
                    time: parts[1].to_string(),
                    name: parts[2].to_string(),
                })
            } else {
                None
            }
        })
        .collect()
}

async fn rankings_update() -> Result<(), Error> {
    dotenv::dotenv().ok();
    let id = env::var("LEADERBOARD_ID").expect("Expected OWNER_ID in env!");
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
            urls.push(format!("https://vps.kodub.com:43273/leaderboard?version=0.4.0&trackId={}&skip={}&amount=500&onlyVerified=false&userTokenHash={}",
            track_id,
            i * 500,
            id));
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
    tokio::fs::write(RANKINGS_FILE, output.clone()).await?;
    Ok(())
}
