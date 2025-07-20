-- Add up migration script here
CREATE TABLE admins (
    id INTEGER NOT NULL PRIMARY KEY AUTOINCREMENT,
    discord VARCHAR NOT NULL,
    privilege INTEGER NOT NULL
);
