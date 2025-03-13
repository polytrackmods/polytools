#!/bin/sh
touch data/blacklist.txt
touch data/alt_accounts.txt
touch data/poly_rankings.txt
touch data/hof_blacklist.txt
touch data/hof_alt_accounts.txt
touch data/hof_rankings.txt
touch data/0.5_poly_rankings.txt
touch data/custom_tracks.txt

cat .env &> /dev/null || cp .env.example .env

touch templates/privacy_policy.html.tera
echo "Add your Privacy Policy in templates/privacy_policy.html.tera"

diesel migration run
