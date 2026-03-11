pub mod api;
pub mod parsers;

use std::net::SocketAddr;

use api::get_api;
use askama::Template;
use axum::response::Html;
use axum::routing::get;
use axum::{Router, extract::Path};
use filenamify::filenamify;
use parsers::{
    get_standard_leaderboard, parse_history, parse_leaderboard, parse_leaderboard_with_records,
};
use polycore::{
    COMMUNITY_RANKINGS_FILE, COMMUNITY_TRACK_FILE, HOF_RANKINGS_FILE, OFFICIAL_RANKINGS_FILE,
    OFFICIAL_TRACK_FILE, PolyLeaderBoard, read_track_file,
};
use tokio::net::TcpListener;
use tower_http::services::ServeDir;

use crate::api::get_lbfunc;

async fn index() -> Html<String> {
    #[derive(Template)]
    #[template(path = "index.html")]
    struct IndexTemplate;
    Html(IndexTemplate.render().expect("failed to render template"))
}

async fn global() -> Html<String> {
    #[derive(Template)]
    #[template(path = "leaderboard.html")]
    struct LbTemplate {
        leaderboard: PolyLeaderBoard,
    }
    let leaderboard = parse_leaderboard(OFFICIAL_RANKINGS_FILE).await;
    Html(
        (LbTemplate { leaderboard })
            .render()
            .expect("failed to render template"),
    )
}

async fn community() -> Html<String> {
    #[derive(Template)]
    #[template(path = "community.html")]
    struct CommunityTemplate {
        leaderboard: (PolyLeaderBoard, PolyLeaderBoard),
    }
    let leaderboard = parse_leaderboard_with_records(COMMUNITY_RANKINGS_FILE).await;
    Html(
        (CommunityTemplate { leaderboard })
            .render()
            .expect("failed to render template"),
    )
}

async fn hof() -> Html<String> {
    #[derive(Template)]
    #[template(path = "hof.html")]
    struct HofTemplate {
        leaderboard: (PolyLeaderBoard, PolyLeaderBoard),
    }
    let leaderboard = parse_leaderboard_with_records(HOF_RANKINGS_FILE).await;
    Html(
        (HofTemplate { leaderboard })
            .render()
            .expect("failed to render template"),
    )
}

async fn standard_lb_home() -> Html<String> {
    #[derive(Template)]
    #[template(path = "lb_standard_home.html")]
    struct StandardLbTemplate {
        track_names: Vec<String>,
    }
    let track_names: Vec<String> = read_track_file(OFFICIAL_TRACK_FILE)
        .await
        .into_iter()
        .map(|(_, name)| name)
        .collect();
    Html(
        (StandardLbTemplate { track_names })
            .render()
            .expect("failed to render template"),
    )
}

async fn standard_lb(Path(track_id): Path<String>) -> Html<String> {
    #[derive(Template)]
    #[template(path = "track_leaderboard.html")]
    struct StandardLbTemplate {
        track_name: String,
        leaderboard: PolyLeaderBoard,
    }
    let leaderboard = get_standard_leaderboard(&track_id).await;
    Html(
        (StandardLbTemplate {
            track_name: format!("Track {} ", track_id),
            leaderboard,
        })
        .render()
        .expect("failed to render template"),
    )
}

async fn policy() -> Html<String> {
    #[derive(Template)]
    #[template(path = "privacy_policy.html")]
    struct PolicyTemplate;
    Html(PolicyTemplate.render().expect("failed to render template"))
}

async fn tutorial() -> Html<String> {
    #[derive(Template)]
    #[template(path = "tutorial.html")]
    struct TutorialTemplate;
    Html(
        TutorialTemplate
            .render()
            .expect("failed to render template"),
    )
}

async fn history_home() -> Html<String> {
    #[derive(Template)]
    #[template(path = "history_home.html")]
    struct HistoryTemplate {
        track_names: Vec<String>,
    }
    let mut track_names: Vec<String> = read_track_file(OFFICIAL_TRACK_FILE)
        .await
        .into_iter()
        .map(|(_, name)| name)
        .collect();
    track_names.append(
        &mut read_track_file(COMMUNITY_TRACK_FILE)
            .await
            .into_iter()
            .map(|(_, name)| filenamify(name))
            .collect(),
    );
    Html(
        (HistoryTemplate { track_names })
            .render()
            .expect("failed to render template"),
    )
}

async fn history(Path(track_id): Path<String>) -> Html<String> {
    #[derive(Template)]
    #[template(path = "history.html")]
    struct HistoryTemplate {
        track_name: String,
        records: Vec<(String, String, String, String)>,
    }
    let records = parse_history(&track_id).await;
    Html(
        (HistoryTemplate {
            track_name: format!("Track {track_id}"),
            records,
        })
        .render()
        .expect("failed to render template"),
    )
}

#[tokio::main]
async fn main() {
    let subscriber = tracing_subscriber::FmtSubscriber::new();
    tracing::subscriber::set_global_default(subscriber).expect("Failed to set subscriber");
    let app = Router::new()
        .route("/", get(index))
        .route("/global", get(global))
        .route("/community", get(community))
        .route("/hof", get(hof))
        .route("/lb-standard", get(standard_lb_home))
        .route("/lb-standard/{track_id}", get(standard_lb))
        .route("/policy", get(policy))
        .route("/tutorial", get(tutorial))
        .route("/history", get(history_home))
        .route("/history/{track_id}", get(history))
        .route("/lbfunc", get(get_lbfunc))
        .route("/api/{list}", get(get_api))
        .nest_service("/static", ServeDir::new("static"));
    let addr = SocketAddr::from(([127, 0, 0, 1], 8000));
    let listener = TcpListener::bind(addr)
        .await
        .expect("failed to bind listener");
    tracing::info!("Listening on {addr}");
    axum::serve(listener, app)
        .await
        .expect("failed to serve app");
}
