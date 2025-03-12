use crate::schema::users;
use diesel::prelude::*;
use diesel::sqlite::Sqlite;
use dotenvy::dotenv;
use std::env;

pub fn establish_connection() -> SqliteConnection {
    dotenv().ok();

    let database_url = env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    SqliteConnection::establish(&database_url)
        .unwrap_or_else(|_| panic!("Error connecting to {}", database_url))
}

#[derive(Queryable, Selectable)]
#[diesel(table_name = users)]
#[diesel(check_for_backend(Sqlite))]
pub struct User {
    pub id: i32,
    pub name: String,
    pub game_id: String,
    pub discord: Option<String>,
}

#[derive(Insertable)]
#[diesel(table_name = users)]
pub struct NewUser<'a> {
    pub name: &'a str,
    pub game_id: &'a str,
    pub discord: Option<&'a str>,
}
