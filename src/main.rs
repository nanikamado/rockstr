mod display_as_json;
mod error;
mod nostr;
mod relay;

use axum::extract::ws::{self, WebSocket};
use axum::extract::{State, WebSocketUpgrade};
use axum::response::Response;
use axum::routing::get;
use axum::Router;
use display_as_json::AsJson;
use error::Error;
use log::{debug, info};
use nostr::ClientToRelay;
use relay::{Db, QueryIter};
use std::fmt::Debug;
use std::sync::Arc;
use std::time::Duration;

const BIND_ADDRESS: &str = "127.0.0.1:8001";

#[derive(Debug)]
pub struct AppState {
    db: Db,
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
) -> Result<Response, Error> {
    info!("root");
    Ok(ws.on_upgrade(|mut ws| async {
        let _ = ws_handler(state, &mut ws).await;
        debug!("close");
        let _ = ws.close();
    }))
}

const TIMEOUT_DURATION: Duration = Duration::from_secs(60 * 3);

async fn ws_handler(state: Arc<AppState>, ws: &mut WebSocket) -> Result<(), Error> {
    let mut waiting_for_pong = false;
    loop {
        let m = tokio::time::timeout(TIMEOUT_DURATION, ws.recv()).await;
        match m {
            Ok(Some(Ok(m))) => {
                waiting_for_pong = false;
                if !handle_message(&state, ws, m).await? {
                    break;
                }
            }
            Err(e) => {
                debug!("timeout: {e}");
                if waiting_for_pong {
                    break;
                } else {
                    let _ = ws.send(ws::Message::Ping(Vec::new())).await;
                    waiting_for_pong = true;
                }
            }
            _ => break,
        }
    }
    Ok(())
}

async fn handle_message(
    state: &Arc<AppState>,
    ws: &mut WebSocket,
    m: ws::Message,
) -> Result<bool, Error> {
    use axum::extract::ws::Message;
    let c = match m {
        Message::Text(m) => {
            let Ok(m): Result<ClientToRelay, _> = serde_json::from_str(&m) else {
                return Ok(true);
            };
            match m {
                ClientToRelay::Event(e) => {
                    let id = e.id;
                    state.db.add_event(e);
                    ws.send(Message::Text(format!(r#"["OK",{},true,""]"#, AsJson(&id))))
                        .await?;
                    true
                }
                ClientToRelay::Req { id, filters } => {
                    for (_, n) in QueryIter::new(&state.db, filters) {
                        let m = {
                            let n_to_event = state.db.n_to_event.read();
                            let e = n_to_event.get(&n).unwrap();
                            Message::Text(format!(r#"["EVENT",{},{}]"#, AsJson(&id), AsJson(&e)))
                        };
                        ws.send(m).await?;
                    }
                    ws.send(Message::Text(format!(r#"["EOSE",{}]"#, AsJson(&id))))
                        .await?;
                    true
                }
                ClientToRelay::Close(_) => false,
            }
        }
        Message::Binary(_) => true,
        Message::Ping(a) => {
            ws.send(Message::Pong(a)).await?;
            true
        }
        Message::Pong(_) => true,
        Message::Close(_) => false,
    };
    Ok(c)
}

async fn handler_404(uri: axum::http::Uri) -> Error {
    info!("handler_404: {uri}");
    Error::NotFound
}

#[tokio::main]
async fn main() -> Result<(), Error> {
    env_logger::init();

    let state = Arc::new(AppState { db: Db::default() });
    listen(state).await
}
