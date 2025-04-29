#!/bin/sh
touch data/blacklist.txt
touch data/alt_accounts.txt
touch data/poly_rankings.txt
touch data/hof_blacklist.txt
touch data/hof_alt_accounts.txt
touch data/hof_rankings.txt
touch data/custom_tracks.txt

test -e .env || cp .env.example .env

touch templates/privacy_policy.html.tera
echo "Add your Privacy Policy in templates/privacy_policy.html.tera"

eval "$(cat .env)"
diesel migration run
