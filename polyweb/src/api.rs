use polycore::{
    ALT_ACCOUNT_FILE, BLACKLIST_FILE, COMMUNITY_RANKINGS_FILE, COMMUNITY_TIME_RANKINGS_FILE,
    HOF_RANKINGS_FILE, HOF_TIME_RANKINGS_FILE, PolyLeaderBoard, OFFICIAL_RANKINGS_FILE,
};
use rocket::{get, request::FromParam, tokio::fs};

use crate::parsers;

const HISTORY_START: &str = "history-";

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
impl<'a> FromParam<'a> for ApiList {
    type Error = &'a str;
    fn from_param(param: &'a str) -> Result<Self, Self::Error> {
        use ApiList::{
            AltList, BlackList, Community, CommunityTime, Global, History, Hof, HofTime,
        };
        if param.starts_with(HISTORY_START) {
            return Ok(History(
                param
                    .get(HISTORY_START.len()..)
                    .unwrap_or_default()
                    .to_string(),
            ));
        }
        match param.to_lowercase().as_str() {
            "global" => Ok(Global),
            "hof" => Ok(Hof),
            "hof_time" => Ok(HofTime),
            "community" => Ok(Community),
            "community_time" => Ok(CommunityTime),
            "altlist" => Ok(AltList),
            "blacklist" => Ok(BlackList),
            _ => Err("Failed to find enum"),
        }
    }
}

#[get("/api/<list>")]
pub(crate) async fn get_api(list: ApiList) -> String {
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

#[get("/lbfunc?<query..>")]
pub(crate) async fn get_lbfunc(query: LbFuncQuery) -> String {
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
    facet_json::to_string(&leaderboard_out)
}

#[derive(rocket::FromForm)]
pub struct LbFuncQuery {
    leaderboard: String,
    skip: usize,
    amount: usize,
}
