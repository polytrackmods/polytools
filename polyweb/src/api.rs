use polymanager::{
    COMMUNITY_RANKINGS_FILE, COMMUNITY_TIME_RANKINGS_FILE, HOF_RANKINGS_FILE,
    HOF_TIME_RANKINGS_FILE, RANKINGS_FILE,
};
use rocket::{get, request::FromParam, tokio::fs};

pub enum ApiList {
    Global,
    Hof,
    HofTime,
    Community,
    CommunityTime,
}
impl<'a> FromParam<'a> for ApiList {
    type Error = &'a str;
    fn from_param(param: &'a str) -> Result<Self, Self::Error> {
        use ApiList::{Community, CommunityTime, Global, Hof, HofTime};
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
        use ApiList::{Community, CommunityTime, Global, Hof, HofTime};
        match list {
            Global => RANKINGS_FILE,
            Hof => HOF_RANKINGS_FILE,
            HofTime => HOF_TIME_RANKINGS_FILE,
            Community => COMMUNITY_RANKINGS_FILE,
            CommunityTime => COMMUNITY_TIME_RANKINGS_FILE,
        }
    };
    fs::read_to_string(file).await.expect("Failed to read file")
}
