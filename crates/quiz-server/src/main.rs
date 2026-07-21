//! The hosted quiz server: the same Room brain the native apps run,
//! behind one port, many rooms. Zero install for teachers - the host
//! console is a browser tab speaking /hostws; students join exactly as
//! they do on LAN. Rooms are ephemeral: reclaimable by host token,
//! garbage-collected after inactivity, nothing persisted.

use anyhow::Result;
use classroom_core::protocol::*;
use classroom_core::room::{Outgoing, Phase, Room};
use classroom_core::QuizItem;
use futures::{SinkExt, StreamExt};
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::Message;

fn now_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).expect("clock").as_millis() as u64
}

struct RoomState {
    room: Room,
    host_token: String,
    clients: HashMap<String, mpsc::UnboundedSender<HostMessage>>,
    host_sink: Option<mpsc::UnboundedSender<HostMessage>>,
    current: usize,
    open_epoch: u64,
    last_activity: Instant,
}

type Registry = Arc<Mutex<HashMap<String, Arc<Mutex<RoomState>>>>>;

/// Teacher-side commands over /hostws. Everything else the teacher sees
/// arrives as ordinary HostMessages on the same socket.
#[derive(Deserialize)]
#[serde(tag = "t", rename_all = "snake_case")]
enum HostCommand {
    Create { quiz: Vec<QuizItem>, #[serde(default = "default_time_limit")] time_limit_secs: u32 },
    Reclaim { room: String, host_token: String },
    Control { action: String },
}

fn default_time_limit() -> u32 {
    30
}

#[tokio::main]
async fn main() -> Result<()> {
    let port: u16 = std::env::var("PORT").ok().and_then(|p| p.parse().ok()).unwrap_or(8080);
    let registry: Registry = Arc::new(Mutex::new(HashMap::new()));
    // Sweep rooms idle beyond two hours; a classroom never is.
    {
        let registry = registry.clone();
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(Duration::from_secs(600)).await;
                registry.lock().expect("registry").retain(|_, room| {
                    room.lock().expect("room").last_activity.elapsed() < Duration::from_secs(7200)
                });
            }
        });
    }
    let listener = TcpListener::bind(("0.0.0.0", port)).await?;
    eprintln!("quiz-server listening on :{port}");
    loop {
        let Ok((stream, _)) = listener.accept().await else { continue };
        tokio::spawn(serve(stream, registry.clone()));
    }
}

async fn serve(stream: TcpStream, registry: Registry) {
    let mut head = [0u8; 12];
    let Ok(n) = stream.peek(&mut head).await else { return };
    let head = &head[..n];
    if head.starts_with(b"GET /hostws") {
        host_socket(stream, registry).await;
    } else if head.starts_with(b"GET /ws") {
        student_socket(stream, registry).await;
    } else {
        static_page(stream, head.starts_with(b"GET /host")).await;
    }
}

async fn static_page(mut stream: TcpStream, host: bool) {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let mut sink = [0u8; 2048];
    let _ = stream.read(&mut sink).await;
    let body: &str = if host { include_str!("host.html") } else { include_str!("join.html") };
    let response = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        body.len(), body
    );
    let _ = stream.write_all(response.as_bytes()).await;
}

fn deliver(state: &mut RoomState, out: Vec<Outgoing>) {
    state.last_activity = Instant::now();
    for message in out {
        match message.to {
            Some(token) => {
                if let Some(sink) = state.clients.get(&token) {
                    let _ = sink.send(message.message);
                }
            }
            None => {
                if let Some(sink) = &state.host_sink {
                    let _ = sink.send(message.message);
                }
            }
        }
    }
}

fn control(handle: &Arc<Mutex<RoomState>>, action: &str) -> Result<Value, String> {
    let mut state = handle.lock().expect("room");
    let out = match action {
        "stage" | "next" => {
            if action == "next" {
                if matches!(state.room.phase, Phase::Lobby) {
                    state.current = 0;
                } else {
                    state.current += 1;
                }
            }
            let q = state.current;
            state.room.stage(q)?
        }
        "go" => {
            let out = state.room.go(now_ms())?;
            state.open_epoch += 1;
            let epoch = state.open_epoch;
            let deadline = match state.room.phase {
                Phase::Open { deadline_ms, .. } => deadline_ms,
                _ => unreachable!(),
            };
            let auto = handle.clone();
            tokio::spawn(async move {
                tokio::time::sleep(Duration::from_millis(deadline.saturating_sub(now_ms()) + 40)).await;
                let mut state = auto.lock().expect("room");
                if state.open_epoch == epoch {
                    if let Ok(out) = state.room.close() {
                        deliver(&mut state, out);
                    }
                }
            });
            out
        }
        "close" => {
            state.open_epoch += 1;
            state.room.close()?
        }
        "leaderboard" => state.room.leaderboard(),
        "podium" => state.room.podium(),
        other => return Err(format!("unknown action {other}")),
    };
    deliver(&mut state, out);
    Ok(json!({"ok": true}))
}

async fn host_socket(stream: TcpStream, registry: Registry) {
    let Ok(ws) = tokio_tungstenite::accept_async(stream).await else { return };
    let (mut sink, mut source) = ws.split();
    let (tx, mut rx) = mpsc::unbounded_channel::<HostMessage>();
    let writer = tokio::spawn(async move {
        while let Some(message) = rx.recv().await {
            let text = serde_json::to_string(&message).expect("serializes");
            if sink.send(Message::Text(text)).await.is_err() {
                break;
            }
        }
    });
    let mut my_room: Option<Arc<Mutex<RoomState>>> = None;
    while let Some(Ok(Message::Text(text))) = source.next().await {
        let Ok(command) = serde_json::from_str::<HostCommand>(&text) else {
            let _ = tx.send(HostMessage::Error { v: PROTOCOL_VERSION, reason: "unrecognized command".into() });
            continue;
        };
        match command {
            HostCommand::Create { quiz, time_limit_secs } => {
                if quiz.is_empty() || quiz.iter().any(|q| q.stem.is_empty() || q.options.len() < 2) {
                    let _ = tx.send(HostMessage::Error { v: PROTOCOL_VERSION, reason: "every question needs a stem and two or more options".into() });
                    continue;
                }
                let code = {
                    let registry = registry.lock().expect("registry");
                    loop {
                        let candidate = format!("{:06}", rand::random::<u32>() % 1_000_000);
                        if !registry.contains_key(&candidate) {
                            break candidate;
                        }
                    }
                };
                let host_token = uuid::Uuid::new_v4().to_string();
                let state = Arc::new(Mutex::new(RoomState {
                    room: Room::new(code.clone(), quiz, time_limit_secs, now_ms() ^ 0x51D3),
                    host_token: host_token.clone(),
                    clients: HashMap::new(),
                    host_sink: Some(tx.clone()),
                    current: 0,
                    open_epoch: 0,
                    last_activity: Instant::now(),
                }));
                registry.lock().expect("registry").insert(code.clone(), state.clone());
                my_room = Some(state);
                let _ = tx.send(HostMessage::Error { v: PROTOCOL_VERSION, reason: format!("created:{code}:{host_token}") });
            }
            HostCommand::Reclaim { room, host_token } => {
                let found = registry.lock().expect("registry").get(&room).cloned();
                match found {
                    Some(state) if state.lock().expect("room").host_token == host_token => {
                        state.lock().expect("room").host_sink = Some(tx.clone());
                        my_room = Some(state);
                        let _ = tx.send(HostMessage::Error { v: PROTOCOL_VERSION, reason: "reclaimed".into() });
                    }
                    _ => {
                        let _ = tx.send(HostMessage::Error { v: PROTOCOL_VERSION, reason: "no such room or wrong token".into() });
                    }
                }
            }
            HostCommand::Control { action } => match &my_room {
                Some(handle) => {
                    if let Err(reason) = control(handle, &action) {
                        let _ = tx.send(HostMessage::Error { v: PROTOCOL_VERSION, reason });
                    }
                }
                None => {
                    let _ = tx.send(HostMessage::Error { v: PROTOCOL_VERSION, reason: "create or reclaim a room first".into() });
                }
            },
        }
    }
    if let Some(handle) = my_room {
        handle.lock().expect("room").host_sink = None;
    }
    writer.abort();
}

async fn student_socket(stream: TcpStream, registry: Registry) {
    let Ok(ws) = tokio_tungstenite::accept_async(stream).await else { return };
    let (mut sink, mut source) = ws.split();
    let (tx, mut rx) = mpsc::unbounded_channel::<HostMessage>();
    let writer = tokio::spawn(async move {
        while let Some(message) = rx.recv().await {
            let text = serde_json::to_string(&message).expect("serializes");
            if sink.send(Message::Text(text)).await.is_err() {
                break;
            }
        }
    });
    let mut my: Option<(Arc<Mutex<RoomState>>, String)> = None;
    while let Some(Ok(Message::Text(text))) = source.next().await {
        let Ok(message) = serde_json::from_str::<ClientMessage>(&text) else { continue };
        match message {
            ClientMessage::Join { room, name, token, .. } => {
                let Some(handle) = registry.lock().expect("registry").get(&room).cloned() else {
                    let _ = tx.send(HostMessage::Error { v: PROTOCOL_VERSION, reason: "no room with that PIN".into() });
                    continue;
                };
                let mut state = handle.lock().expect("room");
                let fresh = uuid::Uuid::new_v4().to_string();
                match state.room.join(&name, token.as_deref(), fresh) {
                    Ok((accepted, out)) => {
                        state.clients.insert(accepted.clone(), tx.clone());
                        deliver(&mut state, out);
                        drop(state);
                        my = Some((handle, accepted));
                    }
                    Err(reason) => {
                        let _ = tx.send(HostMessage::Error { v: PROTOCOL_VERSION, reason });
                    }
                }
            }
            ClientMessage::Answer { q, choice, .. } => {
                if let Some((handle, token)) = &my {
                    let mut state = handle.lock().expect("room");
                    let out = state.room.answer(token, q, choice, now_ms());
                    deliver(&mut state, out);
                }
            }
            ClientMessage::Ping { id, .. } => {
                let _ = tx.send(HostMessage::Pong { v: PROTOCOL_VERSION, id, host_ms: now_ms() });
            }
        }
    }
    if let Some((handle, token)) = my {
        let mut state = handle.lock().expect("room");
        state.clients.remove(&token);
        state.room.disconnect(&token);
    }
    writer.abort();
}
