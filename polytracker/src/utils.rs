use crate::{Context, Error, MAX_MSG_AGE};
use diesel::prelude::*;
use dotenvy::dotenv;
use poise::serenity_prelude::{self as serenity, CacheHttp};
use poise::{CreateReply, Modal};
use polymanager::db::{Admin, NewUser, User};
use serde::{Deserialize, Serialize};
use serenity::{
    Color, ComponentInteractionCollector, CreateActionRow, CreateAttachment, CreateButton,
    CreateEmbed, CreateInteractionResponse, CreateInteractionResponseMessage,
};
use std::collections::HashMap;
use std::sync::Mutex;

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
    pub frames: f64,
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

// the bot's shared data
pub struct BotData {
    pub user_ids: Mutex<HashMap<String, String>>,
    pub admins: Mutex<HashMap<String, u32>>,
    pub conn: Mutex<SqliteConnection>,
}

impl BotData {
    pub async fn load(&self) {
        use polymanager::schema::admins::dsl::*;
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
    pub async fn delete(&self, delete_name: &str) {
        use polymanager::schema::users::dsl::*;
        let connection = &mut *self.conn.lock().unwrap();
        diesel::delete(users.filter(name.eq(delete_name)))
            .execute(connection)
            .expect("Error deleting user");
    }
}

// non-embed output function
pub async fn write(ctx: &Context<'_>, mut text: String) -> Result<(), Error> {
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

// output function using embeds
pub async fn write_embed(
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
        let fields = headers.iter().enumerate().map(|(i, h)| {
            (
                h.to_string(),
                pages.get(i).unwrap().first().unwrap().clone(),
                inlines[i],
            )
        });
        let embed = CreateEmbed::default()
            .title(title.clone())
            .description(description.clone())
            .fields(fields.clone())
            .color(Color::from_rgb(0, 128, 128))
            .url("https://polyweb.ireo.xyz");
        let reply = if pages.first().unwrap().len() > 1 {
            let components = CreateActionRow::Buttons(vec![
                CreateButton::new(&prev_id).emoji('◀'),
                CreateButton::new(&next_id).emoji('▶'),
                CreateButton::new(&start_id).emoji('🔝'),
            ]);

            CreateReply::default()
                .embed(embed)
                .components(vec![components])
        } else {
            CreateReply::default().embed(embed)
        };
        ctx.send(reply.clone()).await?;
        if pages.first().unwrap().len() > 1 {
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
                let fields = headers.iter().enumerate().map(|(i, h)| {
                    (
                        h.to_string(),
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
        }
    } else {
        panic!("Different amounts of columns for write_embed!");
    }
    Ok(())
}

// checks whether invoking user is an admin with the required privilege level
pub async fn is_admin(ctx: &Context<'_>, level: u32) -> (bool, String) {
    let admin_list = ctx.data().admins.lock().unwrap().clone();
    if let Ok(application_info) = ctx.http().get_current_application_info().await {
        if let Some(owner) = application_info.owner {
            if ctx.author().id == owner.id {
                return (true, String::new());
            }
        }
    }
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

// autocompletion function for registered users
pub async fn autocomplete_users(ctx: Context<'_>, partial: &str) -> Vec<String> {
    let user_ids: Vec<String> = ctx
        .data()
        .user_ids
        .lock()
        .unwrap()
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
