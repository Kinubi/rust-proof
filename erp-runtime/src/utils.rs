#![allow(unused)]
use log::{info, warn};

const TAG: &str = "utils";
#[repr(C)]
#[derive(Clone, Copy, Default)]
struct CoprocessorFirmwareVersion {
    major1: u32,
    minor1: u32,
    patch1: u32,
}

unsafe extern "C" {
    fn esp_hosted_get_coprocessor_fwversion(version: *mut CoprocessorFirmwareVersion) -> i32;
}

fn get_slave_firmware_version() -> Result<CoprocessorFirmwareVersion, i32> {
    let mut version = CoprocessorFirmwareVersion::default();
    let err = unsafe { esp_hosted_get_coprocessor_fwversion(&mut version) };

    if err == 0 { Ok(version) } else { Err(err) }
}

fn print_slave_firmware_version(version: CoprocessorFirmwareVersion) {
    info!(
        target: TAG,
        "Slave firmware version: {}.{}.{}",
        version.major1,
        version.minor1,
        version.patch1
    );
}
