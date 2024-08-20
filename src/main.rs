use expiration_queue::wait_expiration;
use itertools::Itertools;
use parking_lot::RwLock;
use rockstr::{expiration_queue, listen, AppState, Config, Db, Error, PluginState};
use std::env;
use std::fs::File;
use std::io::Read;
use std::path::PathBuf;
use std::sync::Arc;

#[tokio::main]
async fn main() -> Result<(), Error> {
    env_logger::init();
    let args = env::args().collect_vec();
    let config_file = args.get(1).unwrap();
    let config_file = PathBuf::from(config_file);
    let mut f = File::open(&config_file).unwrap();
    let mut contents = String::new();
    f.read_to_string(&mut contents).unwrap();
    let config: Config = toml::from_str(&contents).unwrap();

    let broadcast_sender = tokio::sync::broadcast::Sender::new(1000);
    let (event_expiration_sender, event_expiration_receiver) = tokio::sync::mpsc::channel(10);
    let config_dir = config_file.parent().unwrap().to_path_buf();
    let db = Db::new(&config_dir);
    let plugin = if config.plugin.is_empty() {
        None
    } else {
        Some(PluginState::new(config.plugin.clone()))
    };
    let state = Arc::new(AppState {
        db: RwLock::new(db),
        broadcast_sender,
        event_expiration_sender,
        config,
        config_dir,
        plugin,
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
                return Err(Error::Internal(
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
