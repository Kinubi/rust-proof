use embassy_time::{ Duration, Instant, Timer };
use futures::{ SinkExt, StreamExt };
use rp_node::{ contract::{ Wake, WakeAt }, errors::ContractError };

use crate::runtime::{ errors::RuntimeError, node::{ EventTx, RuntimeEvent, WakeCommand, WakeRx } };

pub struct WakeManager {
    wake_at: u64,
    event_tx: EventTx,
    wake_rx: WakeRx,
}

impl WakeManager {
    pub fn new(wake_at: u64, event_tx: EventTx, wake_rx: WakeRx) -> Self {
        Self { wake_at, event_tx, wake_rx }
    }

    pub async fn run(&mut self) -> Result<(), RuntimeError> {
        if let Ok(()) = self.wake_task().await {
            while let Some(command) = self.wake_rx.next().await {
                match command {
                    WakeCommand::Schedule { at_ms } => {
                        let _ = self.cancel_wake();
                        let _ = self
                            .schedule_wake(WakeAt { deadline_ms: at_ms })
                            .map_err(RuntimeError::Contract);
                        let _ = self.wake_task().await;
                    }
                }
            }
        }
        Ok(())
    }

    fn now_ms(&self) -> u64 {
        Instant::now().as_millis() as u64
    }

    async fn wake_task(&mut self) -> Result<(), RuntimeError> {
        let delay_ms = self.wake_at.saturating_sub(self.now_ms());
        Timer::after(Duration::from_millis(delay_ms)).await;

        if
            let Err(error) = self.event_tx.send(RuntimeEvent::Tick {
                now_ms: self.now_ms(),
            }).await
        {
            return Err(RuntimeError::event_send(error));
        }
        Ok(())
    }
}

impl Wake for WakeManager {
    fn schedule_wake(&mut self, at: WakeAt) -> Result<(), ContractError> {
        self.wake_at = at.deadline_ms;
        Ok(())
    }
    fn cancel_wake(&mut self) -> Result<(), ContractError> {
        self.wake_at = 0;
        Ok(())
    }
}
