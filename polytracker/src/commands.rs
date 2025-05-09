use crate::utils::{
    autocomplete_users, is_admin, write, write_embed, AddAdminModal, BotData, EditAdminModal,
    EditModal, LeaderBoard, LeaderBoardEntry, RemoveAdminModal, WriteEmbed,
};
use crate::{Context, Error, MAX_RANKINGS_AGE};
use dotenvy::dotenv;
use poise::serenity_prelude as serenity;
use poise::{builtins, ApplicationContext, ChoiceParameter, CommandParameterChoice, Modal};
use polymanager::{
    community_update, global_rankings_update, hof_update, PolyLeaderBoard, ALT_ACCOUNT_FILE,
    BLACKLIST_FILE, COMMUNITY_RANKINGS_FILE, COMMUNITY_TIME_RANKINGS_FILE, COMMUNITY_TRACK_FILE,
    HOF_ALL_TRACK_FILE, HOF_ALT_ACCOUNT_FILE, HOF_BLACKLIST_FILE, HOF_RANKINGS_FILE,
    HOF_TIME_RANKINGS_FILE, HOF_TRACK_FILE, RANKINGS_FILE, REQUEST_RETRY_COUNT, TRACK_FILE,
    VERSION,
};
use reqwest::Client;
use serenity::futures::future::join_all;
use std::time::Duration;
use std::{collections::HashMap, env};
use tokio::time::sleep;
use tokio::{fs, task};

// argument enum for leaderboard related commands
#[derive(Clone)]
enum LeaderboardChoice {
    Global,
    Community,
    Hof,
}

impl ChoiceParameter for LeaderboardChoice {
    fn list() -> Vec<CommandParameterChoice> {
        use LeaderboardChoice::*;
        [Global, Community, Hof]
            .iter()
            .map(|c| CommandParameterChoice {
                name: c.name().to_string(),
                localizations: HashMap::new(),
                __non_exhaustive: (),
            })
            .collect()
    }
    fn name(&self) -> &'static str {
        use LeaderboardChoice::*;
        match self {
            Global => "Global",
            Community => "Community",
            Hof => "HOF",
        }
    }
    fn from_index(index: usize) -> Option<Self> {
        use LeaderboardChoice::*;
        [Global, Community, Hof].get(index).cloned()
    }
    fn localized_name(&self, _: &str) -> Option<&'static str> {
        Some(self.name())
    }
    fn from_name(name: &str) -> Option<Self> {
        use LeaderboardChoice::*;
        match name.to_lowercase().as_str() {
            "global" => Some(Global),
            "community" => Some(Community),
            "hof" => Some(Hof),
            _ => None,
        }
    }
}

// argument enum for edit_lists()
#[derive(Clone)]
pub enum EditModalChoice {
    Black,
    Alt,
    HOFBlack,
    HOFAlt,
}

impl ChoiceParameter for EditModalChoice {
    fn list() -> Vec<CommandParameterChoice> {
        let names = ["Black List", "Alt List", "HOF Black List", "HOF Alt List"];
        names
            .iter()
            .map(|n| CommandParameterChoice {
                name: n.to_string(),
                localizations: HashMap::new(),
                __non_exhaustive: (),
            })
            .collect()
    }
    fn from_index(index: usize) -> Option<Self> {
        use EditModalChoice::*;
        let values = [Black, Alt, HOFBlack, HOFAlt];
        values.get(index).cloned()
    }
    fn localized_name(&self, _: &str) -> Option<&'static str> {
        Some(self.name())
    }
    fn from_name(name: &str) -> Option<Self> {
        use EditModalChoice::*;
        match name {
            "Black List" => Some(Black),
            "Alt List" => Some(Alt),
            "HOF Black List" => Some(HOFBlack),
            "HOF Alt List" => Some(HOFAlt),
            _ => None,
        }
    }
    fn name(&self) -> &'static str {
        use EditModalChoice::*;
        match self {
            Black => "Blacklist",
            Alt => "Alt-List",
            HOFBlack => "HOF Blacklist",
            HOFAlt => "HOF Alt-List",
        }
    }
}

#[derive(Clone)]
pub enum UpdateAdminsChoice {
    Add,
    Remove,
    Edit,
}

impl ChoiceParameter for UpdateAdminsChoice {
    fn list() -> Vec<CommandParameterChoice> {
        let names = ["Add", "Remove", "Edit"];
        names
            .iter()
            .map(|n| CommandParameterChoice {
                name: n.to_string(),
                localizations: HashMap::new(),
                __non_exhaustive: (),
            })
            .collect()
    }
    fn from_index(index: usize) -> Option<Self> {
        use UpdateAdminsChoice::*;
        let values = [Add, Remove, Edit];
        values.get(index).cloned()
    }
    fn localized_name(&self, _locale: &str) -> Option<&'static str> {
        Some(self.name())
    }
    fn from_name(name: &str) -> Option<Self> {
        use UpdateAdminsChoice::*;
        match name {
            "Add" => Some(Add),
            "Remove" => Some(Remove),
            "Edit" => Some(Edit),
            _ => None,
        }
    }
    fn name(&self) -> &'static str {
        use UpdateAdminsChoice::*;
        match self {
            Add => "Add",
            Remove => "Remove",
            Edit => "Edit",
        }
    }
}

/// Assign a username an ID
///
/// The ID can be found by going from the main menu to "Profile", clicking on the profile \
/// and copying the "User ID" in the bottom left.
#[poise::command(slash_command, prefix_command, category = "Setup")]
pub async fn assign(
    ctx: Context<'_>,
    #[description = "Username"] user: String,
    #[description = "Player ID"] id: String,
) -> Result<(), Error> {
    ctx.defer_ephemeral().await?;
    let mut user_id = id;
    if user_id.starts_with("User ID: ") {
        user_id = user_id.trim_start_matches("User ID: ").to_string();
    }
    if ctx.data().user_ids.lock().unwrap().contains_key(&user) {
        let response = format!(
            "`User '{}' is already assigned an ID, to reassign please contact this bot's owner`",
            user
        );
        write(&ctx, response).await?;
        return Ok(());
    }
    let response = format!("`Added user '{}' with ID '{}'`", user, user_id);
    ctx.data()
        .user_ids
        .lock()
        .unwrap()
        .insert(user.clone(), user_id.clone());
    ctx.data().add(user.as_str(), user_id.as_str()).await;
    write(&ctx, response).await?;
    Ok(())
}

/// Delete an already assigned username-ID pair (bot-admin only)
///
/// Only deletes the data from the bot, you game account stays intact.
#[poise::command(slash_command, prefix_command, category = "Administration")]
pub async fn delete(
    ctx: Context<'_>,
    #[description = "Username"]
    #[autocomplete = "autocomplete_users"]
    user: String,
) -> Result<(), Error> {
    ctx.defer_ephemeral().await?;
    let (is_admin, is_admin_msg) = is_admin(&ctx, 1).await;
    if !is_admin {
        write(&ctx, is_admin_msg).await?;
        return Ok(());
    }
    let bot_data = ctx.data();
    let response;
    if bot_data.user_ids.lock().unwrap().contains_key(&user) {
        let id = bot_data.user_ids.lock().unwrap().remove(&user).unwrap();
        ctx.data().delete(user.as_str()).await;
        response = format!("`Removed user '{}' with ID '{}'`", user, id,);
    } else {
        response = "`User not found!`".to_string();
    }
    write(&ctx, response).await?;
    Ok(())
}

#[poise::command(slash_command, category = "Administration")]
pub async fn update_admins(
    ctx: ApplicationContext<'_, BotData, Error>,
    #[description = "Operation"] operation: UpdateAdminsChoice,
) -> Result<(), Error> {
    let (is_admin, is_admin_msg) = is_admin(&ctx.into(), 0).await;
    if !is_admin {
        write(&ctx.into(), is_admin_msg).await?;
        return Ok(());
    }
    use UpdateAdminsChoice::*;
    let output = match operation {
        Add => {
            let modal_output = AddAdminModal::execute(ctx).await?.unwrap();
            let discord = modal_output.discord;
            let privilege = modal_output.privilege.parse()?;
            ctx.data()
                .admins
                .lock()
                .unwrap()
                .insert(discord.clone(), privilege);
            ctx.data().add_admin(&discord, privilege as i32).await;
            format!("Added admin {} with privilege level {}", discord, privilege)
        }
        Remove => {
            let modal_output = RemoveAdminModal::execute(ctx).await?.unwrap();
            let discord = modal_output.discord;
            if ctx.data().admins.lock().unwrap().contains_key(&discord) {
                let privilege = ctx.data().admins.lock().unwrap().remove(&discord).unwrap();
                ctx.data().remove_admin(&discord).await;
                format!(
                    "Removed admin {} with former privilege level {}",
                    discord, privilege
                )
            } else {
                format!("Admin {} does not exist", discord)
            }
        }
        Edit => {
            let modal_output = EditAdminModal::execute(ctx).await?.unwrap();
            let discord = modal_output.discord;
            let privilege = modal_output.privilege.parse()?;
            if ctx.data().admins.lock().unwrap().contains_key(&discord) {
                ctx.data()
                    .admins
                    .lock()
                    .unwrap()
                    .insert(discord.clone(), privilege);
                ctx.data().edit_admin(&discord, privilege as i32).await;
                format!("Updated admin {} to privilege level {}", discord, privilege)
            } else {
                format!("Admin {} does not exist", discord)
            }
        }
    };
    write(&ctx.into(), output).await?;
    Ok(())
}

/// Request data about a track
///
/// Choose between standard tracks (off=True) or custom tracks (off=False).
/// For standard tracks use the track number (1-13).
/// For custom tracks use the track ID.
#[poise::command(slash_command, prefix_command, category = "Query")]
pub async fn request(
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
            let track_ids: Vec<(String, String)> = fs::read_to_string(TRACK_FILE)
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
            let track_id = track_ids.get(track.parse::<usize>().unwrap() - 1).unwrap();
            url = format!("https://vps.kodub.com:43273/leaderboard?version=0.5.0&trackId={}&skip=0&amount=500&onlyVerified=false&userTokenHash={}",
            track_id.0,
            id);
        } else {
            url = format!("https://vps.kodub.com:43273/leaderboard?version=0.5.0&trackId={}&skip=0&amount=500&onlyVerified=false&userTokenHash={}",
            track,
            id);
        }
        let contents: Vec<String>;
        if let Ok(response) = client.get(url).send().await {
            if let Ok(body) = response.text().await {
                if let Ok(leaderboard) = serde_json::from_str::<LeaderBoard>(&body) {
                    if let Some(user_entry) = leaderboard.user_entry {
                        let position = user_entry.position;
                        let frames = user_entry.frames;
                        if position <= 501 {
                            let entries = leaderboard.entries;
                            let mut found: Vec<String> = Vec::new();
                            let mut i = 0;
                            for entry in entries {
                                i += 1;
                                if i == position {
                                    break;
                                }
                                if !found.contains(&entry.name) && entry.verified_state == 1 {
                                    found.push(entry.name);
                                }
                            }
                            let mut time = (frames / 1000.0).to_string();
                            time.push('s');
                            contents =
                                vec![position.to_string(), time, (found.len() + 1).to_string()];
                            write_embed(
                                ctx,
                                vec![WriteEmbed::new(3)
                                    .title("Leaderboard")
                                    .headers(vec!["Ranking", "Time", "Unique"])
                                    .contents(contents)],
                            )
                            .await?;
                        } else {
                            let mut time = (frames / 1000.0).to_string();
                            time.push('s');
                            contents = vec![position.to_string(), time];
                            write_embed(
                                ctx,
                                vec![WriteEmbed::new(2)
                                    .title("Leaderboard")
                                    .headers(vec!["Ranking", "Time"])
                                    .contents(contents)],
                            )
                            .await?;
                        }
                    } else {
                        write(&ctx, "`Record not found!`".to_string()).await?;
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
pub async fn list(
    ctx: Context<'_>,
    #[description = "User"]
    #[autocomplete = "autocomplete_users"]
    user: String,
    #[description = "Tracks"] tracks: Option<LeaderboardChoice>,
    #[description = "Hidden"] hidden: Option<bool>,
) -> Result<(), Error> {
    if hidden.is_some_and(|x| x) {
        ctx.defer_ephemeral().await?;
    } else {
        ctx.defer().await?;
    }
    let tracks = tracks.unwrap_or(LeaderboardChoice::Global);
    let track_file = {
        use LeaderboardChoice::*;
        match tracks {
            Global => TRACK_FILE,
            Community => COMMUNITY_TRACK_FILE,
            Hof => HOF_TRACK_FILE,
        }
    };
    let mut id = String::new();
    if let Some(id_test) = ctx.data().user_ids.lock().unwrap().get(&user) {
        id = id_test.clone();
    }
    if !id.is_empty() {
        let client = Client::new();
        let mut line_num: u32 = 0;
        let mut total_time = 0.0;
        let mut display_total = true;
        let track_ids: Vec<(String, String)> = fs::read_to_string(track_file)
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
        let futures = track_ids.iter().enumerate().map(|(i, track_id)| {
            let client = client.clone();
            let url = format!("https://vps.kodub.com:{}/leaderboard?version={}&trackId={}&skip=0&amount=500&onlyVerified=false&userTokenHash={}",
            43273,
            VERSION,
            track_id.0,
            id);
            task::spawn(
            async move {
                let mut att = 0;
                let mut res = client.get(&url).send().await.unwrap().text().await.unwrap();
                    while res.is_empty() && att < REQUEST_RETRY_COUNT {
                        att += 1;
                        sleep(Duration::from_millis(500)).await;
                        res = client.get(&url).send().await.unwrap().text().await.unwrap();
                    }
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
            if let Ok(leaderboard) = serde_json::from_str::<LeaderBoard>(&response) {
                if let Some(user_entry) = leaderboard.user_entry {
                    let position = user_entry.position;
                    let frames = user_entry.frames;
                    if position <= 501 {
                        let entries = leaderboard.entries;
                        let mut found: Vec<String> = Vec::new();
                        let mut i = 0;
                        for entry in entries {
                            i += 1;
                            if i == position {
                                break;
                            }
                            if entry.verified_state == 1 && !found.contains(&entry.name) {
                                found.push(entry.name);
                            }
                        }
                        let time = frames / 1000.0;
                        total_time += time;
                        let mut time = time.to_string();
                        time.push('s');
                        contents[0]
                            .push_str(format!("{}\n", track_ids[line_num as usize].1).as_str());
                        contents[1]
                            .push_str(format!("{} [{}]\n", position, (found.len() + 1)).as_str());
                        contents[2].push_str(format!("{}\n", time).as_str());
                    } else {
                        let time = frames.to_string().parse::<f64>().unwrap() / 1000.0;
                        total_time += time;
                        let mut time = time.to_string();
                        time.push('s');
                        contents[0]
                            .push_str(format!("{}\n", track_ids[line_num as usize].1).as_str());
                        contents[1].push_str(format!("{}\n", position).as_str());
                        contents[2].push_str(format!("{}\n", time).as_str());
                    };
                } else {
                    display_total = false;
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
        if display_total && matches!(tracks, LeaderboardChoice::Global) {
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
            ctx,
            vec![WriteEmbed::new(headers.len())
                .title(&user)
                .headers(headers)
                .contents(contents)
                .inlines(inlines)],
        )
        .await?;
    } else {
        write(&ctx, "`User ID not found`".to_string()).await?;
    }
    Ok(())
}

/// Compares two user's record times and placements
#[poise::command(slash_command, prefix_command, category = "Query")]
pub async fn compare(
    ctx: Context<'_>,
    #[description = "User 1"]
    #[autocomplete = "autocomplete_users"]
    user1: String,
    #[description = "User 2"]
    #[autocomplete = "autocomplete_users"]
    user2: String,
    #[description = "Tracks"] tracks: Option<LeaderboardChoice>,
    #[description = "Hidden"] hidden: Option<bool>,
) -> Result<(), Error> {
    if hidden.is_some_and(|x| x) {
        ctx.defer_ephemeral().await?;
    } else {
        ctx.defer().await?;
    }
    let tracks = tracks.unwrap_or(LeaderboardChoice::Global);
    let mut results: Vec<Vec<(u32, f64)>> = Vec::new();
    let track_ids: Vec<(String, String)> = fs::read_to_string({
        use LeaderboardChoice::*;
        match tracks {
            Global => TRACK_FILE,
            Community => COMMUNITY_TRACK_FILE,
            Hof => HOF_TRACK_FILE,
        }
    })
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
    let track_names: Vec<String> = track_ids.iter().map(|(_, name)| name.clone()).collect();
    for user in [user1.clone(), user2.clone()] {
        let mut user_results: Vec<(u32, f64)> = Vec::new();
        let mut id = String::new();
        if let Some(id_test) = ctx.data().user_ids.lock().unwrap().get(&user) {
            id = id_test.clone();
        }
        if !id.is_empty() {
            let client = Client::new();
            let mut total_time = 0.0;
            let mut display_total = true;
            let futures = track_ids.iter().enumerate().map(|(i, track_id)| {
            let client = client.clone();
            let url = format!("https://vps.kodub.com:{}/leaderboard?version={}&trackId={}&skip=0&amount=1&onlyVerified=false&userTokenHash={}",
            43273,
            VERSION,
            track_id.0,
            id);
            task::spawn(
            async move {
                let mut att = 0;
                let mut res = client.get(&url).send().await.unwrap().text().await.unwrap();
                    while res.is_empty() && att < REQUEST_RETRY_COUNT {
                        att += 1;
                        sleep(Duration::from_millis(500)).await;
                        res = client.get(&url).send().await.unwrap().text().await.unwrap();
                    }
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
                if let Ok(leaderboard) = serde_json::from_str::<LeaderBoard>(&response) {
                    if let Some(user_entry) = leaderboard.user_entry {
                        let position = user_entry.position;
                        let frames = user_entry.frames;
                        let time = frames / 1000.0;
                        user_results.push((position, time));
                        total_time += time;
                    } else {
                        user_results.push((0, 0.0));
                        display_total = false;
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
    let max_track_len = track_ids.iter().map(|(_, t)| t.len()).max().unwrap().max(5);
    let column_gap = 3;
    output.push_str(&format!("```\n{}", " ".repeat(max_track_len + 2)));
    for user in [user1.clone(), user2.clone()] {
        output.push_str(format!("{:>18}", user).as_str());
        output.push_str(&" ".repeat(column_gap));
    }
    output.push_str("Difference\n");
    for i in 0..results[0].len() - 1 {
        let mut display_diff = true;
        output.push_str(&format!(
            "{:>width$}: ",
            track_names[i],
            width = max_track_len
        ));
        for track in results.iter() {
            if track[i].1 != 0.0 {
                output.push_str(&format!(
                    "{:>6}. - {:>7.3}s{}",
                    track[i].0,
                    track[i].1,
                    " ".repeat(column_gap)
                ));
            } else {
                output.push_str(
                    format!("{:>18}{}", "Record not found", " ".repeat(column_gap)).as_str(),
                );
                display_diff = false;
            }
        }
        if display_diff {
            output.push_str(format!("{:>9.3}s", (results[0][i].1 - results[1][i].1)).as_str());
        }
        output.push('\n');
    }
    output.push_str(&format!("\n{:>width$}: ", "Total", width = max_track_len));
    for track in &results {
        let total = track.last().unwrap().1 as u32;
        if total != 0 {
            output.push_str(&format!(
                "{}{:>2}:{:0>2}.{:0>3}{}",
                " ".repeat(9),
                total / 60000,
                total % 60000 / 1000,
                total % 1000,
                " ".repeat(column_gap)
            ));
        } else {
            output.push_str(&format!("{:>18}", "Tracks not done"));
            display_total_diff = false
        }
    }
    if display_total_diff {
        output.push_str(
            format!(
                "{:>9.3}s",
                ((results[0].last().unwrap().1 - results[1].last().unwrap().1) / 1000.0)
            )
            .as_str(),
        );
    }
    output.push_str("\n```");
    write(&ctx, output).await?;
    Ok(())
}

/// Update leaderboard for official tracks
///
/// displays users with top 10k records on all tracks
#[poise::command(slash_command, prefix_command, category = "Administration")]
pub async fn update_rankings(
    ctx: Context<'_>,
    #[description = "Updated Leaderboard"] leaderboard: LeaderboardChoice,
) -> Result<(), Error> {
    ctx.defer_ephemeral().await?;
    let (is_admin, is_admin_msg) = is_admin(&ctx, 2).await;
    if !is_admin {
        write(&ctx, is_admin_msg).await?;
        return Ok(());
    }
    use LeaderboardChoice::*;
    match leaderboard {
        Global => global_rankings_update().await,
        Community => community_update().await,
        Hof => hof_update().await,
    }?;
    let headers: Vec<&str> = vec![
        "Ranking",
        {
            use LeaderboardChoice::*;
            match leaderboard {
                Global => "Time",
                _ => "Points",
            }
        },
        "Player",
    ];
    let mut contents: Vec<String> = vec![String::new(), String::new(), String::new()];
    let content = fs::read_to_string(match leaderboard {
        Global => RANKINGS_FILE,
        Community => COMMUNITY_RANKINGS_FILE,
        Hof => HOF_RANKINGS_FILE,
    })
    .await?;
    let line = content.lines().next().unwrap();
    let lb: PolyLeaderBoard = serde_json::from_str(line).unwrap();
    for i in 0..lb.total {
        contents[0].push_str(&format!("{}\n", lb.entries[i as usize].rank));
        contents[1].push_str(&format!("{}\n", lb.entries[i as usize].stat));
        contents[2].push_str(&format!("{}\n", lb.entries[i as usize].name));
    }
    let inlines: Vec<bool> = vec![true, true, true];
    write_embed(
        ctx,
        vec![WriteEmbed::new(headers.len())
            .title(&format!("{} Leaderboard", leaderboard.name()))
            .headers(headers)
            .contents(contents)
            .inlines(inlines)],
    )
    .await?;
    Ok(())
}

/// Leaderboard for official tracks
#[poise::command(slash_command, prefix_command, category = "Query")]
pub async fn rankings(
    ctx: Context<'_>,
    #[description = "Leaderboard"] leaderboard: Option<LeaderboardChoice>,
    #[description = "Mode (HOF/community only)"] time_based: Option<bool>,
    #[description = "Hidden"] hidden: Option<bool>,
) -> Result<(), Error> {
    if hidden.is_some_and(|x| x) {
        ctx.defer_ephemeral().await?;
    } else {
        ctx.defer().await?;
    }
    let leaderboard = leaderboard.unwrap_or(LeaderboardChoice::Global);
    let time_based = time_based.unwrap_or(false);
    use LeaderboardChoice::*;
    let rankings_file = match leaderboard {
        Global => RANKINGS_FILE,
        Community => match time_based {
            false => COMMUNITY_RANKINGS_FILE,
            true => COMMUNITY_TIME_RANKINGS_FILE,
        },
        Hof => match time_based {
            false => HOF_RANKINGS_FILE,
            true => HOF_TIME_RANKINGS_FILE,
        },
    };
    if fs::try_exists(rankings_file).await? {
        let age = fs::metadata(rankings_file).await?.modified()?.elapsed()?;
        if age > MAX_RANKINGS_AGE {
            match leaderboard {
                Global => global_rankings_update().await?,
                Community => community_update().await?,
                Hof => hof_update().await?,
            }
        }
    } else {
        match leaderboard {
            Global => global_rankings_update().await?,
            Community => community_update().await?,
            Hof => hof_update().await?,
        }
    }
    let headers: Vec<&str> = vec![
        "Ranking",
        {
            match leaderboard {
                Global => "Time",
                _ => match time_based {
                    false => "Points",
                    true => "Time",
                },
            }
        },
        "Player",
    ];
    let mut contents: Vec<String> = vec![String::new(), String::new(), String::new()];
    let content = fs::read_to_string(match leaderboard {
        Global => RANKINGS_FILE,
        Community => COMMUNITY_RANKINGS_FILE,
        Hof => HOF_RANKINGS_FILE,
    })
    .await?;
    for line in content.lines() {
        let lb: PolyLeaderBoard = serde_json::from_str(line).unwrap();
        for i in 0..lb.total {
            contents[0].push_str(&format!("{}\n", lb.entries[i as usize].rank));
            contents[1].push_str(&format!("{}\n", lb.entries[i as usize].stat));
            contents[2].push_str(&format!("{}\n", lb.entries[i as usize].name));
        }
    }
    let inlines: Vec<bool> = vec![true, true, true];
    write_embed(
        ctx,
        vec![WriteEmbed::new(headers.len())
            .title(&format!("{} Leaderboard", leaderboard.name()))
            .headers(headers)
            .contents(contents)
            .inlines(inlines)],
    )
    .await?;
    Ok(())
}

/// Lets privileged users edit certain internal data
#[poise::command(slash_command, category = "Administration", ephemeral)]
pub async fn edit_lists(
    ctx: ApplicationContext<'_, BotData, Error>,
    #[description = "List to edit"] list: EditModalChoice,
) -> Result<(), Error> {
    let (is_admin, is_admin_msg) = is_admin(&ctx.into(), 2).await;
    if !is_admin {
        write(&ctx.into(), is_admin_msg).await?;
        return Ok(());
    }
    let list_file = {
        use EditModalChoice::*;
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
pub async fn users(ctx: Context<'_>) -> Result<(), Error> {
    let bot_data = ctx.data();
    let mut users = String::new();
    for (user, id) in bot_data.user_ids.lock().unwrap().iter() {
        users.push_str(format!("{}: {}\n", user, id).as_str());
    }
    write(&ctx, format!("```{}```", users)).await?;
    Ok(())
}

/// Displays player numbers
#[poise::command(slash_command, prefix_command, category = "Info")]
pub async fn players(
    ctx: Context<'_>,
    #[description = "Tracks"] tracks: Option<LeaderboardChoice>,
    #[description = "Hidden"] hidden: Option<bool>,
) -> Result<(), Error> {
    if hidden.is_some_and(|x| x) {
        ctx.defer_ephemeral().await?;
    } else {
        ctx.defer().await?;
    }
    let tracks = tracks.unwrap_or(LeaderboardChoice::Global);
    let track_ids: Vec<(String, String)> = fs::read_to_string({
        use LeaderboardChoice::*;
        match tracks {
            Global => TRACK_FILE,
            Community => COMMUNITY_TRACK_FILE,
            Hof => HOF_ALL_TRACK_FILE,
        }
    })
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
            43273,
            VERSION,
            id);
        let mut att = 0;
        let mut res = client.get(&url).send().await?.text().await?;
        while res.is_empty() && att < REQUEST_RETRY_COUNT {
            att += 1;
            sleep(Duration::from_millis(500)).await;
            res = client.get(&url).send().await?.text().await?;
        }
        let number = serde_json::from_str::<LeaderBoard>(&res)?.total;
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
        ctx,
        vec![WriteEmbed::new(2)
            .title("Player numbers")
            .headers(vec!["Track", "Players"])
            .contents(contents)],
    )
    .await?;
    Ok(())
}

#[poise::command(slash_command, prefix_command, category = "Info")]
pub async fn records(
    ctx: Context<'_>,
    #[description = "Tracks"] tracks: Option<LeaderboardChoice>,
    #[description = "Hidden"] hidden: Option<bool>,
) -> Result<(), Error> {
    if hidden.is_some_and(|x| x) {
        ctx.defer_ephemeral().await?;
    } else {
        ctx.defer().await?;
    }
    let tracks = tracks.unwrap_or(LeaderboardChoice::Global);
    let track_ids: Vec<(String, String)> = fs::read_to_string({
        use LeaderboardChoice::*;
        match tracks {
            Global => TRACK_FILE,
            Community => COMMUNITY_TRACK_FILE,
            Hof => HOF_ALL_TRACK_FILE,
        }
    })
    .await
    .unwrap()
    .lines()
    .map(|s| {
        let mut parts = s.splitn(2, " ").map(|s| s.to_string());
        (parts.next().unwrap(), parts.next().unwrap())
    })
    .collect();
    let mut contents = vec![String::new(), String::new(), String::new()];
    let client = Client::new();
    let mut wr_amounts: HashMap<String, u32> = HashMap::new();
    for (id, name) in track_ids {
        let url = format!("https://vps.kodub.com:{}/leaderboard?version={}&trackId={}&skip=0&amount=1&onlyVerified=true",
            43273,
            VERSION,
            id,
        );
        let mut att = 0;
        let mut res = client.get(&url).send().await?.text().await?;
        while res.is_empty() && att < REQUEST_RETRY_COUNT {
            att += 1;
            sleep(Duration::from_millis(1000)).await;
            res = client.get(&url).send().await?.text().await?;
        }
        let leaderboard = serde_json::from_str::<LeaderBoard>(&res)?;
        let default_winner = LeaderBoardEntry {
            name: "unknown".to_string(),
            frames: 69420.0,
            verified_state: 1,
        };
        let winner = leaderboard.entries.first().unwrap_or(&default_winner);
        let winner_name = winner.name.clone();
        let winner_time = winner.frames / 1000.0;
        *wr_amounts.entry(winner_name.clone()).or_default() += 1;
        contents
            .get_mut(0)
            .unwrap()
            .push_str(&format!("{}\n", name));
        contents
            .get_mut(1)
            .unwrap()
            .push_str(&format!("{}\n", winner_name));
        contents
            .get_mut(2)
            .unwrap()
            .push_str(&format!("{}s\n", winner_time));
    }
    let embed1 = WriteEmbed::new(3)
        .title("World Records")
        .headers(vec!["Track", "Player", "Time"])
        .contents(contents);
    let mut wr_amounts: Vec<(String, u32)> = wr_amounts.into_iter().collect();
    wr_amounts.sort_by_key(|(_, k)| -(*k as i32));
    let mut contents = vec![String::new(), String::new()];
    for (name, amount) in wr_amounts {
        contents
            .get_mut(0)
            .unwrap()
            .push_str(&format!("{}\n", name));
        contents
            .get_mut(1)
            .unwrap()
            .push_str(&format!("{}\n", amount));
    }
    let embed2 = WriteEmbed::new(2)
        .title("WR Amounts")
        .headers(vec!["Player", "Amount"])
        .contents(contents);
    write_embed(ctx, vec![embed1, embed2]).await?;
    Ok(())
}

#[poise::command(slash_command, prefix_command, category = "Info")]
pub async fn top(
    ctx: Context<'_>,
    #[description = "Position"] position: u32,
    #[description = "Tracks"] tracks: Option<LeaderboardChoice>,
    #[description = "Hidden"] hidden: Option<bool>,
) -> Result<(), Error> {
    if hidden.is_some_and(|x| x) {
        ctx.defer_ephemeral().await?;
    } else {
        ctx.defer().await?;
    }
    let tracks = tracks.unwrap_or(LeaderboardChoice::Global);
    let track_ids: Vec<(String, String)> = fs::read_to_string({
        use LeaderboardChoice::*;
        match tracks {
            Global => TRACK_FILE,
            Community => COMMUNITY_TRACK_FILE,
            Hof => HOF_ALL_TRACK_FILE,
        }
    })
    .await
    .unwrap()
    .lines()
    .map(|s| {
        let mut parts = s.splitn(2, " ").map(|s| s.to_string());
        (parts.next().unwrap(), parts.next().unwrap())
    })
    .collect();
    let mut contents = vec![String::new(), String::new(), String::new()];
    let client = Client::new();
    for (id, name) in track_ids {
        let url = format!("https://vps.kodub.com:{}/leaderboard?version={}&trackId={}&skip={}&amount=1&onlyVerified=true",
            43273,
            VERSION,
            id,
            position - 1,
        );
        let mut att = 0;
        let mut res = client.get(&url).send().await?.text().await?;
        while res.is_empty() && att < REQUEST_RETRY_COUNT {
            att += 1;
            sleep(Duration::from_millis(1000)).await;
            res = client.get(&url).send().await?.text().await?;
        }
        let leaderboard = serde_json::from_str::<LeaderBoard>(&res)?;
        let default_winner = LeaderBoardEntry {
            name: "unknown".to_string(),
            frames: 69420.0,
            verified_state: 1,
        };
        let winner = leaderboard.entries.first().unwrap_or(&default_winner);
        let winner_name = winner.name.clone();
        let winner_time = winner.frames / 1000.0;
        contents
            .get_mut(0)
            .unwrap()
            .push_str(&format!("{}\n", name));
        contents
            .get_mut(1)
            .unwrap()
            .push_str(&format!("{}\n", winner_name));
        contents
            .get_mut(2)
            .unwrap()
            .push_str(&format!("{}s\n", winner_time));
    }
    write_embed(
        ctx,
        vec![WriteEmbed::new(3)
            .title(&format!("Top {}", position))
            .headers(vec!["Track", "Player", "Time"])
            .contents(contents)],
    )
    .await?;
    Ok(())
}

/// Links the privacy policy
#[poise::command(slash_command, prefix_command, category = "Info", ephemeral)]
pub async fn policy(ctx: Context<'_>) -> Result<(), Error> {
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
pub async fn help(
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
