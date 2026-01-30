#!/bin/sh
touch data/alt_accounts.json
touch data/blacklist.json
touch data/community_rankings.json
touch data/community_time_rankings.json
touch data/hof_rankings.json
touch data/hof_time_rankings.json
touch data/official_rankings.json
touch data/official_time_rankings.json

test -e .env || cp .env.example .env

touch templates/privacy_policy.html.tera
echo "Add your Privacy Policy in templates/privacy_policy.html.tera"

export DATABASE_URL="${DATABASE_URL:-sqlite://poly.db}"
sqlx database setup --no-dotenv
