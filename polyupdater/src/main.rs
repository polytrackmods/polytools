use std::time::Duration;

use anyhow::Result;
use facet::Facet;
use poise::{
    Framework, FrameworkOptions, builtins,
    serenity_prelude::{
        ChannelId, ClientBuilder, CreateMessage, EditMessage, GatewayIntents, GetMessages, GuildId,
        Http,
    },
};
use polycore::{
    COMMUNITY_TRACK_FILE, LeaderBoardEntry, OFFICIAL_TRACK_FILE, VERSION, send_to_networker,
};
use tokio::{fs, task, time::sleep};

const GUILD_ID: GuildId = GuildId::new(1_115_776_502_592_708_720);
const RESOURCES_ID: ChannelId = ChannelId::new(1_239_092_743_582_646_412);
const CT_RESOURCES_ID: ChannelId = ChannelId::new(1_384_494_680_439_259_248);
// const GUILD_ID: GuildId = GuildId::new(1_156_668_508_462_125_106);
// const RESOURCES_ID: ChannelId = ChannelId::new(1_467_883_158_811_967_620);
// const CT_RESOURCES_ID: ChannelId = ChannelId::new(1_468_287_913_438_740_675);

#[tokio::main]
async fn main() {
    let subscriber = tracing_subscriber::fmt().compact().finish();
    tracing::subscriber::set_global_default(subscriber)
        .expect("Failed to set up tracing subscriber");
    dotenvy::dotenv().ok();
    let token = std::env::var("UPDATER_DISCORD_TOKEN").expect("Token missing");
    let intents = GatewayIntents::non_privileged() | GatewayIntents::MESSAGE_CONTENT;
    let framework: Framework<(), anyhow::Error> = Framework::builder()
        .options(FrameworkOptions {
            commands: vec![],
            ..Default::default()
        })
        .setup(|ctx, _ready, framework| {
            Box::pin(async move {
                builtins::register_globally(ctx, &framework.options().commands).await?;
                Ok(())
            })
        })
        .build();
    let mut client = ClientBuilder::new(token, intents)
        .framework(framework)
        .await
        .expect("Failed to create client");
    let http = client.http.clone();
    let resources_task = task::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(10 * 60)).await;
            update_resources(&http)
                .await
                .expect("Failed to update resources");
            tokio::time::sleep(Duration::from_secs(50 * 60)).await;
        }
    });
    let client_task = task::spawn(async move {
        client.start().await.expect("Failed to start client");
    });
    let leaderboard_update_task = task::spawn(async {
        loop {
            tokio::join!(
                polycore::hof_update(),
                sleep(polycore::UPDATE_CYCLE_LEN / polycore::UPDATE_LB_COUNT)
            )
            .0
            .unwrap_or_else(|_| tracing::error!("Failed HOF update"));
            tracing::info!("HOF update done");
            tokio::join!(
                polycore::community_update(),
                sleep(polycore::UPDATE_CYCLE_LEN / polycore::UPDATE_LB_COUNT)
            )
            .0
            .unwrap_or_else(|_| tracing::error!("Failed CT update"));
            tracing::info!("CT update done");
            tokio::join!(
                polycore::et_rankings_update(),
                sleep(polycore::UPDATE_CYCLE_LEN / polycore::UPDATE_LB_COUNT)
            )
            .0
            .unwrap_or_else(|_| tracing::error!("Failed ET update"));
            tracing::info!("ET update done");
            tokio::join!(
                polycore::official_update(),
                sleep(polycore::UPDATE_CYCLE_LEN / polycore::UPDATE_LB_COUNT)
            )
            .0
            .unwrap_or_else(|_| tracing::error!("Failed Global update"));
            tracing::info!("Global update done");
        }
    });
    tokio::select! {
        _ = client_task => tracing::error!("Client stopped."),
        _ = resources_task => tracing::error!("Resource updater stopped."),
        _ = leaderboard_update_task => tracing::error!("Leaderboard updater stopped."),
    }
}

async fn update_resources(http: &Http) -> Result<()> {
    let server = http.get_guild(GUILD_ID).await?;
    for (channel_id, channel_type) in [
        (RESOURCES_ID, ResourceChannel::Official),
        (CT_RESOURCES_ID, ResourceChannel::Ct),
    ] {
        if let Some(resources_channel) = server.channels(http).await?.get(&channel_id) {
            let messages = resources_channel.messages(http, GetMessages::new()).await?;
            let new_content = prepare_resources_msg(channel_type).await?;
            let user_id = http.get_current_user().await?.id;
            if let Some(mut old_msg) = messages.into_iter().find(|msg| msg.author.id == user_id) {
                let new_msg = EditMessage::new().content(new_content);
                old_msg.edit(http, new_msg).await?;
            } else {
                let new_msg = CreateMessage::new().content(new_content);
                resources_channel.send_message(http, new_msg).await?;
            }
        } else {
            tracing::error!("Could not find resources channel");
        }
    }
    Ok(())
}

#[derive(Facet)]
struct Leaderboard {
    entries: Vec<LeaderBoardEntry>,
}

enum ResourceChannel {
    Official,
    Ct,
}

async fn prepare_resources_msg(channel: ResourceChannel) -> Result<String> {
    let track_list = fs::read_to_string(match channel {
        ResourceChannel::Official => OFFICIAL_TRACK_FILE,
        ResourceChannel::Ct => COMMUNITY_TRACK_FILE,
    })
    .await?;
    let tracks: Vec<(&str, &str)> = track_list
        .trim()
        .lines()
        .filter_map(|l| l.split_once(" "))
        .collect();
    let max_name_len = tracks
        .iter()
        .map(|(_, name)| name.len())
        .max()
        .unwrap_or_default();
    let client = reqwest::Client::new();
    let futures = tracks.iter().map(|(track_id, _)| {
        let client = client.clone();
        let url = format!(
            "https://vps.kodub.com/leaderboard?version={}&trackId={}&skip=0&amount=1&onlyVerified=true",
            VERSION, track_id
        );
        task::spawn(async move {
            send_to_networker(&client, &url).await
        })
    });
    let results = futures::future::join_all(futures).await;
    let rankings: Vec<_> = results
        .into_iter()
        .map(|res| {
            if let Ok(Ok(res)) = res {
                let ranking: Leaderboard =
                    facet_json::from_str(&res).expect("failed to parse JSON");
                ranking
            } else {
                Leaderboard {
                    entries: Vec::new(),
                }
            }
        })
        .collect();
    let default_lb_entry = LeaderBoardEntry {
        frames: 0,
        name: String::from("Unknown"),
        user_id: String::new(),
    };
    let message = rankings
        .into_iter()
        .zip(tracks)
        .map(|(ranking, (_, track_name))| {
            let record = ranking.entries.first().unwrap_or(&default_lb_entry);
            format!(
                "{:>width$}  {}{:0>6.3}\t{}",
                track_name,
                if record.frames / 60000 > 0 {
                    format!("{}:", record.frames / 60000)
                } else {
                    "  ".to_string()
                },
                (record.frames % 60000) as f64 / 1000.0,
                record.name,
                width = max_name_len
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    Ok(format!("```\n{message}\n```"))
}
