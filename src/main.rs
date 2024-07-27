mod display_as_json;
mod error;
mod expiration_queue;
mod nostr;
mod priority_queue;
mod relay;

use crate::nostr::Filter;
use axum::extract::ws::rejection::WebSocketUpgradeRejection;
use axum::extract::ws::{self, WebSocket};
use axum::extract::{State, WebSocketUpgrade};
use axum::http::HeaderMap;
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::Router;
use display_as_json::AsJson;
use error::Error;
use expiration_queue::wait_expiration;
use log::{debug, info, warn};
use nostr::{ClientToRelay, Event, FirstTagValue, PubKey, Tag};
use parking_lot::RwLock;
use rand::RngCore;
use relay::{AddEventError, Db, GetEvents, GetEventsStopped, Time};
use rustc_hash::FxHashMap;
use serde_json::json;
use smallvec::SmallVec;
use std::fmt::Debug;
use std::str::FromStr;
use std::sync::{Arc, LazyLock as Lazy};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const BIND_ADDRESS: &str = "127.0.0.1:8017";

#[derive(Debug)]
pub struct AppState {
    db: RwLock<Db>,
    broadcast_sender: tokio::sync::broadcast::Sender<Arc<Event>>,
    event_expiration_sender: tokio::sync::mpsc::Sender<Time>,
}

pub async fn listen(state: Arc<AppState>) -> Result<(), Error> {
    info!("Listening on {BIND_ADDRESS}");
    let app = Router::new()
        .route("/", get(root))
        .fallback(handler_404)
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(BIND_ADDRESS).await.unwrap();
    Ok(axum::serve(listener, app).await?)
}

const MAX_MESSAGE_LENGTH: usize = 0xFFFF;
const CREATED_AT_UPPER_LIMIT: u64 = 600;

static RELAY_INFORMATION: Lazy<String> = Lazy::new(|| {
    json!({
        "description": "This is a rockstr instance.",
        "name": "rockstr for momostr",
        "software": "rockstr",
        "supported_nips": [1, 9, 11, 40],
        "version": "0.0.1",
        "limitation": {
            "max_message_length": MAX_MESSAGE_LENGTH,
            "created_at_upper_limit": CREATED_AT_UPPER_LIMIT,
        },
    })
    .to_string()
});

pub async fn root(
    State(state): State<Arc<AppState>>,
    ws: Result<WebSocketUpgrade, WebSocketUpgradeRejection>,
    headers: HeaderMap,
) -> Response {
    info!("root");
    match ws {
        Ok(ws) => ws
            .on_failed_upgrade(|a| {
                info!("on_failed_upgrade: {a}");
            })
            .on_upgrade(|ws| async {
                let receiver = state.broadcast_sender.subscribe();
                let mut challenge = [0u8; 16];
                rand::thread_rng().fill_bytes(&mut challenge);
                let mut cs = ConnectionState {
                    ws,
                    broadcast_receiver: receiver,
                    req: FxHashMap::default(),
                    challenge: hex::encode(challenge),
                };
                let _ = ws_handler(state, &mut cs).await;
                let _ = cs.ws.close().await;
            }),
        Err(e) => {
            use WebSocketUpgradeRejection::*;
            if !matches!(e, InvalidConnectionHeader(_) | InvalidUpgradeHeader(_)) {
                e.into_response()
            } else if headers
                .get("accept")
                .and_then(|a| a.to_str().ok())
                .map_or(false, |a| a.contains("application/nostr+json"))
            {
                RELAY_INFORMATION.as_str().into_response()
            } else {
                "rockstr relay\n".into_response()
            }
        }
    }
}

const TIMEOUT_DURATION: Duration = Duration::from_secs(60 * 3);

#[derive(Debug)]
enum CloseReason {
    WsClosed,
    NoResponse,
}

struct ConnectionState {
    ws: WebSocket,
    broadcast_receiver: tokio::sync::broadcast::Receiver<Arc<Event>>,
    req: FxHashMap<String, SmallVec<[Filter; 2]>>,
    challenge: String,
}

async fn ws_handler(state: Arc<AppState>, cs: &mut ConnectionState) -> Result<CloseReason, Error> {
    cs.ws
        .send(ws::Message::Text(format!(
            r#"["AUTH", "{}"]"#,
            cs.challenge
        )))
        .await?;
    let mut waiting_for_pong = false;
    let r = loop {
        tokio::select! {
            m = tokio::time::timeout(TIMEOUT_DURATION, cs.ws.recv()) => {
                match m {
                    Ok(Some(Ok(m))) => {
                        waiting_for_pong = false;
                        if let Some(r) = handle_message(&state, cs, m).await? {
                            break r;
                        }
                    }
                    Err(e) => {
                        debug!("timeout: {e}");
                        if waiting_for_pong {
                            break CloseReason::NoResponse;
                        } else {
                            let _ = cs.ws.send(ws::Message::Ping(Vec::new())).await;
                            waiting_for_pong = true;
                        }
                    }
                    _ => break CloseReason::WsClosed,
                }
            }
            e = cs.broadcast_receiver.recv() => {
                receive_broadcast(cs, e).await
            },
        }
    };
    Ok(r)
}

async fn receive_broadcast(
    cs: &mut ConnectionState,
    e: Result<Arc<Event>, tokio::sync::broadcast::error::RecvError>,
) {
    match e {
        Ok(e) => {
            for (id, filters) in &cs.req {
                if filters.iter().any(|f| f.matches(&e)) {
                    let _ = cs.ws.send(ws::Message::Text(event_message(id, &e))).await;
                }
            }
        }
        Err(e) => {
            log::error!("receive error: {e}")
        }
    }
}

fn event_message(id: &str, e: &Event) -> String {
    format!(r#"["EVENT",{},{}]"#, AsJson(&id), AsJson(e))
}

fn expiration_time(e: &Event) -> Option<u64> {
    e.tags.iter().find_map(|Tag(k, v)| {
        if k != "expiration" {
            return None;
        }
        match &v.as_ref()?.0 {
            FirstTagValue::Hex32(_) => None,
            FirstTagValue::String(t) => t.parse::<u64>().ok(),
        }
    })
}

async fn handle_message(
    state: &Arc<AppState>,
    cs: &mut ConnectionState,
    m: ws::Message,
) -> Result<Option<CloseReason>, Error> {
    use axum::extract::ws::Message;
    let continue_ = match m {
        Message::Text(s) => match serde_json::from_str(&s) {
            Ok(m) => {
                let now = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_secs();
                match m {
                    ClientToRelay::Event(e) => {
                        handle_event(state, cs, e, s.len(), now).await?;
                        None
                    }
                    ClientToRelay::Req { id, filters } => {
                        debug!("req with {s}");
                        'filters_loop: for f in &filters {
                            let mut limit = f.limit;
                            enum St {
                                Init,
                                Middle(GetEventsStopped),
                                End,
                            }
                            let mut continuation = St::Init;
                            loop {
                                let mut ms = Vec::with_capacity(100);
                                continuation = {
                                    let db = state.db.read();
                                    let mut s = match continuation {
                                        St::Init => {
                                            let Some(s) = GetEvents::new(f, &db) else {
                                                continue 'filters_loop;
                                            };
                                            s
                                        }
                                        St::Middle(s) => s.restart(&db),
                                        St::End => panic!(),
                                    };
                                    loop {
                                        if limit == 0 {
                                            break St::End;
                                        }
                                        limit -= 1;
                                        let m = {
                                            let Some(Time(t, n)) = s.next(&db) else {
                                                break St::End;
                                            };
                                            if t < f.since {
                                                break St::End;
                                            }
                                            let e = db.n_to_event_get(n).unwrap();
                                            Message::Text(event_message(&id, &e))
                                        };
                                        ms.push(m);
                                        if ms.len() >= 100 {
                                            break St::Middle(s.stop());
                                        }
                                    }
                                };
                                for m in ms {
                                    cs.ws.send(m).await?;
                                }
                                if matches!(continuation, St::End) {
                                    continue 'filters_loop;
                                }
                            }
                        }
                        cs.ws
                            .send(Message::Text(format!(r#"["EOSE",{}]"#, AsJson(&id))))
                            .await?;
                        cs.req.insert(id, filters);
                        None
                    }
                    ClientToRelay::Close(id) => {
                        debug!("close {id}");
                        cs.req.remove(id.as_ref());
                        None
                    }
                }
            }
            Err(e) => {
                warn!("parse error: {e}, text = {s:?}");
                return Ok(None);
            }
        },
        Message::Binary(_) => None,
        Message::Ping(a) => {
            cs.ws.send(Message::Pong(a)).await?;
            None
        }
        Message::Pong(_) => None,
        Message::Close(_) => Some(CloseReason::WsClosed),
    };
    Ok(continue_)
}

async fn handle_event(
    state: &Arc<AppState>,
    cs: &mut ConnectionState,
    event: Arc<Event>,
    event_len: usize,
    now: u64,
) -> Result<(), Error> {
    use axum::extract::ws::Message;
    let id = event.id;
    let mut expiration = None;
    let (accepted, message) = if event_len > MAX_MESSAGE_LENGTH {
        (false, "invalid: too large event")
    } else {
        let mut db = state.db.write();
        if db.blocked_pubkeys.contains(&event.pubkey) {
            (false, "blocked")
        } else if !event.verify_hash() {
            (false, "invalid: bad event id")
        } else if !event.verify_sig() {
            (false, "invalid: bad signature")
        } else if db.is_deleted(&id) {
            (false, "deleted: user requested deletion")
        } else if event.created_at > now + CREATED_AT_UPPER_LIMIT {
            (false, "invalid: created_at too early")
        } else if event.kind == 22242 {
            (true, "")
        } else if (20_000..30_000).contains(&event.kind) {
            let _ = state.broadcast_sender.send(event);
            (true, "")
        } else {
            let ex = expiration_time(&event);
            if ex.map_or(false, |e| e <= now) {
                (false, "invalid: event expired")
            } else {
                match db.add_event(event.clone()) {
                    Ok(n) => {
                        let _ = state.broadcast_sender.send(event);
                        // if db.n_to_event.len() >= 100_000 {
                        //     let oldest = db.time.first().unwrap().1;
                        //     trace!("remove {oldest}");
                        //     db.remove_event(oldest);
                        // }
                        if let Some(e) = ex {
                            expiration = Some(Time(e, n));
                        }
                        (true, "")
                    }
                    Err(AddEventError::Duplicated) => (true, "duplicate: already have this event"),
                    Err(AddEventError::HaveNewer) => (true, "duplicate: have a newer event"),
                }
            }
        }
    };
    cs.ws
        .send(Message::Text(format!(
            r#"["OK","{id}",{accepted},"{message}"]"#,
        )))
        .await?;
    if let Some(e) = expiration {
        state.event_expiration_sender.send(e).await.unwrap();
    }
    Ok(())
}

async fn handler_404(uri: axum::http::Uri) -> Error {
    info!("handler_404: {uri}");
    Error::NotFound
}

#[tokio::main]
async fn main() -> Result<(), Error> {
    env_logger::init();
    let broadcast_sender = tokio::sync::broadcast::Sender::new(1000);
    let (event_expiration_sender, event_expiration_receiver) = tokio::sync::mpsc::channel(10);
    let mut db = Db::default();
    db.blocked_pubkeys.insert(
        PubKey::from_str("0a7c232a5c4dd0d472d34ca6e768529dffd4683e1968a236a5c789d86837a856")
            .unwrap(),
    );
    let state = Arc::new(AppState {
        db: RwLock::new(db),
        broadcast_sender,
        event_expiration_sender,
    });
    tokio::try_join!(
        listen(state.clone()),
        dead_lock_detection(),
        wait_expiration(state, event_expiration_receiver)
    )?;
    Ok(())
}

async fn dead_lock_detection() -> Result<(), Error> {
    loop {
        tokio::time::sleep(std::time::Duration::from_secs(60 * 2)).await;
        for deadlock in parking_lot::deadlock::check_deadlock() {
            if let Some(d) = deadlock.first() {
                return Err(error::Error::Internal(
                    anyhow::anyhow!(format!(
                        "found deadlock {}:\n{:?}",
                        d.thread_id(),
                        d.backtrace()
                    ))
                    .into(),
                ));
            }
        }
    }
}
