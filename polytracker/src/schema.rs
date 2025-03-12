// @generated automatically by Diesel CLI.

diesel::table! {
    users (id) {
        id -> Integer,
        name -> Text,
        game_id -> Text,
        discord -> Nullable<Text>,
    }
}
