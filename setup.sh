#!/bin/sh
touch userIDs.json

touch blacklist.txt
touch alt_accounts.txt
touch poly_rankings.txt

touch hof_blacklist.txt
touch hof_alt_accounts.txt
touch hof_rankings.txt

touch custom_tracks.txt
touch 0.5_poly_rankings.txt

cat .env &> /dev/null || cp .env.example .env

touch templates/privacy_policy.html.tera
echo "Please setup a Privacy Policy in templates/privacy_policy.html.tera"
