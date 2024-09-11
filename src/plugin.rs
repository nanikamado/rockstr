use crate::SourceInfo;
use cached::{Cached, TimedCache};
use lnostr::{Event, EventId};
use log::error;
use serde::Deserialize;
use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{ChildStdin, Command};
use tokio::sync::{mpsc, oneshot};

#[derive(Debug)]
pub struct PluginInput {
    event: Arc<Event>,
    source_info: Arc<SourceInfo>,
    sender: oneshot::Sender<PluginResponse>,
}

#[derive(Debug)]
pub struct PluginState {
    sender: mpsc::Sender<PluginInput>,
}

type PluginResponse = Result<(), String>;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
enum Action {
    Accept,
    Reject,
}

#[derive(Debug, Deserialize)]
struct Res {
    id: EventId,
    action: Action,
    msg: String,
}

impl PluginState {
    pub fn new(command: String) -> Self {
        let (sender, mut receiver) = mpsc::channel(100);
        fn recv_plugin_response(
            m: String,
            es: &mut TimedCache<EventId, oneshot::Sender<PluginResponse>>,
        ) {
            eprintln!("got {m}");
            if let Ok(m) = serde_json::from_str::<Res>(&m) {
                if let Some(sender) = es.cache_remove(&m.id) {
                    match m.action {
                        Action::Accept => {
                            sender.send(Ok(())).unwrap();
                        }
                        Action::Reject => {
                            sender.send(Err(m.msg)).unwrap();
                        }
                    }
                }
            }
        }
        async fn recv_event(
            input: PluginInput,
            stdin: &mut ChildStdin,
            es: &mut TimedCache<EventId, oneshot::Sender<PluginResponse>>,
        ) {
            es.cache_set(input.event.id, input.sender);
            stdin
                .write_all(
                    format!(
                        "{{\"event\":{},\"sourceInfo\":{},\"userAgent\":{}}}\n",
                        serde_json::to_string(&input.event).unwrap(),
                        serde_json::to_string(&input.source_info.addr).unwrap(),
                        serde_json::to_string(&input.source_info.user_agent).unwrap()
                    )
                    .as_bytes(),
                )
                .await
                .unwrap();
        }
        tokio::spawn(async move {
            'outer: loop {
                eprintln!("starting command {command}");
                let Ok(mut child) = Command::new(&command)
                    .stdout(Stdio::piped())
                    .stdin(Stdio::piped())
                    .spawn()
                else {
                    error!("could not start the plugin command");
                    return;
                };
                let mut stdin = child.stdin.take().unwrap();
                let stdout = child.stdout.take().unwrap();
                let mut ls = BufReader::new(stdout).lines();
                let mut es = TimedCache::with_lifespan(600);
                loop {
                    tokio::select! {
                        m = ls.next_line() => {
                            if let Some(m) = m.unwrap() {
                                recv_plugin_response(m, &mut es);
                            } else {
                                break;
                            }
                        }
                        Some(m) = receiver.recv() => {
                            recv_event(m, &mut stdin, &mut es).await;
                        }
                        else => {
                            // dropped
                            break 'outer;
                        },
                    }
                }
            }
        });
        PluginState { sender }
    }

    pub async fn check_event(
        &self,
        event: Arc<Event>,
        source_info: Arc<SourceInfo>,
    ) -> Result<PluginResponse, ()> {
        let (sender, r) = oneshot::channel();
        if self
            .sender
            .send(PluginInput {
                event,
                source_info,
                sender,
            })
            .await
            .is_ok()
        {
            tokio::time::timeout(Duration::from_secs(600), r)
                .await
                .map_err(|_| ())?
                .map_err(|_| ())
        } else {
            Err(())
        }
    }
}
