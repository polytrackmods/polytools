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
use polycore::recent_et_period;
use serenity::{ClientBuilder, GatewayIntents};
use sqlx::migrate;
use sqlx::sqlite::SqlitePoolOptions;
use std::collections::HashMap;
use std::env;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::sync::Mutex;
use tokio::task;
use tokio::time::{Instant, sleep, sleep_until};
use utils::{BotData, et_tracks_update};

use crate::commands::{add_totw, get_totw_lb, update_totw};
use crate::utils::totw;

const MAX_MSG_AGE: Duration = Duration::from_secs(60 * 10);
pub const ET_PERIOD_DURATION: Duration = Duration::from_secs(60 * 60 * 24 * 7);

type Context<'a> = poise::Context<'a, BotData, Error>;

#[tokio::main]
async fn main() -> Result<()> {
    let subscriber = tracing_subscriber::FmtSubscriber::new();
    tracing::subscriber::set_global_default(subscriber)?;
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
        pool: Arc::new(pool.clone()),
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
                add_totw(),
                update_totw(),
                get_totw_lb(),
            ],
            prefix_options: PrefixFrameworkOptions {
                prefix: Some("~".into()),
                edit_tracker: Some(Arc::new(EditTracker::for_timespan(Duration::from_secs(60)))),
                additional_prefixes: vec![Prefix::Literal("'")],
                ..Default::default()
            },
            pre_command: |ctx| {
                Box::pin(async move {
                    tracing::info!(
                        "Executing command {} issued by {}",
                        ctx.command().qualified_name,
                        ctx.author().display_name(),
                    );
                })
            },
            post_command: |ctx| {
                Box::pin(async move {
                    tracing::info!(
                        "Executed command {} issued by {}!",
                        ctx.command().qualified_name,
                        ctx.author().display_name(),
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
    let et_updater = task::spawn(async move {
        loop {
            et_tracks_update(http.clone())
                .await
                .unwrap_or_else(|_| tracing::error!("Failed to update ET tracks"));
            let next_run = recent_et_period(Utc::now()) + ET_PERIOD_DURATION;
            let duration_until = next_run.timestamp_millis() - Utc::now().timestamp_millis();
            let sleep_duration =
                Duration::from_millis(u64::try_from(duration_until).unwrap_or_default().max(1) + 1);
            let wakeup_time = Instant::now() + sleep_duration;
            sleep_until(wakeup_time).await;
        }
    });
    let pool2 = pool.clone();
    let totw_updater = task::spawn(async move {
        loop {
            sleep(Duration::from_secs(1800)).await;
            totw::update(&pool2)
                .await
                .unwrap_or_else(|_| tracing::error!("Failed to update TOTW"));
            sleep(Duration::from_secs(1800)).await;
        }
    });
    let final_totw_updater = task::spawn(async move {
        loop {
            if let Ok(Some(current_totw)) = totw::get_current_totw(&pool).await {
                if let Some(end) = current_totw.end {
                    let final_update_time =
                        UNIX_EPOCH + Duration::from_secs(end as u64) - Duration::from_secs(60);
                    if let Ok(dur) = final_update_time.duration_since(SystemTime::now()) {
                        sleep(dur).await;
                        totw::update(&pool)
                            .await
                            .unwrap_or_else(|_| tracing::error!("failed to update TOTW"));
                        continue;
                    }
                }
            } else {
                tracing::warn!("Unable to get current TOTW");
            }
            sleep(Duration::from_secs(3600)).await;
        }
    });
    let client_task = task::spawn(async move {
        client.start().await.expect("Failed to start client");
    });
    tokio::select! {
        _ = et_updater => tracing::error!("ET updater task finished unexpectedly."),
        _ = client_task => tracing::error!("Client stopped."),
        _ = totw_updater => tracing::error!("TOTW updater task finished unexpectedly."),
        _ = final_totw_updater => tracing::error!("Final TOTW updater task finished unexpectedly."),
    }
    Ok(())
}
