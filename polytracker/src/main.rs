mod commands;
pub mod utils;

use anyhow::{Error, Result};
use chrono::Utc;
use commands::{admins, roles, tracks, update_admins};
use commands::{
    assign, compare, delete, edit_lists, help, list, players, policy, rankings, records, request,
    top, update_rankings, users,
};
use dotenvy::dotenv;
use poise::builtins;
use poise::serenity_prelude as serenity;
use poise::{EditTracker, Framework, FrameworkOptions, Prefix, PrefixFrameworkOptions};
use polycore::{get_datetime, recent_et_period};
use serenity::{ClientBuilder, GatewayIntents};
use sqlx::migrate;
use sqlx::sqlite::SqlitePoolOptions;
use std::collections::HashMap;
use std::env;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::task;
use tokio::time::{sleep_until, Instant};
use utils::{et_tracks_update, BotData};

const MAX_MSG_AGE: Duration = Duration::from_secs(60 * 10);
pub const ET_PERIOD_DURATION: Duration = Duration::from_secs(60 * 60 * 24 * 7);

type Context<'a> = poise::Context<'a, BotData, Error>;

#[tokio::main]
async fn main() -> Result<()> {
    dotenv().ok();
    let db_url = env::var("DATABASE_URL").unwrap_or_else(|_| "sqlite://poly.db".to_string());
    let pool = SqlitePoolOptions::new()
        .max_connections(10)
        .connect(&db_url)
        .await?;
    migrate!("../migrations").run(&pool).await?;
    let token = env::var("DISCORD_TOKEN").expect("Token missing");
    let intents = GatewayIntents::non_privileged() | GatewayIntents::GUILD_MEMBERS;

    let bot_data = BotData {
        user_ids: Mutex::new(HashMap::new()),
        admins: Mutex::new(HashMap::new()),
        pool: Arc::new(pool),
    };
    bot_data.load().await?;

    let framework = Framework::builder()
        .options(FrameworkOptions {
            commands: vec![
                assign(),
                delete(),
                update_admins(),
                request(),
                list(),
                edit_lists(),
                users(),
                admins(),
                players(),
                help(),
                compare(),
                update_rankings(),
                records(),
                top(),
                tracks(),
                rankings(),
                roles(),
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

    let mut client = ClientBuilder::new(token, intents)
        .framework(framework)
        .await
        .expect("Failed to create client");
    let http = client.http.clone();
    let updater = task::spawn(async move {
        loop {
            et_tracks_update(http.clone())
                .await
                .unwrap_or_else(|_| println!("failed to update ET tracks"));
            let next_run = recent_et_period(Utc::now()) + ET_PERIOD_DURATION;
            let duration_until = next_run.timestamp_millis() - Utc::now().timestamp_millis();
            let sleep_duration =
                Duration::from_millis(u64::try_from(duration_until).unwrap_or_default().max(1) + 1);
            let wakeup_time = Instant::now() + sleep_duration;
            sleep_until(wakeup_time).await;
        }
    });
    let client_task = task::spawn(async move {
        client.start().await.expect("Failed to start client");
    });
    tokio::select! {
        _ = updater => println!("Updater task finished unexpectedly."),
        _ = client_task => println!("Client stopped."),
    }
    Ok(())
}
