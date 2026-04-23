//! Toggle an LED on/off with a button
//!
//! This assumes that a LED is connected to GPIO3.
//! Additionally this assumes a button connected to GPIO35.
//! On an ESP32C3 development board this is the BOOT button.
//!
//! Depending on your target and the board you are using you should change the pins.
//! If your board doesn't have on-board LEDs don't forget to add an appropriate resistor.

use esp_idf_hal::peripherals::Peripherals;

use anyhow::Context;
use embassy_time::{ Duration, Timer }; // Using Embassy for timing
use esp_idf_hal::task::block_on;
use esp_idf_svc::eventloop::EspSystemEventLoop;
use esp_idf_svc::log::EspLogger;
use esp_idf_svc::nvs::EspDefaultNvsPartition;
use esp_idf_svc::wifi::{ AsyncWifi, ClientConfiguration, Configuration, EspWifi };
use futures::future::join;
use futures::channel::mpsc;
use erp_runtime::runtime;
use log::{ info, warn };
use erp_runtime::runtime::node::{ NetworkCommand, NodeRuntime, RuntimeEvent };
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

fn main() -> anyhow::Result<(), NodeError> {
    esp_idf_hal::sys::link_patches();
    EspLogger::initialize_default();
    let block_chain = Blockchain::new()?;
    let node_engine = NodeEngine::new(block_chain);

    let (event_tx, event_rx) = mpsc::channel::<RuntimeEvent>(32);
    let (network_tx, network_rx) = mpsc::channel::<NetworkCommand>(32);
    let mut node_runtime = NodeRuntime::new(node_engine, event_rx, network_tx);

    // let peripherals = Peripherals::take()?;
    // let sys_loop = EspSystemEventLoop::take()?;
    // let nvs = EspDefaultNvsPartition::take()?;

    // let mut wifi = AsyncWifi::wrap(
    //     EspWifi::new(peripherals.modem, sys_loop.clone(), Some(nvs))?,
    //     sys_loop,
    //     esp_idf_svc::timer::EspTimerService::new()?
    // )?;

    // if let Some(version) = get_slave_firmware_version().ok() {
    //     print_slave_firmware_version(version);
    // }

    block_on(async {
        // Run both Wifi and Main loop concurrently
        let _node_result = node_runtime.run().await?;
        Ok(())
    })
}

// async fn app_logic() -> anyhow::Result<()> {
//     loop {
//         info!(target: TAG, "Embassy timer working in the background");
//         Timer::after(Duration::from_millis(1000)).await;
//     }
// }
