mod commands;
pub mod utils;

use anyhow::Error;
use commands::{
    assign, compare, delete, edit_lists, help, list, players, policy, rankings, records, request,
    top, update_rankings, users,
};
use dotenvy::dotenv;
use poise::builtins;
use poise::serenity_prelude as serenity;
use poise::{EditTracker, Framework, FrameworkOptions, Prefix, PrefixFrameworkOptions};
use polymanager::db::establish_connection;
use polymanager::get_datetime;
use serenity::{ClientBuilder, GatewayIntents};
use std::collections::HashMap;
use std::env;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use utils::BotData;

const MAX_RANKINGS_AGE: Duration = Duration::from_secs(60 * 10);
const MAX_MSG_AGE: Duration = Duration::from_secs(60 * 10);

type Context<'a> = poise::Context<'a, BotData, Error>;

#[tokio::main]
async fn main() {
    dotenv().ok();
    let conn = Mutex::new(establish_connection());
    let token = env::var("DISCORD_TOKEN").expect("Token missing");
    let intents = GatewayIntents::non_privileged() | GatewayIntents::GUILD_MEMBERS;

    let bot_data = BotData {
        user_ids: Mutex::new(HashMap::new()),
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
                edit_lists(),
                users(),
                players(),
                help(),
                compare(),
                update_rankings(),
                records(),
                top(),
                rankings(),
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
                        "Executing command {} issued by {} at {}",
                        ctx.command().qualified_name,
                        ctx.author().display_name(),
                        get_datetime()
                    );
                })
            },
            post_command: |ctx| {
                Box::pin(async move {
                    println!(
                        "Executed command {} issued by {} at {}!",
                        ctx.command().qualified_name,
                        ctx.author().display_name(),
                        get_datetime()
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
