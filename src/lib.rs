mod display_as_json;
mod error;
pub mod expiration_queue;
pub mod nostr;
mod priority_queue;
mod relay;

use axum::extract::ws::rejection::WebSocketUpgradeRejection;
use axum::extract::ws::{self, WebSocket};
use axum::extract::{State, WebSocketUpgrade};
use axum::http::HeaderMap;
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::Router;
use display_as_json::AsJson;
pub use error::Error;
use itertools::Itertools;
use log::{debug, info, warn};
use nostr::{ClientToRelay, Event, Filter, FilterCompact, FirstTagValue, PubKey, Tag};
use parking_lot::RwLock;
use rand::RngCore;
pub use relay::{AddEventError, Db, GetEvents, GetEventsStopped, Time};
use rustc_hash::{FxHashMap, FxHashSet};
use serde::Deserialize;
use serde_json::json;
use smallvec::SmallVec;
use std::env;
use std::fmt::Debug;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

#[derive(Debug, Deserialize)]
pub struct Config {
    bind_address: String,
    #[serde(default)]
    banned_pubkeys: FxHashSet<PubKey>,
    relay_name_for_auth: String,
    admin_pubkey: Option<PubKey>,
    #[serde(default = "max_message_length_default")]
    max_message_length: usize,
    #[serde(default = "created_at_upper_limit_default")]
    created_at_upper_limit: u64,
    #[serde(default = "relay_description_default")]
    relay_description: String,
    #[serde(default = "relay_name_default")]
    relay_name: String,
    #[serde(default)]
    db_dir: String,
}

fn max_message_length_default() -> usize {
    0xFFFF
}

fn created_at_upper_limit_default() -> u64 {
    600
}

fn relay_description_default() -> String {
    "A rockstr instance".to_string()
}

fn relay_name_default() -> String {
    "rockstr default".to_string()
}

#[derive(Debug)]
pub struct AppState {
    pub db: RwLock<Db>,
    pub broadcast_sender: tokio::sync::broadcast::Sender<Arc<Event>>,
    pub event_expiration_sender: tokio::sync::mpsc::Sender<Time>,
    pub config: Config,
    pub config_dir: PathBuf,
}

pub async fn listen(state: Arc<AppState>) -> Result<(), Error> {
    let bind_address = state.config.bind_address.clone();
    info!("Listening on {bind_address}");
    let app = Router::new()
        .route("/", get(root))
        .fallback(handler_404)
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(&bind_address).await.unwrap();
    Ok(axum::serve(listener, app).await?)
}

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
                    authed_pubkey: None,
                };
                let a = ws_handler(state, &mut cs).await;
                debug!("ws close: {a:?}");
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
                let mut r = json!({
                    "description": state.config.relay_description,
                    "name": state.config.relay_name,
                    "software": "git+https://github.com/nanikamado/rockstr.git",
                    "supported_nips": [1, 9, 11, 40],
                    "version": env!("CARGO_PKG_VERSION"),
                    "limitation": {
                        "max_message_length": state.config.max_message_length,
                        "created_at_upper_limit": state.config.created_at_upper_limit,
                    },
                })
                .to_string()
                .into_response();
                r.headers_mut().insert(
                    axum::http::header::ACCESS_CONTROL_ALLOW_ORIGIN,
                    axum::http::header::HeaderValue::from_static("*"),
                );
                r
            } else {
                state.config.relay_description.clone().into_response()
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
    authed_pubkey: Option<PubKey>,
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
                    Ok(Some(Err(e))) => {
                        debug!("ws error: {e}");
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

fn important_tags(e: &Event) -> (Option<u64>, bool) {
    let mut expiration = None;
    let mut protected = false;
    for Tag(k, v) in &e.tags {
        if k == "expiration" {
            if let Some((FirstTagValue::String(t), _)) = &v {
                if let Ok(n) = t.parse::<u64>() {
                    expiration = Some(n);
                }
            }
        } else if k == "-" {
            protected = true;
        }
    }
    (expiration, protected)
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
                        handle_event(state, cs, e, s.len(), now, false).await?;
                        None
                    }
                    ClientToRelay::Auth(e) => {
                        handle_event(state, cs, e, s.len(), now, true).await?;
                        None
                    }
                    ClientToRelay::Req { id, filters } => {
                        debug!("req with {s}");
                        'filters_loop: for f in &filters {
                            let f = {
                                let mut db = state.db.write();
                                FilterCompact::new(f, &mut db)
                            };
                            let mut limit = f.limit;
                            if let Some(ids) = f.ids {
                                let es = {
                                    let db = state.db.read();
                                    ids.into_iter()
                                        .map(|id| db.n_to_event_get(id).unwrap())
                                        .sorted_by_key(|e| (e.created_at, e.id))
                                        .take(limit as usize)
                                };
                                for e in es {
                                    let m = Message::Text(event_message(&id, &e));
                                    cs.ws.send(m).await?;
                                }
                            } else {
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
                                                let Some(s) = GetEvents::new(&f, &db) else {
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
                                            let Some(Time(t, n)) = s.next(&db) else {
                                                break St::End;
                                            };
                                            if t < f.since {
                                                break St::End;
                                            }
                                            let e = db.n_to_event_get(n).unwrap();
                                            let m = Message::Text(event_message(&id, &e));
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
    is_auth: bool,
) -> Result<(), Error> {
    use axum::extract::ws::Message;
    let id = event.id;
    let mut expiration = None;
    let (accepted, message) = if event_len > state.config.max_message_length {
        (false, "invalid: too large event")
    } else {
        let mut db = state.db.write();
        if state.config.banned_pubkeys.contains(&event.pubkey) {
            (false, "blocked")
        } else if !event.verify_hash() {
            (false, "invalid: bad event id")
        } else if !event.verify_sig() {
            (false, "invalid: bad signature")
        } else if event.created_at > now + state.config.created_at_upper_limit {
            (false, "invalid: created_at too early")
        } else if is_auth {
            if event.kind == 22_242
                && verify_auth(&state.config.relay_name_for_auth, cs, &event, now)
            {
                cs.authed_pubkey = Some(event.pubkey);
                (true, "")
            } else {
                (false, "invalid: bad auth")
            }
        } else if (20_000..30_000).contains(&event.kind) {
            let _ = state.broadcast_sender.send(event);
            (true, "")
        } else {
            let (ex, protected) = important_tags(&event);
            if ex.map_or(false, |e| e <= now) {
                (false, "invalid: event expired")
            } else if protected
                && cs.authed_pubkey != Some(event.pubkey)
                && cs.authed_pubkey != state.config.admin_pubkey
            {
                if cs.authed_pubkey.is_some() {
                    (false, "restricted: event marked as protected")
                } else {
                    (false, "auth-required: event marked as protected")
                }
            } else {
                match db.add_event(event.clone()) {
                    Ok(n) => {
                        let _ = state.broadcast_sender.send(event);
                        if let Some(e) = ex {
                            expiration = Some(Time(e, n));
                        }
                        (true, "")
                    }
                    Err(AddEventError::HaveNewer) => (true, "duplicate: have a newer event"),
                    Err(AddEventError::Duplicated) => (true, "duplicate: already have this event"),
                    Err(AddEventError::Deleted) => (false, "deleted: user requested deletion"),
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

fn verify_auth(relay_name_for_auth: &str, cs: &ConnectionState, event: &Event, now: u64) -> bool {
    let mut relay = false;
    let mut challenge = false;
    for Tag(k, v) in &event.tags {
        if let Some((FirstTagValue::String(v), _)) = v {
            match k.as_str() {
                "relay" => {
                    relay = v.contains(relay_name_for_auth);
                }
                "challenge" => {
                    challenge = *v == cs.challenge;
                }
                _ => (),
            }
        }
    }
    relay && challenge && event.created_at.abs_diff(now) < 600
}

async fn handler_404(uri: axum::http::Uri) -> Error {
    info!("handler_404: {uri}");
    Error::NotFound
}
