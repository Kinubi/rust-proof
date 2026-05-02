use std::{ fs, path::PathBuf, time::{ SystemTime, UNIX_EPOCH } };

use rp_node::{ blockchain::Blockchain, node_engine::NodeEngine };
use rp_runtime::{
    network::manager::NetworkManager,
    runtime::{
        errors::RuntimeError,
        manager::{
            EventTx,
            NetworkCommand,
            NodeManager,
            RuntimeEvent,
            StorageCommand,
            WakeCommand,
        },
    },
    storage::manager::{ SledStorage, StorageManager },
};
use tokio::{ sync::mpsc, time::Duration };

const EVENT_CHANNEL_CAPACITY: usize = 32;
const NETWORK_CHANNEL_CAPACITY: usize = 32;
const STORAGE_CHANNEL_CAPACITY: usize = 32;
const WAKE_CHANNEL_CAPACITY: usize = 8;
const STARTUP_HEARTBEAT_DELAY_MS: u64 = 1_000;

#[tokio::main]
async fn main() {
    let _ = env_logger::try_init();

    if let Err(error) = run().await {
        eprintln!("rp-runtime failed: {error:?}");
        std::process::exit(1);
    }
}

async fn run() -> Result<(), RuntimeError> {
    let data_dir = runtime_data_dir()?;
    fs::create_dir_all(&data_dir).map_err(RuntimeError::io_other)?;

    let blockchain = Blockchain::new().map_err(RuntimeError::from)?;
    let node_engine = NodeEngine::new(blockchain);

    let (event_tx, event_rx) = mpsc::channel::<RuntimeEvent>(EVENT_CHANNEL_CAPACITY);
    let (network_tx, network_rx) = mpsc::channel::<NetworkCommand>(NETWORK_CHANNEL_CAPACITY);
    let (storage_tx, storage_rx) = mpsc::channel::<StorageCommand>(STORAGE_CHANNEL_CAPACITY);
    let (wake_tx, wake_rx) = mpsc::channel::<WakeCommand>(WAKE_CHANNEL_CAPACITY);

    let storage = SledStorage::new(data_dir.join("sled")).map_err(RuntimeError::io_other)?;
    let mut node_manager = NodeManager::new(node_engine, event_rx, network_tx, storage_tx, wake_tx);
    let mut network_manager = NetworkManager::new(network_rx, event_tx.clone(), &data_dir)?;
    let mut storage_manager = StorageManager::new(storage, event_tx.clone(), storage_rx);

    tokio::try_join!(
        async move {
            node_manager.run().await
        },
        async move {
            network_manager.run().await
        },
        async move {
            storage_manager.run().await
        },
        run_wake_loop(event_tx, wake_rx)
    )?;

    Ok(())
}

async fn run_wake_loop(event_tx: EventTx, mut wake_rx: EventRxWake) -> Result<(), RuntimeError> {
    let mut next_tick_at_ms = Some(now_ms().saturating_add(STARTUP_HEARTBEAT_DELAY_MS));

    loop {
        if let Some(deadline_ms) = next_tick_at_ms {
            let sleep_for_ms = deadline_ms.saturating_sub(now_ms());
            tokio::select! {
                maybe_command = wake_rx.recv() => {
                    let Some(command) = maybe_command else {
                        return Ok(());
                    };
                    match command {
                        WakeCommand::Schedule { at_ms } => next_tick_at_ms = Some(at_ms),
                        WakeCommand::Cancel => next_tick_at_ms = None,
                    }
                }
                _ = tokio::time::sleep(Duration::from_millis(sleep_for_ms)) => {
                    next_tick_at_ms = None;
                    event_tx
                        .send(RuntimeEvent::Tick { now_ms: now_ms() })
                        .await
                        .map_err(RuntimeError::event_send)?;
                }
            }
        } else {
            let Some(command) = wake_rx.recv().await else {
                return Ok(());
            };
            match command {
                WakeCommand::Schedule { at_ms } => {
                    next_tick_at_ms = Some(at_ms);
                }
                WakeCommand::Cancel => {
                    next_tick_at_ms = None;
                }
            }
        }
    }
}

type EventRxWake = mpsc::Receiver<WakeCommand>;

fn runtime_data_dir() -> Result<PathBuf, RuntimeError> {
    Ok(
        PathBuf::from(
            std::env::var("RP_RUNTIME_DATA_DIR").unwrap_or_else(|_| "data/rp-runtime".to_string())
        )
    )
}

fn now_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis() as u64
}
