use std::time::Duration;

use dotenv::dotenv;
use isahc::{AsyncBody, AsyncReadResponseExt, HttpClient, Request, Response};
use itertools::Itertools;
use serde::Deserialize;
use tokio_cron_scheduler::{Job, JobScheduler};
use tracing::{debug, error, info, instrument, Level};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt().with_max_level(Level::INFO).init();

    dotenv().ok();

    let mut sched = JobScheduler::new().await?;

    sched
        .add(Job::new_async("0 0 * * * *", |_, _| {
            Box::pin(async move {
                do_work().await.unwrap();
            })
        })?)
        .await?;

    sched.start().await?;

    loop {
        let dur = sched.time_till_next_job().await?;

        if let Some(dur) = dur {
            tokio::time::sleep(dur).await;
        } else {
            return Err("No jobs scheduled".into());
        }
    }
}

async fn do_work() -> Result<(), Box<dyn std::error::Error>> {
    let client = HttpClient::new()?;

    let from = std::env::var("FROM")?;
    let to = std::env::var("TO")?;

    let token = match get_token(&client).await {
        Ok(token) => token,
        Err(e) => {
            error!("Failed to acquire token: {}", e);
            return Err(e);
        }
    };

    info!("Token acquired, {}, {}", token.access_token, token.scope);

    match reset_reversed(&client, &token.access_token, &to).await {
        Ok(_) => info!("Reversed reset successfully"),
        Err(e) => {
            error!("Failed to reset reversed: {}", e);
            return Err(e);
        }
    }

    let songs = match get_songs(&client, &token.access_token, &from).await {
        Ok(songs) => songs,
        Err(e) => {
            error!("Failed to get songs: {}", e);
            return Err(e);
        }
    };

    info!("Got {} songs", songs.len());

    let mut iter = songs
        .into_iter()
        .filter(|song| !song.is_local)
        .sorted_by(|first, second| Ord::cmp(&first.added_at, &second.added_at))
        .map(|song| song.track.uri)
        .rev();

    while let Some(first) = iter.next() {
        let next_next = iter.next();
        let vec = match next_next {
            Some(next) => vec![first, next],
            None => vec![first],
        };

        match add_songs(&client, &token.access_token, &vec[..], &to).await {
            Ok(_) => debug!("Added songs {:?}", vec),
            Err(e) => {
                error!("Failed to add songs {:?}: {}", vec, e);
                return Err(e);
            }
        }

        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    Ok(())
}

#[instrument(skip(token))]
async fn add_songs(
    client: &HttpClient,
    token: &str,
    uris: &[String],
    playlist_id: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    debug!("Adding songs {:?}", uris);

    let request = Request::post(format!(
        "https://api.spotify.com/v1/playlists/{playlist_id}/tracks"
    ))
    .header("Authorization", format!("Bearer {}", token))
    .body(serde_json::to_string(&serde_json::json!({
        "uris": uris
    }))?)?;

    match client.send_async(request).await?.ok() {
        Ok(_) => Ok(()),
        Err(mut e) => Err(format!("Failed to add song: {}", e.text().await?).into()),
    }
}

#[instrument(skip(token))]
async fn get_songs(
    client: &HttpClient,
    token: &str,
    playlist_id: &str,
) -> Result<Vec<Song>, Box<dyn std::error::Error>> {
    info!("Getting songs");

    let mut vec = vec![];
    let mut should_cont = true;
    let mut offset = 0;

    while should_cont {
        let request = Request::get(format!(
            "https://api.spotify.com/v1/playlists/{playlist_id}/tracks?offset={offset}&limit=50",
        ))
        .header("Authorization", format!("Bearer {}", token))
        .body(())?;

        match client.send_async(request).await?.ok() {
            Ok(mut response) => {
                info!("Got songs at offset {offset}");

                let songs = response.json::<Pagination<Song>>().await?;

                vec.extend(songs.items.into_iter());

                should_cont = songs.next.is_some();

                offset += 50;
            }
            Err(mut e) => return Err(format!("Failed to get songs: {}", e.text().await?).into()),
        }
    }

    Ok(vec)
}

#[instrument(skip(token))]
async fn reset_reversed(
    client: &HttpClient,
    token: &str,
    playlist_id: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    info!("Resetting reversed");

    let request = Request::put(format!(
        "https://api.spotify.com/v1/playlists/{playlist_id}/tracks"
    ))
    .header("Content-Type", "application/json")
    .header("Authorization", format!("Bearer {}", token))
    .body(serde_json::to_string(&serde_json::json!({
        "uris": []
    }))?)?;

    info!("Sending request to reset reversed, {request:?}");

    match client.send_async(request).await?.ok() {
        Ok(_) => Ok(()),
        Err(mut e) => Err(format!("Failed to reset reversed: {}", e.text().await?).into()),
    }
}

#[instrument]
async fn get_token(client: &HttpClient) -> Result<AccessToken, Box<dyn std::error::Error>> {
    use base64::{engine::general_purpose, Engine as _};

    let refresh_token = std::env::var("REFRESH_TOKEN")?;

    let encoded = general_purpose::STANDARD.encode(
        format!(
            "{}:{}",
            std::env::var("CLIENT_ID")?,
            std::env::var("CLIENT_SECRET")?
        )
        .as_bytes(),
    );

    // TODO client id and secret
    let request = Request::post("https://accounts.spotify.com/api/token")
        .header("Content-Type", "application/x-www-form-urlencoded")
        .header("Authorization", format!("Basic {encoded}"))
        .body(format!(
        "grant_type=refresh_token&refresh_token={refresh_token}&scope=playlist-read-private%20playlist-modify-private%20playlist-modify-public%20user-library-read%20user-library-modify"
    ))?;

    info!("Sending request to acquire token");

    match client.send_async(request).await?.ok() {
        Ok(mut response) => Ok(response.json::<AccessToken>().await?),
        Err(mut e) => Err(format!("Failed to acquire token: {}", e.text().await?).into()),
    }
}

#[derive(Deserialize)]
struct Pagination<T> {
    next: Option<String>,
    items: Vec<T>,
}

#[derive(Deserialize, Debug)]
struct Song {
    added_at: String,
    is_local: bool,
    track: Track,
}

#[derive(Deserialize, Debug)]
struct Track {
    uri: String,
}

#[derive(Deserialize)]
struct AccessToken {
    access_token: String,
    scope: String,
}

trait OkExt: Sized {
    fn ok(self) -> Result<Self, Self>;
}

impl OkExt for Response<AsyncBody> {
    fn ok(self) -> Result<Self, Self> {
        match self.status().as_u16() {
            200..=299 => Ok(self),
            _ => Err(self),
        }
    }
}
