use std::collections::HashSet;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use serde::Serialize;
use tauri::{AppHandle, Emitter};

#[derive(Clone, Serialize)]
pub struct DeviceChangePayload {
    pub input_devices: Vec<String>,
    pub output_devices: Vec<String>,
}

fn enumerate_devices(direction: &wasapi::Direction) -> HashSet<String> {
    let collection = match wasapi::DeviceCollection::new(direction) {
        Ok(c) => c,
        Err(_) => return HashSet::new(),
    };
    let mut set = HashSet::new();
    let count = collection.get_nbr_devices().unwrap_or(0);
    for i in 0..count {
        if let Ok(device) = collection.get_device_at_index(i) {
            if let Ok(name) = device.get_friendlyname() {
                set.insert(name);
            }
        }
    }
    set
}

pub fn start_device_watcher(app_handle: AppHandle, interval_secs: u64) {
    let stop_flag = Arc::new(AtomicBool::new(false));
    let stop_flag_clone = stop_flag.clone();

    // 存储 stop_flag 到 app handle 的 managed state，以便后续停止（可选）
    std::thread::Builder::new()
        .name("device-watcher".into())
        .spawn(move || {
            let _ = wasapi::initialize_mta();

            let mut prev_input = enumerate_devices(&wasapi::Direction::Capture);
            let mut prev_output = enumerate_devices(&wasapi::Direction::Render);

            loop {
                std::thread::sleep(Duration::from_secs(interval_secs));

                if stop_flag_clone.load(Ordering::Relaxed) {
                    break;
                }

                let cur_input = enumerate_devices(&wasapi::Direction::Capture);
                let cur_output = enumerate_devices(&wasapi::Direction::Render);

                if cur_input != prev_input || cur_output != prev_output {
                    prev_input = cur_input;
                    prev_output = cur_output;

                    let input_devices: Vec<String> = enumerate_devices_list(&wasapi::Direction::Capture);
                    let output_devices: Vec<String> = enumerate_devices_list(&wasapi::Direction::Render);

                    let _ = app_handle.emit("devices-changed", DeviceChangePayload {
                        input_devices,
                        output_devices,
                    });
                }
            }
        })
        .expect("failed to spawn device watcher thread");
}

fn enumerate_devices_list(direction: &wasapi::Direction) -> Vec<String> {
    let collection = match wasapi::DeviceCollection::new(direction) {
        Ok(c) => c,
        Err(_) => return Vec::new(),
    };
    let mut devices = Vec::new();
    let count = collection.get_nbr_devices().unwrap_or(0);
    for i in 0..count {
        if let Ok(device) = collection.get_device_at_index(i) {
            if let Ok(name) = device.get_friendlyname() {
                devices.push(name);
            }
        }
    }
    devices
}
