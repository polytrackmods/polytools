# PolyTracker
Discord bot to retrieve some information from the game of PolyTrack.

### Features
- register/delete users with arbitrary usernames
  - deletion is owner-access-only
- retrieve a user's record time and position on a track
  - easier access for official tracks
  - records in top 500:
    - additional information about placement diregarding duplicate/unverified records
- compare two user's records on official tracks
- display registered users
- save/load user list to/from disk
  - normally deleted between restarts
  - owner-access only
- list servers the bot is in
  - owner-access only
- display global leaderboards
  - blacklist and support for alt accounts included
  - updates either manually (owner-access only) or automatically on invocation (default: every 10 minutes)
- help menu

### Commands
- `/assign <user> <id>`
  - register a user by supplying the in-game "User ID"
- `/delete <user>`
  - deletes a registered user entry
- `/request <off> <user> <track_id>`
  - display a user's personal best time and rank on the leaderboard on a standard/custom track
- `/list <user>`
  - list a user's personal best time and leaderboard rank on all standard tracks + total time for all tracks
- `/compare <user\_1> <user\_2>`
  - list both user's times like in `/list`
- `/rankings`
  - display global leaderboard
  - configurable entry requirement (default: 10 * 500)
  - updates rankings if older than configurable time (default: 10min)
- `/update_rankings <entry_requirement>`
  - manually updates leaderboard
  - optional parameter `<entry_requirement` (default: 10)
- `/guilds`
  - displays all servers the bot is in
- `/save`
  - save registered users to disk
- `/load`
  - load registered users from disk
- `/users`
  - list currently registered users
- `/help`
  - help menu

### Plans
- add generating track_id from track code
- add leaderboard display
- add integrated user list backups
- improve configuration/setup
