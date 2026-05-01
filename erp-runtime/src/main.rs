use erp_runtime::runtime::errors::RuntimeError;
use esp_idf_svc::log::EspLogger;

fn main() -> Result<(), RuntimeError> {
    esp_idf_hal::sys::link_patches();
    EspLogger::initialize_default();
    erp_runtime::runtime::host::run()
}
