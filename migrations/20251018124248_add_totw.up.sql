CREATE TABLE totws (
    id INTEGER NOT NULL PRIMARY KEY AUTOINCREMENT,
    name TEXT NOT NULL,
    track_id TEXT NOT NULL,
    export_code TEXT,
    end INTEGER
);
CREATE TABLE totw_entries (
    totw_id INTEGER NOT NULL,
    player_id TEXT NOT NULL,
    rank INTEGER NOT NULL,
    points INTEGER NOT NULL,

    PRIMARY KEY (totw_id, player_id),
    FOREIGN KEY (totw_id) REFERENCES totws(id) ON DELETE CASCADE,
    FOREIGN KEY (player_id) REFERENCES totw_players(user_id) ON DELETE CASCADE
);
CREATE TABLE totw_players (
    user_id TEXT NOT NULL PRIMARY KEY,
    name TEXT NOT NULL
);
