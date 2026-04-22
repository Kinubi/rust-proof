use esp_idf_hal::timer::{TimerDriver, config::TimerConfig};
use fugit::{Duration, Instant};

type TickInstant = Instant<u64, 1, 1_000_000>;
pub type TickDuration = Duration<u64, 1, 1_000_000>;

pub struct Timer<'a> {
    end_time: TickInstant,
    ticker: &'a Ticker<'a>,
}

impl<'a> Timer<'a> {
    pub fn new(duration: TickDuration, ticker: &'a Ticker<'a>) -> Self {
        Self {
            end_time: ticker.now() + duration,
            ticker,
        }
    }

    pub fn is_ready(&self) -> bool {
        self.ticker.now() >= self.end_time
    }
}

pub struct Ticker<'a> {
    timer: TimerDriver<'a>,
}

impl<'a> Ticker<'a> {
    pub fn new(config: &'a TimerConfig) -> Self {
        let timer = TimerDriver::new(config).unwrap();
        timer.enable().unwrap();
        timer.start().unwrap();
        Self { timer }
    }

    pub fn now(&self) -> TickInstant {
        TickInstant::from_ticks(self.timer.get_raw_count().unwrap())
    }
}
