pub mod api;
pub mod parsers;

use api::get_api;
use filenamify::filenamify;
use parsers::{
    get_custom_leaderboard, get_standard_leaderboard, parse_history, parse_leaderboard,
    parse_leaderboard_with_records,
};
use polymanager::{
    community_update, et_rankings_update, global_rankings_update, hof_update, COMMUNITY_RANKINGS_FILE, COMMUNITY_TRACK_FILE, CUSTOM_TRACK_FILE, HOF_RANKINGS_FILE, RANKINGS_FILE, TRACK_FILE, UPDATE_CYCLE_LEN, UPDATE_LB_COUNT
};
use rocket::form::Context;
use rocket::fs::FileServer;
use rocket::tokio::join;
use rocket::tokio::{fs, task, time::sleep};
use rocket::{get, main, routes};
use rocket_dyn_templates::{context, Template};

#[get("/")]
fn index() -> Template {
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
        .expect("Failed to read file")
        .lines()
        .map(std::string::ToString::to_string)
        .map(|s| {
            s.split_once(' ')
                .expect("Invalid custom tracks file")
                .1
                .to_string()
        })
        .collect();
    Template::render("lb_custom_home", context! { tracks })
}

#[get("/lb-standard")]
async fn standard_lb_home() -> Template {
    let track_names: Vec<String> = fs::read_to_string(TRACK_FILE)
        .await
        .expect("Failed to read file")
        .lines()
        .map(|s| {
            s.split_once(' ')
                .expect("Invalid track ids file")
                .1
                .to_string()
        })
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
fn policy() -> Template {
    Template::render("privacy_policy", context! {})
}

#[get("/tutorial")]
fn tutorial() -> Template {
    Template::render("tutorial", context! {})
}

#[get("/history")]
async fn history_home() -> Template {
    let mut track_names: Vec<String> = fs::read_to_string(TRACK_FILE)
        .await
        .expect("Failed to read file")
        .lines()
        .map(|s| {
            s.split_once(' ')
                .expect("Invalid track ids file")
                .1
                .to_string()
        })
        .collect();
    track_names.append(
        &mut fs::read_to_string(COMMUNITY_TRACK_FILE)
            .await
            .expect("Failed to read file")
            .lines()
            .map(|s| filenamify(s.split_once(' ').expect("Invalid track ids file").1))
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
async fn main() -> Result<(), Box<rocket::Error>> {
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
            join!(
                hof_update(),
                sleep(
                    UPDATE_CYCLE_LEN
                        / u32::try_from(UPDATE_LB_COUNT).expect("shouldn't have that many lbs")
                )
            )
            .0
            .unwrap_or_else(|_| println!("Failed update"));
            println!("Cycle done");
            join!(
                community_update(),
                sleep(
                    UPDATE_CYCLE_LEN
                        / u32::try_from(UPDATE_LB_COUNT).expect("shouldn't have that many lbs")
                )
            )
            .0
            .unwrap_or_else(|_| println!("Failed update"));
            println!("Cycle done");
            join!(
                global_rankings_update(),
                sleep(
                    UPDATE_CYCLE_LEN
                        / u32::try_from(UPDATE_LB_COUNT).expect("shouldn't have that many lbs")
                )
            )
            .0
            .unwrap_or_else(|_| println!("Failed update"));
            println!("Cycle done");
            join!(
                et_rankings_update(),
                sleep(
                    UPDATE_CYCLE_LEN
                        / u32::try_from(UPDATE_LB_COUNT).expect("shouldn't have that many lbs")
                )
            )
            .0
            .unwrap_or_else(|_| println!("Failed update"));
            println!("Cycle done");
        }
    });
    rocket.launch().await?;
    Ok(())
}
