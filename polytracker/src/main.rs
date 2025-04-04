use diesel::prelude::*;
use dotenvy::dotenv;
use poise::builtins;
use poise::serenity_prelude as serenity;
use poise::{
    ApplicationContext, ChoiceParameter, CommandParameterChoice, CreateReply, EditTracker,
    Framework, FrameworkOptions, Modal, Prefix, PrefixFrameworkOptions,
};
use polymanager::community_update;
use polymanager::db::establish_connection;
use polymanager::db::{Admin, BetaUser, NewBetaUser, NewUser, User};
use polymanager::global_rankings_update;
use polymanager::hof_update;
use polymanager::COMMUNITY_RANKINGS_FILE;
use polymanager::{
    ALT_ACCOUNT_FILE, BETA_RANKINGS_FILE, BLACKLIST_FILE, HOF_ALT_ACCOUNT_FILE, HOF_BLACKLIST_FILE,
    HOF_RANKINGS_FILE, RANKINGS_FILE, TRACK_FILE,
};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use serenity::collector::ComponentInteractionCollector;
use serenity::futures::future::join_all;
use serenity::{
    ClientBuilder, Color, CreateActionRow, CreateAttachment, CreateButton, CreateEmbed,
    CreateInteractionResponse, CreateInteractionResponseMessage, GatewayIntents,
};
use std::env;
use std::sync::{Arc, Mutex};
use std::{collections::HashMap, time::Duration};
use tokio::{fs, task};

const BETA_TRACK_FILE: &str = "lists/0.5_official_tracks.txt";
const MAX_RANKINGS_AGE: Duration = Duration::from_secs(60 * 10);
const MAX_MSG_AGE: Duration = Duration::from_secs(60 * 10);
const BETA_VERSION: &str = "0.5.0-beta5";
const VERSION: &str = "0.4.2";

struct BotData {
    user_ids: Mutex<HashMap<String, String>>,
    beta_user_ids: Mutex<HashMap<String, String>>,
    admins: Mutex<HashMap<String, u32>>,
    conn: Mutex<SqliteConnection>,
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
    total: u32,
}

impl BotData {
    pub async fn load(&self) {
        use polymanager::schema::admins::dsl::*;
        use polymanager::schema::beta_users::dsl::*;
        use polymanager::schema::users::dsl::*;
        let connection = &mut *self.conn.lock().unwrap();
        let results = users
            .select(User::as_select())
            .load(connection)
            .expect("Error loading users");
        let mut user_ids = self.user_ids.lock().unwrap();
        user_ids.clear();
        for user in results {
            user_ids.insert(user.name, user.game_id);
        }
        // beta (temporary)
        let results = beta_users
            .select(BetaUser::as_select())
            .load(connection)
            .expect("Error loading users");
        let mut beta_user_ids = self.beta_user_ids.lock().unwrap();
        beta_user_ids.clear();
        for beta_user in results {
            beta_user_ids.insert(beta_user.name, beta_user.game_id);
        }
        // end of beta
        let results = admins
            .select(Admin::as_select())
            .load(connection)
            .expect("Error loading users");
        let mut admin_ids = self.admins.lock().unwrap();
        admin_ids.clear();
        for admin in results {
            admin_ids.insert(admin.discord, admin.privilege as u32);
        }
    }
    pub async fn add(&self, name: &str, game_id: &str) {
        use polymanager::schema::users;
        let connection = &mut *self.conn.lock().unwrap();
        let new_user = NewUser {
            name,
            game_id,
            discord: None,
        };
        diesel::insert_into(users::table)
            .values(&new_user)
            .returning(User::as_returning())
            .get_result(connection)
            .expect("Error saving new user");
    }
    // beta (temporary)
    pub async fn beta_add(&self, name: &str, game_id: &str) {
        use polymanager::schema::beta_users;
        let connection = &mut *self.conn.lock().unwrap();
        let new_beta_user = NewBetaUser {
            name,
            game_id,
            discord: None,
        };
        diesel::insert_into(beta_users::table)
            .values(&new_beta_user)
            .returning(BetaUser::as_returning())
            .get_result(connection)
            .expect("Error saving new user");
    }
    // end of beta
    pub async fn delete(&self, delete_name: &str) {
        use polymanager::schema::users::dsl::*;
        let connection = &mut *self.conn.lock().unwrap();
        diesel::delete(users.filter(name.eq(delete_name)))
            .execute(connection)
            .expect("Error deleting user");
    }
    // beta (temporary)
    pub async fn beta_delete(&self, delete_name: &str) {
        use polymanager::schema::beta_users::dsl::*;
        let connection = &mut *self.conn.lock().unwrap();
        diesel::delete(beta_users.filter(name.eq(delete_name)))
            .execute(connection)
            .expect("Error deleting user");
    }
    // end of beta
}

#[derive(Modal, Clone)]
#[name = "List Editor"]
struct EditModal {
    #[name = "List"]
    #[paragraph]
    list: String,
}

#[derive(Clone)]
enum EditModalList {
    Black,
    Alt,
    HOFBlack,
    HOFAlt,
}

impl ChoiceParameter for EditModalList {
    fn list() -> Vec<CommandParameterChoice> {
        let names = vec!["Black List", "Alt List", "HOF Black List", "HOF Alt List"];
        names
            .into_iter()
            .map(|n| CommandParameterChoice {
                name: n.to_string(),
                localizations: HashMap::new(),
                __non_exhaustive: (),
            })
            .collect()
    }
    fn from_index(index: usize) -> Option<Self> {
        use EditModalList::*;
        let values = [Black, Alt, HOFBlack, HOFAlt];
        values.get(index).cloned()
    }
    fn localized_name(&self, _: &str) -> Option<&'static str> {
        Some(self.name())
    }
    fn from_name(name: &str) -> Option<Self> {
        use EditModalList::*;
        match name {
            "Black List" => Some(Black),
            "Alt List" => Some(Alt),
            "HOF Black List" => Some(HOFBlack),
            "HOF Alt List" => Some(HOFAlt),
            _ => None,
        }
    }
    fn name(&self) -> &'static str {
        use EditModalList::*;
        match self {
            Black => "Blacklist",
            Alt => "Alt-List",
            HOFBlack => "HOF Blacklist",
            HOFAlt => "HOF Alt-List",
        }
    }
}

async fn write(ctx: &Context<'_>, mut text: String) -> Result<(), Error> {
    if text.chars().count() > 2000 {
        if text.chars().next().unwrap() == text.chars().nth(1).unwrap()
            && text.chars().nth(1).unwrap() == text.chars().nth(2).unwrap()
            && text.chars().nth(2).unwrap() == '`'
        {
            for _ in 0..3 {
                text.remove(0);
                text.pop();
            }
        } else if text.starts_with('`') {
            text.remove(0);
            text.pop();
        }
        let file = CreateAttachment::bytes(text.as_bytes(), "polytracker.txt");
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
        dotenv()?;
        let ctx_id = ctx.id();
        let prev_id = format!("{}prev", ctx_id);
        let next_id = format!("{}next", ctx_id);
        let start_id = format!("{}start", ctx_id);
        let mut pages: Vec<Vec<String>> = Vec::new();
        for content in contents {
            pages.push(
                content
                    .lines()
                    .collect::<Vec<&str>>()
                    .chunks(20)
                    .map(|chunk| chunk.join("\n"))
                    .collect(),
            );
        }
        let fields = headers.clone().into_iter().enumerate().map(|(i, h)| {
            (
                h,
                pages.get(i).unwrap().first().unwrap().clone(),
                inlines[i],
            )
        });
        let embed = CreateEmbed::default()
            .title(title.clone())
            .description(description.clone())
            .fields(fields.clone())
            .color(Color::BLITZ_BLUE);
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
        ctx.send(reply.clone()).await?;
        let mut current_page = 0;
        while let Some(press) = ComponentInteractionCollector::new(ctx)
            .filter(move |press| press.data.custom_id.starts_with(&ctx_id.to_string()))
            .timeout(MAX_MSG_AGE)
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
            let embed = CreateEmbed::default()
                .title(&title)
                .description(&description)
                .fields(fields)
                .color(Color::BLITZ_BLUE);

            press
                .create_response(
                    ctx.serenity_context(),
                    CreateInteractionResponse::UpdateMessage(
                        CreateInteractionResponseMessage::new().embed(embed),
                    ),
                )
                .await?;
        }
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
    #[description = "Beta version"] beta: Option<bool>,
) -> Result<(), Error> {
    ctx.defer_ephemeral().await?;
    let beta = beta.unwrap_or(false);
    let mut user_id = id;
    if user_id.starts_with("User ID: ") {
        user_id = user_id.trim_start_matches("User ID: ").to_string();
    }
    if if beta {
        ctx.data().beta_user_ids.lock().unwrap().contains_key(&user)
    } else {
        ctx.data().user_ids.lock().unwrap().contains_key(&user)
    } {
        let response = format!(
            "`User '{}' is already assigned an ID, to reassign please contact this bot's owner`",
            user
        );
        write(&ctx, response).await?;
        return Ok(());
    }
    let response = format!("`Added user '{}' with ID '{}'`", user, user_id);
    if beta {
        ctx.data()
            .beta_user_ids
            .lock()
            .unwrap()
            .insert(user.clone(), user_id.clone());
        ctx.data().beta_add(user.as_str(), user_id.as_str()).await;
    } else {
        ctx.data()
            .user_ids
            .lock()
            .unwrap()
            .insert(user.clone(), user_id.clone());
        ctx.data().add(user.as_str(), user_id.as_str()).await;
    }
    write(&ctx, response).await?;
    Ok(())
}

/// Delete an already assigned username-ID pair (bot-admin only)
///
/// Only deletes the data from the bot, you game account stays intact.
#[poise::command(slash_command, prefix_command, category = "Administration")]
async fn delete(
    ctx: Context<'_>,
    #[description = "Username"]
    #[autocomplete = "autocomplete_users"]
    mut user: String,
    #[description = "Beta version"] beta: Option<bool>,
) -> Result<(), Error> {
    ctx.defer_ephemeral().await?;
    let (is_admin, is_admin_msg) = is_admin(&ctx, 1).await;
    if !is_admin {
        write(&ctx, is_admin_msg).await?;
        return Ok(());
    }
    let beta = beta.unwrap_or(false);
    if beta {
        user = user.trim_end_matches(" (BETA)").to_string();
    }
    let bot_data = ctx.data();
    let response;
    if if beta {
        bot_data.beta_user_ids.lock().unwrap().contains_key(&user)
    } else {
        bot_data.user_ids.lock().unwrap().contains_key(&user)
    } {
        let id = if beta {
            bot_data
                .beta_user_ids
                .lock()
                .unwrap()
                .remove(&user)
                .unwrap()
        } else {
            bot_data.user_ids.lock().unwrap().remove(&user).unwrap()
        };
        if beta {
            ctx.data().beta_delete(user.as_str()).await;
        } else {
            ctx.data().delete(user.as_str()).await;
        }
        response = format!(
            "`Removed user '{}' with ID '{}'{}`",
            user,
            id,
            if beta { " from beta users" } else { "" }
        );
    } else {
        response = "`User not found!`".to_string();
    }
    write(&ctx, response).await?;
    Ok(())
}

/// Request data about a track
///
/// Choose between standard tracks (off=True) or custom tracks (off=False).
/// For standard tracks use the track number (1-13).
/// For custom tracks use the track ID.
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
    if !id.is_empty() {
        let client = Client::new();
        let url;
        if off {
            if track.parse::<usize>().is_err()
                || !(1..=13).contains(&track.parse::<usize>().unwrap())
            {
                ctx.defer_ephemeral().await?;
                ctx.say("Not an official track!").await?;
                return Ok(());
            }
            let track_ids: Vec<String> = fs::read_to_string(TRACK_FILE)
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
                                                    .unwrap_or(false)
                                            {
                                                found.push(entry.get("name").unwrap().to_string());
                                            }
                                        }
                                        let mut time = (frames.to_string().parse::<f64>().unwrap()
                                            / 1000.0)
                                            .to_string();
                                        time.push('s');
                                        contents = vec![
                                            position.to_string(),
                                            time,
                                            (found.len() + 1).to_string(),
                                        ];
                                        write_embed(
                                            &ctx,
                                            "Leaderboard".to_string(),
                                            String::new(),
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
                                    time.push('s');
                                    contents = vec![position.to_string(), time];
                                    write_embed(
                                        &ctx,
                                        "Leaderboard".to_string(),
                                        String::new(),
                                        vec!["Ranking", "Time"],
                                        contents,
                                        vec![true, true],
                                    )
                                    .await?;
                                }
                            }
                        } else {
                            write(&ctx, "`Record not found!`".to_string()).await?;
                        }
                    }
                } else {
                    write(
                        &ctx,
                        "`Leaderboard servers could not be accessed.`".to_string(),
                    )
                    .await?;
                    return Ok(());
                }
            }
        }
    } else {
        write(&ctx, "`User ID not found`".to_string()).await?;
    }
    Ok(())
}

/// List standard track records for a user
#[poise::command(slash_command, prefix_command, category = "Query")]
async fn list(
    ctx: Context<'_>,
    #[description = "User"]
    #[autocomplete = "autocomplete_users"]
    mut user: String,
    #[description = "Beta version"] beta: Option<bool>,
    #[description = "Hidden"] hidden: Option<bool>,
) -> Result<(), Error> {
    if hidden.is_some_and(|x| x) {
        ctx.defer_ephemeral().await?;
    } else {
        ctx.defer().await?;
    }
    let beta = beta.unwrap_or(false);
    if beta {
        user = user.trim_end_matches(" (BETA)").to_string();
    }
    let mut id = String::new();
    if beta {
        if let Some(id_test) = ctx.data().beta_user_ids.lock().unwrap().get(&user) {
            id = id_test.clone();
        }
    } else if let Some(id_test) = ctx.data().user_ids.lock().unwrap().get(&user) {
        id = id_test.clone();
    }
    if !id.is_empty() {
        let client = Client::new();
        let mut line_num: u32 = 0;
        let mut total_time = 0.0;
        let mut display_total = true;
        let track_ids: Vec<(String, String)> =
            fs::read_to_string(if beta { BETA_TRACK_FILE } else { TRACK_FILE })
                .await?
                .lines()
                .map(|s| {
                    let mut parts = s.splitn(2, " ");
                    (
                        parts.next().unwrap().to_string(),
                        parts.next().unwrap().to_string(),
                    )
                })
                .collect();
        let futures = track_ids.clone().into_iter().enumerate().map(|(i, track_id)| {
            let client = client.clone();
            let url = format!("https://vps.kodub.com:{}/leaderboard?version={}&trackId={}&skip=0&amount=500&onlyVerified=false&userTokenHash={}",
            if beta {43274} else {43273},
            if beta {BETA_VERSION} else {VERSION},
            track_id.0,
            id);
            task::spawn(
            async move {
                let res = client.get(&url).send().await.unwrap().text().await.unwrap();
                Ok::<(usize, String), reqwest::Error>((i, res))
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
                                        if beta {
                                            if !found
                                                .contains(&entry.get("name").unwrap().to_string())
                                                && matches!(
                                                    entry
                                                        .get("verifiedState")
                                                        .unwrap()
                                                        .as_u64()
                                                        .unwrap(),
                                                    1
                                                )
                                            {
                                                found.push(entry.get("name").unwrap().to_string());
                                            }
                                        } else if !found
                                            .contains(&entry.get("name").unwrap().to_string())
                                            && entry
                                                .get("verifiedState")
                                                .unwrap()
                                                .as_bool()
                                                .unwrap_or(false)
                                        {
                                            found.push(entry.get("name").unwrap().to_string());
                                        }
                                    }
                                    let time = frames.to_string().parse::<f64>().unwrap() / 1000.0;
                                    total_time += time;
                                    let mut time = time.to_string();
                                    time.push('s');
                                    contents[0].push_str(
                                        format!("{}\n", track_ids[line_num as usize].1).as_str(),
                                    );
                                    contents[1].push_str(
                                        format!("{} [{}]\n", position, (found.len() + 1)).as_str(),
                                    );
                                    contents[2].push_str(format!("{}\n", time).as_str());
                                }
                            } else {
                                let time = frames.to_string().parse::<f64>().unwrap() / 1000.0;
                                total_time += time;
                                let mut time = time.to_string();
                                time.push('s');
                                contents[0].push_str(
                                    format!("{}\n", track_ids[line_num as usize].1).as_str(),
                                );
                                contents[1].push_str(format!("{}\n", position).as_str());
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
                    "`Leaderboard servers could not be accessed or user is not valid.`".to_string(),
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
                total_time % 60000 / 1000,
                total_time % 1000
            ));
            headers.push("Total");
            inlines.push(false);
        }
        write_embed(
            &ctx,
            if beta {
                format!("{} (Beta)", user)
            } else {
                user
            },
            String::new(),
            headers,
            contents,
            inlines,
        )
        .await?;
    } else {
        write(&ctx, "`User ID not found`".to_string()).await?;
    }
    Ok(())
}

/// Compares two user's record times and placements
#[poise::command(slash_command, prefix_command, category = "Query")]
async fn compare(
    ctx: Context<'_>,
    #[description = "User 1"]
    #[autocomplete = "autocomplete_users"]
    mut user1: String,
    #[description = "User 2"]
    #[autocomplete = "autocomplete_users"]
    mut user2: String,
    #[description = "Beta version"] beta: Option<bool>,
    #[description = "Hidden"] hidden: Option<bool>,
) -> Result<(), Error> {
    if hidden.is_some_and(|x| x) {
        ctx.defer_ephemeral().await?;
    } else {
        ctx.defer().await?;
    }
    let beta = beta.unwrap_or(false);
    if beta {
        user1 = user1.trim_end_matches(" (BETA)").to_string();
        user2 = user2.trim_end_matches(" (BETA)").to_string();
    }
    let mut results: Vec<Vec<(u32, f64)>> = Vec::new();
    let track_ids: Vec<(String, String)> =
        fs::read_to_string(if beta { BETA_TRACK_FILE } else { TRACK_FILE })
            .await?
            .lines()
            .map(|s| {
                let mut parts = s.splitn(2, " ");
                (
                    parts.next().unwrap().to_string(),
                    parts.next().unwrap().to_string(),
                )
            })
            .collect();
    for user in [user1.clone(), user2.clone()] {
        let mut user_results: Vec<(u32, f64)> = Vec::new();
        let mut id = String::new();
        if beta {
            if let Some(id_test) = ctx.data().beta_user_ids.lock().unwrap().get(&user) {
                id = id_test.clone();
            }
        } else if let Some(id_test) = ctx.data().user_ids.lock().unwrap().get(&user) {
            id = id_test.clone();
        }
        if !id.is_empty() {
            let client = Client::new();
            let mut total_time = 0.0;
            let mut display_total = true;
            let futures = track_ids.clone().into_iter().enumerate().map(|(i, track_id)| {
            let client = client.clone();
            let url = format!("https://vps.kodub.com:{}/leaderboard?version={}&trackId={}&skip=0&amount=1&onlyVerified=false&userTokenHash={}",
            if beta {43274} else {43273},
            if beta {BETA_VERSION} else {VERSION},
            track_id.0,
            id);
            task::spawn(
            async move {
                let res = client.get(&url).send().await.unwrap().text().await.unwrap();
                Ok::<(usize, String), reqwest::Error>((i, res))
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
                        "`Leaderboard servers could not be accessed.`".to_string(),
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
            write(&ctx, "`User ID not found`".to_string()).await?;
        }
        results.push(user_results);
    }
    let mut output = String::new();
    let mut display_total_diff = true;
    output.push_str("```\n    ");
    for user in [user1.clone(), user2.clone()] {
        output.push_str(format!("{:<21}", user).as_str());
    }
    output.push_str("Difference\n");
    for i in 0..results[0].len() - 1 {
        let mut display_diff = true;
        output.push_str(format!("{}: ", track_ids[i].1).as_str());
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
        output.push('\n');
    }
    output.push_str("\nTotal:");
    for track in &results {
        let total = track.last().unwrap().1 as u32;
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
            format!(
                "{:>7.3}s",
                ((results[0].last().unwrap().1 - results[1].last().unwrap().1) / 1000.0)
            )
            .as_str(),
        );
    }
    output.push_str("\n```");
    write(&ctx, output).await?;
    Ok(())
}

#[derive(Clone)]
enum UpdateLeaderboard {
    Global,
    Beta,
    Community,
    Hof,
}

impl ChoiceParameter for UpdateLeaderboard {
    fn list() -> Vec<CommandParameterChoice> {
        use UpdateLeaderboard::*;
        [Global, Beta, Community, Hof]
            .iter()
            .map(|c| CommandParameterChoice {
                name: c.name().to_string(),
                localizations: HashMap::new(),
                __non_exhaustive: (),
            })
            .collect()
    }
    fn name(&self) -> &'static str {
        use UpdateLeaderboard::*;
        match self {
            Global => "Global",
            Beta => "Beta",
            Community => "Community",
            Hof => "HOF",
        }
    }
    fn from_index(index: usize) -> Option<Self> {
        use UpdateLeaderboard::*;
        [Global, Beta, Community, Hof].get(index).cloned()
    }
    fn localized_name(&self, _: &str) -> Option<&'static str> {
        Some(self.name())
    }
    fn from_name(name: &str) -> Option<Self> {
        use UpdateLeaderboard::*;
        match name.to_lowercase().as_str() {
            "global" => Some(Global),
            "beta" => Some(Beta),
            "community" => Some(Community),
            "hof" => Some(Hof),
            _ => None,
        }
    }
}

/// Update leaderboard for official tracks
///
/// displays users with top (500 * entry_requirement) records on all tracks (default: 2500)
#[poise::command(slash_command, prefix_command, category = "Administration")]
async fn update_rankings(
    ctx: Context<'_>,
    #[description = "Updated Leaderboard"] leaderboard: UpdateLeaderboard,
) -> Result<(), Error> {
    ctx.defer_ephemeral().await?;
    let (is_admin, is_admin_msg) = is_admin(&ctx, 2).await;
    if !is_admin {
        write(&ctx, is_admin_msg).await?;
        return Ok(());
    }
    use UpdateLeaderboard::*;
    match leaderboard {
        Global => global_rankings_update(false).await,
        Beta => global_rankings_update(true).await,
        Community => community_update().await,
        Hof => hof_update().await,
    }?;
    let headers: Vec<&str> = vec!["Ranking", "Time", "Player"];
    let mut contents: Vec<String> = vec![String::new(), String::new(), String::new()];
    for line in fs::read_to_string(match leaderboard {
        Global => RANKINGS_FILE,
        Beta => BETA_RANKINGS_FILE,
        Community => COMMUNITY_RANKINGS_FILE,
        Hof => HOF_RANKINGS_FILE,
    })
    .await?
    .lines()
    .filter(|s| !s.starts_with("<|-|>"))
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
        format!("{} Leaderboard", leaderboard.name()),
        String::new(),
        headers,
        contents,
        inlines,
    )
    .await?;
    Ok(())
}

/// Leaderboard for official tracks
#[poise::command(slash_command, prefix_command, category = "Query")]
async fn rankings(
    ctx: Context<'_>,
    #[description = "Beta version"] beta: Option<bool>,
    #[description = "Hidden"] hidden: Option<bool>,
) -> Result<(), Error> {
    if hidden.is_some_and(|x| x) {
        ctx.defer_ephemeral().await?;
    } else {
        ctx.defer().await?;
    }
    let beta = beta.unwrap_or(false);
    if fs::try_exists(if beta {
        BETA_RANKINGS_FILE
    } else {
        RANKINGS_FILE
    })
    .await?
    {
        let age = fs::metadata(if beta {
            BETA_RANKINGS_FILE
        } else {
            RANKINGS_FILE
        })
        .await?
        .modified()?
        .elapsed()?;
        if age > MAX_RANKINGS_AGE {
            global_rankings_update(beta).await?;
        }
    } else {
        global_rankings_update(beta).await?;
    }
    let headers: Vec<&str> = vec!["Ranking", "Time", "Player"];
    let mut contents: Vec<String> = vec![String::new(), String::new(), String::new()];

    for line in fs::read_to_string(if beta {
        BETA_RANKINGS_FILE
    } else {
        RANKINGS_FILE
    })
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
        if beta {
            "Beta Leaderboard"
        } else {
            "Global Leaderboard"
        }
        .to_string(),
        String::new(),
        headers,
        contents,
        inlines,
    )
    .await?;
    Ok(())
}

/// HOF leaderboard
#[poise::command(slash_command, prefix_command, category = "Query")]
async fn hof_rankings(
    ctx: Context<'_>,
    #[description = "Hidden"] hidden: Option<bool>,
) -> Result<(), Error> {
    if hidden.is_some_and(|x| x) {
        ctx.defer_ephemeral().await?;
    } else {
        ctx.defer().await?;
    }
    let headers: Vec<&str> = vec!["Ranking", "Points", "Player"];
    let mut contents: Vec<String> = vec![String::new(), String::new(), String::new()];

    for line in fs::read_to_string(HOF_RANKINGS_FILE)
        .await?
        .lines()
        .filter(|s| !s.starts_with("<|-|>"))
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
        "HOF Leaderboard".to_string(),
        String::new(),
        headers,
        contents,
        inlines,
    )
    .await?;
    Ok(())
}

/// Lists guilds the bot is in (bot-admin only)
#[poise::command(slash_command, prefix_command, category = "Administration", ephemeral)]
async fn guilds(ctx: Context<'_>) -> Result<(), Error> {
    let (is_admin, is_admin_msg) = is_admin(&ctx, 0).await;
    if !is_admin {
        write(&ctx, is_admin_msg).await?;
        return Ok(());
    }
    let guilds = ctx.http().get_guilds(None, None).await?;
    let guild_names = guilds
        .iter()
        .map(|g| g.name.clone())
        .collect::<Vec<_>>()
        .join("\n");
    let guild_icons = guilds
        .iter()
        .map(|g| {
            format!("[O](<{}>)", g.icon_url().unwrap_or_default())
                .trim_start_matches("[O](<>)")
                .to_string()
        })
        .collect::<Vec<_>>()
        .join("\n");
    write_embed(
        &ctx,
        "Guilds".to_string(),
        String::new(),
        vec!["Icon", "Name"],
        vec![guild_icons, guild_names],
        vec![true, true],
    )
    .await?;
    Ok(())
}

/// Lets privileged users edit certain internal data
#[poise::command(slash_command, category = "Administration", ephemeral)]
async fn edit_lists(
    ctx: ApplicationContext<'_, BotData, Error>,
    #[description = "List to edit"] list: EditModalList,
) -> Result<(), Error> {
    let (is_admin, is_admin_msg) = is_admin(&ctx.into(), 2).await;
    if !is_admin {
        write(&ctx.into(), is_admin_msg).await?;
        return Ok(());
    }
    let list_file = {
        use EditModalList::*;
        match list {
            Black => BLACKLIST_FILE,
            Alt => ALT_ACCOUNT_FILE,
            HOFBlack => HOF_BLACKLIST_FILE,
            HOFAlt => HOF_ALT_ACCOUNT_FILE,
        }
    };
    let list = fs::read_to_string(list_file).await?;
    let modal_defaults = EditModal { list };
    let modal_returned = EditModal::execute_with_defaults(ctx, modal_defaults.clone())
        .await?
        .unwrap_or(modal_defaults);
    fs::write(list_file, modal_returned.list).await.unwrap();
    Ok(())
}

/// Lists currently registered users and their IDs
#[poise::command(slash_command, prefix_command, category = "Info", ephemeral)]
async fn users(
    ctx: Context<'_>,
    #[description = "Beta version"] beta: Option<bool>,
) -> Result<(), Error> {
    let beta = beta.unwrap_or(false);
    let bot_data = ctx.data();
    let mut users = String::new();
    if beta {
        for (user, id) in bot_data.beta_user_ids.lock().unwrap().iter() {
            users.push_str(format!("{}: {}\n", user, id).as_str());
        }
    } else {
        for (user, id) in bot_data.user_ids.lock().unwrap().iter() {
            users.push_str(format!("{}: {}\n", user, id).as_str());
        }
    }
    write(&ctx, format!("```{}```", users)).await?;
    Ok(())
}

/// Displays player numbers
#[poise::command(slash_command, prefix_command, category = "Info")]
async fn players(
    ctx: Context<'_>,
    #[description = "Beta version"] beta: Option<bool>,
) -> Result<(), Error> {
    let beta = beta.unwrap_or(false);
    let track_ids: Vec<(String, String)> =
        fs::read_to_string(if beta { BETA_TRACK_FILE } else { TRACK_FILE })
            .await
            .unwrap()
            .lines()
            .map(|s| {
                let mut parts = s.splitn(2, " ").map(|s| s.to_string());
                (parts.next().unwrap(), parts.next().unwrap())
            })
            .collect();
    let mut contents = vec![String::new(), String::new()];
    let client = Client::new();
    for (id, name) in track_ids {
        let url = format!("https://vps.kodub.com:{}/leaderboard?version={}&trackId={}&skip=0&amount=1&onlyVerified=false",
            if beta {43274} else {43273},
            if beta {BETA_VERSION} else {VERSION},
            id);
        let number =
            serde_json::from_str::<LeaderBoard>(&client.get(&url).send().await?.text().await?)?
                .total;
        contents
            .get_mut(0)
            .unwrap()
            .push_str(&format!("{}\n", name));
        contents
            .get_mut(1)
            .unwrap()
            .push_str(&format!("{}\n", number));
    }
    write_embed(
        &ctx,
        "Player numbers".to_string(),
        String::new(),
        vec!["Track", "Players"],
        contents,
        vec![true, true],
    )
    .await?;
    Ok(())
}

/// Links the privacy policy
#[poise::command(slash_command, prefix_command, category = "Info", ephemeral)]
async fn policy(ctx: Context<'_>) -> Result<(), Error> {
    dotenv().ok();
    let url = format!(
        "https://{}/policy",
        env::var("WEBSITE_URL").expect("Expected WEBSITE_URL in env!")
    );
    write(&ctx, format!("Privacy Policy: <{}>", url)).await?;
    Ok(())
}

/// Displays help
#[poise::command(slash_command, prefix_command, track_edits, category = "Info")]
async fn help(
    ctx: Context<'_>,
    #[description = "Command"] cmd: Option<String>,
) -> Result<(), Error> {
    let config = builtins::HelpConfiguration {
        extra_text_at_bottom: "\
            Type /help <cmd> for more detailed help.",
        ephemeral: true,
        ..Default::default()
    };
    builtins::help(ctx, cmd.as_deref(), config).await?;
    Ok(())
}

#[tokio::main]
async fn main() {
    dotenv().ok();
    let conn = Mutex::new(establish_connection());
    let token = env::var("DISCORD_TOKEN").expect("Token missing");
    let intents = GatewayIntents::non_privileged() | GatewayIntents::GUILD_MEMBERS;

    let bot_data = BotData {
        user_ids: Mutex::new(HashMap::new()),
        beta_user_ids: Mutex::new(HashMap::new()),
        admins: Mutex::new(HashMap::new()),
        conn,
    };
    bot_data.load().await;

    let framework = Framework::builder()
        .options(FrameworkOptions {
            commands: vec![
                assign(),
                delete(),
                request(),
                list(),
                guilds(),
                edit_lists(),
                users(),
                players(),
                help(),
                compare(),
                update_rankings(),
                rankings(),
                hof_rankings(),
                policy(),
            ],
            prefix_options: PrefixFrameworkOptions {
                prefix: Some("~".into()),
                edit_tracker: Some(Arc::new(EditTracker::for_timespan(Duration::from_secs(60)))),
                additional_prefixes: vec![Prefix::Literal("'")],
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
                builtins::register_globally(ctx, &framework.options().commands).await?;
                Ok(bot_data)
            })
        })
        .build();

    let client = ClientBuilder::new(token, intents)
        .framework(framework)
        .await;
    client.unwrap().start().await.unwrap();
}

async fn is_admin(ctx: &Context<'_>, level: u32) -> (bool, String) {
    let admin_list = ctx.data().admins.lock().unwrap();
    if admin_list.contains_key(&ctx.author().name) {
        if admin_list.get(&ctx.author().name).unwrap() <= &level {
            (true, String::new())
        } else {
            (false, "Not privileged!".to_string())
        }
    } else {
        (false, "Not an admin!".to_string())
    }
}

async fn autocomplete_users(ctx: Context<'_>, partial: &str) -> Vec<String> {
    let mut user_ids: Vec<String> = ctx
        .data()
        .user_ids
        .lock()
        .unwrap()
        .keys()
        .cloned()
        .collect();
    user_ids.append(
        &mut ctx
            .data()
            .beta_user_ids
            .lock()
            .unwrap()
            .keys()
            .cloned()
            .map(|k| format!("{} (BETA)", k))
            .collect(),
    );
    let user_ids = user_ids.into_iter();
    if user_ids.clone().filter(|k| k.starts_with(partial)).count() > 0 {
        user_ids.filter(|k| k.starts_with(partial)).collect()
    } else if user_ids.clone().filter(|k| k.contains(partial)).count() > 0 {
        user_ids.filter(|k| k.contains(partial)).collect()
    } else if user_ids
        .clone()
        .filter(|k| k.to_lowercase().starts_with(&partial.to_lowercase()))
        .count()
        > 0
    {
        return user_ids
            .filter(|k| k.to_lowercase().starts_with(&partial.to_lowercase()))
            .collect();
    } else if user_ids
        .clone()
        .filter(|k| k.to_lowercase().contains(&partial.to_lowercase()))
        .count()
        > 0
    {
        return user_ids
            .filter(|k| k.to_lowercase().contains(&partial.to_lowercase()))
            .collect();
    } else {
        return user_ids
            .filter(|key| key.to_lowercase().starts_with(&partial.to_lowercase()))
            .collect();
    }
}
