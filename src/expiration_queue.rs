use crate::error::Error;
use crate::relay::{HashStatus, Time};
use crate::AppState;
use rocksdb::DB as Rocks;
use std::fs::create_dir_all;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::time::Instant;

pub async fn wait_expiration(
    state: Arc<AppState>,
    mut event_expiration_receiver: tokio::sync::mpsc::Receiver<Time>,
) -> Result<(), Error> {
    let far_future = far_future();
    let config_dir = &state.config_dir.join(&state.config.db_dir);
    create_dir_all(config_dir).unwrap();
    let mut opts = rocksdb::Options::default();
    opts.create_if_missing(true);
    let expiration_queue = Rocks::open(&opts, config_dir.join("expiration_queue.rocksdb")).unwrap();
    loop {
        let until = first(&expiration_queue)
            .map(|t| unix_to_instant(u64::from_be_bytes(t[0..8].try_into().unwrap())))
            .unwrap_or(far_future);
        tokio::select! {
            m = event_expiration_receiver.recv() => {
                match m {
                    Some(t) => {
                        expiration_queue.put(t.to_vec(), []).unwrap();
                    }
                    None => break,
                }
            }
            _ = tokio::time::sleep_until(until) => {
                delete_expired_events(&state, &expiration_queue);
            }
        }
    }
    let e = Error::Internal(anyhow::anyhow!("unexpected".to_string()).into());
    Err(e)
}

fn first(queue: &Rocks) -> Option<Box<[u8]>> {
    queue
        .iterator(rocksdb::IteratorMode::Start)
        .next()
        .map(|a| a.unwrap().0)
}

fn delete_expired_events(state: &AppState, queue_db: &Rocks) {
    let now = Instant::now();
    while let Some(s) = first(queue_db) {
        let t = Time::from_slice(&s);
        if unix_to_instant(t.0) > now {
            break;
        }
        queue_db.delete(s).unwrap();
        state.db.remove_event(t.1, HashStatus::Unknown);
    }
}

fn unix_to_instant(t: u64) -> Instant {
    let epoch = Instant::now() - SystemTime::now().duration_since(UNIX_EPOCH).unwrap();
    epoch + Duration::from_secs(t)
}

// copied from https://github.com/tokio-rs/tokio/blob/c8f3539bc11e57843745c68ee60ca5276248f9f9/tokio/src/time/instant.rs#L57
fn far_future() -> Instant {
    // Roughly 30 years from now.
    // API does not provide a way to obtain max `Instant`
    // or convert specific date in the future to instant.
    // 1000 years overflows on macOS, 100 years overflows on FreeBSD.
    Instant::now() + Duration::from_secs(86400 * 365 * 30)
}
