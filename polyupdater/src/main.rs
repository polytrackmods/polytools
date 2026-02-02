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
use polycore::{LeaderBoardEntry, OFFICIAL_TRACK_FILE, VERSION, send_to_networker};
use tokio::{fs, task};

const GUILD_ID: GuildId = GuildId::new(1_115_776_502_592_708_720);
const RESOURCES_ID: ChannelId = ChannelId::new(1_239_092_743_582_646_412);
// const GUILD_ID: GuildId = GuildId::new(1156668508462125106);
// const RESOURCES_ID: ChannelId = ChannelId::new(1467883158811967620);

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
            update_resources(&http)
                .await
                .expect("Failed to update resources");
            tokio::time::sleep(Duration::from_secs(60 * 60)).await;
        }
    });
    let client_task = task::spawn(async move {
        client.start().await.expect("Failed to start client");
    });
    tokio::select! {
        _ = client_task => tracing::error!("Client stopped."),
        _ = resources_task => tracing::error!("Resource updater stopped."),
    }
}

async fn update_resources(http: &Http) -> Result<()> {
    let server = http.get_guild(GUILD_ID).await?;
    if let Some(resources_channel) = server.channels(http).await?.get(&RESOURCES_ID) {
        let messages = resources_channel.messages(http, GetMessages::new()).await?;
        let new_content = prepare_resources_msg().await?;
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
    Ok(())
}

#[derive(Facet)]
struct Leaderboard {
    entries: Vec<LeaderBoardEntry>,
}

async fn prepare_resources_msg() -> Result<String> {
    let track_list = fs::read_to_string(OFFICIAL_TRACK_FILE).await?;
    let tracks: Vec<(&str, &str)> = track_list
        .trim()
        .lines()
        .filter_map(|l| l.split_once(" "))
        .collect();
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
                "{}\t{:>2.3}\t{}",
                track_name,
                record.frames as f64 / 1000.0,
                record.name
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    Ok(message)
}
