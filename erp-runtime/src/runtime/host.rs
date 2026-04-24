use embassy_time::{ Duration, Timer };
use edge_executor::{ block_on as edge_block_on, LocalExecutor };
use esp_idf_hal::task::block_on as esp_block_on;
use futures::channel::mpsc;

use log::error;
use std::thread;

use crate::network::manager::NetworkManager;
use crate::wake::manager::WakeManager;
use crate::runtime::errors::RuntimeError;
use crate::runtime::node::{
    NetworkCommand,
    NodeRuntime,
    RuntimeEvent,
    StorageCommand,
    WakeCommand,
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
    let blockchain = Blockchain::new().map_err(RuntimeError::from)?;
    let node_engine = NodeEngine::new(blockchain);

    let (event_tx, event_rx) = mpsc::channel::<RuntimeEvent>(EVENT_CHANNEL_CAPACITY);
    let (network_tx, network_rx) = mpsc::channel::<NetworkCommand>(NETWORK_CHANNEL_CAPACITY);
    let (storage_tx, storage_rx) = mpsc::channel::<StorageCommand>(STORAGE_CHANNEL_CAPACITY);
    let (wake_tx, wake_rx) = mpsc::channel::<WakeCommand>(WAKE_CHANNEL_CAPACITY);

    let node_runtime = NodeRuntime::new(node_engine, event_rx, network_tx, storage_tx, wake_tx);
    let network_manager = NetworkManager::new(network_rx, event_tx.clone(), [0; 32]);
    let nvs_storage = NvsStorage::new().map_err(RuntimeError::StorageInit)?;
    let storage_manager = StorageManager::new(nvs_storage, event_tx.clone(), storage_rx);
    let wake_manager = WakeManager::new(event_tx.clone(), wake_rx);

    let runtime_handle = spawn_node_runtime(node_runtime);
    let io_handle = spawn_io_runtime(network_manager, storage_manager, wake_manager);

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
    network_manager: NetworkManager,
    storage_manager: StorageManager,
    wake_manager: WakeManager
) -> thread::JoinHandle<Result<(), RuntimeError>> {
    thread::Builder
        ::new()
        .name("io-runtime".into())
        .stack_size(IO_RUNTIME_STACK_SIZE)
        .spawn(move || run_io_runtime(network_manager, storage_manager, wake_manager))
        .expect("failed to spawn io-runtime thread")
}

fn run_io_runtime(
    network_manager: NetworkManager,
    storage_manager: StorageManager,
    wake_manager: WakeManager
) -> Result<(), RuntimeError> {
    let io_executor: LocalExecutor = Default::default();

    edge_block_on(
        io_executor.run(async {
            let network_worker = io_executor.spawn(async move {
                Timer::after(Duration::from_millis(NETWORK_STARTUP_DELAY_MS)).await;
                let mut network_manager = network_manager;
                let result = network_manager.run().await;
                if let Err(error) = &result {
                    error!(target: "host", "network worker failed: {:?}", error);
                }
                result
            });
            let storage_worker = io_executor.spawn(async move {
                let mut storage_manager = storage_manager;
                let result = storage_manager.run().await;
                if let Err(error) = &result {
                    error!(target: "host", "storage worker failed: {:?}", error);
                }
                result
            });
            let wake_worker = io_executor.spawn(async move {
                let mut wake_manager = wake_manager;
                let result = wake_manager.run().await;
                if let Err(error) = &result {
                    error!(target: "host", "wake worker failed: {:?}", error);
                }
                result
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
