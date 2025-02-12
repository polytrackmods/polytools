use poise::serenity_prelude as serenity;
use poise::serenity_prelude::futures::future::join_all;
use poise::serenity_prelude::CreateActionRow;
use poise::serenity_prelude::CreateAttachment;
use poise::serenity_prelude::CreateButton;
use poise::serenity_prelude::CreateInteractionResponseMessage;
use poise::CreateReply;
use serde::{Deserialize, Serialize};
use serde_json::{Result as JsonResult, Value};
use std::env;
use std::sync::Arc;
use std::sync::Mutex;
use std::{collections::HashMap, time::Duration};
use tokio::fs;
use tokio::io;
use tokio::task;

const USER_FILE: &str = "userIDs.json";
const BLACKLIST_FILE: &str = "blacklist.txt";
const ALT_ACCOUNT_FILE: &str = "alt_accounts.txt";
const RANKINGS_FILE: &str = "poly_rankings.txt";
const MAX_RANKINGS_AGE: Duration = Duration::from_secs(60 * 10);
const MAX_EMBED_AGE: Duration = Duration::from_secs(60 * 60);

#[derive(Serialize, Deserialize, Debug)]
struct BotData {
    user_ids: Mutex<HashMap<String, String>>,
}
type Error = Box<dyn std::error::Error + Send + Sync>;
type Context<'a> = poise::Context<'a, BotData, Error>;

#[derive(Deserialize, Serialize)]
struct LeaderBoardEntry {
    name: String,
    frames: f64,
}

#[derive(Deserialize, Serialize)]
struct LeaderBoard {
    entries: Vec<LeaderBoardEntry>,
}

impl BotData {
    pub async fn load_from_file(file_path: &str) -> JsonResult<Self> {
        let data = fs::read_to_string(file_path)
            .await
            .unwrap_or_else(|_| String::from("{}"));
        let bot_data: BotData = serde_json::from_str(&data)?;
        Ok(bot_data)
    }
    pub async fn save_to_file(&self, file_path: &str) -> io::Result<()> {
        let data = serde_json::to_string(self).unwrap();
        println!("Data saved: {}", data);
        fs::write(file_path, data).await
    }
}
async fn write(ctx: &Context<'_>, mut text: String) -> Result<(), Error> {
    if text.chars().count() > 2000 {
        if text.chars().nth(0).unwrap() == text.chars().nth(1).unwrap()
            && text.chars().nth(1).unwrap() == text.chars().nth(2).unwrap()
            && text.chars().nth(2).unwrap() == '`'
        {
            for _ in 0..3 {
                text.remove(0);
                text.pop();
            }
        } else if text.chars().nth(0).unwrap() == '`' {
            text.remove(0);
            text.pop();
        }
        let file = CreateAttachment::bytes(text.as_bytes(), "polytrackerMsg.txt");
        ctx.send(CreateReply::default().attachment(file)).await?;
    } else {
        ctx.say(text).await?;
    }
    Ok(())
}

async fn write_embed(
    ctx: &Context<'_>,
    title: String,
    description: String,
    headers: Vec<&str>,
    contents: Vec<String>,
    inlines: Vec<bool>,
) -> Result<(), Error> {
    if headers.len() == contents.len() && contents.len() == inlines.len() {
        dotenv::dotenv()?;
        let ctx_id = ctx.id();
        let prev_id = format!("{}prev", ctx_id);
        let next_id = format!("{}next", ctx_id);
        let start_id = format!("{}start", ctx_id);
        let mut pages: Vec<Vec<String>> = Vec::new();
        for i in 0..contents.len() {
            pages.push(
                contents[i]
                    .lines()
                    .collect::<Vec<&str>>()
                    .chunks(20)
                    .map(|chunk| chunk.join("\n"))
                    .collect(),
            );
        }
        let fields = headers
            .clone()
            .into_iter()
            .enumerate()
            .map(|(i, h)| (h, pages.get(i).unwrap().get(0).unwrap().clone(), inlines[i]));
        let embed = serenity::CreateEmbed::default()
            .title(title.clone())
            .description(description.clone())
            .fields(fields.clone())
            .color(serenity::Color::BLITZ_BLUE);
        let reply = {
            let components = CreateActionRow::Buttons(vec![
                CreateButton::new(&prev_id).emoji('◀'),
                CreateButton::new(&next_id).emoji('▶'),
                CreateButton::new(&start_id).emoji('🔝'),
            ]);

            CreateReply::default()
                .embed(embed)
                .components(vec![components])
        };
        let response = ctx.send(reply.clone()).await?;
        let mut current_page = 0;
        while let Some(press) = serenity::collector::ComponentInteractionCollector::new(ctx)
            .filter(move |press| press.data.custom_id.starts_with(&ctx_id.to_string()))
            .timeout(MAX_EMBED_AGE)
            .await
        {
            if press.data.custom_id == next_id {
                current_page += 1;
                if current_page >= pages[0].len() {
                    current_page = 0;
                }
            } else if press.data.custom_id == prev_id {
                current_page = current_page.checked_sub(1).unwrap_or(pages[0].len() - 1);
            } else if press.data.custom_id == start_id {
                current_page = 0;
            } else {
                continue;
            }
            let fields = headers.clone().into_iter().enumerate().map(|(i, h)| {
                (
                    h,
                    pages.get(i).unwrap().get(current_page).unwrap().clone(),
                    inlines[i],
                )
            });
            let embed = serenity::CreateEmbed::default()
                .title(&title)
                .description(&description)
                .fields(fields)
                .color(serenity::Color::BLITZ_BLUE);

            press
                .create_response(
                    ctx.serenity_context(),
                    serenity::CreateInteractionResponse::UpdateMessage(
                        CreateInteractionResponseMessage::new().embed(embed),
                    ),
                )
                .await?;
        }
        response.delete(*ctx).await?;
    } else {
        panic!("Different amounts of columns for write_embed!");
    }
    Ok(())
}

/// Assign a username an ID
///
/// The ID can be found by going from the main menu to "Profile", clicking on the profile \
/// and copying the "User ID" in the bottom left.
#[poise::command(slash_command, prefix_command, category = "Setup")]
async fn assign(
    ctx: Context<'_>,
    #[description = "Username"] user: String,
    #[description = "Player ID"] id: String,
) -> Result<(), Error> {
    ctx.defer_ephemeral().await?;
    let response = format!("`Added user '{}' with ID '{}'`", user, id);
    ctx.data().user_ids.lock().unwrap().insert(user, id);
    write(&ctx, response).await?;
    Ok(())
}

/// Delete an already assigned username-ID pair (bot-admin only)
///
/// Only deletes the data from the bot, you account stays intact.
#[poise::command(
    slash_command,
    prefix_command,
    category = "Administration",
    check = "is_owner"
)]
async fn delete(
    ctx: Context<'_>,
    #[description = "Username"]
    #[autocomplete = "autocomplete_users"]
    user: String,
) -> Result<(), Error> {
    ctx.defer_ephemeral().await?;
    let bot_data = ctx.data();
    let response;
    if bot_data.user_ids.lock().unwrap().contains_key(&user) {
        let id = bot_data.user_ids.lock().unwrap().remove(&user).unwrap();
        response = format!("`Removed user '{}' with ID '{}'`", user, id);
    } else {
        response = format!("`User not found!`");
    }
    write(&ctx, response).await?;
    Ok(())
}

/// Request data about a track
///
/// Choose between standard tracks (off=True) or custom tracks (off=False).
/// For standard tracks use the track number (1-13).
/// For custom tracks use the track ID (Tutorial in help).
#[poise::command(slash_command, prefix_command, category = "Query")]
async fn request(
    ctx: Context<'_>,
    #[description = "IsOfficial"] off: bool,
    #[description = "User"]
    #[autocomplete = "autocomplete_users"]
    user: String,
    #[description = "Track"] track: String,
    #[description = "Hidden"] hidden: Option<bool>,
) -> Result<(), Error> {
    if hidden.is_some_and(|x| x) {
        ctx.defer_ephemeral().await?;
    } else {
        ctx.defer().await?;
    }
    let mut id = String::new();
    if let Some(id_test) = ctx.data().user_ids.lock().unwrap().get(&user) {
        id = id_test.clone();
    }
    if id.len() > 0 {
        let client = reqwest::Client::new();
        let url;
        if off {
            if let Err(_) = track.parse::<usize>() {
                ctx.defer_ephemeral().await?;
                ctx.say("Not an official track!").await?;
                return Ok(());
            } else if !(1..=13).contains(&track.parse::<usize>().unwrap()) {
                ctx.defer_ephemeral().await?;
                ctx.say("Not an official track!").await?;
                return Ok(());
            }
            let track_ids: Vec<String> = tokio::fs::read_to_string("official_tracks.txt")
                .await?
                .lines()
                .map(|s| s.to_string())
                .collect();
            let track_id = track_ids.get(track.parse::<usize>().unwrap() - 1).unwrap();
            url = format!("https://vps.kodub.com:43273/leaderboard?version=0.4.0&trackId={}&skip=0&amount=500&onlyVerified=false&userTokenHash={}",
            track_id,
            id);
        } else {
            url = format!("https://vps.kodub.com:43273/leaderboard?version=0.4.0&trackId={}&skip=0&amount=500&onlyVerified=false&userTokenHash={}",
            track,
            id);
        }
        let contents: Vec<String>;
        if let Ok(response) = client.get(url).send().await {
            if let Ok(body) = response.text().await {
                if let Ok(json) = serde_json::from_str::<Value>(&body) {
                    if let Some(user_entry) = json.get("userEntry") {
                        if let Some(position) = user_entry.get("position") {
                            if let Some(frames) = user_entry.get("frames") {
                                if position.to_string().parse::<u32>().unwrap() <= 501 {
                                    if let Some(entries) = json["entries"].as_array() {
                                        let mut found: Vec<String> = Vec::new();
                                        let mut i = 0;
                                        for entry in entries {
                                            i += 1;
                                            if i == position.to_string().parse::<u32>().unwrap() {
                                                break;
                                            }
                                            if !found
                                                .contains(&entry.get("name").unwrap().to_string())
                                                && entry
                                                    .get("verifiedState")
                                                    .unwrap()
                                                    .as_bool()
                                                    .unwrap_or_else(|| false)
                                            {
                                                found.push(entry.get("name").unwrap().to_string());
                                            }
                                        }
                                        let mut time = (frames.to_string().parse::<f64>().unwrap()
                                            / 1000.0)
                                            .to_string();
                                        time.push_str("s");
                                        contents = vec![
                                            position.to_string(),
                                            time,
                                            (found.len() + 1).to_string(),
                                        ];
                                        write_embed(
                                            &ctx,
                                            format!("Leaderboard"),
                                            format!(""),
                                            vec!["Ranking", "Time", "Unique"],
                                            contents,
                                            vec![true, true, true],
                                        )
                                        .await?;
                                    }
                                } else {
                                    let mut time = (frames.to_string().parse::<f64>().unwrap()
                                        / 1000.0)
                                        .to_string();
                                    time.push_str("s");
                                    contents = vec![position.to_string(), time];
                                    write_embed(
                                        &ctx,
                                        format!("Leaderboard"),
                                        format!(""),
                                        vec!["Ranking", "Time"],
                                        contents,
                                        vec![true, true],
                                    )
                                    .await?;
                                }
                            }
                        } else {
                            write(&ctx, format!("`Record not found!`")).await?;
                        }
                    }
                } else {
                    write(
                        &ctx,
                        format!("`Leaderboard servers could not be accessed.`"),
                    )
                    .await?;
                    return Ok(());
                }
            }
        }
    } else {
        write(&ctx, format!("`User ID not found`")).await?;
    }
    Ok(())
}

/// List standard track records for a user
#[poise::command(slash_command, prefix_command, category = "Query")]
async fn list(
    ctx: Context<'_>,
    #[description = "User"]
    #[autocomplete = "autocomplete_users"]
    user: String,
    #[description = "Hidden"] hidden: Option<bool>,
) -> Result<(), Error> {
    if hidden.is_some_and(|x| x) {
        ctx.defer_ephemeral().await?;
    } else {
        ctx.defer().await?;
    }
    let mut id = String::new();
    if let Some(id_test) = ctx.data().user_ids.lock().unwrap().get(&user) {
        id = id_test.clone();
    }
    if id.len() > 0 {
        let client = reqwest::Client::new();
        let mut line_num: u32 = 1;
        let mut total_time = 0.0;
        let mut display_total = true;
        let track_ids: Vec<String> = tokio::fs::read_to_string("official_tracks.txt")
            .await?
            .lines()
            .map(|s| s.to_string())
            .collect();
        let futures = track_ids.into_iter().enumerate().map(|(i, track_id)| {
            let client = client.clone();
            let url = format!("https://vps.kodub.com:43273/leaderboard?version=0.4.0&trackId={}&skip=0&amount=500&onlyVerified=false&userTokenHash={}",
            track_id,
            id);
            task::spawn(
            async move {
                let res = client.get(&url).send().await.unwrap().text().await.unwrap();
                return Ok::<(usize, String), reqwest::Error>((i, res));
            })
        });
        let mut results: Vec<(usize, String)> = join_all(futures)
            .await
            .into_iter()
            .map(|res| res.unwrap())
            .filter_map(|res| res.ok())
            .collect();
        results.sort_by_key(|(i, _)| *i);
        let responses: Vec<String> = results.into_iter().map(|(_, res)| res).collect();
        let mut contents: Vec<String> = vec![String::new(), String::new(), String::new()];
        let mut headers = vec!["Track", "Ranking", "Time"];
        let mut inlines = vec![true, true, true];
        for response in responses {
            if let Ok(json) = serde_json::from_str::<Value>(&response) {
                if let Some(user_entry) = json.get("userEntry") {
                    if let Some(position) = user_entry.get("position") {
                        if let Some(frames) = user_entry.get("frames") {
                            if position.to_string().parse::<u32>().unwrap() <= 501 {
                                if let Some(entries) = json["entries"].as_array() {
                                    let mut found: Vec<String> = Vec::new();
                                    let mut i = 0;
                                    for entry in entries {
                                        i += 1;
                                        if i == position.to_string().parse::<u32>().unwrap() {
                                            break;
                                        }
                                        if !found.contains(&entry.get("name").unwrap().to_string())
                                            && entry
                                                .get("verifiedState")
                                                .unwrap()
                                                .as_bool()
                                                .unwrap_or_else(|| false)
                                        {
                                            found.push(entry.get("name").unwrap().to_string());
                                        }
                                    }
                                    let time = frames.to_string().parse::<f64>().unwrap() / 1000.0;
                                    total_time += time;
                                    let mut time = time.to_string();
                                    time.push_str("s");
                                    contents[0].push_str(format!("{}\n", line_num).as_str());
                                    contents[1].push_str(
                                        format!(
                                            "{} [{}]\n",
                                            position.to_string(),
                                            (found.len() + 1).to_string()
                                        )
                                        .as_str(),
                                    );
                                    contents[2].push_str(format!("{}\n", time).as_str());
                                }
                            } else {
                                let time = frames.to_string().parse::<f64>().unwrap() / 1000.0;
                                total_time += time;
                                let mut time = time.to_string();
                                time.push_str("s");
                                contents[0].push_str(format!("{}\n", line_num).as_str());
                                contents[1]
                                    .push_str(format!("{}\n", position.to_string()).as_str());
                                contents[2].push_str(format!("{}\n", time).as_str());
                            }
                        }
                    } else {
                        display_total = false;
                    }
                }
            } else {
                write(
                    &ctx,
                    format!("`Leaderboard servers could not be accessed or user is not valid.`"),
                )
                .await?;
                return Ok(());
            }
            line_num += 1;
        }
        if display_total {
            let total_time = (total_time * 1000.0) as u32;
            contents.push(format!(
                "{:>2}:{:0>2}.{:0>3}",
                total_time / 60000,
                total_time & 60000 / 1000,
                total_time & 1000
            ));
            headers.push("Total");
            inlines.push(false);
        }
        write_embed(
            &ctx,
            format!("Record list"),
            format!(""),
            headers,
            contents,
            inlines,
        )
        .await?;
    } else {
        write(&ctx, format!("`User ID not found`")).await?;
    }
    Ok(())
}

/// Compares two user's record times and placements
#[poise::command(slash_command, prefix_command, category = "Query")]
async fn compare(
    ctx: Context<'_>,
    #[description = "User 1"]
    #[autocomplete = "autocomplete_users"]
    user1: String,
    #[description = "User 2"]
    #[autocomplete = "autocomplete_users"]
    user2: String,
    #[description = "Hidden"] hidden: Option<bool>,
) -> Result<(), Error> {
    if hidden.is_some_and(|x| x) {
        ctx.defer_ephemeral().await?;
    } else {
        ctx.defer().await?;
    }
    let mut results: Vec<Vec<(u32, f64)>> = Vec::new();
    for user in vec![user1.clone(), user2.clone()] {
        let mut user_results: Vec<(u32, f64)> = Vec::new();
        let mut id = String::new();
        if let Some(id_test) = ctx.data().user_ids.lock().unwrap().get(&user) {
            id = id_test.clone();
        }
        if id.len() > 0 {
            let client = reqwest::Client::new();
            let mut total_time = 0.0;
            let mut display_total = true;
            let track_ids: Vec<String> = tokio::fs::read_to_string("official_tracks.txt")
                .await?
                .lines()
                .map(|s| s.to_string())
                .collect();
            let futures = track_ids.into_iter().enumerate().map(|(i, track_id)| {
            let client = client.clone();
            let url = format!("https://vps.kodub.com:43273/leaderboard?version=0.4.0&trackId={}&skip=0&amount=1&onlyVerified=false&userTokenHash={}",
            track_id,
            id);
            task::spawn(
            async move {
                let res = client.get(&url).send().await.unwrap().text().await.unwrap();
                return Ok::<(usize, String), reqwest::Error>((i, res));
            })
        });
            let mut results: Vec<(usize, String)> = join_all(futures)
                .await
                .into_iter()
                .map(|res| res.unwrap())
                .filter_map(|res| res.ok())
                .collect();
            results.sort_by_key(|(i, _)| *i);
            let responses: Vec<String> = results.into_iter().map(|(_, res)| res).collect();
            for response in responses {
                if let Ok(json) = serde_json::from_str::<Value>(&response) {
                    if let Some(user_entry) = json.get("userEntry") {
                        if let Some(position) = user_entry.get("position") {
                            if let Some(frames) = user_entry.get("frames") {
                                let time = frames.to_string().parse::<f64>().unwrap() / 1000.0;
                                user_results.push((position.to_string().parse()?, time));
                                total_time += time;
                            }
                        } else {
                            user_results.push((0, 0.0));
                            display_total = false;
                        }
                    }
                } else {
                    write(
                        &ctx,
                        format!("`Leaderboard servers could not be accessed.`"),
                    )
                    .await?;
                    return Ok(());
                }
            }
            if display_total {
                let total_time = total_time * 1000.0;
                user_results.push((0, total_time));
            } else {
                user_results.push((0, 0.0));
            }
        } else {
            write(&ctx, format!("`User ID not found`")).await?;
        }
        results.push(user_results);
    }
    let mut output = String::new();
    let mut display_total_diff = true;
    output.push_str("```\n    ");
    for user in vec![user1.clone(), user2.clone()] {
        output.push_str(format!("{:<21}", user).as_str());
    }
    output.push_str("Difference\n");
    for i in 0..results[0].len() - 1 {
        let mut display_diff = true;
        output.push_str(format!("{:>2}: ", i + 1).as_str());
        for track in &results {
            if track[i].1 != 0.0 {
                output.push_str(
                    format!("{:>6}. - {:3.3}s{}", track[i].0, track[i].1, " ".repeat(4)).as_str(),
                );
            } else {
                output.push_str(format!("{:>17}{}", "Record not found", " ".repeat(4)).as_str());
                display_diff = false;
            }
        }
        if display_diff {
            output.push_str(format!("{:>9.3}s", (results[0][i].1 - results[1][i].1)).as_str());
        }
        output.push_str("\n");
    }
    output.push_str("\nTotal:");
    for track in &results {
        let total = track[13].1 as u32;
        if total != 0 {
            output.push_str(
                format!(
                    "{}{:>2}:{:0>2}.{:0>3}{}",
                    " ".repeat(6),
                    total / 60000,
                    total % 60000 / 1000,
                    total % 1000,
                    " ".repeat(6)
                )
                .as_str(),
            );
        } else {
            output.push_str(format!("{}Tracks not done", " ".repeat(0)).as_str());
            display_total_diff = false
        }
    }
    if display_total_diff {
        output.push_str(
            format!("{:>7.3}s", ((results[0][13].1 - results[1][13].1) / 1000.0)).as_str(),
        );
    }
    output.push_str("\n```");
    write(&ctx, output).await?;
    Ok(())
}

/// Update leaderboard for official tracks
///
/// displays users with top (500 * entry_requirement) records on all tracks (default: 2500)
#[poise::command(
    slash_command,
    prefix_command,
    check = "is_owner",
    category = "Administration"
)]
async fn update_rankings(
    ctx: Context<'_>,
    #[description = "Ranking multiple of 500 that is needed on all tracks to enter"]
    entry_requirement: Option<usize>,
) -> Result<(), Error> {
    ctx.defer_ephemeral().await?;
    rankings_update(entry_requirement).await?;
    let headers: Vec<&str> = vec!["Ranking", "Time", "Player"];
    let mut contents: Vec<String> = vec![String::new(), String::new(), String::new()];
    for line in tokio::fs::read_to_string(RANKINGS_FILE)
        .await?
        .lines()
        .map(|s| s.splitn(3, " - ").collect::<Vec<&str>>())
    {
        for i in 0..contents.len() {
            contents
                .get_mut(i)
                .unwrap()
                .push_str(format!("{}\n", line.get(i).unwrap()).as_str());
        }
    }
    let inlines: Vec<bool> = vec![true, true, true];
    write_embed(
        &ctx,
        format!("Global Leaderboard"),
        format!(""),
        headers,
        contents,
        inlines,
    )
    .await?;
    Ok(())
}

async fn rankings_update(entry_requirement: Option<usize>) -> Result<(), Error> {
    dotenv::dotenv().ok();
    let id = env::var("LEADERBOARD_ID").expect("Expected OWNER_ID in env!");
    let lb_size = entry_requirement.unwrap_or_else(|| {
        env::var("LEADERBOARD_SIZE")
            .expect("Expected LEADERBOARD_SIZE in env!")
            .parse()
            .expect("LEADERBOARD_SIZE not a valid integer!")
    });
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

/// Leaderboard for official tracks (very much WIP)
#[poise::command(slash_command, prefix_command, category = "Query")]
async fn rankings(
    ctx: Context<'_>,
    #[description = "Hidden"] hidden: Option<bool>,
) -> Result<(), Error> {
    if hidden.is_some_and(|x| x) {
        ctx.defer_ephemeral().await?;
    } else {
        ctx.defer().await?;
    }
    if tokio::fs::try_exists(RANKINGS_FILE).await? {
        let age = tokio::fs::metadata(RANKINGS_FILE)
            .await?
            .modified()?
            .elapsed()?;
        if age > MAX_RANKINGS_AGE {
            rankings_update(None).await?;
        }
    } else {
        rankings_update(None).await?;
    }
    let headers: Vec<&str> = vec!["Ranking", "Time", "Player"];
    let mut contents: Vec<String> = vec![String::new(), String::new(), String::new()];

    for line in tokio::fs::read_to_string(RANKINGS_FILE)
        .await?
        .lines()
        .map(|s| s.splitn(3, " - ").collect::<Vec<&str>>())
    {
        for i in 0..contents.len() {
            contents
                .get_mut(i)
                .unwrap()
                .push_str(format!("{}\n", line.get(i).unwrap()).as_str());
        }
    }
    let inlines: Vec<bool> = vec![true, true, true];
    write_embed(
        &ctx,
        format!("Global Leaderboard"),
        format!(""),
        headers,
        contents,
        inlines,
    )
    .await?;
    Ok(())
}

/// Lists guilds the bot is in (bot-admin only)
#[poise::command(
    slash_command,
    prefix_command,
    check = "is_owner",
    category = "Administration",
    ephemeral
)]
async fn guilds(ctx: Context<'_>) -> Result<(), Error> {
    let guilds = ctx.http().get_guilds(None, None).await?;
    let guild_names = guilds
        .iter()
        .map(|g| g.name.clone())
        .collect::<Vec<_>>()
        .join("\n");
    write_embed(
        &ctx,
        format!("Guilds"),
        format!(""),
        vec!["Guild name"],
        vec![guild_names],
        vec![true],
    )
    .await?;
    Ok(())
}

/// Saves data to disk (bot-admin only)
#[poise::command(
    slash_command,
    prefix_command,
    check = "is_owner",
    category = "Administration",
    ephemeral
)]
async fn save(ctx: Context<'_>) -> Result<(), Error> {
    ctx.defer_ephemeral().await?;
    let bot_data = ctx.data();
    bot_data.save_to_file(USER_FILE).await.unwrap();
    write(&ctx, format!("`User IDs saved.`")).await?;
    Ok(())
}

/// Loads data from disk (bot-admin only)
#[poise::command(
    slash_command,
    prefix_command,
    check = "is_owner",
    category = "Administration",
    ephemeral
)]
async fn load(ctx: Context<'_>) -> Result<(), Error> {
    let user_ids = BotData::load_from_file(USER_FILE)
        .await
        .unwrap_or_else(|_| BotData {
            user_ids: Mutex::new(HashMap::new()),
        })
        .user_ids;
    ctx.data().user_ids.lock().unwrap().clear();
    for (user, id) in user_ids.lock().unwrap().iter() {
        ctx.data()
            .user_ids
            .lock()
            .unwrap()
            .insert(user.to_string(), id.to_string());
    }
    write(&ctx, format!("`User IDs loaded.`")).await?;
    Ok(())
}

/// Lists currently registered users and their IDs
#[poise::command(slash_command, prefix_command, category = "Info", ephemeral)]
async fn users(ctx: Context<'_>) -> Result<(), Error> {
    let bot_data = ctx.data();
    let mut users = String::new();
    for (user, id) in bot_data.user_ids.lock().unwrap().iter() {
        users.push_str(format!("{}: {}\n", user, id).as_str());
    }
    write(&ctx, format!("```{}```", users)).await?;
    Ok(())
}

/// Displays help
#[poise::command(slash_command, prefix_command, track_edits, category = "Info")]
async fn help(
    ctx: Context<'_>,
    #[description = "Command"] cmd: Option<String>,
) -> Result<(), Error> {
    let config = poise::builtins::HelpConfiguration {
        extra_text_at_bottom: "\
            Type /help <cmd> for more detailed help.",
        ephemeral: true,
        ..Default::default()
    };
    poise::builtins::help(ctx, cmd.as_deref(), config).await?;
    Ok(())
}

#[tokio::main]
async fn main() {
    dotenv::dotenv().ok();
    let token = env::var("DISCORD_TOKEN").expect("Token missing");
    let intents =
        serenity::GatewayIntents::non_privileged() | serenity::GatewayIntents::GUILD_MEMBERS;

    let bot_data = BotData::load_from_file(USER_FILE)
        .await
        .unwrap_or_else(|_| BotData {
            user_ids: Mutex::new(HashMap::new()),
        });

    let framework = poise::Framework::builder()
        .options(poise::FrameworkOptions {
            commands: vec![
                assign(),
                delete(),
                request(),
                list(),
                guilds(),
                save(),
                load(),
                users(),
                help(),
                compare(),
                update_rankings(),
                rankings(),
            ],
            prefix_options: poise::PrefixFrameworkOptions {
                prefix: Some("~".into()),
                edit_tracker: Some(Arc::new(poise::EditTracker::for_timespan(
                    Duration::from_secs(60),
                ))),
                additional_prefixes: vec![poise::Prefix::Literal("'")],
                ..Default::default()
            },
            pre_command: |ctx| {
                Box::pin(async move {
                    println!(
                        "Executing command {} issued by {}...",
                        ctx.command().qualified_name,
                        ctx.author().display_name()
                    );
                })
            },
            post_command: |ctx| {
                Box::pin(async move {
                    println!(
                        "Executed command {} issued by {}!",
                        ctx.command().qualified_name,
                        ctx.author().display_name()
                    );
                })
            },
            ..Default::default()
        })
        .setup(|ctx, _ready, framework| {
            Box::pin(async move {
                poise::builtins::register_globally(ctx, &framework.options().commands).await?;
                Ok(bot_data)
            })
        })
        .build();

    let client = serenity::ClientBuilder::new(token, intents)
        .framework(framework)
        .await;
    client.unwrap().start().await.unwrap();
}

async fn is_owner(ctx: Context<'_>) -> Result<bool, Error> {
    dotenv::dotenv().ok();
    let owner_id = env::var("OWNER_ID").expect("Expected owner ID in environment");
    Ok(ctx.author().name == owner_id)
}

async fn autocomplete_users(ctx: Context<'_>, partial: &str) -> Vec<String> {
    let user_ids = ctx.data().user_ids.lock().unwrap();
    if user_ids.keys().filter(|k| k.starts_with(partial)).count() > 0 {
        return user_ids
            .keys()
            .filter(|k| k.starts_with(partial))
            .cloned()
            .collect();
    } else if user_ids.keys().filter(|k| k.contains(partial)).count() > 0 {
        return user_ids
            .keys()
            .filter(|k| k.contains(partial))
            .cloned()
            .collect();
    } else if user_ids
        .keys()
        .filter(|k| k.to_lowercase().starts_with(&partial.to_lowercase()))
        .count()
        > 0
    {
        return user_ids
            .keys()
            .filter(|k| k.to_lowercase().starts_with(&partial.to_lowercase()))
            .cloned()
            .collect();
    } else if user_ids
        .keys()
        .filter(|k| k.to_lowercase().contains(&partial.to_lowercase()))
        .count()
        > 0
    {
        return user_ids
            .keys()
            .filter(|k| k.to_lowercase().contains(&partial.to_lowercase()))
            .cloned()
            .collect();
    } else {
        return user_ids
            .keys()
            .filter(|key| key.to_lowercase().starts_with(&partial.to_lowercase()))
            .cloned()
            .collect();
    }
}
