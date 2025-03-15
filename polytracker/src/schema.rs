// @generated automatically by Diesel CLI.

diesel::table! {
    beta_users (id) {
        id -> Integer,
        name -> Text,
        game_id -> Text,
        discord -> Nullable<Text>,
    }
}

diesel::table! {
    users (id) {
        id -> Integer,
        name -> Text,
        game_id -> Text,
        discord -> Nullable<Text>,
    }
}

diesel::allow_tables_to_appear_in_same_query!(beta_users, users,);
