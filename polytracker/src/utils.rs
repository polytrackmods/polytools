use crate::commands::LeaderboardChoice;
use crate::{Context, ET_PERIOD_DURATION, MAX_MSG_AGE};
use anyhow::Result;
use chrono::Utc;
use diesel::prelude::*;
use poise::serenity_prelude::{self as serenity, CacheHttp, CreateEmbedFooter, GetMessages, Http};
use poise::{CreateReply, Modal};
use polymanager::db::{Admin, NewAdmin, NewUser, User};
use polymanager::{
    check_blacklist, export_to_id, get_alt, recent_et_period, send_to_networker, ALT_ACCOUNT_FILE,
    BLACKLIST_FILE, COMMUNITY_TRACK_FILE, ET_CODE_FILE, ET_TRACK_FILE, HOF_ALL_TRACK_FILE,
    HOF_ALT_ACCOUNT_FILE, HOF_BLACKLIST_FILE, REQUEST_RETRY_COUNT, TRACK_FILE, VERSION,
};
use regex::Regex;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serenity::{
    ChannelId, Color, ComponentInteractionCollector, CreateActionRow, CreateAttachment,
    CreateButton, CreateEmbed, CreateInteractionResponse, CreateInteractionResponseMessage,
    GuildId,
};
use std::collections::HashMap;
use std::fmt::Write;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::fs;
use tokio::time::sleep;

const EMBED_PAGE_LEN: usize = 20;
const MAX_COL_WIDTH: usize = 25;
const TRACK_CODE_STARTS: [&str; 2] = ["PolyTrack14p", "v3"];

// structs for deserializing leaderboards
#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LeaderBoardEntry {
    pub name: String,
    pub frames: f64,
    pub verified_state: u8,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LeaderBoard {
    pub entries: Vec<LeaderBoardEntry>,
    pub total: u32,
    pub user_entry: Option<UserEntry>,
}

#[derive(Deserialize, Serialize)]
pub struct UserEntry {
    pub position: u32,
    pub frames: u32,
    id: u64,
}

// used by edit_lists() for the modal
#[derive(Modal, Clone)]
#[name = "List Editor"]
pub struct EditModal {
    #[name = "List"]
    #[paragraph]
    pub list: String,
}

#[derive(Modal, Default)]
#[name = "Add Admin"]
pub struct AddAdminModal {
    #[name = "Discord username"]
    pub discord: String,
    #[name = "Privilege level"]
    pub privilege: String,
}
#[derive(Modal, Default)]
#[name = "Remove Admin"]
pub struct RemoveAdminModal {
    #[name = "Discord username"]
    pub discord: String,
}
#[derive(Modal, Default)]
#[name = "Edit Admin"]
pub struct EditAdminModal {
    #[name = "Discord username"]
    pub discord: String,
    #[name = "Privilege"]
    pub privilege: String,
}

// the bot's shared data
pub struct BotData {
    pub user_ids: Mutex<HashMap<String, String>>,
    pub admins: Mutex<HashMap<String, u32>>,
    pub conn: Mutex<SqliteConnection>,
}

#[allow(clippy::missing_panics_doc)]
#[allow(clippy::cast_sign_loss)]
impl BotData {
    pub fn load(&self) {
        use polymanager::schema::admins::dsl::admins;
        use polymanager::schema::users::dsl::users;
        let connection = &mut *self.conn.lock().expect("Failed to acquire Mutex");
        let results = users
            .select(User::as_select())
            .load(connection)
            .expect("Error loading users");
        let mut user_ids = self.user_ids.lock().expect("Failed to acquire Mutex");
        user_ids.clear();
        for user in results {
            user_ids.insert(user.name, user.game_id);
        }
        drop(user_ids);
        let results = admins
            .select(Admin::as_select())
            .load(connection)
            .expect("Error loading users");
        let mut admin_ids = self.admins.lock().expect("Failed to acquire Mutex");
        admin_ids.clear();
        for admin in results {
            admin_ids.insert(admin.discord, admin.privilege as u32);
        }
    }
    pub fn add(&self, name: &str, game_id: &str) {
        use polymanager::schema::users;
        let connection = &mut *self.conn.lock().expect("Failed to acquire Mutex");
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
    pub fn delete(&self, delete_name: &str) {
        use polymanager::schema::users::dsl::{name, users};
        let connection = &mut *self.conn.lock().expect("Failed to acquire Mutex");
        diesel::delete(users.filter(name.eq(delete_name)))
            .execute(connection)
            .expect("Error deleting user");
    }
    pub fn add_admin(&self, discord: &str, privilege: i32) {
        use polymanager::schema::admins;
        let connection = &mut *self.conn.lock().expect("Failed to acquire Mutex");
        let new_admin = NewAdmin {
            discord,
            privilege: &privilege,
        };
        diesel::insert_into(admins::table)
            .values(new_admin)
            .returning(Admin::as_returning())
            .get_result(connection)
            .expect("Error adding new admin");
    }
    pub fn remove_admin(&self, admin_discord: &str) {
        use polymanager::schema::admins::dsl::{admins, discord};
        let connection = &mut *self.conn.lock().expect("Failed to acquire Mutex");
        diesel::delete(admins.filter(discord.eq(admin_discord)))
            .execute(connection)
            .expect("Error deleting admin");
    }
    pub fn edit_admin(&self, admin_discord: &str, new_privilege: i32) {
        use polymanager::schema::admins::dsl::{admins, discord, privilege};
        let connection = &mut *self.conn.lock().expect("Failed to acquire Mutex");
        diesel::update(admins.filter(discord.eq(admin_discord)))
            .set(privilege.eq(new_privilege))
            .returning(Admin::as_returning())
            .get_result(connection)
            .expect("Error editing admin");
    }
}

// non-embed output function
#[allow(clippy::missing_panics_doc)]
#[allow(clippy::missing_errors_doc)]
pub async fn write(ctx: &Context<'_>, mut text: String) -> Result<()> {
    if text.len() > 2000 {
        if text.starts_with("```") {
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

#[derive(Default, Debug)]
pub struct WriteEmbed {
    title: String,
    description: String,
    headers: Vec<String>,
    contents: Vec<String>,
    inlines: Vec<bool>,
}

impl WriteEmbed {
    #[must_use]
    pub fn new(field_count: usize) -> Self {
        Self {
            title: "PolyTracker Embed".to_string(),
            description: String::new(),
            headers: vec![String::new(); field_count],
            contents: vec![String::new(); field_count],
            inlines: vec![true; field_count],
        }
    }
    #[must_use]
    pub fn title(mut self, title: &str) -> Self {
        self.title = title.to_string();
        self
    }
    #[must_use]
    pub fn description(mut self, description: &str) -> Self {
        self.description = description.to_string();
        self
    }
    #[must_use]
    pub fn headers(mut self, headers: &[&str]) -> Self {
        self.headers = headers
            .iter()
            .map(std::string::ToString::to_string)
            .collect();
        self
    }
    #[must_use]
    pub fn contents(mut self, contents: Vec<String>) -> Self {
        self.contents = contents;
        self
    }
    #[must_use]
    pub fn inlines(mut self, inlines: Vec<bool>) -> Self {
        self.inlines = inlines;
        self
    }
}

#[derive(Clone, Debug)]
struct PagedEmbed {
    title: String,
    description: String,
    pages: Vec<EmbedPage>,
}
impl PagedEmbed {
    fn to_create_embed(&self, page: usize, max_page_amt: usize) -> CreateEmbed {
        let mut create_embed = CreateEmbed::default()
            .title(self.title.clone())
            .description(self.description.clone())
            .colour(Color::from_rgb(0, 128, 128))
            .fields(self.pages.get(page).expect("should have that page").clone());
        if max_page_amt > 1 {
            create_embed = create_embed.footer(CreateEmbedFooter::new(format!(
                "Page {}/{max_page_amt}",
                page + 1
            )));
        }
        create_embed
    }
}
#[derive(Clone, Debug)]
struct EmbedPage {
    columns: Vec<EmbedColumn>,
}
impl IntoIterator for EmbedPage {
    type Item = (String, String, bool);
    type IntoIter = std::vec::IntoIter<Self::Item>;
    fn into_iter(self) -> Self::IntoIter {
        self.columns
            .into_iter()
            .map(|c| (c.header, c.content, c.inline))
            .collect::<Vec<_>>()
            .into_iter()
    }
}
#[derive(Clone, Debug)]
struct EmbedColumn {
    header: String,
    content: String,
    inline: bool,
}

// output function using embeds
#[allow(clippy::missing_panics_doc)]
#[allow(clippy::missing_errors_doc)]
#[allow(clippy::too_many_lines)]
pub async fn write_embed(
    ctx: Context<'_>,
    write_embeds: Vec<WriteEmbed>,
    mobile_friendly: bool,
) -> Result<()> {
    if write_embeds
        .iter()
        .all(|e| e.headers.len() == e.contents.len() && e.headers.len() == e.inlines.len())
    {
        let ctx_id = ctx.id();
        let prev_id = format!("{ctx_id}prev");
        let next_id = format!("{ctx_id}next");
        let start_id = format!("{ctx_id}start");
        let mut embeds = Vec::new();
        for (i, write_embed) in write_embeds.iter().enumerate() {
            let content_columns: Vec<Vec<&str>> = write_embed
                .contents
                .iter()
                .map(|col| col.lines().collect::<Vec<_>>())
                .collect();
            let mut paged_embed: PagedEmbed = PagedEmbed {
                title: write_embed.title.clone(),
                description: write_embed.description.clone(),
                pages: Vec::new(),
            };
            let mut max_page_amt = content_columns
                .iter()
                .map(std::vec::Vec::len)
                .max()
                .expect("should have content")
                .div_ceil(EMBED_PAGE_LEN);
            max_page_amt += usize::from(max_page_amt == 0);
            for page in 0..max_page_amt {
                let new_page = EmbedPage {
                    columns: if mobile_friendly {
                        let col_lens = write_embed
                            .inlines
                            .iter()
                            .enumerate()
                            .map(|(i, inline)| {
                                if *inline {
                                    let max_len = content_columns
                                        .get(i)
                                        .expect("should have that column")
                                        .get(EMBED_PAGE_LEN * page..EMBED_PAGE_LEN * (page + 1))
                                        .unwrap_or_else(|| {
                                            content_columns
                                                .get(i)
                                                .expect("should have that column")
                                                .get(EMBED_PAGE_LEN * page..)
                                                .expect("should have that many rows")
                                        })
                                        .iter()
                                        .max_by_key(|l| l.len())
                                        .expect("column shouldn't be empty")
                                        .len();
                                    Some(
                                        (max_len.max(
                                            write_embed
                                                .headers
                                                .get(i)
                                                .expect("should have that header")
                                                .len(),
                                        ) + 2)
                                            .min(MAX_COL_WIDTH),
                                    )
                                } else {
                                    None
                                }
                            })
                            .collect::<Vec<_>>();
                        let mut joined_columns =
                            vec![
                                EmbedColumn {
                                    header: String::new(),
                                    content: String::new(),
                                    inline: false,
                                };
                                col_lens.iter().filter(|l| l.is_none()).count()
                                    + usize::from(
                                        col_lens.iter().any(std::option::Option::is_some)
                                    )
                            ];
                        for row in 0..content_columns
                            .first()
                            .expect("should have first column")
                            .get(EMBED_PAGE_LEN * page..EMBED_PAGE_LEN * (page + 1))
                            .unwrap_or_else(|| {
                                content_columns
                                    .first()
                                    .expect("should have first column")
                                    .get(EMBED_PAGE_LEN * page..)
                                    .expect("should have that many rows")
                            })
                            .len()
                        {
                            for (i, col_len) in col_lens
                                .iter()
                                .enumerate()
                                .filter(|(_, l)| l.is_some())
                                .map(|(i, l)| (i, l.expect("filtered out earlier")))
                            {
                                write!(
                                    joined_columns
                                        .first_mut()
                                        .expect("should have first column")
                                        .content,
                                    "{:<width$}",
                                    content_columns
                                        .get(i)
                                        .expect("should have that column")
                                        .get(row + EMBED_PAGE_LEN * page)
                                        .expect("should have that many rows")
                                        .chars()
                                        .take(col_len)
                                        .collect::<String>(),
                                    width = col_len
                                )?;
                            }
                            joined_columns
                                .first_mut()
                                .expect("should have first column")
                                .content
                                .push('\n');
                        }
                        for (i, col_len) in col_lens
                            .iter()
                            .enumerate()
                            .filter(|(_, l)| l.is_some())
                            .map(|(i, l)| (i, l.expect("filtered out earlier")))
                        {
                            write!(
                                joined_columns
                                    .first_mut()
                                    .expect("should have first column")
                                    .header,
                                "{:<width$}",
                                write_embed.headers.get(i).expect("should have that header"),
                                width = col_len
                            )?;
                        }
                        if col_lens.iter().any(std::option::Option::is_some) {
                            joined_columns
                                .first_mut()
                                .expect("should have first column")
                                .content = format!(
                                "```{}\n{}```",
                                joined_columns[0].header, joined_columns[0].content
                            );
                            joined_columns
                                .first_mut()
                                .expect("should have first column")
                                .header = String::new();
                        }
                        for (result_col, (input_col, _)) in col_lens
                            .iter()
                            .enumerate()
                            .filter(|(_, l)| l.is_none())
                            .enumerate()
                        {
                            joined_columns
                                .get_mut(result_col + 1)
                                .expect("should have that column")
                                .content = content_columns
                                .get(input_col)
                                .expect("should have that column")
                                .get(EMBED_PAGE_LEN * page..EMBED_PAGE_LEN * (page + 1))
                                .unwrap_or_else(|| {
                                    let column = content_columns
                                        .get(input_col)
                                        .expect("should have that column");
                                    if column.len() > EMBED_PAGE_LEN {
                                        column
                                            .get(EMBED_PAGE_LEN * page..)
                                            .expect("should have that many rows")
                                    } else {
                                        column
                                    }
                                })
                                .concat();
                            joined_columns
                                .get_mut(result_col + 1)
                                .expect("should have that column")
                                .header = write_embed
                                .headers
                                .get(input_col)
                                .expect("should have that column")
                                .to_string();
                        }
                        joined_columns
                    } else {
                        content_columns
                            .iter()
                            .enumerate()
                            .map(|(c, column)| EmbedColumn {
                                header: write_embed
                                    .headers
                                    .get(c)
                                    .expect("should have that embed")
                                    .clone(),
                                content: {
                                    if column.len() > EMBED_PAGE_LEN {
                                        column
                                            .get(EMBED_PAGE_LEN * page..EMBED_PAGE_LEN * (page + 1))
                                            .unwrap_or_else(|| {
                                                column
                                                    .get(EMBED_PAGE_LEN * page..)
                                                    .expect("should have that many rows")
                                            })
                                    } else {
                                        column
                                    }
                                    .join("\n")
                                },
                                inline: *write_embed
                                    .inlines
                                    .get(c)
                                    .expect("should have that embed"),
                            })
                            .collect()
                    },
                };
                paged_embed.pages.push(new_page);
            }
            let mut embed = paged_embed.to_create_embed(0, max_page_amt);
            if i == 0 {
                embed = embed.url("https://polyweb.ireo.xyz");
            }
            embeds.push((embed, paged_embed));
        }
        let mut reply = CreateReply::default();
        reply
            .embeds
            .append(&mut embeds.iter().map(|(embed, _)| embed.clone()).collect());
        if embeds
            .iter()
            .any(|(_, paged_embed)| paged_embed.pages.len() > 1)
        {
            let components = CreateActionRow::Buttons(vec![
                CreateButton::new(&prev_id).emoji('◀'),
                CreateButton::new(&next_id).emoji('▶'),
                CreateButton::new(&start_id).emoji('🔝'),
            ]);
            reply.components = Some(vec![components]);
        }
        ctx.send(reply).await?;
        if embeds
            .iter()
            .any(|(_, paged_embed)| paged_embed.pages.len() > 1)
        {
            let mut current_page: i32 = 0;
            while let Some(press) = ComponentInteractionCollector::new(ctx)
                .filter(move |press| press.data.custom_id.starts_with(&ctx_id.to_string()))
                .timeout(MAX_MSG_AGE)
                .await
            {
                if press.data.custom_id == next_id {
                    current_page += 1;
                } else if press.data.custom_id == prev_id {
                    current_page -= 1;
                } else if press.data.custom_id == start_id {
                    current_page = 0;
                } else {
                    continue;
                }
                for (i, (embed, paged_embed)) in embeds.iter_mut().enumerate() {
                    let pages_len = i32::try_from(paged_embed.pages.len())?;
                    let page_id =
                        usize::try_from((current_page % pages_len + pages_len) % pages_len)
                            .expect("should not have that many pages");
                    let mut new_embed =
                        paged_embed.to_create_embed(page_id, paged_embed.pages.len());
                    if i == 0 {
                        new_embed = new_embed.url("https://polyweb.ireo.xyz");
                    }
                    if paged_embed.pages.len() > 1 {
                        new_embed = new_embed.footer(CreateEmbedFooter::new(format!(
                            "Page {}/{}",
                            page_id + 1,
                            paged_embed.pages.len(),
                        )));
                    }
                    *embed = new_embed;
                }
                press
                    .create_response(
                        ctx.serenity_context(),
                        CreateInteractionResponse::UpdateMessage(
                            CreateInteractionResponseMessage::new()
                                .embeds(embeds.iter().map(|(embed, _)| embed.clone()).collect()),
                        ),
                    )
                    .await?;
            }
        }
    } else {
        panic!("Different amounts of columns for write_embed!");
    }
    Ok(())
}

// checks whether invoking user is an admin with the required privilege level
#[allow(clippy::missing_panics_doc)]
pub async fn is_admin(ctx: &Context<'_>, level: u32) -> (bool, String) {
    let admin_list = ctx
        .data()
        .admins
        .lock()
        .expect("Failed to acquire Mutex")
        .clone();
    if let Ok(application_info) = ctx.http().get_current_application_info().await {
        if let Some(owner) = application_info.owner {
            if ctx.author().id == owner.id {
                return (true, String::new());
            }
        }
    }
    if admin_list.contains_key(&ctx.author().name) {
        if admin_list
            .get(&ctx.author().name)
            .expect("should have that key")
            <= &level
        {
            (true, String::new())
        } else {
            (false, "Not privileged!".to_string())
        }
    } else {
        (false, "Not an admin!".to_string())
    }
}

// autocompletion function for registered users
#[allow(clippy::missing_panics_doc)]
pub async fn autocomplete_users(ctx: Context<'_>, partial: &str) -> Vec<String> {
    let user_ids: Vec<String> = ctx
        .data()
        .user_ids
        .lock()
        .expect("Failed to acquire Mutex")
        .keys()
        .cloned()
        .collect();
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

pub struct PolyRecords {
    pub records: Vec<Vec<String>>,
    pub wr_amounts: HashMap<String, u32>,
}

#[allow(clippy::missing_panics_doc)]
#[allow(clippy::missing_errors_doc)]
pub async fn get_records(tracks: LeaderboardChoice) -> Result<PolyRecords> {
    use LeaderboardChoice::{Community, Et, Global, Hof};
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
    let mut records = vec![Vec::new(); 3];
    let client = Client::new();
    let mut wr_amounts: HashMap<String, u32> = HashMap::new();
    for (id, name) in track_ids {
        let url = format!("https://vps.kodub.com/leaderboard?version={VERSION}&trackId={id}&skip=0&amount=500&onlyVerified=true");
        let mut att = 0;
        let mut res = String::new();
        while res.is_empty() && att <= REQUEST_RETRY_COUNT {
            res = send_to_networker(&client, &url).await?;
            sleep(Duration::from_millis(1000)).await;
            att += 1;
        }
        let leaderboard = serde_json::from_str::<LeaderBoard>(&res)?;
        let default_winner = LeaderBoardEntry {
            name: "unknown".to_string(),
            frames: 69420.0,
            verified_state: 1,
        };
        let blacklist_file = match tracks {
            LeaderboardChoice::Hof => HOF_BLACKLIST_FILE,
            _ => BLACKLIST_FILE,
        };
        let altlist_file = match tracks {
            LeaderboardChoice::Hof => HOF_ALT_ACCOUNT_FILE,
            _ => ALT_ACCOUNT_FILE,
        };
        let mut winner = &default_winner;
        for entry in &leaderboard.entries {
            if check_blacklist(blacklist_file, &get_alt(altlist_file, &entry.name).await?).await? {
                winner = entry;
                break;
            }
        }
        let winner_name = winner.name.clone();
        let winner_time = winner.frames / 1000.0;
        *wr_amounts.entry(winner_name.clone()).or_default() += 1;
        records
            .get_mut(0)
            .expect("should have that entry")
            .push(name);
        records
            .get_mut(1)
            .expect("should have that entry")
            .push(winner_name);
        records
            .get_mut(2)
            .expect("should have that entry")
            .push(winner_time.to_string());
    }
    let poly_records = PolyRecords {
        records,
        wr_amounts,
    };
    Ok(poly_records)
}

#[allow(clippy::missing_errors_doc)]
pub async fn et_tracks_update(http: Arc<Http>) -> Result<()> {
    let codes = get_ets(http).await?;
    fs::write(
        ET_CODE_FILE,
        codes
            .iter()
            .map(|track_info| track_info.join(" "))
            .collect::<Vec<_>>()
            .join("\n"),
    )
    .await?;
    let ids = codes
        .into_iter()
        .map(|[code, name]| [export_to_id(&code).unwrap_or_default(), name].join(" "))
        .collect::<Vec<_>>();
    fs::write(ET_TRACK_FILE, ids.join("\n")).await?;
    Ok(())
}

async fn get_ets(http: Arc<Http>) -> Result<Vec<[String; 2]>> {
    let mut tracks = Vec::new();
    // #elite-tracks channel
    let et_channel_id = ChannelId::new(1_291_381_174_358_511_719);
    // PolyTrack guild
    let pt_guild_id = GuildId::new(1_115_776_502_592_708_720);
    let active_threads = pt_guild_id.get_active_threads(http.clone()).await?;
    let archived_threads = et_channel_id
        .get_archived_public_threads(http.clone(), None, None)
        .await?
        .threads;
    let mut period_threads = Vec::new();
    let period_end = recent_et_period(Utc::now()).timestamp();
    let period_start = period_end.saturating_sub_unsigned(ET_PERIOD_DURATION.as_secs());
    let period = period_start..period_end;
    for active_thread in active_threads.threads {
        if active_thread.parent_id == Some(et_channel_id) {
            if let Some(thread_metadata) = active_thread.thread_metadata {
                if period.contains(
                    &thread_metadata
                        .create_timestamp
                        .unwrap_or_default()
                        .timestamp(),
                ) {
                    period_threads.push(active_thread);
                }
            }
        }
    }
    for archived_thread in archived_threads {
        if let Some(thread_metadata) = archived_thread.thread_metadata {
            if period.contains(
                &thread_metadata
                    .create_timestamp
                    .unwrap_or_default()
                    .timestamp(),
            ) {
                period_threads.push(archived_thread);
            }
        }
    }
    let client = Client::new();
    for thread in period_threads {
        let mut messages = Vec::new();
        let mut oldest_id = thread
            .messages(http.clone(), GetMessages::new().limit(1))
            .await?
            .first()
            .expect("guaranteed to have at least one message")
            .id;
        messages.push(thread.message(http.clone(), oldest_id).await?);
        loop {
            let page = thread
                .messages(
                    http.clone(),
                    GetMessages::new().before(oldest_id).limit(100),
                )
                .await?;
            messages.append(&mut page.clone());
            if page.len() < 100 {
                break;
            }
            oldest_id = page.last().expect("should have 100 messages").id;
        }
        messages.reverse();
        for message in messages {
            if let Some(code) = message
                .content
                .split_whitespace()
                .find(|word| TRACK_CODE_STARTS.iter().any(|part| word.starts_with(part)))
            {
                tracks.push([code.to_string(), thread.name]);
                break;
            }
            if let Some(txt_file) = message.attachments.iter().find(|a| {
                Path::new(&a.filename)
                    .extension()
                    .is_some_and(|ext| ext.eq_ignore_ascii_case("txt"))
            }) {
                let file_url = &txt_file.url;
                let content = client.get(file_url).send().await?.text().await?;
                if TRACK_CODE_STARTS
                    .iter()
                    .any(|start| content.starts_with(start))
                {
                    tracks.push([content, thread.name]);
                    break;
                }
            }
            if let Some(code) = find_pastebin_content(message.content).await {
                tracks.push([code, thread.name]);
                break;
            }
        }
    }
    Ok(tracks)
}

async fn find_pastebin_content(message: String) -> Option<String> {
    let client = Client::new();
    let pastebins = vec![
        (
            r"https://pastes.dev/([0-9a-zA-Z]+)/?",
            "https://api.pastes.dev/",
        ),
        (
            r"https://pastebin.com/([0-9a-zA-Z]+)/?",
            "https://pastebin.com/raw/",
        ),
    ];
    for (regex_str, api_url) in pastebins {
        let regex = Regex::new(regex_str).expect("failed to create Regex");
        if let Some(captures) = regex.captures(&message) {
            let (_full, [paste_id]) = captures.extract();
            let url = format!("{api_url}{paste_id}");
            if let Ok(response) = client.get(url).send().await {
                if let Ok(text) = response.text().await {
                    return Some(text);
                }
            }
        }
    }
    // stupid pastecode.dev API forces me to do this
    let reg_last =
        Regex::new(r"https://pastecode.dev/s/([0-9a-zA-Z]+)/?").expect("failed to create Regex");
    if let Some(reg_match) = reg_last.find(&message) {
        let url = reg_match.as_str();
        if let Ok(response) = client.get(url).send().await {
            if let Ok(text) = response.text().await {
                let code_regex = Regex::new(r#"<div class="hljs-ln-line">([0-9a-zA-Z]+)</div>"#)
                    .expect("failed to create Regex");
                if let Some(captures) = code_regex.captures(&text) {
                    let (_full, [code]) = captures.extract();
                    if TRACK_CODE_STARTS
                        .iter()
                        .any(|start| code.starts_with(start))
                    {
                        return Some(code.to_string());
                    }
                }
            }
        }
    }
    None
}
