#!/bin/sh
touch data/alt_accounts.txt
touch data/blacklist.txt
touch data/community_rankings.txt
touch data/community_time_rankings.txt
touch data/hof_rankings.txt
touch data/hof_time_rankings.txt
touch data/poly_rankings.txt

test -e .env || cp .env.example .env

touch templates/privacy_policy.html.tera
echo "Add your Privacy Policy in templates/privacy_policy.html.tera"

export DATABASE_URL="${DATABASE_URL:-sqlite://poly.db}"
sqlx database setup --no-dotenv
