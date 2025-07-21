use polycore::{
    COMMUNITY_RANKINGS_FILE, COMMUNITY_TIME_RANKINGS_FILE, HOF_RANKINGS_FILE,
    HOF_TIME_RANKINGS_FILE, RANKINGS_FILE,
};
use rocket::{get, request::FromParam, tokio::fs};

const HISTORY_START: &str = "history-";

pub enum ApiList {
    Global,
    Hof,
    HofTime,
    Community,
    CommunityTime,
    History(String),
}
impl<'a> FromParam<'a> for ApiList {
    type Error = &'a str;
    fn from_param(param: &'a str) -> Result<Self, Self::Error> {
        use ApiList::{Community, CommunityTime, Global, History, Hof, HofTime};
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
            _ => Err("Failed to find enum"),
        }
    }
}

#[allow(clippy::missing_panics_doc)]
#[get("/api/<list>")]
pub async fn get_api(list: ApiList) -> String {
    let file = {
        use ApiList::{Community, CommunityTime, Global, History, Hof, HofTime};
        match list {
            Global => RANKINGS_FILE,
            Hof => HOF_RANKINGS_FILE,
            HofTime => HOF_TIME_RANKINGS_FILE,
            Community => COMMUNITY_RANKINGS_FILE,
            CommunityTime => COMMUNITY_TIME_RANKINGS_FILE,
            History(history_name) => &format!("histories/HISTORY_{history_name}.txt"),
        }
    };
    fs::read_to_string(file).await.expect("Failed to read file")
}
