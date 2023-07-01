# spot_rev
Copy all songs from one spotify playlist to another, in reverse order of add date. Creates a cron job (in-process) that runs every hour.

# Why
Spotify for cars does not allow custom sorting, so this allows seeing the newest songs at the top in the copied playlist.

# Run
You'll need to create a file called .env with `CLIENT_ID`, `CLIENT_SECRET`, `REFRESH_TOKEN`, `FROM` (playlist id), `TO` (playlist_id) variables set appropriately. You'll need to go through the spotify authorization code process (https://developer.spotify.com/documentation/web-api/tutorials/code-flow) to accquire the first three.

I have it deployed to fly.io, but you can run the script with `cargo run`. If you don't want the scheduling (but rather run it immediately, then exit), just change the code in `main` to `do_work().await?`