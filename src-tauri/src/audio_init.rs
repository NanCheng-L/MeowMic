/// 音频设备初始化模块
///
/// 负责 WASAPI 输入/输出/监听设备的初始化和配置。

use crate::debug::debug_log;
use crate::device::find_device;
use wasapi::*;

/// 音频设备资源（输入 + 输出）
pub struct AudioDevices {
    pub input_client: AudioClient,
    pub input_capture: AudioCaptureClient,
    pub input_handle: wasapi::Handle,
    pub input_format: WaveFormat,
    pub input_device_id: String,
    pub output_client: AudioClient,
    pub output_render: AudioRenderClient,
    pub output_handle: wasapi::Handle,
    pub output_format: WaveFormat,
}

/// 监听设备状态
pub struct MonitorState {
    pub client: Option<AudioClient>,
    pub render: Option<AudioRenderClient>,
    pub event: Option<wasapi::Handle>,
    pub sample_rate: u32,
    pub buffer: Vec<u8>,
    pub current_device_id: String,
    pub was_streaming: bool,
}

/// 初始化输入/输出 WASAPI 设备，返回格式信息和客户端句柄
pub fn init_audio_devices(
    input_device_name: Option<&str>,
    output_device_name: Option<&str>,
) -> Result<AudioDevices, String> {
    let input_device = find_device(input_device_name, true)
        .map_err(|e| format!("Failed to find input device: {}", e))?;
    let output_device = find_device(output_device_name, false)
        .map_err(|e| format!("Failed to find output device: {}", e))?;

    let input_friendly = input_device.get_friendlyname().unwrap_or_else(|_| "unknown".into());
    let output_friendly = output_device.get_friendlyname().unwrap_or_else(|_| "unknown".into());
    debug_log(&format!("Input device: '{}'", input_friendly));
    debug_log(&format!("Output device: '{}'", output_friendly));
    log::info!("Input device: '{}'", input_friendly);
    log::info!("Output device: '{}'", output_friendly);

    let input_device_id = input_device.get_id().unwrap_or_default();

    // 获取设备原生格式，避免 WASAPI 内部重采样增加延迟
    let fallback_input_format = WaveFormat::new(16, 16, &SampleType::Int, 48000, 1, None);
    let fallback_output_format = WaveFormat::new(16, 16, &SampleType::Int, 48000, 2, None);

    let mut input_client = input_device
        .get_iaudioclient()
        .map_err(|e| format!("Failed to get input client: {}", e))?;

    let input_format = input_client.get_mixformat().unwrap_or_else(|_| {
        log::warn!("Failed to get input mixformat, using fallback 48kHz");
        fallback_input_format
    });
    log::info!(
        "Input device format: {}Hz, {}ch, {}bit, {:?}",
        input_format.get_samplespersec(),
        input_format.get_nchannels(),
        input_format.get_bitspersample(),
        input_format.get_subformat().unwrap_or(SampleType::Int)
    );

    let (def_time, _min_time) = input_client
        .get_periods()
        .map_err(|e| format!("Failed to get input periods: {}", e))?;

    input_client
        .initialize_client(
            &input_format,
            def_time,
            &Direction::Capture,
            &ShareMode::Shared,
            true,
        )
        .map_err(|e| format!("Failed to initialize input client: {}", e))?;

    let input_handle = input_client
        .set_get_eventhandle()
        .map_err(|e| format!("Failed to set input event handle: {}", e))?;

    let mut output_client = output_device
        .get_iaudioclient()
        .map_err(|e| format!("Failed to get output client: {}", e))?;

    let output_format = output_client.get_mixformat().unwrap_or_else(|_| {
        log::warn!("Failed to get output mixformat, using fallback 48kHz");
        fallback_output_format
    });
    log::info!(
        "Output device format: {}Hz, {}ch, {}bit, {:?}",
        output_format.get_samplespersec(),
        output_format.get_nchannels(),
        output_format.get_bitspersample(),
        output_format.get_subformat().unwrap_or(SampleType::Int)
    );

    let (def_time, _min_time) = output_client
        .get_periods()
        .map_err(|e| format!("Failed to get output periods: {}", e))?;

    output_client
        .initialize_client(
            &output_format,
            def_time,
            &Direction::Render,
            &ShareMode::Shared,
            true,
        )
        .map_err(|e| format!("Failed to initialize output client: {}", e))?;
    log::info!("Output client initialized on '{}'", output_friendly);

    let output_handle = output_client
        .set_get_eventhandle()
        .map_err(|e| format!("Failed to set output event handle: {}", e))?;

    let input_capture = input_client
        .get_audiocaptureclient()
        .map_err(|e| format!("Failed to get input capture client: {}", e))?;

    let output_render = output_client
        .get_audiorenderclient()
        .map_err(|e| format!("Failed to get output render client: {}", e))?;

    Ok(AudioDevices {
        input_client,
        input_capture,
        input_handle,
        input_format,
        input_device_id,
        output_client,
        output_render,
        output_handle,
        output_format,
    })
}

/// 初始化监听客户端（系统默认输出设备）
pub fn init_monitor(
    input_device_id: &str,
    output_sample_rate: u32,
    frame_size: usize,
) -> MonitorState {
    let monitor_max_frames = frame_size * (output_sample_rate as usize / 48000 + 1);
    let mut state = MonitorState {
        client: None,
        render: None,
        event: None,
        sample_rate: output_sample_rate,
        buffer: vec![0u8; monitor_max_frames * 2 * 2], // stereo i16
        current_device_id: String::new(),
        was_streaming: false,
    };

    debug_log(&format!(
        "Monitor: looking for default output device... input_device_id={}",
        input_device_id
    ));
    match find_device(None, false) {
        Ok(monitor_output) => {
            let monitor_output_name = monitor_output.get_friendlyname().unwrap_or_default();
            debug_log(&format!("Monitor: default output = '{}'", monitor_output_name));

            let device_id = monitor_output.get_id().unwrap_or_default();
            let device_name = monitor_output.get_friendlyname().unwrap_or_default();

            // 检查同 USB 设备冲突
            let extract_usb_id = |id: &str| -> String {
                if let Some(start) = id.find("vid_") {
                    let rest = &id[start..];
                    if let Some(end) = rest.find('&') {
                        if let Some(pid_start) = rest.find("pid_") {
                            let pid_rest = &rest[pid_start..];
                            if let Some(pid_end) = pid_rest.find(|c: char| c == '&' || c == '#') {
                                return rest[..end + pid_end].to_string();
                            }
                        }
                    }
                }
                String::new()
            };
            let monitor_usb = extract_usb_id(&device_id);
            let input_usb = extract_usb_id(input_device_id);
            debug_log(&format!(
                "Monitor USB check: monitor='{}' input='{}'",
                monitor_usb, input_usb
            ));
            if !monitor_usb.is_empty() && monitor_usb == input_usb {
                log::warn!(
                    "Monitor skipped: device '{}' shares USB ID with input",
                    device_name
                );
                debug_log(&format!(
                    "Monitor: SKIPPED - '{}' same USB device as input",
                    device_name
                ));
                return state;
            }

            if let Ok(mut m_client) = monitor_output.get_iaudioclient() {
                let device_sample_rate = m_client
                    .get_mixformat()
                    .map(|f| f.get_samplespersec())
                    .unwrap_or(48000);
                let monitor_format =
                    WaveFormat::new(16, 16, &SampleType::Int, device_sample_rate as usize, 2, None);
                let (def_time, _) = m_client.get_periods().unwrap_or((0, 0));
                debug_log(&format!(
                    "Monitor: initializing client on '{}' ({}Hz, format={:?})",
                    device_name, device_sample_rate, monitor_format
                ));
                if m_client
                    .initialize_client(
                        &monitor_format,
                        def_time,
                        &Direction::Render,
                        &ShareMode::Shared,
                        true,
                    )
                    .is_ok()
                {
                    if let Ok(render) = m_client.get_audiorenderclient() {
                        let evt = m_client.set_get_eventhandle();
                        log::info!(
                            "Monitor client ready on '{}' (format: {}bit, {}Hz)",
                            device_name,
                            monitor_format.get_bitspersample(),
                            device_sample_rate
                        );
                        debug_log(&format!(
                            "Monitor: READY on '{}' ({}Hz)",
                            device_name, device_sample_rate
                        ));
                        state.client = Some(m_client);
                        state.render = Some(render);
                        state.event = evt.ok();
                        state.current_device_id = device_id;
                        state.sample_rate = device_sample_rate;
                        // 按监听设备实际采样率重新分配 buffer（可能与 output_sample_rate 不同）
                        let monitor_max_frames = frame_size * (device_sample_rate as usize / 48000 + 1);
                        state.buffer = vec![0u8; monitor_max_frames * 2 * 2];
                    } else {
                        debug_log(&format!("Monitor: failed to get render client on '{}'", device_name));
                    }
                } else {
                    debug_log(&format!("Monitor: failed to initialize client on '{}'", device_name));
                }
            } else {
                debug_log(&format!("Monitor: failed to get AudioClient on '{}'", device_name));
            }
        }
        Err(e) => {
            debug_log(&format!(
                "Monitor: failed to find default output device: {}",
                e
            ));
        }
    }

    state
}

/// WASAPI 流启动预热：等待设备真正就绪
pub fn warmup_streams(
    input_client: &AudioClient,
    input_handle: &wasapi::Handle,
    input_capture: &AudioCaptureClient,
    input_bytes_per_frame: usize,
    frame_size: usize,
) {
    debug_log("Starting warmup...");
    let warmup_buf_size = frame_size * input_bytes_per_frame;
    let mut warmup_buf = vec![0u8; warmup_buf_size];
    let max_retries = 3;
    let mut warmup_ok = false;

    for attempt in 0..max_retries {
        debug_log(&format!("Warmup attempt {}/{}", attempt + 1, max_retries));
        let warmup_start = std::time::Instant::now();
        let mut warmup_frames = 0;
        let mut got_signal = false;

        while warmup_start.elapsed().as_millis() < 300 {
            if input_handle.wait_for_event(50).is_ok() {
                if let Ok((frames_read, _)) = input_capture.read_from_device(&mut warmup_buf) {
                    if frames_read > 0 {
                        warmup_frames += 1;
                        let bytes_read = frames_read as usize * input_bytes_per_frame;
                        if warmup_buf[..bytes_read].iter().any(|&b| b != 0) {
                            got_signal = true;
                            break;
                        }
                    }
                }
            }
        }

        if got_signal {
            debug_log(&format!(
                "Warmup OK: {} frames, {}ms",
                warmup_frames,
                warmup_start.elapsed().as_millis()
            ));
            warmup_ok = true;
            break;
        }

        debug_log(&format!(
            "Warmup attempt {} failed, restarting streams...",
            attempt + 1
        ));
        let _ = input_client.stop_stream();
        std::thread::sleep(std::time::Duration::from_millis(100));
        let _ = input_client.start_stream();
    }

    if !warmup_ok {
        debug_log("All warmup attempts failed!");
    }
    debug_log("Entering main audio loop");
}
