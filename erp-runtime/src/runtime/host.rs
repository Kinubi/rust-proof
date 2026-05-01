use embassy_time::{ Duration, Timer };
use embedded_svc::wifi::{ AuthMethod, ClientConfiguration, Configuration };
use edge_executor::{ block_on as edge_block_on, LocalExecutor };
use esp_idf_hal::task::block_on as esp_block_on;
use esp_idf_svc::eventloop::EspSystemEventLoop;
use esp_idf_svc::hal::peripherals::Peripherals;
use esp_idf_svc::nvs::EspDefaultNvsPartition;
use esp_idf_svc::timer::EspTaskTimerService;
use esp_idf_svc::wifi::{ AsyncWifi, EspWifi };
use futures::channel::mpsc;

use log::error;
use std::thread;

use crate::identity::manager::IdentityManager;
use crate::network::manager::NetworkManager;
use crate::wake::manager::WakeManager;
use crate::runtime::errors::RuntimeError;
use crate::runtime::manager::{
    NetworkCommand,
    NodeManager,
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

const WIFI_SSID_MISSING: &str = "missing WIFI_SSID environment variable";
const WIFI_PASS_MISSING: &str = "missing WIFI_PASS environment variable";
const WIFI_SSID_INVALID: &str = "WIFI_SSID exceeds ESP-IDF client configuration limits";
const WIFI_PASS_INVALID: &str = "WIFI_PASS exceeds ESP-IDF client configuration limits";

pub fn run() -> Result<(), RuntimeError> {
    let blockchain = Blockchain::new().map_err(RuntimeError::from)?;
    let node_engine = NodeEngine::new(blockchain);

    let (event_tx, event_rx) = mpsc::channel::<RuntimeEvent>(EVENT_CHANNEL_CAPACITY);
    let (network_tx, network_rx) = mpsc::channel::<NetworkCommand>(NETWORK_CHANNEL_CAPACITY);
    let (storage_tx, storage_rx) = mpsc::channel::<StorageCommand>(STORAGE_CHANNEL_CAPACITY);
    let (wake_tx, wake_rx) = mpsc::channel::<WakeCommand>(WAKE_CHANNEL_CAPACITY);
    let identity_manager = IdentityManager::select()?;
    let nvs_partition = EspDefaultNvsPartition::take().map_err(RuntimeError::esp)?;
    let wifi = create_wifi(nvs_partition.clone())?;

    let node_runtime = NodeManager::new(node_engine, event_rx, network_tx, storage_tx, wake_tx);
    let network_manager = NetworkManager::new(
        network_rx,
        event_tx.clone(),
        identity_manager,
        wifi,
        nvs_partition.clone()
    )?;
    let nvs_storage = NvsStorage::new(nvs_partition).map_err(RuntimeError::StorageInit)?;
    let storage_manager = StorageManager::new(nvs_storage, event_tx.clone(), storage_rx);
    let wake_manager = WakeManager::new(event_tx.clone(), wake_rx);

    let runtime_handle = spawn_node_runtime(node_runtime);
    let io_handle = spawn_io_runtime(network_manager, storage_manager, wake_manager);

    runtime_handle.join().expect("node-runtime thread panicked")?;
    io_handle.join().expect("io-runtime thread panicked")?;

    Ok(())
}

fn create_wifi(
    nvs_partition: EspDefaultNvsPartition
) -> Result<AsyncWifi<EspWifi<'static>>, RuntimeError> {
    let ssid = option_env!("WIFI_SSID").ok_or(RuntimeError::config(WIFI_SSID_MISSING))?;
    let password = option_env!("WIFI_PASS").ok_or(RuntimeError::config(WIFI_PASS_MISSING))?;

    let peripherals = Peripherals::take().map_err(RuntimeError::esp)?;
    let sys_loop = EspSystemEventLoop::take().map_err(RuntimeError::esp)?;
    let timer_service = EspTaskTimerService::new().map_err(RuntimeError::esp)?;

    let mut wifi = AsyncWifi::wrap(
        EspWifi::new(peripherals.modem, sys_loop.clone(), Some(nvs_partition)).map_err(
            RuntimeError::esp
        )?,
        sys_loop,
        timer_service
    ).map_err(RuntimeError::esp)?;

    let configuration = Configuration::Client(ClientConfiguration {
        ssid: ssid.try_into().map_err(|_| RuntimeError::config(WIFI_SSID_INVALID))?,
        password: password.try_into().map_err(|_| RuntimeError::config(WIFI_PASS_INVALID))?,
        auth_method: AuthMethod::WPA2Personal,
        ..Default::default()
    });

    wifi.set_configuration(&configuration).map_err(RuntimeError::esp)?;

    Ok(wifi)
}

fn spawn_node_runtime(node_runtime: NodeManager) -> thread::JoinHandle<Result<(), RuntimeError>> {
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
