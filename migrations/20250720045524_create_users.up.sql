-- Add up migration script here
CREATE TABLE users (
    id INTEGER NOT NULL PRIMARY KEY AUTOINCREMENT,
    name VARCHAR NOT NULL,
    game_id VARCHAR(64) NOT NULL,
    discord VARCHAR
);
