mod display_as_json;
mod error;
pub mod expiration_queue;
pub mod nostr;
mod plugin;
mod priority_queue;
mod relay;
mod utils;

use axum::extract::ws::rejection::WebSocketUpgradeRejection;
use axum::extract::ws::{self, CloseFrame, WebSocket};
use axum::extract::{State, WebSocketUpgrade};
use axum::http::header::USER_AGENT;
use axum::http::HeaderMap;
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::Router;
use bitcoin_hashes::{sha256, Hash};
use display_as_json::AsJson;
pub use error::Error;
use futures_util::sink::SinkExt;
use hex_conservative::DisplayHex;
use itertools::Itertools;
use lnostr::{kinds, EventId};
use log::{debug, info, warn};
use nostr::{ClientMessage, Event, Filter, FilterCompact, FirstTagValue, PubKey, Tag};
pub use plugin::PluginState;
use rand::RngCore;
pub use relay::{AddEventError, Db, GetEvents, GetEventsStopped, Time};
use rustc_hash::{FxHashMap, FxHashSet};
use serde::Deserialize;
use serde_json::json;
use smallvec::SmallVec;
use std::borrow::Cow;
use std::env;
use std::fmt::Debug;
use std::io::Write;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::sync::mpsc;
use tokio::time::Instant;
use tokio_util::sync::PollSender;
use utils::{DisplayIfSome, LineLimit};

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
    #[serde(default)]
    pub plugin: String,
    #[serde(default)]
    block_note_without_profile: bool,
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
    pub db: Db,
    pub broadcast_sender: tokio::sync::broadcast::Sender<Arc<Event>>,
    pub event_expiration_sender: tokio::sync::mpsc::Sender<Time>,
    pub config: Config,
    pub config_dir: PathBuf,
    pub plugin: Option<PluginState>,
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
    let addr = headers
        .get("X-Forwarded-For")
        .and_then(|a| a.to_str().ok())
        .map(|a| a.to_string())
        .unwrap_or_default();
    info!("root: {addr}");
    match ws {
        Ok(ws) => ws
            .on_failed_upgrade(|a| {
                info!("on_failed_upgrade: {a}");
            })
            .on_upgrade(move |ws| async move {
                let receiver = state.broadcast_sender.subscribe();
                let mut challenge = [0u8; 16];
                rand::thread_rng().fill_bytes(&mut challenge);
                let mut cs = ConnectionState {
                    ws,
                    broadcast_receiver: receiver,
                    req: FxHashMap::default(),
                    challenge: challenge.to_lower_hex_string(),
                    authed_pubkey: None,
                    credit: DEFAULT_CREDIT,
                    source_info: SourceInfo {
                        addr,
                        user_agent: headers
                            .get(USER_AGENT)
                            .and_then(|a| a.to_str().ok())
                            .map(|a| a.to_string())
                            .unwrap_or_default(),
                    }
                    .into(),
                    req_count: 0,
                    accept_rumors: false,
                    publish_rumors: false,
                };
                let a = ws_handler(state, &mut cs).await;
                debug!("ws close: {a:?} ({:?})", cs.source_info);
            }),
        Err(e) => {
            use WebSocketUpgradeRejection::*;
            if !matches!(e, InvalidConnectionHeader(_) | InvalidUpgradeHeader(_)) {
                e.into_response()
            } else if headers
                .get("accept")
                .and_then(|a| a.to_str().ok())
                .is_some_and(|a| a.contains("application/nostr+json"))
            {
                let mut r = json!({
                    "description": state.config.relay_description,
                    "name": state.config.relay_name,
                    "software": "git+https://github.com/nanikamado/rockstr.git",
                    "supported_nips": [1, 9, 11, 17, 40, 59],
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

// cloudflare's timeout is 100s, so the timeout should be less than 100s
const TIMEOUT_DURATION: Duration = Duration::from_secs(60);

#[derive(Debug)]
enum CloseReason {
    WsClosed,
    NoResponse,
    MaliciousConnection,
}

const DEFAULT_CREDIT: u32 = 30;

#[derive(Debug)]
pub struct SourceInfo {
    addr: String,
    user_agent: String,
}

pub struct ReqState {
    filters: SmallVec<[Filter; 2]>,
    req_handler: tokio::task::JoinHandle<()>,
}

struct ConnectionState {
    ws: WebSocket,
    broadcast_receiver: tokio::sync::broadcast::Receiver<Arc<Event>>,
    req: FxHashMap<String, ReqState>,
    challenge: String,
    authed_pubkey: Option<PubKey>,
    credit: u32,
    source_info: Arc<SourceInfo>,
    req_count: u64,
    /// if the client accepts rumors
    accept_rumors: bool,
    /// if the client publish rumors
    publish_rumors: bool,
}

async fn ws_handler(state: Arc<AppState>, cs: &mut ConnectionState) -> Result<CloseReason, Error> {
    cs.ws
        .send(ws::Message::Text(format!(r#"["AUTH","{}"]"#, cs.challenge)))
        .await?;
    let mut waiting_for_pong = false;
    let timeout_init = || Instant::now() + TIMEOUT_DURATION;
    let mut timeout = timeout_init();
    let (relay_message_sender, mut relay_message_receiver) = mpsc::channel(100);
    let r = loop {
        tokio::select! {
            m = cs.ws.recv() => {
                timeout = timeout_init();
                match m {
                    Some(Ok(m)) => {
                        waiting_for_pong = false;
                        if let Some(r) = handle_message(&state, cs, &relay_message_sender, m).await? {
                            break r;
                        }
                    }
                    Some(Err(e)) => {
                        debug!("ws error: {e}");
                    }
                    _ => break CloseReason::WsClosed,
                }
            }
            m = relay_message_receiver.recv() => {
                if let Some(m) = m {
                    let _ = cs.ws.send(m).await;
                }
            }
            _ = tokio::time::sleep_until(timeout) => {
                debug!("timeout");
                if waiting_for_pong {
                    break CloseReason::NoResponse;
                } else {
                    timeout = timeout_init();
                    let _ = cs.ws.send(ws::Message::Ping(Vec::new())).await;
                    waiting_for_pong = true;
                }
            }
            e = cs.broadcast_receiver.recv() => {
                receive_broadcast(cs, e).await
            },
        }
    };
    let reason = match r {
        CloseReason::WsClosed => "unexpected",
        CloseReason::NoResponse => {
            "Closing connection because your client did not respond to our pings"
        }
        CloseReason::MaliciousConnection => "Closing connection because your client is buggy",
    };
    let _ = cs
        .ws
        .send(ws::Message::Text(format!(r#"["NOTICE","{reason}"]"#)))
        .await;
    let _ = cs
        .ws
        .send(ws::Message::Close(Some(CloseFrame {
            reason: reason.into(),
            code: 1000,
        })))
        .await;
    Ok(r)
}

fn is_addressed_to(event: &Event, to: &PubKey) -> bool {
    event.tags.iter().any(|a| {
        if let lnostr::Tag(t, Some((FirstTagValue::Hex32(p), _))) = a {
            t == "p" && &to.to_bytes() == p
        } else {
            false
        }
    })
}

async fn send_event<T: futures_util::Sink<axum::extract::ws::Message> + Unpin>(
    ws: &mut T,
    authed_pubkey: &Option<PubKey>,
    accept_rumors: bool,
    req_id: &str,
    event: &Event,
) -> Result<(), <T as futures_util::Sink<axum::extract::ws::Message>>::Error> {
    use axum::extract::ws::Message;
    // To protect recipient metadata, relays SHOULD guard access to `kind 1059` events based on user AUTH
    // https://github.com/nostr-protocol/nips/blob/3f11c00fb93f118f207130344032710e34de4710/59.md?plain=1#L93
    if event.kind == kinds::GIFT_WRAP && authed_pubkey.is_none_or(|p| !is_addressed_to(event, &p)) {
        return Ok(());
    }
    if event.sig.is_none() && !accept_rumors {
        return Ok(());
    }
    let m = Message::Text(format!(
        r#"["EVENT",{},{}]"#,
        AsJson(&req_id),
        AsJson(event)
    ));
    Ok(ws.send(m).await?)
}

async fn receive_broadcast(
    cs: &mut ConnectionState,
    e: Result<Arc<Event>, tokio::sync::broadcast::error::RecvError>,
) {
    match e {
        Ok(e) => {
            for (req_id, rs) in &cs.req {
                if rs.filters.iter().any(|f| f.matches(&e)) {
                    let _ = send_event(&mut cs.ws, &cs.authed_pubkey, cs.accept_rumors, req_id, &e)
                        .await;
                }
            }
        }
        Err(e) => {
            log::error!("receive error: {e}")
        }
    }
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

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

async fn handle_message(
    state: &Arc<AppState>,
    cs: &mut ConnectionState,
    relay_message_sender: &mpsc::Sender<ws::Message>,
    m: ws::Message,
) -> Result<Option<CloseReason>, Error> {
    use axum::extract::ws::Message;
    let continue_ = match m {
        Message::Text(s) => match serde_json::from_str(&s) {
            Ok(m) => match m {
                ClientMessage::Event(e) => {
                    handle_event(state, cs, e, s.len(), now_unix(), false).await?;
                    if cs.credit == 0 {
                        Some(CloseReason::MaliciousConnection)
                    } else {
                        None
                    }
                }
                ClientMessage::InvalidEvent { id } => {
                    cs.ws
                        .send(Message::Text(format!(
                            r#"["OK","{id}",false,"could not parse event"]"#,
                        )))
                        .await?;
                    None
                }
                ClientMessage::Auth(e) => {
                    handle_event(state, cs, e, s.len(), now_unix(), true).await?;
                    None
                }
                ClientMessage::Req {
                    id: req_id,
                    filters,
                } => {
                    debug!(
                        "[{}] req with {}{}",
                        cs.req_count,
                        LineLimit(&s),
                        if cs.req_count > 100
                            && !(cs.authed_pubkey.is_some()
                                && cs.authed_pubkey == state.config.admin_pubkey)
                        {
                            let mut h = sha256::HashEngine::default();
                            h.write_all(s.as_bytes()).unwrap();
                            DisplayIfSome::Some(format!(
                                " {:x} {:?}",
                                sha256::Hash::from_engine(h),
                                cs.source_info
                            ))
                        } else {
                            DisplayIfSome::None
                        }
                    );
                    cs.req_count += 1;
                    let relay_message_sender = PollSender::new(relay_message_sender.clone());
                    let state = state.clone();
                    let authed_pubkey = cs.authed_pubkey;
                    let accept_rumors = cs.accept_rumors;
                    let req_id_clone = req_id.clone();
                    let filters_clone = filters.clone();
                    let req_handler = tokio::spawn(async move {
                        let _ = handle_req(
                            &state,
                            &filters_clone,
                            relay_message_sender,
                            &authed_pubkey,
                            accept_rumors,
                            req_id_clone,
                        )
                        .await;
                    });
                    let prev = cs.req.insert(
                        req_id,
                        ReqState {
                            filters,
                            req_handler,
                        },
                    );
                    if let Some(prev) = prev {
                        prev.req_handler.abort();
                    }
                    None
                }
                ClientMessage::Close(id) => {
                    debug!("close {id}");
                    if let Some(task) = cs.req.remove(id.as_ref()) {
                        task.req_handler.abort();
                    }
                    None
                }
                ClientMessage::AcceptRumors(accept_rumors) => {
                    cs.accept_rumors = accept_rumors;
                    None
                }
            },
            Err(e) => {
                warn!("parse error: {e}, text = {s:?}");
                cs.ws
                    .send(Message::Text(format!(
                        r#"["NOTICE",{}]"#,
                        serde_json::to_string(&format!("could not parse the message: {}", s))
                            .unwrap()
                    )))
                    .await?;
                None
            }
        },
        Message::Binary(_) => None,
        Message::Ping(a) => {
            cs.ws.send(Message::Pong(a)).await?;
            None
        }
        Message::Pong(_) => None,
        Message::Close(_) => None,
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
    fn check_sig(cs: &mut ConnectionState, event: &Event) -> Result<(), &'static str> {
        if event.sig.is_some() {
            if event.verify_sig() {
                Ok(())
            } else {
                Err("invalid: bad signature")
            }
        } else if let Some(a) = cs.authed_pubkey {
            if a == event.pubkey {
                Ok(())
            } else {
                Err("invalid: authenticate with the same pubkey as the pubkey of the rumor")
            }
        } else {
            Err("invalid: auth is required to publish a rumor")
        }
    }
    let (accepted, message): (_, Cow<str>) = if event_len > state.config.max_message_length {
        (false, "invalid: too large event".into())
    } else if state.config.banned_pubkeys.contains(&event.pubkey) {
        cs.credit = 0;
        (false, "blocked".into())
    } else if !event.verify_hash() {
        (false, "invalid: bad event id".into())
    } else if let Err(e) = check_sig(cs, &event) {
        (false, e.into())
    } else if event.created_at > now + state.config.created_at_upper_limit {
        (false, "invalid: created_at too early".into())
    } else if is_auth {
        if event.kind != kinds::CLIENT_AUTHENTICATION {
            (
                false,
                format!("invalid: auth of kind {} is not supported", event.kind).into(),
            )
        } else if !verify_auth(
            &state.config.relay_name_for_auth,
            cs,
            &event,
            &state.config.admin_pubkey,
            now,
        ) {
            (false, "invalid: bad auth".into())
        } else {
            cs.authed_pubkey = Some(event.pubkey);
            cs.publish_rumors = event.tags.iter().any(|t| {
                if let Tag(t, None) = t {
                    t == "publish_rumors"
                } else {
                    false
                }
            });
            (true, "".into())
        }
    } else {
        let r = if cs.authed_pubkey == state.config.admin_pubkey || leading_zeros(event.id) >= 25 {
            Ok(())
        } else if state.config.block_note_without_profile
            && [1, 3].contains(&event.kind)
            && !state.db.have_kind_0(event.pubkey)
        {
            info!(
                "blocked: {}\t{}\t{}",
                cs.source_info.addr,
                cs.source_info.user_agent,
                serde_json::to_string(&event).unwrap()
            );
            Err(format!(
                "blocked: publish your kind 0 event to this relay before publishing kind {}",
                event.kind
            )
            .into())
        } else if let Some(p) = &state.plugin {
            p.check_event(event.clone(), cs.source_info.clone())
                .await
                .map(|a| a.map_err(Cow::Owned))
                .unwrap_or_else(|_| Err("error: internal server error".into()))
        } else {
            Ok(())
        };
        if let Err(e) = r {
            cs.credit = cs.credit.saturating_sub(DEFAULT_CREDIT / 2);
            (false, e)
        } else if (20_000..30_000).contains(&event.kind) {
            let _ = state.broadcast_sender.send(event);
            (true, "".into())
        } else {
            let (ex, protected) = important_tags(&event);
            if ex.is_some_and(|e| e <= now) {
                (false, "invalid: event expired".into())
            } else if protected
                && cs.authed_pubkey != Some(event.pubkey)
                && cs.authed_pubkey != state.config.admin_pubkey
            {
                if cs.authed_pubkey.is_some() {
                    (false, "restricted: event marked as protected".into())
                } else {
                    (false, "auth-required: event marked as protected".into())
                }
            } else {
                match state.db.add_event(event.clone()) {
                    Ok(n) => {
                        let _ = state.broadcast_sender.send(event);
                        if let Some(e) = ex {
                            expiration = Some(Time(e, n));
                        }
                        (true, "".into())
                    }
                    Err(AddEventError::HaveNewer) => (true, "duplicate: have a newer event".into()),
                    Err(AddEventError::Duplicated) => {
                        (true, "duplicate: already have this event".into())
                    }
                    Err(AddEventError::Deleted) => {
                        (false, "deleted: user requested deletion".into())
                    }
                }
            }
        }
    };
    if accepted {
        cs.credit = DEFAULT_CREDIT;
    }
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

async fn handle_req(
    state: &Arc<AppState>,
    filters: &SmallVec<[Filter; 2]>,
    mut relay_message_sender: PollSender<axum::extract::ws::Message>,
    authed_pubkey: &Option<PubKey>,
    accept_rumors: bool,
    req_id: String,
) -> Result<(), Error> {
    use axum::extract::ws::Message;
    'filters_loop: for f in filters {
        let f = FilterCompact::new(f, &state.db);
        let mut limit = f.limit;
        if let Some(ids) = f.ids {
            let es = {
                ids.into_iter()
                    .filter_map(|id| state.db.n_to_event_get(id))
                    .sorted_by_key(|e| (e.created_at, e.id))
                    .take(limit as usize)
            };
            for e in es {
                send_event(
                    &mut relay_message_sender,
                    &authed_pubkey,
                    accept_rumors,
                    &req_id,
                    &e,
                )
                .await?;
            }
        } else {
            enum St {
                Init,
                Middle(GetEventsStopped),
                End,
            }
            let mut continuation = St::Init;
            loop {
                let mut es = Vec::with_capacity(100);
                continuation = {
                    let db = &state.db;
                    let mut s = match continuation {
                        St::Init => {
                            let Some(s) = GetEvents::new(&f, db) else {
                                continue 'filters_loop;
                            };
                            s
                        }
                        St::Middle(s) => s.restart(db),
                        St::End => panic!(),
                    };
                    loop {
                        if limit == 0 {
                            break St::End;
                        }
                        let Some(Time(t, n)) = s.next(db) else {
                            break St::End;
                        };
                        if t < f.since {
                            break St::End;
                        }
                        let Some(e) = db.n_to_event_get(n) else {
                            continue;
                        };
                        limit -= 1;
                        es.push(e);
                        if es.len() >= 100 {
                            break St::Middle(s.stop());
                        }
                    }
                };
                for e in es {
                    send_event(
                        &mut relay_message_sender,
                        authed_pubkey,
                        accept_rumors,
                        &req_id,
                        &e,
                    )
                    .await?;
                }
                if matches!(continuation, St::End) {
                    continue 'filters_loop;
                }
            }
        }
    }
    relay_message_sender
        .send(Message::Text(format!(r#"["EOSE",{}]"#, AsJson(&req_id))))
        .await?;
    Ok(())
}

pub fn leading_zeros(s: EventId) -> u32 {
    let mut zeros = 0;
    for &n in s.as_byte_array() {
        zeros += n.leading_zeros();
        if n != 0 {
            break;
        }
    }
    zeros
}

fn verify_auth(
    relay_name_for_auth: &str,
    cs: &ConnectionState,
    event: &Event,
    admin_pubkey: &Option<PubKey>,
    now: u64,
) -> bool {
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
    (relay || &Some(event.pubkey) == admin_pubkey)
        && challenge
        && event.created_at.abs_diff(now) < 600
}

async fn handler_404(uri: axum::http::Uri) -> Error {
    info!("handler_404: {uri}");
    Error::NotFound
}

#[cfg(test)]
mod tests {
    use crate::leading_zeros;
    use lnostr::EventId;
    use std::str::FromStr;

    #[test]
    fn lz() {
        assert_eq!(
            leading_zeros(
                EventId::from_str(
                    "0002158589aa52e00f5b332a1871e89aacbb414ef1a4f95753583c48e501114e"
                )
                .unwrap()
            ),
            14
        );
        assert_eq!(
            leading_zeros(
                EventId::from_str(
                    "00000affbc006bf45bdde66e92897d73f740a37eb5f94d65a029f76bf4ad4702"
                )
                .unwrap()
            ),
            20
        );
        assert_eq!(
            leading_zeros(
                EventId::from_str(
                    "000000000e9d97a1ab09fc381030b346cdd7a142ad57e6df0b46dc9bef6c7e2d"
                )
                .unwrap()
            ),
            36
        );
    }
}
