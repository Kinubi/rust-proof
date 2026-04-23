//! Toggle an LED on/off with a button
//!
//! This assumes that a LED is connected to GPIO3.
//! Additionally this assumes a button connected to GPIO35.
//! On an ESP32C3 development board this is the BOOT button.
//!
//! Depending on your target and the board you are using you should change the pins.
//! If your board doesn't have on-board LEDs don't forget to add an appropriate resistor.

use erp_runtime::network::manager::NetworkManager;
use erp_runtime::runtime::errors::RuntimeError;
use esp_idf_hal::peripherals::Peripherals;

use anyhow::{ Context, Error };
use embassy_time::{ Duration, Timer, Instant }; // Using Embassy for timing
use esp_idf_hal::task::block_on;
use esp_idf_svc::eventloop::EspSystemEventLoop;
use esp_idf_svc::log::EspLogger;
use esp_idf_svc::nvs::EspDefaultNvsPartition;
use esp_idf_svc::wifi::{ AsyncWifi, ClientConfiguration, Configuration, EspWifi };
use futures::future::{ join };
use futures::channel::mpsc;
use futures::{ SinkExt, StreamExt };

use erp_runtime::runtime::node::{
    EventTx,
    NetworkCommand,
    NodeRuntime,
    RuntimeEvent,
    StorageCommand,
    WakeCommand,
    WakeRx,
};
use rp_node::blockchain::{ Blockchain };
use rp_node::errors::NodeError;
use rp_node::node_engine::NodeEngine;

const TAG: &str = "erp";

// async fn connect_wifi(wifi: &mut AsyncWifi<EspWifi<'_>>) -> anyhow::Result<()> {
//     let ssid = option_env!("WIFI_SSID").unwrap_or("");
//     let password = option_env!("WIFI_PASSWORD").unwrap_or("");

//     anyhow::ensure!(
//         !ssid.is_empty(),
//         "Missing WIFI_SSID. Set it when building, e.g. WIFI_SSID=... WIFI_PASSWORD=... cargo run"
//     );

//     wifi.set_configuration(
//         &Configuration::Client(ClientConfiguration {
//             ssid: ssid.try_into().map_err(|_| anyhow::anyhow!("WIFI_SSID is too long"))?,
//             password: password
//                 .try_into()
//                 .map_err(|_| anyhow::anyhow!("WIFI_PASSWORD is too long"))?,
//             ..Default::default()
//         })
//     )?;

//     wifi.start().await.context("failed to start Wi-Fi")?;

//     for _ in 0..=5 {
//         match wifi.connect().await {
//             Ok(_) => {
//                 wifi.wait_netif_up().await.context("Wi-Fi connected but netif did not come up")?;
//                 info!(target: TAG, "Wi-Fi connected");
//                 return Ok(());
//             }
//             Err(error) => {
//                 warn!(target: TAG, "Wi-Fi error: {error:?}. Retrying...");
//                 Timer::after(Duration::from_secs(5)).await;
//             }
//         }
//     }

//     Err(anyhow::anyhow!("Wi-Fi init failed"))
// }

fn main() -> Result<(), RuntimeError> {
    esp_idf_hal::sys::link_patches();
    EspLogger::initialize_default();

    let block_chain = Blockchain::new().map_err(RuntimeError::NodeError)?;
    let node_engine = NodeEngine::new(block_chain);

    let (event_tx, event_rx) = mpsc::channel::<RuntimeEvent>(32);
    let (network_tx, network_rx) = mpsc::channel::<NetworkCommand>(32);
    let (storage_tx, _storage_rx) = mpsc::channel::<StorageCommand>(32);
    let (mut wake_tx, wake_rx) = mpsc::channel::<WakeCommand>(8);

    let mut node_runtime = NodeRuntime::new(
        node_engine,
        event_rx,
        network_tx,
        storage_tx,
        wake_tx.clone()
    );

    let mut network_manager = NetworkManager::new(network_rx, event_tx.clone(), [0; 32]);

    block_on(async {
        let runtime_fut = node_runtime.run();
        let wake_fut = wake_task(event_tx.clone(), wake_rx);
        let network_fut = network_manager.run();

        wake_tx.send(WakeCommand::Schedule { at_ms: 10000 }).await;

        let (runtime_res, wake_res, network_res) = futures::join!(
            runtime_fut,
            wake_fut,
            network_fut
        );

        runtime_res?;
        wake_res?;
        let _ = network_res.map_err(RuntimeError::NetworkError);
        Ok(())
    })
}

// async fn app_logic() -> anyhow::Result<()> {
//     loop {
//         info!(target: TAG, "Embassy timer working in the background");
//         Timer::after(Duration::from_millis(1000)).await;
//     }
// }

fn now_ms() -> u64 {
    Instant::now().as_millis() as u64
}

async fn wake_task(mut event_tx: EventTx, mut wake_rx: WakeRx) -> Result<(), RuntimeError> {
    while let Some(WakeCommand::Schedule { at_ms }) = wake_rx.next().await {
        let delay_ms = at_ms.saturating_sub(now_ms());
        Timer::after(Duration::from_millis(delay_ms)).await;

        if let Err(error) = event_tx.send(RuntimeEvent::Tick { now_ms: now_ms() }).await {
            return Err(RuntimeError::EventChannelSendError(error));
        }
    }
    Ok(())
}
