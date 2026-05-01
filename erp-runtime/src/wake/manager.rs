use embassy_time::{Duration, Instant, Timer};
use futures::{FutureExt, SinkExt, StreamExt, pin_mut};
use rp_node::{
    contract::{Clock, Wake, WakeAt},
    errors::ContractError,
};

use crate::runtime::{
    errors::RuntimeError,
    manager::{EventTx, RuntimeEvent, WakeCommand, WakeRx},
};

const STARTUP_HEARTBEAT_DELAY_MS: u64 = 1_000;

fn startup_heartbeat_deadline_ms(now_ms: u64) -> u64 {
    now_ms.saturating_add(STARTUP_HEARTBEAT_DELAY_MS)
}

pub struct WakeManager {
    wake_at: Option<u64>,
    event_tx: EventTx,
    wake_rx: WakeRx,
}

impl WakeManager {
    pub fn new(event_tx: EventTx, wake_rx: WakeRx) -> Self {
        let now_ms = Instant::now().as_millis() as u64;
        Self {
            wake_at: Some(startup_heartbeat_deadline_ms(now_ms)),
            event_tx,
            wake_rx,
        }
    }

    pub async fn run(&mut self) -> Result<(), RuntimeError> {
        loop {
            if let Some(wake_at) = self.wake_at {
                let delay_ms = wake_at.saturating_sub(self.now_ms());
                let next_command = self.wake_rx.next().fuse();
                let wake_timer = Timer::after(Duration::from_millis(delay_ms)).fuse();
                pin_mut!(next_command, wake_timer);

                futures::select_biased! {
                    command = next_command => {
                        let Some(command) = command else {
                            break;
                        };
                        self.handle_command(command)?;
                    }
                    _ = wake_timer => {
                        self.wake_at = None;
                        self.emit_tick().await?;
                    }
                }
            } else {
                let Some(command) = self.wake_rx.next().await else {
                    break;
                };
                self.handle_command(command)?;
            }
        }

        Ok(())
    }

    fn handle_command(&mut self, command: WakeCommand) -> Result<(), ContractError> {
        match command {
            WakeCommand::Schedule { at_ms } => self.schedule_wake(WakeAt { deadline_ms: at_ms }),
            WakeCommand::Cancel => self.cancel_wake(),
        }
    }

    async fn emit_tick(&mut self) -> Result<(), RuntimeError> {
        if let Err(error) = self
            .event_tx
            .send(RuntimeEvent::Tick {
                now_ms: self.now_ms(),
            })
            .await
        {
            return Err(RuntimeError::event_send(error));
        }
        Ok(())
    }
}

impl Wake for WakeManager {
    fn schedule_wake(&mut self, at: WakeAt) -> Result<(), ContractError> {
        self.wake_at = Some(at.deadline_ms);
        Ok(())
    }
    fn cancel_wake(&mut self) -> Result<(), ContractError> {
        self.wake_at = None;
        Ok(())
    }
}

impl Clock for WakeManager {
    fn now_ms(&self) -> u64 {
        Instant::now().as_millis() as u64
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::channel::mpsc;

    #[test]
    fn startup_heartbeat_deadline_is_relative_to_now() {
        assert_eq!(startup_heartbeat_deadline_ms(5_000), 6_000);
    }

    #[test]
    fn new_initializes_an_absolute_startup_deadline() {
        let (event_tx, _) = mpsc::channel(1);
        let (_, wake_rx) = mpsc::channel(1);
        let before_now = Instant::now().as_millis() as u64;

        let manager = WakeManager::new(event_tx, wake_rx);

        let wake_at = manager
            .wake_at
            .expect("startup wake deadline should be set");
        assert!(wake_at >= before_now.saturating_add(STARTUP_HEARTBEAT_DELAY_MS));
    }
}
