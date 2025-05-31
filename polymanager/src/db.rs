use crate::schema::{admins, users};
use anyhow::Error;
use diesel::prelude::*;
use diesel::sqlite::Sqlite;
use dotenvy::dotenv;
use std::env;

#[allow(clippy::missing_errors_doc)]
#[allow(clippy::missing_panics_doc)]
pub fn establish_connection() -> Result<SqliteConnection, Error> {
    dotenv().ok();

    let database_url = env::var("DATABASE_URL").expect("Expected DATABASE_URL in env");
    Ok(SqliteConnection::establish(&database_url)?)
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

#[derive(Queryable, Selectable)]
#[diesel(table_name = admins)]
#[diesel(check_for_backend(Sqlite))]
pub struct Admin {
    pub id: i32,
    pub discord: String,
    pub privilege: i32,
}

#[derive(Insertable)]
#[diesel(table_name = admins)]
pub struct NewAdmin<'a> {
    pub discord: &'a str,
    pub privilege: &'a i32,
}
