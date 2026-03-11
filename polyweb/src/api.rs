use axum::extract::{Path, Query};
use facet_json::Json;
use polycore::{
    ALT_ACCOUNT_FILE, BLACKLIST_FILE, COMMUNITY_RANKINGS_FILE, COMMUNITY_TIME_RANKINGS_FILE,
    HOF_RANKINGS_FILE, HOF_TIME_RANKINGS_FILE, OFFICIAL_RANKINGS_FILE, PolyLeaderBoard,
};
use serde::Deserialize;
use tokio::fs;

use crate::parsers;

#[derive(Deserialize)]
pub enum ApiList {
    Global,
    Hof,
    HofTime,
    Community,
    CommunityTime,
    History(String),
    AltList,
    BlackList,
}

pub(crate) async fn get_api(Path(list): Path<ApiList>) -> String {
    let file = {
        use ApiList::{
            AltList, BlackList, Community, CommunityTime, Global, History, Hof, HofTime,
        };
        match list {
            Global => OFFICIAL_RANKINGS_FILE,
            Hof => HOF_RANKINGS_FILE,
            HofTime => HOF_TIME_RANKINGS_FILE,
            Community => COMMUNITY_RANKINGS_FILE,
            CommunityTime => COMMUNITY_TIME_RANKINGS_FILE,
            AltList => ALT_ACCOUNT_FILE,
            BlackList => BLACKLIST_FILE,
            History(history_name) => &format!("histories/HISTORY_{history_name}.txt"),
        }
    };
    fs::read_to_string(file).await.expect("Failed to read file")
}

pub(crate) async fn get_lbfunc(Query(query): Query<LbFuncQuery>) -> Json<PolyLeaderBoard> {
    let file = {
        match query.leaderboard.as_str() {
            "global" => OFFICIAL_RANKINGS_FILE,
            "community" => COMMUNITY_RANKINGS_FILE,
            "community-time" => COMMUNITY_TIME_RANKINGS_FILE,
            _ => panic!("invalid leaderboard"),
        }
    };
    let leaderboard = parsers::parse_leaderboard(file).await;
    let leaderboard_out = PolyLeaderBoard {
        total: leaderboard.total,
        entries: leaderboard
            .entries
            .into_iter()
            .skip(query.skip)
            .take(query.amount)
            .collect::<Vec<_>>(),
    };
    Json(leaderboard_out)
}

#[derive(Deserialize)]
pub struct LbFuncQuery {
    leaderboard: String,
    skip: usize,
    amount: usize,
}
