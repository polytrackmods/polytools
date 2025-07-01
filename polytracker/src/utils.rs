use crate::commands::LeaderboardChoice;
use crate::{Context, MAX_MSG_AGE};
use anyhow::Error;
use diesel::prelude::*;
use poise::serenity_prelude::{self as serenity, CacheHttp, CreateEmbedFooter};
use poise::{CreateReply, Modal};
use polymanager::db::{Admin, NewAdmin, NewUser, User};
use polymanager::{
    check_blacklist, get_alt, ALT_ACCOUNT_FILE, BLACKLIST_FILE, COMMUNITY_TRACK_FILE,
    HOF_ALL_TRACK_FILE, HOF_ALT_ACCOUNT_FILE, HOF_BLACKLIST_FILE, REQUEST_RETRY_COUNT, TRACK_FILE,
    VERSION,
};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serenity::{
    Color, ComponentInteractionCollector, CreateActionRow, CreateAttachment, CreateButton,
    CreateEmbed, CreateInteractionResponse, CreateInteractionResponseMessage,
};
use std::collections::HashMap;
use std::fmt::Write;
use std::sync::Mutex;
use std::time::Duration;
use tokio::fs;
use tokio::time::sleep;

const EMBED_PAGE_LEN: usize = 20;
const MAX_COL_WIDTH: usize = 25;

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
pub async fn write(ctx: &Context<'_>, mut text: String) -> Result<(), Error> {
    if text.chars().count() > 2000 {
        if text.chars().next().expect("Guaranteed to be there")
            == text.chars().nth(1).expect("Guaranteed to be there")
            && text.chars().nth(1).expect("Guaranteed to be there")
                == text.chars().nth(2).expect("Guaranteed to be there")
            && text.chars().nth(2).expect("Guaranteed to be there") == '`'
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
) -> Result<(), Error> {
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
            let mut paged_embed: PagedEmbed = PagedEmbed {
                title: write_embed.title.clone(),
                description: write_embed.description.clone(),
                pages: Vec::new(),
            };
            let max_page_amt = write_embed
                .contents
                .iter()
                .max_by_key(|content| content.lines().count())
                .expect("should have contents")
                .lines()
                .count()
                .div_ceil(EMBED_PAGE_LEN);
            for page in 0..max_page_amt {
                let new_page = EmbedPage {
                    columns: if mobile_friendly {
                        let col_lens = write_embed
                            .inlines
                            .iter()
                            .enumerate()
                            .map(|(i, inline)| {
                                if *inline {
                                    let max_len = write_embed
                                        .contents
                                        .get(i)
                                        .expect("should have that column")
                                        .lines()
                                        .skip(EMBED_PAGE_LEN * page)
                                        .take(EMBED_PAGE_LEN)
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
                        for row in 0..write_embed
                            .contents
                            .first()
                            .expect("should have first column")
                            .lines()
                            .skip(EMBED_PAGE_LEN * page)
                            .take(EMBED_PAGE_LEN)
                            .count()
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
                                    write_embed
                                        .contents
                                        .get(i)
                                        .expect("should have that column")
                                        .lines()
                                        .nth(row + EMBED_PAGE_LEN * page)
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
                                .content = write_embed
                                .contents
                                .get(input_col)
                                .expect("should have that column")
                                .lines()
                                .skip(EMBED_PAGE_LEN * page)
                                .take(EMBED_PAGE_LEN)
                                .collect::<String>();
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
                        write_embed
                            .contents
                            .iter()
                            .enumerate()
                            .map(|(c, content)| EmbedColumn {
                                header: write_embed
                                    .headers
                                    .get(c)
                                    .expect("should have that embed")
                                    .clone(),
                                content: {
                                    if content.lines().count() > 1 {
                                        content
                                            .lines()
                                            .skip(EMBED_PAGE_LEN * page)
                                            .take(EMBED_PAGE_LEN)
                                            .collect::<Vec<_>>()
                                            .join("\n")
                                    } else {
                                        content.to_string()
                                    }
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
            let mut embed = CreateEmbed::default()
                .title(write_embed.title.clone())
                .description(write_embed.description.clone())
                .fields(
                    paged_embed
                        .pages
                        .first()
                        .expect("should have first page")
                        .clone(),
                )
                .color(Color::from_rgb(0, 128, 128))
                .footer(CreateEmbedFooter::new(format!("Page 1/{max_page_amt}")));
            if i == 0 {
                embed = embed.url("https://polyweb.ireo.xyz");
            }
            dbg!(&embed);
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
                    let fields = paged_embed
                        .pages
                        .get(page_id)
                        .expect("should have that page");
                    *embed = CreateEmbed::default()
                        .title(&paged_embed.title)
                        .description(&paged_embed.description)
                        .fields(fields.clone())
                        .color(Color::from_rgb(0, 128, 128))
                        .footer(CreateEmbedFooter::new(format!(
                            "Page {}/{}",
                            page_id + 1,
                            paged_embed.pages.len(),
                        )));
                    if i == 0 {
                        *embed = CreateEmbed::default()
                            .title(&paged_embed.title)
                            .description(&paged_embed.description)
                            .fields(fields.clone())
                            .color(Color::from_rgb(0, 128, 128))
                            .url("https://polyweb.ireo.xyz")
                            .footer(CreateEmbedFooter::new(format!(
                                "Page {}/{}",
                                page_id + 1,
                                paged_embed.pages.len()
                            )));
                    } else {
                        *embed = CreateEmbed::default()
                            .title(&paged_embed.title)
                            .description(&paged_embed.description)
                            .fields(fields.clone())
                            .color(Color::from_rgb(0, 128, 128))
                            .footer(CreateEmbedFooter::new(format!(
                                "Page {}/{}",
                                page_id + 1,
                                paged_embed.pages.len()
                            )));
                    }
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
pub async fn get_records(tracks: LeaderboardChoice) -> Result<PolyRecords, Error> {
    use LeaderboardChoice::{Community, Global, Hof};
    let track_ids: Vec<(String, String)> = fs::read_to_string({
        match tracks {
            Global => TRACK_FILE,
            Community => COMMUNITY_TRACK_FILE,
            Hof => HOF_ALL_TRACK_FILE,
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
        let url = format!("https://vps.kodub.com:{}/leaderboard?version={}&trackId={}&skip=0&amount=500&onlyVerified=true",
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
