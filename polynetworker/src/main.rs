use anyhow::Result;
use axum::{
    Json, Router,
    extract::State,
    response::IntoResponse,
    routing::{get, post},
};
use reqwest::Client;
use reqwest::StatusCode;
use serde::Deserialize;
use std::{
    collections::VecDeque,
    net::SocketAddr,
    sync::{Arc, Mutex, atomic::AtomicU32},
    time::{Duration, Instant},
};
use tokio::{net::TcpListener, sync::oneshot::Sender, task, time::sleep};

// current Kodub rate limit value, slightly adapted to be safe
const MAX_PER_MINUTE: f64 = 59.0;
const WINDOW_SECONDS: f64 = 300.0;

#[derive(Deserialize)]
struct UrlRequest {
    url: String,
}

#[derive(Debug)]
struct QueueEntry {
    url: String,
    responder: Sender<(StatusCode, String)>,
}

#[derive(Clone)]
struct AppState {
    queue: SharedQueue,
    count: Arc<AtomicU32>,
}

type SharedQueue = Arc<Mutex<VecDeque<QueueEntry>>>;

#[tokio::main]
async fn main() -> Result<()> {
    let subscriber = tracing_subscriber::fmt().compact().finish();
    tracing::subscriber::set_global_default(subscriber)?;
    let queue: SharedQueue = Arc::new(Mutex::new(VecDeque::new()));
    let count = Arc::new(AtomicU32::new(0));
    let state = AppState {
        queue: Arc::clone(&queue),
        count,
    };
    let limiter: RateLimiter = RateLimiter::new(MAX_PER_MINUTE, WINDOW_SECONDS);
    let client = Client::new();

    {
        let queue = Arc::clone(&queue);
        task::spawn(async move {
            dispatcher(queue, limiter, client).await;
        });
    }

    let app = Router::new()
        .route("/submit", post(handle_submit))
        .route("/queue", get(get_queue))
        .route("/count", get(get_count))
        .route("/reset_count", get(reset_count))
        .with_state(state);

    let addr = SocketAddr::from(([127, 0, 0, 1], 3000));
    let listener = TcpListener::bind(addr).await?;
    tracing::info!("Listening on http://{addr}");

    axum::serve(listener, app).await?;
    Ok(())
}

#[allow(clippy::significant_drop_tightening)]
async fn get_queue(State(state): State<AppState>) -> impl IntoResponse {
    let queue = state.queue.lock().expect("other threads should not panic");
    let queue_out: Vec<_> = queue.iter().map(|entry| entry.url.clone()).collect();
    Json(queue_out)
}

async fn get_count(State(state): State<AppState>) -> String {
    let count = state.count.load(std::sync::atomic::Ordering::Relaxed);
    count.to_string()
}

async fn reset_count(State(state): State<AppState>) -> String {
    let count = state.count.load(std::sync::atomic::Ordering::Relaxed);
    tracing::info!("Resetting! Current request count: {count}");
    state.count.store(0, std::sync::atomic::Ordering::Relaxed);
    count.to_string()
}

async fn handle_submit(
    State(state): State<AppState>,
    Json(payload): Json<UrlRequest>,
) -> impl IntoResponse {
    let (tx, rx) = tokio::sync::oneshot::channel();
    {
        let mut q = state.queue.lock().expect("other threads should not panic");
        q.push_back(QueueEntry {
            url: payload.url,
            responder: tx,
        });
    }
    state
        .count
        .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    rx.await.unwrap_or_else(|_| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            "Sender dropped".to_string(),
        )
    })
}

async fn dispatcher(queue: SharedQueue, mut limiter: RateLimiter, client: Client) {
    loop {
        let task_opt = {
            let mut queue = queue.lock().expect("other threads should not panic");
            if queue.front().is_some() {
                if limiter.is_limited() {
                    None
                } else {
                    queue.pop_front()
                }
            } else {
                None
            }
        };

        if let Some(entry) = task_opt {
            let client = client.clone();
            task::spawn(async move {
                let res = client.get(entry.url).send().await;
                let response = match res {
                    Ok(resp) => {
                        let status = resp.status();
                        let text = resp.text().await.unwrap_or_default();
                        (status, text)
                    }
                    Err(e) => (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        format!("Request error: {e}"),
                    ),
                };
                entry
                    .responder
                    .send(response)
                    .unwrap_or_else(|_| tracing::error!("Receiver dropped"));
            });
        } else {
            sleep(Duration::from_millis(100)).await;
        }
    }
}

struct RateLimiter {
    max_per_minute: f64,
    window_seconds: f64,
    client: ClientQuota,
}

struct ClientQuota {
    quota: f64,
    last_update: Instant,
}

impl RateLimiter {
    fn new(max_per_minute: f64, window_seconds: f64) -> Self {
        Self {
            max_per_minute,
            window_seconds,
            client: ClientQuota {
                quota: 0.0,
                last_update: Instant::now(),
            },
        }
    }

    fn is_limited(&mut self) -> bool {
        let now = Instant::now();
        let cost = 60.0 / self.max_per_minute / self.window_seconds;

        let elapsed = now.duration_since(self.client.last_update).as_secs_f64();
        self.client.quota = (self.client.quota + elapsed / self.window_seconds).min(1.0);
        self.client.last_update = now;

        if self.client.quota < cost {
            true
        } else {
            self.client.quota -= cost;
            false
        }
    }
}
