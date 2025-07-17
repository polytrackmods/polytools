use crate::utils::{
    autocomplete_users, get_records, is_admin, write, write_embed, AddAdminModal, BotData,
    EditAdminModal, EditModal, LeaderBoard, LeaderBoardEntry, RemoveAdminModal, WriteEmbed,
};
use crate::{Context, Error};
use anyhow::Result;
use dotenvy::dotenv;
use poise::serenity_prelude::{
    self as serenity, ComponentInteractionCollector, ComponentInteractionDataKind, CreateActionRow,
    CreateAttachment, CreateInteractionResponseMessage, CreateSelectMenu, CreateSelectMenuKind,
    CreateSelectMenuOption,
};
use poise::{
    builtins, ApplicationContext, ChoiceParameter, CommandParameterChoice, CreateReply, Modal,
};
use polymanager::{
    check_blacklist, community_update, et_rankings_update, get_alt, global_rankings_update,
    hof_update, read_altlist, read_blacklist, write_altlist, write_blacklist, PolyLeaderBoard,
    ALT_ACCOUNT_FILE, BLACKLIST_FILE, COMMUNITY_RANKINGS_FILE, COMMUNITY_TIME_RANKINGS_FILE,
    COMMUNITY_TRACK_FILE, ET_CODE_FILE, ET_RANKINGS_FILE, ET_TRACK_FILE, HOF_ALL_TRACK_FILE,
    HOF_ALT_ACCOUNT_FILE, HOF_BLACKLIST_FILE, HOF_CODE_FILE, HOF_RANKINGS_FILE,
    HOF_TIME_RANKINGS_FILE, HOF_TRACK_FILE, RANKINGS_FILE, REQUEST_RETRY_COUNT, TRACK_FILE,
    UPDATE_CYCLE_LEN, VERSION,
};
use reqwest::Client;
use serenity::futures::future::join_all;
use sha256::digest;
use std::fmt::Write as _;
use std::time::Duration;
use std::{collections::HashMap, env};
use tokio::time::sleep;
use tokio::{fs, task};

// argument enum for leaderboard related commands
#[derive(Clone)]
pub enum LeaderboardChoice {
    Global,
    Community,
    Hof,
    Et,
}

impl ChoiceParameter for LeaderboardChoice {
    fn list() -> Vec<CommandParameterChoice> {
        use LeaderboardChoice::{Community, Et, Global, Hof};
        [Global, Community, Hof, Et]
            .iter()
            .map(|c| CommandParameterChoice {
                name: c.name().to_string(),
                localizations: HashMap::new(),
                __non_exhaustive: (),
            })
            .collect()
    }
    fn name(&self) -> &'static str {
        use LeaderboardChoice::{Community, Et, Global, Hof};
        match self {
            Global => "Global",
            Community => "Community",
            Hof => "HOF",
            Et => "ET",
        }
    }
    fn from_index(index: usize) -> Option<Self> {
        use LeaderboardChoice::{Community, Et, Global, Hof};
        [Global, Community, Hof, Et].get(index).cloned()
    }
    fn localized_name(&self, _: &str) -> Option<&'static str> {
        Some(self.name())
    }
    fn from_name(name: &str) -> Option<Self> {
        use LeaderboardChoice::{Community, Et, Global, Hof};
        match name.to_lowercase().as_str() {
            "global" => Some(Global),
            "community" => Some(Community),
            "hof" => Some(Hof),
            "et" => Some(Et),
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
                name: (*n).to_string(),
                localizations: HashMap::new(),
                __non_exhaustive: (),
            })
            .collect()
    }
    fn from_index(index: usize) -> Option<Self> {
        use EditModalChoice::{Alt, Black, HOFAlt, HOFBlack};
        let values = [Black, Alt, HOFBlack, HOFAlt];
        values.get(index).cloned()
    }
    fn localized_name(&self, _: &str) -> Option<&'static str> {
        Some(self.name())
    }
    fn from_name(name: &str) -> Option<Self> {
        use EditModalChoice::{Alt, Black, HOFAlt, HOFBlack};
        match name {
            "Black List" => Some(Black),
            "Alt List" => Some(Alt),
            "HOF Black List" => Some(HOFBlack),
            "HOF Alt List" => Some(HOFAlt),
            _ => None,
        }
    }
    fn name(&self) -> &'static str {
        use EditModalChoice::{Alt, Black, HOFAlt, HOFBlack};
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
                name: (*n).to_string(),
                localizations: HashMap::new(),
                __non_exhaustive: (),
            })
            .collect()
    }
    fn from_index(index: usize) -> Option<Self> {
        use UpdateAdminsChoice::{Add, Edit, Remove};
        let values = [Add, Remove, Edit];
        values.get(index).cloned()
    }
    fn localized_name(&self, _locale: &str) -> Option<&'static str> {
        Some(self.name())
    }
    fn from_name(name: &str) -> Option<Self> {
        use UpdateAdminsChoice::{Add, Edit, Remove};
        match name {
            "Add" => Some(Add),
            "Remove" => Some(Remove),
            "Edit" => Some(Edit),
            _ => None,
        }
    }
    fn name(&self) -> &'static str {
        use UpdateAdminsChoice::{Add, Edit, Remove};
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
) -> Result<()> {
    ctx.defer_ephemeral().await?;
    let mut user_id = id;
    if user_id.starts_with("User ID: ") {
        user_id = user_id.trim_start_matches("User ID: ").to_string();
    }
    let client = Client::new();
    let response = client
        .get(format!(
            "https://vps.kodub.com/user?version={VERSION}&userToken={user_id}",
        ))
        .send()
        .await?
        .text()
        .await?;
    if response != "null" {
        user_id = digest(user_id);
    }
    if ctx
        .data()
        .user_ids
        .lock()
        .expect("Failed to acquire Mutex")
        .contains_key(&user)
    {
        let response = format!(
            "`User '{user}' is already assigned an ID, to reassign please contact this bot's owner`"
        );
        write(&ctx, response).await?;
        return Ok(());
    }
    let response = format!("`Added user '{user}' with ID '{user_id}'`");
    ctx.data()
        .user_ids
        .lock()
        .expect("Failed to acquire Mutex")
        .insert(user.clone(), user_id.clone());
    ctx.data().add(user.as_str(), user_id.as_str());
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
) -> Result<()> {
    ctx.defer_ephemeral().await?;
    let (is_admin, is_admin_msg) = is_admin(&ctx, 1).await;
    if !is_admin {
        write(&ctx, is_admin_msg).await?;
        return Ok(());
    }
    let bot_data = ctx.data();
    let response = if bot_data
        .user_ids
        .lock()
        .expect("Failed to acquire Mutex")
        .contains_key(&user)
    {
        let id = bot_data
            .user_ids
            .lock()
            .expect("Failed to acquire Mutex")
            .remove(&user)
            .expect("Checked for user earlier");
        ctx.data().delete(user.as_str());
        format!("`Removed user '{user}' with ID '{id}'`")
    } else {
        "`User not found!`".to_string()
    };
    write(&ctx, response).await?;
    Ok(())
}

#[poise::command(slash_command, category = "Administration", ephemeral)]
pub async fn update_admins(
    ctx: ApplicationContext<'_, BotData, Error>,
    #[description = "Operation"] operation: UpdateAdminsChoice,
) -> Result<()> {
    use UpdateAdminsChoice::{Add, Edit, Remove};
    let (is_admin, is_admin_msg) = is_admin(&ctx.into(), 0).await;
    if !is_admin {
        write(&ctx.into(), is_admin_msg).await?;
        return Ok(());
    }
    let output = match operation {
        Add => {
            let modal_output = AddAdminModal::execute(ctx)
                .await?
                .expect("Empty modal output");
            let discord = modal_output.discord;
            let privilege = modal_output.privilege.parse()?;
            ctx.data()
                .admins
                .lock()
                .expect("Failed to acquire Mutex")
                .insert(discord.clone(), privilege);
            ctx.data().add_admin(&discord, i32::try_from(privilege)?);
            format!("Added admin {discord} with privilege level {privilege}")
        }
        Remove => {
            let modal_output = RemoveAdminModal::execute(ctx)
                .await?
                .expect("Empty modal output");
            let discord = modal_output.discord;
            if ctx
                .data()
                .admins
                .lock()
                .expect("Failed to acquire Mutex")
                .contains_key(&discord)
            {
                let privilege = ctx
                    .data()
                    .admins
                    .lock()
                    .expect("Failed to acquire Mutex")
                    .remove(&discord)
                    .expect("Failed to remove entry");
                ctx.data().remove_admin(&discord);
                format!("Removed admin {discord} with former privilege level {privilege}")
            } else {
                format!("Admin {discord} does not exist")
            }
        }
        Edit => {
            let modal_output = EditAdminModal::execute(ctx)
                .await?
                .expect("Empty modal output");
            let discord = modal_output.discord;
            let privilege = modal_output.privilege.parse()?;
            if ctx
                .data()
                .admins
                .lock()
                .expect("Failed to acquire Mutex")
                .contains_key(&discord)
            {
                ctx.data()
                    .admins
                    .lock()
                    .expect("Failed to acquire Mutex")
                    .insert(discord.clone(), privilege);
                ctx.data().edit_admin(&discord, i32::try_from(privilege)?);
                format!("Updated admin {discord} to privilege level {privilege}")
            } else {
                format!("Admin {discord} does not exist")
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
#[allow(clippy::too_many_lines)]
#[poise::command(slash_command, prefix_command, category = "Query")]
pub async fn request(
    ctx: Context<'_>,
    #[description = "IsOfficial"] off: bool,
    #[description = "User"]
    #[autocomplete = "autocomplete_users"]
    user: String,
    #[description = "Track"] track: String,
    #[description = "Hidden"] hidden: Option<bool>,
    #[description = "Mobile friendly mode"] mobile_friendly: Option<bool>,
) -> Result<()> {
    let mobile_friendly = mobile_friendly.unwrap_or(false);
    if hidden.is_some_and(|x| x) {
        ctx.defer_ephemeral().await?;
    } else {
        ctx.defer().await?;
    }
    let mut id = String::new();
    if let Some(id_test) = ctx
        .data()
        .user_ids
        .lock()
        .expect("Failed to acquire Mutex")
        .get(&user)
    {
        id.clone_from(id_test);
    }
    if id.is_empty() {
        write(&ctx, "`User ID not found`".to_string()).await?;
    } else {
        let client = Client::new();
        let url = if off {
            if track.parse::<usize>().is_err() || !(1..=15).contains(&track.parse::<usize>()?) {
                ctx.defer_ephemeral().await?;
                ctx.say("Not an official track!").await?;
                return Ok(());
            }
            let track_ids: Vec<(String, String)> = fs::read_to_string(TRACK_FILE)
                .await?
                .lines()
                .map(|s| {
                    let parts = s.split_once(' ').expect("Invalid track ids file");
                    (parts.0.to_string(), parts.1.to_string())
                })
                .collect();
            let track_id = track_ids
                .get(track.parse::<usize>()? - 1)
                .expect("Couldn't find track");
            format!("https://vps.kodub.com/leaderboard?version={VERSION}&trackId={}&skip=0&amount=500&onlyVerified=false&userTokenHash={id}",track_id.0)
        } else {
            format!("https://vps.kodub.com/leaderboard?version={VERSION}&trackId={track}&skip=0&amount=500&onlyVerified=false&userTokenHash={id}")
        };
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
                            let mut time = (f64::from(frames) / 1000.0).to_string();
                            time.push('s');
                            contents =
                                vec![position.to_string(), time, (found.len() + 1).to_string()];
                            write_embed(
                                ctx,
                                vec![WriteEmbed::new(3)
                                    .title("Leaderboard")
                                    .headers(&["Rank", "Time", "Unique"])
                                    .contents(contents)],
                                mobile_friendly,
                            )
                            .await?;
                        } else {
                            let mut time = (f64::from(frames) / 1000.0).to_string();
                            time.push('s');
                            contents = vec![position.to_string(), time];
                            write_embed(
                                ctx,
                                vec![WriteEmbed::new(2)
                                    .title("Leaderboard")
                                    .headers(&["Rank", "Time"])
                                    .contents(contents)],
                                mobile_friendly,
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
    }
    Ok(())
}

/// List standard track records for a user
#[allow(clippy::too_many_lines)]
#[poise::command(slash_command, prefix_command, category = "Query")]
pub async fn list(
    ctx: Context<'_>,
    #[description = "User"]
    #[autocomplete = "autocomplete_users"]
    user: String,
    #[description = "Tracks"] tracks: Option<LeaderboardChoice>,
    #[description = "Hidden"] hidden: Option<bool>,
    #[description = "Mobile friendly mode"] mobile_friendly: Option<bool>,
) -> Result<()> {
    let mobile_friendly = mobile_friendly.unwrap_or(false);
    if hidden.is_some_and(|x| x) {
        ctx.defer_ephemeral().await?;
    } else {
        ctx.defer().await?;
    }
    let tracks = tracks.unwrap_or(LeaderboardChoice::Global);
    let track_file = {
        use LeaderboardChoice::{Community, Et, Global, Hof};
        match tracks {
            Global => TRACK_FILE,
            Community => COMMUNITY_TRACK_FILE,
            Hof => HOF_TRACK_FILE,
            Et => ET_TRACK_FILE,
        }
    };
    let mut id = String::new();
    if let Some(id_test) = ctx
        .data()
        .user_ids
        .lock()
        .expect("Failed to acquire Mutex")
        .get(&user)
    {
        id.clone_from(id_test);
    }
    if id.is_empty() {
        write(&ctx, "`User ID not found`".to_string()).await?;
    } else {
        let client = Client::new();
        let mut line_num: u32 = 0;
        let mut total_time = 0.0;
        let mut display_total = true;
        let track_ids: Vec<(String, String)> = fs::read_to_string(track_file)
            .await?
            .lines()
            .map(|s| {
                let parts = s.split_once(' ').expect("Invalid track ids file");
                (parts.0.to_string(), parts.1.to_string())
            })
            .collect();
        let futures = track_ids.iter().enumerate().map(|(i, track_id)| {
            let client = client.clone();
            let url = format!("https://vps.kodub.com/leaderboard?version={}&trackId={}&skip=0&amount=500&onlyVerified=false&userTokenHash={}",
            VERSION,
            track_id.0,
            id);
            task::spawn(
            async move {
                let mut att = 0;
                let mut res = client.get(&url).send().await?.text().await?;
                    while res.is_empty() && att < REQUEST_RETRY_COUNT {
                        att += 1;
                        sleep(Duration::from_millis(500)).await;
                        res = client.get(&url).send().await?.text().await?;
                    }
                Ok::<(usize, String), reqwest::Error>((i, res))
            })
        });
        let mut results: Vec<(usize, String)> = join_all(futures)
            .await
            .into_iter()
            .map(|res| res.expect("JoinError ig"))
            .filter_map(std::result::Result::ok)
            .collect();
        results.sort_by_key(|(i, _)| *i);
        let responses: Vec<String> = results.into_iter().map(|(_, res)| res).collect();
        let blacklist_file = match tracks {
            LeaderboardChoice::Hof => HOF_BLACKLIST_FILE,
            _ => BLACKLIST_FILE,
        };
        let altlist_file = match tracks {
            LeaderboardChoice::Hof => HOF_ALT_ACCOUNT_FILE,
            _ => ALT_ACCOUNT_FILE,
        };
        let mut contents: Vec<String> = vec![String::new(), String::new(), String::new()];
        let mut headers = vec!["Track", "Rank", "Time"];
        let mut inlines = vec![true, true, true];
        for response in responses {
            if let Ok(leaderboard) = serde_json::from_str::<LeaderBoard>(&response) {
                if let Some(user_entry) = leaderboard.user_entry {
                    let position = user_entry.position;
                    let frames = user_entry.frames;
                    let time = f64::from(frames) / 1000.0;
                    total_time += time;
                    let mut time = format!("{time:.3}");
                    time.push('s');
                    contents[0].push_str(format!("{}\n", track_ids[line_num as usize].1).as_str());
                    contents[2].push_str(format!("{time}\n").as_str());
                    if position <= 501 {
                        let entries = leaderboard.entries;
                        let mut found: Vec<String> = Vec::new();
                        let mut i = 0;
                        for entry in entries {
                            i += 1;
                            if i == position {
                                break;
                            }
                            let name = get_alt(altlist_file, &entry.name).await?;
                            if entry.verified_state == 1
                                && !found.contains(&name)
                                && check_blacklist(blacklist_file, &name).await?
                            {
                                found.push(name);
                            }
                        }
                        writeln!(contents[1], "{position} [{}]", (found.len() + 1))?;
                    } else {
                        writeln!(contents[1], "{position}")?;
                    }
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
            let total_time = (total_time * 1000.0).floor();
            contents.push(format!(
                "{:>2}:{:0>2}.{:0>3}",
                (total_time / 60000.0).floor(),
                (total_time % 60000.0 / 1000.0).floor(),
                (total_time % 1000.0).floor()
            ));
            headers.push("Total");
            inlines.push(false);
        }
        write_embed(
            ctx,
            vec![WriteEmbed::new(headers.len())
                .title(&user)
                .headers(&headers)
                .contents(contents)
                .inlines(inlines)],
            mobile_friendly,
        )
        .await?;
    }
    Ok(())
}

/// Compares two user's record times and placements
#[allow(clippy::too_many_lines)]
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
) -> Result<()> {
    if hidden.is_some_and(|x| x) {
        ctx.defer_ephemeral().await?;
    } else {
        ctx.defer().await?;
    }
    let tracks = tracks.unwrap_or(LeaderboardChoice::Global);
    let mut results: Vec<Vec<(u32, f64)>> = Vec::new();
    let track_ids: Vec<(String, String)> = fs::read_to_string({
        use LeaderboardChoice::{Community, Et, Global, Hof};
        match tracks {
            Global => TRACK_FILE,
            Community => COMMUNITY_TRACK_FILE,
            Hof => HOF_TRACK_FILE,
            Et => ET_TRACK_FILE,
        }
    })
    .await?
    .lines()
    .map(|s| {
        let parts = s.split_once(' ').expect("Invalid track ids file");
        (parts.0.to_string(), parts.1.to_string())
    })
    .collect();
    let track_names: Vec<String> = track_ids.iter().map(|(_, name)| name.clone()).collect();
    for user in [user1.clone(), user2.clone()] {
        let mut user_results: Vec<(u32, f64)> = Vec::new();
        let mut id = String::new();
        if let Some(id_test) = ctx
            .data()
            .user_ids
            .lock()
            .expect("Failed to acquire Mutex")
            .get(&user)
        {
            id.clone_from(id_test);
        }
        if id.is_empty() {
            write(&ctx, "`User ID not found`".to_string()).await?;
        } else {
            let client = Client::new();
            let mut total_time = 0.0;
            let mut display_total = true;
            let futures = track_ids.iter().enumerate().map(|(i, track_id)| {
                let client = client.clone();
                let url = format!("https://vps.kodub.com/leaderboard?version={}&trackId={}&skip=0&amount=1&onlyVerified=false&userTokenHash={}",
                    VERSION,
                    track_id.0,
                    id
                );
                task::spawn(
                    async move {
                        let mut att = 0;
                        let mut res = client.get(&url).send().await?.text().await?;
                        while res.is_empty() && att < REQUEST_RETRY_COUNT {
                            att += 1;
                            sleep(Duration::from_millis(500)).await;
                            res = client.get(&url).send().await?.text().await?;
                        }
                        Ok::<(usize, String), reqwest::Error>((i, res))
                    }
                )
            });
            let mut results: Vec<(usize, String)> = join_all(futures)
                .await
                .into_iter()
                .map(|res| res.expect("JoinError ig"))
                .filter_map(std::result::Result::ok)
                .collect();
            results.sort_by_key(|(i, _)| *i);
            let responses: Vec<String> = results.into_iter().map(|(_, res)| res).collect();
            for response in responses {
                if let Ok(leaderboard) = serde_json::from_str::<LeaderBoard>(&response) {
                    if let Some(user_entry) = leaderboard.user_entry {
                        let position = user_entry.position;
                        let frames = user_entry.frames;
                        let time = f64::from(frames) / 1000.0;
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
        }
        results.push(user_results);
    }
    let mut output = String::new();
    let mut display_total_diff = true;
    let max_track_len = track_ids
        .iter()
        .map(|(_, t)| t.len())
        .max()
        .expect("Empty track ids file")
        .max(5);
    let column_gap = 3;
    write!(output, "```\n{}", " ".repeat(max_track_len + 2))?;
    for user in [user1.clone(), user2.clone()] {
        write!(output, "{user:>18}")?;
        output.push_str(&" ".repeat(column_gap));
    }
    output.push_str("Difference\n");
    for i in 0..results[0].len() - 1 {
        let mut display_diff = true;
        write!(
            output,
            "{:>width$}: ",
            track_names[i],
            width = max_track_len
        )?;
        for track in &results {
            if track[i].1 == 0.0 {
                output.push_str(
                    format!("{:>18}{}", "Record not found", " ".repeat(column_gap)).as_str(),
                );
                display_diff = false;
            } else {
                write!(
                    output,
                    "{:>6}. - {:>7.3}s{}",
                    track[i].0,
                    track[i].1,
                    " ".repeat(column_gap)
                )?;
            }
        }
        if display_diff {
            output.push_str(format!("{:>9.3}s", (results[0][i].1 - results[1][i].1)).as_str());
        }
        output.push('\n');
    }
    write!(output, "\n{:>width$}: ", "Total", width = max_track_len)?;
    for track in &results {
        let total = track.last().expect("Should have a last track").1.floor();
        if total == 0.0 {
            write!(output, "{:>18}", "Tracks not done")?;
            display_total_diff = false;
        } else {
            write!(
                output,
                "{}{:>2}:{:0>2}.{:0>3}{}",
                " ".repeat(9),
                (total / 60000.0).floor(),
                (total % 60000.0 / 1000.0).floor(),
                (total % 1000.0).floor(),
                " ".repeat(column_gap)
            )?;
        }
    }
    if display_total_diff {
        output.push_str(
            format!(
                "{:>9.3}s",
                ((results[0].last().expect("should have last one").1
                    - results[1].last().expect("should have last one").1)
                    / 1000.0)
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
    #[description = "Mobile friendly mode"] mobile_friendly: Option<bool>,
) -> Result<()> {
    use LeaderboardChoice::{Community, Et, Global, Hof};
    let mobile_friendly = mobile_friendly.unwrap_or(false);
    ctx.defer_ephemeral().await?;
    let (is_admin, is_admin_msg) = is_admin(&ctx, 2).await;
    if !is_admin {
        write(&ctx, is_admin_msg).await?;
        return Ok(());
    }
    match leaderboard {
        Global => global_rankings_update().await,
        Community => community_update().await,
        Hof => hof_update().await,
        Et => et_rankings_update().await,
    }?;
    let headers: Vec<&str> = vec![
        "Rank",
        {
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
        Et => ET_RANKINGS_FILE,
    })
    .await?;
    let line = content.lines().next().expect("Should have next line");
    let lb: PolyLeaderBoard = serde_json::from_str(line).expect("Invalid leaderboard");
    for i in 0..lb.total {
        writeln!(contents[0], "{}", lb.entries[i].rank)?;
        writeln!(contents[1], "{}", lb.entries[i].stat)?;
        writeln!(contents[2], "{}", lb.entries[i].name)?;
    }
    let inlines: Vec<bool> = vec![true, true, true];
    write_embed(
        ctx,
        vec![WriteEmbed::new(headers.len())
            .title(&format!("{} Leaderboard", leaderboard.name()))
            .headers(&headers)
            .contents(contents)
            .inlines(inlines)],
        mobile_friendly,
    )
    .await?;
    Ok(())
}

#[allow(clippy::too_many_lines)]
#[poise::command(slash_command, prefix_command, category = "Query")]
pub async fn roles(
    ctx: Context<'_>,
    #[description = "Mobile friendly mode"] mobile_friendly: Option<bool>,
) -> Result<()> {
    let mobile_friendly = mobile_friendly.unwrap_or(false);
    ctx.defer_ephemeral().await?;
    let mut embeds: Vec<WriteEmbed> = Vec::new();
    let champion_contents = {
        let mut champions = vec![String::new(); 2];
        let ct_champion = serde_json::from_str::<PolyLeaderBoard>(
            fs::read_to_string(COMMUNITY_RANKINGS_FILE)
                .await?
                .lines()
                .next()
                .expect("Should have next line"),
        )?
        .entries
        .first()
        .expect("Should have first entry")
        .name
        .clone();
        writeln!(champions[0], "{ct_champion}")?;
        champions[1].push_str("CT Champion\n");
        let hof_champion = serde_json::from_str::<PolyLeaderBoard>(
            fs::read_to_string(HOF_RANKINGS_FILE)
                .await?
                .lines()
                .next()
                .expect("Should have next line"),
        )?
        .entries
        .first()
        .expect("Should have first entry")
        .name
        .clone();
        writeln!(champions[0], "{hof_champion}")?;
        champions[1].push_str("HOF Champion\n");
        let wr_champion = get_records(LeaderboardChoice::Global)
            .await?
            .wr_amounts
            .iter()
            .max_by_key(|(_, v)| *v)
            .expect("Should have max")
            .0
            .clone();
        writeln!(champions[0], "{wr_champion}")?;
        champions[1].push_str("WR Champion\n");
        champions
    };
    let champion_embed = WriteEmbed::new(2)
        .title("Champions")
        .headers(&["User", "Title"])
        .contents(champion_contents);
    embeds.push(champion_embed);
    let wr_holder_contents = {
        let mut wr_holders = vec![String::new(); 2];
        let hof_poly_records = get_records(LeaderboardChoice::Hof).await?;
        let hof_records = hof_poly_records
            .wr_amounts
            .keys()
            .filter(|k| *k != "Anonymous" && *k != "unknown");
        let hof_record_amount = hof_records.clone().count();
        wr_holders[0].push_str(&hof_records.fold(String::new(), |acc, k| acc + &format!("{k}\n")));
        wr_holders[1].push_str(&"HOF WR Holder\n".repeat(hof_record_amount));
        let ct_poly_records = get_records(LeaderboardChoice::Community).await?;
        let ct_records = ct_poly_records
            .wr_amounts
            .keys()
            .filter(|k| *k != "Anonymous" && *k != "unknown");
        let ct_record_amount = ct_records.clone().count();
        wr_holders[0].push_str(&ct_records.fold(String::new(), |acc, k| acc + &format!("{k}\n")));
        wr_holders[1].push_str(&"CT WR Holder\n".repeat(ct_record_amount));
        wr_holders
    };
    let wr_holder_embed = WriteEmbed::new(2)
        .title("WR Holders")
        .headers(&["User", "Title"])
        .contents(wr_holder_contents);
    embeds.push(wr_holder_embed);
    let global_grandmaster_contents = {
        let mut global_grandmasters = Vec::new();
        let mut main_leaderboard =
            serde_json::from_str::<PolyLeaderBoard>(&fs::read_to_string(RANKINGS_FILE).await?)?
                .entries
                .iter()
                .take_while(|entry| entry.rank < 21)
                .map(|e| e.name.clone())
                .collect();
        global_grandmasters.append(&mut main_leaderboard);
        let mut community_leaderboard = serde_json::from_str::<PolyLeaderBoard>(
            fs::read_to_string(COMMUNITY_RANKINGS_FILE)
                .await?
                .lines()
                .next()
                .expect("Should have first line"),
        )?
        .entries
        .iter()
        .take_while(|entry| entry.rank < 21)
        .map(|e| e.name.clone())
        .collect();
        global_grandmasters.append(&mut community_leaderboard);
        global_grandmasters.sort();
        global_grandmasters.dedup();
        global_grandmasters.join("\n")
    };
    let global_grandmaster_embed = WriteEmbed::new(1)
        .title("Global Grandmaster")
        .headers(&["User"])
        .contents(vec![global_grandmaster_contents]);
    embeds.push(global_grandmaster_embed);
    write_embed(ctx, embeds, mobile_friendly).await?;
    Ok(())
}

/// Leaderboard for official tracks
#[poise::command(slash_command, prefix_command, category = "Query")]
pub async fn rankings(
    ctx: Context<'_>,
    #[description = "Leaderboard"] leaderboard: Option<LeaderboardChoice>,
    #[description = "Mode (HOF/community only)"] time_based: Option<bool>,
    #[description = "Hidden"] hidden: Option<bool>,
    #[description = "Mobile friendly mode"] mobile_friendly: Option<bool>,
) -> Result<()> {
    use LeaderboardChoice::{Community, Et, Global, Hof};
    let mobile_friendly = mobile_friendly.unwrap_or(false);
    if hidden.is_some_and(|x| x) {
        ctx.defer_ephemeral().await?;
    } else {
        ctx.defer().await?;
    }
    let leaderboard = leaderboard.unwrap_or(LeaderboardChoice::Global);
    let time_based = time_based.unwrap_or(false);
    let rankings_file = match leaderboard {
        Global => RANKINGS_FILE,
        Community => {
            if time_based {
                COMMUNITY_TIME_RANKINGS_FILE
            } else {
                COMMUNITY_RANKINGS_FILE
            }
        }
        Hof => {
            if time_based {
                HOF_TIME_RANKINGS_FILE
            } else {
                HOF_RANKINGS_FILE
            }
        }
        Et => ET_RANKINGS_FILE,
    };
    let duration = if fs::try_exists(rankings_file).await? {
        let age = fs::metadata(rankings_file).await?.modified()?.elapsed()?;
        if age > UPDATE_CYCLE_LEN {
            UPDATE_CYCLE_LEN
        } else {
            UPDATE_CYCLE_LEN - age
        }
    } else {
        match leaderboard {
            Global => global_rankings_update().await?,
            Community => community_update().await?,
            Hof => hof_update().await?,
            Et => et_rankings_update().await?,
        }
        UPDATE_CYCLE_LEN
    }
    .as_millis();
    let headers: Vec<&str> = vec![
        "Rank",
        {
            match leaderboard {
                Global => "Time",
                _ => {
                    if time_based {
                        "Time"
                    } else {
                        "Points"
                    }
                }
            }
        },
        "Player",
        "Update in",
    ];
    let mut contents: Vec<String> = vec![
        String::new(),
        String::new(),
        String::new(),
        format!(
            "{}:{:0>2}.{:0>3}",
            duration / 60000,
            duration / 1000 % 60,
            duration % 1000
        ),
    ];
    let content = fs::read_to_string(rankings_file).await?;
    let line = content.lines().next().expect("Should have first line");
    let lb: PolyLeaderBoard = serde_json::from_str(line).expect("Invalid leaderboard");
    for i in 0..lb.total {
        writeln!(contents[0], "{}", lb.entries[i].rank)?;
        writeln!(contents[1], "{}", lb.entries[i].stat)?;
        writeln!(contents[2], "{}", lb.entries[i].name)?;
    }
    let inlines: Vec<bool> = vec![true, true, true, false];
    write_embed(
        ctx,
        vec![WriteEmbed::new(headers.len())
            .title(&format!("{} Leaderboard", leaderboard.name()))
            .headers(&headers)
            .contents(contents)
            .inlines(inlines)],
        mobile_friendly,
    )
    .await?;
    Ok(())
}

#[poise::command(slash_command, category = "Query")]
pub async fn tracks(
    ctx: Context<'_>,
    #[description = "Tracks"] tracks: LeaderboardChoice,
) -> Result<()> {
    use LeaderboardChoice::{Et, Hof};
    let track_file = match tracks {
        Et => Some(ET_CODE_FILE),
        Hof => Some(HOF_CODE_FILE),
        _ => None,
    };
    if let Some(track_file) = track_file {
        let content = fs::read_to_string(track_file).await?;
        let mut codes = Vec::new();
        let mut options: Vec<CreateSelectMenuOption> = Vec::new();
        for (i, line) in content.lines().enumerate() {
            let (code, name) = line.split_once(' ').unwrap_or_default();
            codes.push(code);
            options.push(CreateSelectMenuOption::new(name, i.to_string()));
        }
        let mut reply = CreateReply::default().ephemeral(true);
        let select_menu =
            CreateSelectMenu::new("track_selector", CreateSelectMenuKind::String { options })
                .min_values(1)
                .max_values(1);
        let action_row = CreateActionRow::SelectMenu(select_menu);
        reply = reply.components(vec![action_row]);
        let reply_handle = ctx.send(reply).await?;
        if let Some(interaction) = ComponentInteractionCollector::new(ctx)
            .filter(move |select| select.data.custom_id == "track_selector")
            .timeout(Duration::from_secs(60))
            .await
        {
            if let ComponentInteractionDataKind::StringSelect { ref values } = interaction.data.kind
            {
                let selection = values.first().expect("guaranteed to exist");
                let code = codes
                    .get(selection.parse::<usize>().expect("should be integer"))
                    .expect("should be in range");
                interaction
                    .create_response(
                        ctx.serenity_context().http.clone(),
                        serenity::CreateInteractionResponse::Message(
                            if code.len() + "Track Code:\n".len() > 1024 {
                                let attachment =
                                    CreateAttachment::bytes(code.as_bytes(), "track_code.txt");
                                CreateInteractionResponseMessage::new()
                                    .add_file(attachment)
                                    .ephemeral(true)
                            } else {
                                CreateInteractionResponseMessage::new()
                                    .content(format!("Track Code:\n{code}"))
                                    .ephemeral(true)
                            },
                        ),
                    )
                    .await?;
                reply_handle.delete(ctx).await?;
            }
        }
    } else {
        ctx.defer_ephemeral().await?;
        write(
            &ctx,
            format!("Could not find track list for {}", tracks.name()),
        )
        .await?;
    }
    Ok(())
}

/// Lets privileged users edit certain internal data
#[poise::command(slash_command, category = "Administration", ephemeral)]
pub async fn edit_lists(
    ctx: ApplicationContext<'_, BotData, Error>,
    #[description = "List to edit"] list: EditModalChoice,
) -> Result<()> {
    use EditModalChoice::{Alt, Black, HOFAlt, HOFBlack};
    let (is_admin, is_admin_msg) = is_admin(&ctx.into(), 2).await;
    if !is_admin {
        write(&ctx.into(), is_admin_msg).await?;
        return Ok(());
    }
    let list_file = {
        match list {
            Black => BLACKLIST_FILE,
            Alt => ALT_ACCOUNT_FILE,
            HOFBlack => HOF_BLACKLIST_FILE,
            HOFAlt => HOF_ALT_ACCOUNT_FILE,
        }
    };
    let list_content = match list {
        Black | HOFBlack => read_blacklist(list_file).await?,
        Alt | HOFAlt => read_altlist(list_file).await?,
    };
    let modal_defaults = EditModal { list: list_content };
    let modal_returned = EditModal::execute_with_defaults(ctx, modal_defaults.clone())
        .await?
        .unwrap_or(modal_defaults);
    match list {
        Black | HOFBlack => write_blacklist(list_file, modal_returned.list).await?,
        Alt | HOFAlt => write_altlist(list_file, modal_returned.list).await?,
    }
    Ok(())
}

/// Lists currently registered users and their IDs
#[poise::command(slash_command, prefix_command, category = "Info", ephemeral)]
pub async fn users(ctx: Context<'_>) -> Result<()> {
    let bot_data = ctx.data();
    let mut users = String::new();
    for (user, id) in bot_data
        .user_ids
        .lock()
        .expect("Failed to acquire Mutex")
        .iter()
    {
        writeln!(users, "{user}: {id}")?;
    }
    write(&ctx, format!("```{users}```")).await?;
    Ok(())
}

#[poise::command(slash_command, prefix_command, category = "Administration", ephemeral)]
pub async fn admins(ctx: Context<'_>) -> Result<()> {
    let bot_data = ctx.data();
    let mut admins = String::new();
    for (admin, privilege) in bot_data
        .admins
        .lock()
        .expect("Failed to acquire Mutex")
        .iter()
    {
        writeln!(admins, "{admin}: {privilege}")?;
    }
    write(&ctx, format!("```{admins}```")).await?;
    Ok(())
}

/// Displays player numbers
#[poise::command(slash_command, prefix_command, category = "Info")]
pub async fn players(
    ctx: Context<'_>,
    #[description = "Tracks"] tracks: Option<LeaderboardChoice>,
    #[description = "Hidden"] hidden: Option<bool>,
    #[description = "Mobile friendly mode"] mobile_friendly: Option<bool>,
) -> Result<()> {
    use LeaderboardChoice::{Community, Et, Global, Hof};
    let mobile_friendly = mobile_friendly.unwrap_or(false);
    if hidden.is_some_and(|x| x) {
        ctx.defer_ephemeral().await?;
    } else {
        ctx.defer().await?;
    }
    let tracks = tracks.unwrap_or(LeaderboardChoice::Global);
    let track_ids: Vec<(String, String)> = fs::read_to_string({
        match tracks {
            Global => TRACK_FILE,
            Community => COMMUNITY_TRACK_FILE,
            Hof => HOF_ALL_TRACK_FILE,
            Et => ET_TRACK_FILE,
        }
    })
    .await
    .expect("Failed to read file")
    .lines()
    .map(|s| {
        let parts = s.split_once(' ').expect("Invalid track ids file");
        (parts.0.to_string(), parts.1.to_string())
    })
    .collect();
    let mut contents = vec![String::new(), String::new()];
    let client = Client::new();
    for (id, name) in track_ids {
        let url = format!("https://vps.kodub.com/leaderboard?version={}&trackId={}&skip=0&amount=1&onlyVerified=false",
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
        writeln!(
            contents.get_mut(0).expect("Should have first entry"),
            "{name}"
        )?;
        writeln!(
            contents.get_mut(1).expect("Should have second entry"),
            "{number}"
        )?;
    }
    write_embed(
        ctx,
        vec![WriteEmbed::new(2)
            .title("Player numbers")
            .headers(&["Track", "Players"])
            .contents(contents)],
        mobile_friendly,
    )
    .await?;
    Ok(())
}

#[poise::command(slash_command, prefix_command, category = "Info")]
pub async fn records(
    ctx: Context<'_>,
    #[description = "Tracks"] tracks: Option<LeaderboardChoice>,
    #[description = "Hidden"] hidden: Option<bool>,
    #[description = "Mobile friendly mode"] mobile_friendly: Option<bool>,
) -> Result<()> {
    let mobile_friendly = mobile_friendly.unwrap_or(false);
    if hidden.is_some_and(|x| x) {
        ctx.defer_ephemeral().await?;
    } else {
        ctx.defer().await?;
    }
    let tracks = tracks.unwrap_or(LeaderboardChoice::Global);
    let poly_records = get_records(tracks).await?;
    let contents = poly_records.records.iter().map(|v| v.join("\n")).collect();
    let embed1 = WriteEmbed::new(3)
        .title("World Records")
        .headers(&["Track", "Player", "Time"])
        .contents(contents);
    let mut wr_amounts: Vec<(String, u32)> = poly_records.wr_amounts.into_iter().collect();
    wr_amounts.sort_by_key(|(_, k)| *k);
    wr_amounts.reverse();
    let mut contents = vec![String::new(), String::new()];
    for (name, amount) in wr_amounts {
        writeln!(
            contents.get_mut(0).expect("Should have first entry"),
            "{name}",
        )?;
        writeln!(
            contents.get_mut(1).expect("Should have second entry"),
            "{amount}",
        )?;
    }
    let embed2 = WriteEmbed::new(2)
        .title("WR Amounts")
        .headers(&["Player", "Amount"])
        .contents(contents);
    write_embed(ctx, vec![embed1, embed2], mobile_friendly).await?;
    Ok(())
}

#[poise::command(slash_command, prefix_command, category = "Info")]
pub async fn top(
    ctx: Context<'_>,
    #[description = "Position"] position: u32,
    #[description = "Tracks"] tracks: Option<LeaderboardChoice>,
    #[description = "Hidden"] hidden: Option<bool>,
    #[description = "Mobile friendly mode"] mobile_friendly: Option<bool>,
) -> Result<()> {
    use LeaderboardChoice::{Community, Et, Global, Hof};
    let mobile_friendly = mobile_friendly.unwrap_or(false);
    if hidden.is_some_and(|x| x) {
        ctx.defer_ephemeral().await?;
    } else {
        ctx.defer().await?;
    }
    let tracks = tracks.unwrap_or(LeaderboardChoice::Global);
    let track_ids: Vec<(String, String)> = fs::read_to_string({
        match tracks {
            Global => TRACK_FILE,
            Community => COMMUNITY_TRACK_FILE,
            Hof => HOF_ALL_TRACK_FILE,
            Et => ET_TRACK_FILE,
        }
    })
    .await
    .expect("Failed to read file")
    .lines()
    .map(|s| {
        let parts = s.split_once(' ').expect("Invalid track ids file");
        (parts.0.to_string(), parts.1.to_string())
    })
    .collect();
    let mut contents = vec![String::new(), String::new(), String::new()];
    let client = Client::new();
    for (id, name) in track_ids {
        let url = format!("https://vps.kodub.com/leaderboard?version={}&trackId={}&skip={}&amount=1&onlyVerified=true",
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
        writeln!(
            contents.get_mut(0).expect("Should have first entry"),
            "{name}",
        )?;
        writeln!(
            contents.get_mut(1).expect("Should have second entry"),
            "{winner_name}",
        )?;
        writeln!(
            contents.get_mut(2).expect("Should have third entry"),
            "{winner_time}s",
        )?;
    }
    write_embed(
        ctx,
        vec![WriteEmbed::new(3)
            .title(&format!("Top {position}"))
            .headers(&["Track", "Player", "Time"])
            .contents(contents)],
        mobile_friendly,
    )
    .await?;
    Ok(())
}

/// Links the privacy policy
#[poise::command(slash_command, prefix_command, category = "Info", ephemeral)]
pub async fn policy(ctx: Context<'_>) -> Result<()> {
    dotenv().ok();
    let url = format!(
        "https://{}/policy",
        env::var("WEBSITE_URL").expect("Expected WEBSITE_URL in env!")
    );
    write(&ctx, format!("Privacy Policy: <{url}>")).await?;
    Ok(())
}

/// Displays help
#[poise::command(slash_command, prefix_command, track_edits, category = "Info")]
pub async fn help(ctx: Context<'_>, #[description = "Command"] cmd: Option<String>) -> Result<()> {
    let config = builtins::HelpConfiguration {
        extra_text_at_bottom: "\
            Type /help <cmd> for more detailed help.",
        ephemeral: true,
        ..Default::default()
    };
    builtins::help(ctx, cmd.as_deref(), config).await?;
    Ok(())
}
