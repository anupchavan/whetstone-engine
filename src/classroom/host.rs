//! The async shell around `Room`: one TCP listener that speaks plain HTTP
//! for the student join page and upgrades /ws connections to WebSocket.
//! The teacher's frontend never touches sockets — it drives this through
//! the sidecar's JSON-lines commands (host_start / host_control / host_status).

use classroom_core::protocol::*;
use classroom_core::room::{Outgoing, Phase, Room};
use classroom_core::QuizItem;
use anyhow::{Context, Result};
use futures::{SinkExt, StreamExt};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::net::TcpListener;
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::Message;

fn now_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).expect("clock").as_millis() as u64
}

struct Shared {
    room: Room,
    clients: HashMap<String, mpsc::UnboundedSender<HostMessage>>,
    /// Teacher-screen events since the last host_status poll.
    teacher_feed: Vec<HostMessage>,
    open_epoch: u64,
}

pub struct HostHandle {
    shared: Arc<Mutex<Shared>>,
    pub port: u16,
    pub room_code: String,
    current: Arc<Mutex<usize>>,
    shutdown: mpsc::UnboundedSender<()>,
}

pub async fn start(
    questions: Vec<QuizItem>,
    time_limit_secs: u32,
    preferred_port: u16,
) -> Result<HostHandle> {
    let room_code = format!("{:04}", (now_ms() / 7) % 10_000);
    let seed = now_ms() ^ 0xC1A5_5900;
    let shared = Arc::new(Mutex::new(Shared {
        room: Room::new(room_code.clone(), questions, time_limit_secs, seed),
        clients: HashMap::new(),
        teacher_feed: Vec::new(),
        open_epoch: 0,
    }));
    let listener = match TcpListener::bind(("0.0.0.0", preferred_port)).await {
        Ok(listener) => listener,
        Err(_) => TcpListener::bind(("0.0.0.0", 0)).await.context("no port available")?,
    };
    let port = listener.local_addr()?.port();
    let (shutdown, mut shutdown_rx) = mpsc::unbounded_channel();
    let accept_shared = shared.clone();
    tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = shutdown_rx.recv() => break,
                accepted = listener.accept() => {
                    let Ok((stream, _)) = accepted else { continue };
                    tokio::spawn(serve_connection(stream, accept_shared.clone()));
                }
            }
        }
    });
    Ok(HostHandle { shared, port, room_code, current: Arc::new(Mutex::new(0)), shutdown })
}

impl HostHandle {
    /// Teacher controls, mapped 1:1 from the sidecar command surface.
    pub fn control(&self, action: &str) -> Result<Value> {
        let mut shared = self.shared.lock().expect("host lock");
        let out = match action {
            "stage" | "next" => {
                let mut current = self.current.lock().expect("index lock");
                if action == "next" {
                    if matches!(shared.room.phase, Phase::Lobby) {
                        *current = 0;
                    } else {
                        *current += 1;
                    }
                }
                shared.room.stage(*current).map_err(anyhow::Error::msg)?
            }
            "go" => {
                let out = shared.room.go(now_ms()).map_err(anyhow::Error::msg)?;
                shared.open_epoch += 1;
                let epoch = shared.open_epoch;
                let deadline = match shared.room.phase {
                    Phase::Open { deadline_ms, .. } => deadline_ms,
                    _ => unreachable!(),
                };
                let auto = self.shared.clone();
                // Deadline closes the question even if the teacher walks off;
                // the epoch guard makes a manual close + next race harmless.
                tokio::spawn(async move {
                    tokio::time::sleep(std::time::Duration::from_millis(
                        deadline.saturating_sub(now_ms()) + 40,
                    ))
                    .await;
                    let mut shared = auto.lock().expect("host lock");
                    if shared.open_epoch == epoch {
                        if let Ok(out) = shared.room.close() {
                            deliver(&mut shared, out);
                        }
                    }
                });
                out
            }
            "close" => {
                shared.open_epoch += 1;
                shared.room.close().map_err(anyhow::Error::msg)?
            }
            "leaderboard" => shared.room.leaderboard(),
            "podium" => shared.room.podium(),
            other => anyhow::bail!("unknown action {other}"),
        };
        deliver(&mut shared, out);
        Ok(json!({"phase": phase_name(&shared.room.phase)}))
    }

    pub fn status(&self) -> Value {
        let mut shared = self.shared.lock().expect("host lock");
        let feed = std::mem::take(&mut shared.teacher_feed);
        json!({
            "room_code": self.room_code,
            "port": self.port,
            "players": shared.room.player_count(),
            "phase": phase_name(&shared.room.phase),
            "feed": feed,
            "results": shared.room.results().iter().map(|(name, score, per_q)| json!({
                "name": name, "score": score, "answers": per_q,
            })).collect::<Vec<_>>(),
        })
    }

    pub fn stop(&self) {
        let _ = self.shutdown.send(());
    }
}

fn phase_name(phase: &Phase) -> &'static str {
    match phase {
        Phase::Lobby => "lobby",
        Phase::Staged { .. } => "staged",
        Phase::Open { .. } => "open",
        Phase::Closed { .. } => "closed",
        Phase::Podium => "podium",
    }
}

fn deliver(shared: &mut Shared, out: Vec<Outgoing>) {
    for message in out {
        match message.to {
            Some(token) => {
                if let Some(sink) = shared.clients.get(&token) {
                    let _ = sink.send(message.message);
                }
            }
            None => shared.teacher_feed.push(message.message),
        }
    }
}

/// One student connection: sniff the first bytes — a WebSocket upgrade
/// handshake starts "GET /ws"; anything else gets the join page.
async fn serve_connection(stream: tokio::net::TcpStream, shared: Arc<Mutex<Shared>>) {
    let mut peeked = [0u8; 8];
    let Ok(n) = stream.peek(&mut peeked).await else { return };
    if !peeked[..n].starts_with(b"GET /ws") {
        serve_join_page(stream).await;
        return;
    }
    let Ok(ws) = tokio_tungstenite::accept_async(stream).await else { return };
    let (mut sink, mut source) = ws.split();
    let (tx, mut rx) = mpsc::unbounded_channel::<HostMessage>();
    let writer = tokio::spawn(async move {
        while let Some(message) = rx.recv().await {
            let text = serde_json::to_string(&message).expect("protocol serializes");
            if sink.send(Message::Text(text)).await.is_err() {
                break;
            }
        }
    });
    let mut my_token: Option<String> = None;
    while let Some(Ok(frame)) = source.next().await {
        let Message::Text(text) = frame else { continue };
        let Ok(message) = serde_json::from_str::<ClientMessage>(&text) else {
            let _ = tx.send(HostMessage::Error { v: PROTOCOL_VERSION, reason: "unrecognized message".into() });
            continue;
        };
        let mut shared_lock = shared.lock().expect("host lock");
        match message {
            ClientMessage::Join { room, name, token, .. } => {
                if room != shared_lock.room.code {
                    let _ = tx.send(HostMessage::Error { v: PROTOCOL_VERSION, reason: "wrong room code".into() });
                    continue;
                }
                let fresh = uuid::Uuid::new_v4().to_string();
                match shared_lock.room.join(&name, token.as_deref(), fresh) {
                    Ok((accepted_token, out)) => {
                        shared_lock.clients.insert(accepted_token.clone(), tx.clone());
                        my_token = Some(accepted_token);
                        deliver(&mut shared_lock, out);
                    }
                    Err(reason) => {
                        let _ = tx.send(HostMessage::Error { v: PROTOCOL_VERSION, reason });
                    }
                }
            }
            ClientMessage::Answer { q, choice, .. } => {
                if let Some(token) = &my_token {
                    let out = shared_lock.room.answer(token, q, choice, now_ms());
                    deliver(&mut shared_lock, out);
                }
            }
            ClientMessage::Ping { id, .. } => {
                let _ = tx.send(HostMessage::Pong { v: PROTOCOL_VERSION, id, host_ms: now_ms() });
            }
        }
    }
    if let Some(token) = my_token {
        let mut shared = shared.lock().expect("host lock");
        shared.clients.remove(&token);
        shared.room.disconnect(&token);
    }
    writer.abort();
}

/// The built-in join page is a functional placeholder; a frontend bundle
/// dropped at data_dir/classroom/join.html replaces it wholesale (the
/// sidecar wires that path in at startup via JOIN_PAGE_OVERRIDE).
pub static JOIN_PAGE_OVERRIDE: Mutex<Option<String>> = Mutex::new(None);

async fn serve_join_page(stream: tokio::net::TcpStream) {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let mut stream = stream;
    let mut sink_buffer = [0u8; 2048];
    let _ = stream.read(&mut sink_buffer).await; // drain the request
    let custom = JOIN_PAGE_OVERRIDE.lock().expect("page lock").clone();
    let body = custom.unwrap_or_else(|| include_str!("join.html").to_owned());
    let response = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        body.len(),
        body
    );
    let _ = stream.write_all(response.as_bytes()).await;
}
