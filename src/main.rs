mod display_as_json;
mod error;
mod nostr;
mod priority_queue;
mod relay;

use crate::nostr::Filter;
use axum::extract::ws::{self, WebSocket};
use axum::extract::{State, WebSocketUpgrade};
use axum::http::header::UPGRADE;
use axum::http::HeaderMap;
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::Router;
use display_as_json::AsJson;
use error::Error;
use log::{debug, info, trace};
use nostr::{ClientToRelay, Event};
use parking_lot::RwLock;
use relay::{AddEventError, Db, GetEvents, Time};
use rustc_hash::FxHashMap;
use smallvec::SmallVec;
use std::fmt::Debug;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const BIND_ADDRESS: &str = "127.0.0.1:8017";

#[derive(Debug)]
pub struct AppState {
    db: RwLock<Db>,
    broadcast_sender: tokio::sync::broadcast::Sender<Arc<Event>>,
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

pub async fn root(
    State(state): State<Arc<AppState>>,
    ws: WebSocketUpgrade,
    headers: HeaderMap,
) -> Result<Response, Error> {
    info!("root");
    if headers.contains_key(UPGRADE) {
        Ok(ws
            .on_failed_upgrade(|a| {
                info!("on_failed_upgrade: {a}");
            })
            .on_upgrade(|ws| async {
                let receiver = state.broadcast_sender.subscribe();
                let mut cs = ConnectionState {
                    ws,
                    broadcast_receiver: receiver,
                    req: FxHashMap::default(),
                };
                let _ = ws_handler(state, &mut cs).await;
                let _ = cs.ws.close().await;
            }))
    } else {
        Ok("rockstr relay".into_response())
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
}

async fn ws_handler(state: Arc<AppState>, cs: &mut ConnectionState) -> Result<CloseReason, Error> {
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

async fn handle_message(
    state: &Arc<AppState>,
    cs: &mut ConnectionState,
    m: ws::Message,
) -> Result<Option<CloseReason>, Error> {
    use axum::extract::ws::Message;
    let continue_ = match m {
        Message::Text(s) => {
            let Ok(m): Result<ClientToRelay, _> = serde_json::from_str(&s) else {
                return Ok(None);
            };
            match m {
                ClientToRelay::Event(e) => {
                    let id = e.id;
                    if s.len() > 100_000 {
                        cs.ws
                            .send(Message::Text(format!(
                                r#"["OK","{id}",false,"invalid: too large event"]"#,
                            )))
                            .await?;
                    } else if !e.verify_hash() {
                        cs.ws
                            .send(Message::Text(format!(
                                r#"["OK","{id}",false,"invalid: bad event id"]"#,
                            )))
                            .await?;
                    } else if !e.verify_sig() {
                        cs.ws
                            .send(Message::Text(format!(
                                r#"["OK","{id}",false,"invalid: bad signature"]"#,
                            )))
                            .await?;
                    } else if state.db.read().deleted.contains(&id) {
                        cs.ws
                            .send(Message::Text(format!(
                                r#"["OK","{id}",false,"deleted: user requested deletion"]"#,
                            )))
                            .await?;
                    } else if (20_000..30_000).contains(&e.kind) {
                        let _ = state.broadcast_sender.send(e);
                        cs.ws
                            .send(Message::Text(format!(r#"["OK","{id}",true,""]"#,)))
                            .await?;
                    } else if e.created_at
                        > SystemTime::now()
                            .duration_since(UNIX_EPOCH)
                            .unwrap()
                            .as_secs()
                            + 600
                    {
                        cs.ws
                            .send(Message::Text(format!(
                                r#"["OK","{id}",false,"invalid: created_at too early"]"#,
                            )))
                            .await?;
                    } else {
                        let r = state.db.write().add_event(e.clone());
                        match r {
                            Ok(_) => {
                                let _ = state.broadcast_sender.send(e);
                                cs.ws
                                    .send(Message::Text(format!(r#"["OK","{id}",true,""]"#,)))
                                    .await?;
                                let mut db = state.db.write();
                                if db.n_to_event.len() >= 100_000 {
                                    let oldest = db.time.first().unwrap().1;
                                    trace!("remove {oldest}");
                                    db.remove_event(oldest);
                                }
                            }
                            Err(AddEventError::Duplicated) => {
                                cs.ws
                                .send(Message::Text(format!(
                                    r#"["OK","{id}",true,"duplicate: already have this event"]"#,
                                )))
                                .await?;
                            }
                            Err(AddEventError::HaveNewer) => {
                                cs.ws
                                    .send(Message::Text(format!(
                                        r#"["OK","{id}",true,"duplicate: have newer event"]"#,
                                    )))
                                    .await?;
                            }
                        }
                    }
                    None
                }
                ClientToRelay::Req { id, filters } => {
                    debug!("req with {s}");
                    for f in &filters {
                        if let Some(mut s) = GetEvents::new(f) {
                            for _ in 0..f.limit {
                                let m = {
                                    let db = state.db.read();
                                    let Some(Time(t, n)) = s.next(&db) else {
                                        break;
                                    };
                                    if t < f.since {
                                        break;
                                    }
                                    let e =
                                        db.n_to_event.get(&n).unwrap_or_else(|| panic!("n = {n}"));
                                    if filters.iter().all(|f| !f.matches(e)) {
                                        log::error!("weird");
                                    }
                                    Message::Text(event_message(&id, e))
                                };
                                cs.ws.send(m).await?;
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

async fn handler_404(uri: axum::http::Uri) -> Error {
    info!("handler_404: {uri}");
    Error::NotFound
}

#[tokio::main]
async fn main() -> Result<(), Error> {
    env_logger::init();
    let broadcast_sender = tokio::sync::broadcast::Sender::new(1000);
    let state = Arc::new(AppState {
        db: Default::default(),
        broadcast_sender,
    });
    tokio::try_join!(listen(state), dead_lock_detection())?;
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
