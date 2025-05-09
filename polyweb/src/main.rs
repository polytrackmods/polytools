pub mod api;
pub mod parsers;

use api::get_api;
use filenamify::filenamify;
use parsers::{
    get_custom_leaderboard, get_standard_leaderboard, parse_history, parse_leaderboard,
    parse_leaderboard_with_records,
};
use polymanager::{
    community_update, global_rankings_update, hof_update, COMMUNITY_RANKINGS_FILE,
    COMMUNITY_TRACK_FILE, CUSTOM_TRACK_FILE, HOF_RANKINGS_FILE, RANKINGS_FILE, TRACK_FILE,
};
use rocket::form::Context;
use rocket::fs::FileServer;
use rocket::tokio::{
    fs, task,
    time::{sleep, Duration},
};
use rocket::{get, main, routes};
use rocket_dyn_templates::{context, Template};
use std::collections::HashMap;

const AUTOUPDATE_TIMER: Duration = Duration::from_secs(60 * 30);

#[get("/")]
async fn index() -> Template {
    Template::render("index", Context::default())
}

#[get("/global")]
async fn global() -> Template {
    let leaderboard = parse_leaderboard(RANKINGS_FILE).await;
    Template::render("leaderboard", context! { leaderboard })
}

#[get("/community")]
async fn community() -> Template {
    let leaderboard = parse_leaderboard_with_records(COMMUNITY_RANKINGS_FILE).await;
    Template::render("community", context! { leaderboard })
}

#[get("/hof")]
async fn hof() -> Template {
    let leaderboard = parse_leaderboard_with_records(HOF_RANKINGS_FILE).await;
    Template::render("hof", context! { leaderboard })
}

#[get("/lb-custom")]
async fn custom_lb_home() -> Template {
    let tracks: Vec<String> = fs::read_to_string(CUSTOM_TRACK_FILE)
        .await
        .unwrap()
        .lines()
        .map(|s| s.to_string())
        .map(|s| s.split_once(" ").unwrap().1.to_string())
        .collect();
    Template::render("lb_custom_home", context! { tracks })
}

#[get("/lb-standard")]
async fn standard_lb_home() -> Template {
    let track_names: Vec<String> = fs::read_to_string(TRACK_FILE)
        .await
        .unwrap()
        .lines()
        .map(|s| s.split_once(" ").unwrap().1.to_string())
        .collect();
    Template::render("lb_standard_home", context! { track_names })
}

#[get("/lb-custom/<track_id>")]
async fn custom_lb(track_id: &str) -> Template {
    let (name, leaderboard) = get_custom_leaderboard(track_id).await;
    Template::render(
        "track_leaderboard",
        context! { track_name: name, leaderboard },
    )
}

#[get("/lb-standard/<track_id>")]
async fn standard_lb(track_id: &str) -> Template {
    let leaderboard = get_standard_leaderboard(track_id).await;
    Template::render(
        "track_leaderboard",
        context! { track_name: format!("Track {} ", track_id), leaderboard },
    )
}

#[get("/policy")]
async fn policy() -> Template {
    let context: HashMap<String, String> = HashMap::new();
    Template::render("privacy_policy", context)
}

#[get("/tutorial")]
async fn tutorial() -> Template {
    let context: HashMap<String, String> = HashMap::new();
    Template::render("tutorial", context)
}

#[get("/history")]
async fn history_home() -> Template {
    let mut track_names: Vec<String> = fs::read_to_string(TRACK_FILE)
        .await
        .unwrap()
        .lines()
        .map(|s| s.split_once(" ").unwrap().1.to_string())
        .collect();
    track_names.append(
        &mut fs::read_to_string(COMMUNITY_TRACK_FILE)
            .await
            .unwrap()
            .lines()
            .map(|s| filenamify(s.split_once(" ").unwrap().1))
            .collect(),
    );
    Template::render("history_home", context! { track_names })
}

#[get("/history/<track_id>")]
async fn history(track_id: &str) -> Template {
    let records = parse_history(track_id).await;
    Template::render(
        "history",
        context! {track_name: format!("Track {} ", track_id), records},
    )
}

#[main]
async fn main() -> Result<(), rocket::Error> {
    let rocket = rocket::build()
        .mount(
            "/",
            routes![
                index,
                global,
                hof,
                community,
                tutorial,
                standard_lb_home,
                standard_lb,
                custom_lb_home,
                custom_lb,
                policy,
                history_home,
                history,
                get_api,
            ],
        )
        .mount("/static", FileServer::from("static"))
        .attach(Template::fairing());
    task::spawn(async {
        loop {
            community_update()
                .await
                .unwrap_or_else(|_| println!("Failed update"));
            sleep(AUTOUPDATE_TIMER / 3).await;
            hof_update()
                .await
                .unwrap_or_else(|_| println!("Failed update"));
            sleep(AUTOUPDATE_TIMER / 3).await;
            global_rankings_update()
                .await
                .unwrap_or_else(|_| println!("Failed update"));
            sleep(AUTOUPDATE_TIMER / 3).await;
        }
    });
    rocket.launch().await?;
    Ok(())
}
