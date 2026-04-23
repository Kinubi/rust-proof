use embassy_time::{ Duration, Instant, Timer };
use edge_executor::{ block_on as edge_block_on, LocalExecutor };
use esp_idf_hal::task::block_on as esp_block_on;
use futures::channel::mpsc;
use futures::{ SinkExt, StreamExt };
use std::thread;

use crate::network::manager::NetworkManager;
use crate::runtime::errors::RuntimeError;
use crate::runtime::node::{
    EventTx,
    NetworkCommand,
    NodeRuntime,
    RuntimeEvent,
    StorageCommand,
    WakeCommand,
    WakeRx,
};
use crate::storage::manager::StorageManager;
use crate::storage::nvs_storage::{ NvsStorage };
use rp_node::blockchain::Blockchain;
use rp_node::node_engine::NodeEngine;

const EVENT_CHANNEL_CAPACITY: usize = 32;
const NETWORK_CHANNEL_CAPACITY: usize = 32;
const STORAGE_CHANNEL_CAPACITY: usize = 32;
const WAKE_CHANNEL_CAPACITY: usize = 8;

const NODE_RUNTIME_STACK_SIZE: usize = 64 * 1024;
const IO_RUNTIME_STACK_SIZE: usize = 48 * 1024;
const NETWORK_STARTUP_DELAY_MS: u64 = 500;

pub fn run() -> Result<(), RuntimeError> {
    let blockchain = Blockchain::new().map_err(RuntimeError::NodeError)?;
    let node_engine = NodeEngine::new(blockchain);

    let (event_tx, event_rx) = mpsc::channel::<RuntimeEvent>(EVENT_CHANNEL_CAPACITY);
    let (network_tx, network_rx) = mpsc::channel::<NetworkCommand>(NETWORK_CHANNEL_CAPACITY);
    let (storage_tx, storage_rx) = mpsc::channel::<StorageCommand>(STORAGE_CHANNEL_CAPACITY);
    let (mut wake_tx, wake_rx) = mpsc::channel::<WakeCommand>(WAKE_CHANNEL_CAPACITY);

    let node_runtime = NodeRuntime::new(
        node_engine,
        event_rx,
        network_tx,
        storage_tx,
        wake_tx.clone()
    );
    let network_manager = NetworkManager::new(network_rx, event_tx.clone(), [0; 32]);
    let nvs_storage = NvsStorage::new().unwrap();
    let storage_manager = StorageManager::new(nvs_storage, event_tx.clone(), storage_rx);

    let runtime_handle = spawn_node_runtime(node_runtime);
    let io_handle = spawn_io_runtime(event_tx.clone(), network_manager, storage_manager, wake_rx);

    esp_block_on(async {
        wake_tx
            .send(WakeCommand::Schedule { at_ms: 10_000 }).await
            .map_err(RuntimeError::WakeChannelSendError)
    })?;
    drop(wake_tx);
    drop(event_tx);

    runtime_handle.join().expect("node-runtime thread panicked")?;
    io_handle.join().expect("io-runtime thread panicked")?;

    Ok(())
}

fn spawn_node_runtime(node_runtime: NodeRuntime) -> thread::JoinHandle<Result<(), RuntimeError>> {
    thread::Builder
        ::new()
        .name("node-runtime".into())
        .stack_size(NODE_RUNTIME_STACK_SIZE)
        .spawn(move ||
            esp_block_on(async move {
                let mut node_runtime = node_runtime;
                node_runtime.run().await
            })
        )
        .expect("failed to spawn node-runtime thread")
}

fn spawn_io_runtime(
    event_tx: EventTx,
    network_manager: NetworkManager,
    storage_manager: StorageManager,
    wake_rx: WakeRx
) -> thread::JoinHandle<Result<(), RuntimeError>> {
    thread::Builder
        ::new()
        .name("io-runtime".into())
        .stack_size(IO_RUNTIME_STACK_SIZE)
        .spawn(move || run_io_runtime(event_tx, network_manager, storage_manager, wake_rx))
        .expect("failed to spawn io-runtime thread")
}

fn run_io_runtime(
    event_tx: EventTx,
    network_manager: NetworkManager,
    storage_manager: StorageManager,
    wake_rx: WakeRx
) -> Result<(), RuntimeError> {
    let io_executor: LocalExecutor = Default::default();

    edge_block_on(
        io_executor.run(async {
            let wake_event_tx = event_tx.clone();

            let network_worker = io_executor.spawn(async move {
                Timer::after(Duration::from_millis(NETWORK_STARTUP_DELAY_MS)).await;
                let mut network_manager = network_manager;
                network_manager.run().await.map_err(RuntimeError::NetworkError)
            });
            let storage_worker = io_executor.spawn(async move {
                let mut storage_manager = storage_manager;
                storage_manager.run().await.map_err(RuntimeError::StorageError)
            });
            let wake_worker = io_executor.spawn(async move {
                run_wake_task(wake_event_tx, wake_rx).await
            });

            let (network_res, storage_res, wake_res) = futures::join!(
                network_worker,
                storage_worker,
                wake_worker
            );

            network_res?;
            storage_res?;
            wake_res?;

            Ok(())
        })
    )
}

fn now_ms() -> u64 {
    Instant::now().as_millis() as u64
}

async fn run_wake_task(mut event_tx: EventTx, mut wake_rx: WakeRx) -> Result<(), RuntimeError> {
    while let Some(WakeCommand::Schedule { at_ms }) = wake_rx.next().await {
        let delay_ms = at_ms.saturating_sub(now_ms());
        Timer::after(Duration::from_millis(delay_ms)).await;

        if let Err(error) = event_tx.send(RuntimeEvent::Tick { now_ms: now_ms() }).await {
            return Err(RuntimeError::EventChannelSendError(error));
        }
    }

    Ok(())
}
