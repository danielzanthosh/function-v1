//! Audio capture and recording service using cpal and hound.
//!
//! Provides cross-platform microphone recording, downsampling to 16kHz mono,
//! and WAV encoding for speech-to-text processing.

use crate::PlatformError;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{SampleFormat, Stream};
use hound::{SampleFormat as HoundSampleFormat, WavSpec, WavWriter};
use std::io::Cursor;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

#[cfg(target_os = "macos")]
pub mod macos_mic {
    use std::ffi::c_void;
    use std::sync::mpsc::channel;
    use std::time::Duration;

    #[link(name = "AVFoundation", kind = "framework")]
    #[link(name = "Foundation", kind = "framework")]
    extern "C" {
        fn objc_getClass(name: *const i8) -> *mut c_void;
        fn sel_registerName(name: *const i8) -> *mut c_void;
        fn objc_msgSend();
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum MacMicPermission {
        Authorized,
        Denied,
        Restricted,
        NotDetermined,
        Unavailable,
    }

    #[repr(C)]
    struct BlockDescriptor {
        reserved: u64,
        size: u64,
    }

    #[repr(C)]
    struct BlockLiteral {
        isa: *const c_void,
        flags: i32,
        reserved: i32,
        invoke: extern "C" fn(*mut BlockLiteral, bool),
        descriptor: *const BlockDescriptor,
    }

    static BLOCK_DESCRIPTOR: BlockDescriptor = BlockDescriptor {
        reserved: 0,
        size: std::mem::size_of::<BlockLiteral>() as u64,
    };

    extern "C" {
        static _NSConcreteStackBlock: c_void;
    }

    pub fn check_mic_permission() -> MacMicPermission {
        unsafe {
            let cls = objc_getClass(b"AVCaptureDevice\0".as_ptr() as *const i8);
            if cls.is_null() {
                return MacMicPermission::Unavailable;
            }
            let sel_status = sel_registerName(b"authorizationStatusForMediaType:\0".as_ptr() as *const i8);
            let sel_str = sel_registerName(b"stringWithUTF8String:\0".as_ptr() as *const i8);
            let ns_string_cls = objc_getClass(b"NSString\0".as_ptr() as *const i8);
            if ns_string_cls.is_null() {
                return MacMicPermission::Unavailable;
            }

            let msg_send_str: unsafe extern "C" fn(*mut c_void, *mut c_void, *const i8) -> *mut c_void =
                std::mem::transmute(objc_msgSend as *const ());
            let media_type = msg_send_str(ns_string_cls, sel_str, b"soun\0".as_ptr() as *const i8);

            let msg_send_status: unsafe extern "C" fn(*mut c_void, *mut c_void, *mut c_void) -> i64 =
                std::mem::transmute(objc_msgSend as *const ());
            let status = msg_send_status(cls, sel_status, media_type);

            match status {
                3 => MacMicPermission::Authorized,
                2 => MacMicPermission::Denied,
                1 => MacMicPermission::Restricted,
                0 => MacMicPermission::NotDetermined,
                _ => MacMicPermission::Unavailable,
            }
        }
    }

    pub fn ensure_mic_permission() -> MacMicPermission {
        let status = check_mic_permission();
        tracing::info!("macOS microphone permission state: {:?}", status);

        if status != MacMicPermission::NotDetermined {
            return status;
        }

        tracing::info!("Requesting macOS microphone permission...");
        let (tx, rx) = channel::<bool>();
        let tx_ptr = Box::into_raw(Box::new(tx));

        extern "C" fn invoke_block(block: *mut BlockLiteral, granted: bool) {
            unsafe {
                let tx_ptr = (block as *mut c_void).offset(std::mem::size_of::<BlockLiteral>() as isize)
                    as *mut std::sync::mpsc::Sender<bool>;
                let tx = Box::from_raw(tx_ptr);
                let _ = tx.send(granted);
            }
        }

        #[repr(C)]
        struct BlockWithTx {
            literal: BlockLiteral,
            tx_ptr: *mut std::sync::mpsc::Sender<bool>,
        }

        let mut block = BlockWithTx {
            literal: BlockLiteral {
                isa: unsafe { &_NSConcreteStackBlock as *const c_void },
                flags: 1 << 30,
                reserved: 0,
                invoke: invoke_block,
                descriptor: &BLOCK_DESCRIPTOR,
            },
            tx_ptr,
        };

        unsafe {
            let cls = objc_getClass(b"AVCaptureDevice\0".as_ptr() as *const i8);
            let sel_req = sel_registerName(
                b"requestAccessForMediaType:completionHandler:\0".as_ptr() as *const i8,
            );
            let sel_str = sel_registerName(b"stringWithUTF8String:\0".as_ptr() as *const i8);
            let ns_string_cls = objc_getClass(b"NSString\0".as_ptr() as *const i8);
            let msg_send_str: unsafe extern "C" fn(*mut c_void, *mut c_void, *const i8) -> *mut c_void =
                std::mem::transmute(objc_msgSend as *const ());
            let media_type = msg_send_str(ns_string_cls, sel_str, b"soun\0".as_ptr() as *const i8);

            let msg_send_req:
                unsafe extern "C" fn(*mut c_void, *mut c_void, *mut c_void, *mut BlockWithTx) =
                std::mem::transmute(objc_msgSend as *const ());
            msg_send_req(cls, sel_req, media_type, &mut block);
        }

        let granted = rx.recv_timeout(Duration::from_secs(30)).unwrap_or(false);
        let final_status = if granted {
            MacMicPermission::Authorized
        } else {
            MacMicPermission::Denied
        };
        tracing::info!("macOS microphone permission request result: {:?}", final_status);
        final_status
    }
}

/// Abstraction for recording audio from the system's microphone.
pub trait AudioCapture: Send + Sync {
    /// Check whether a microphone input device is currently detected by the OS.
    fn is_microphone_available(&self) -> bool;

    /// Begin recording audio from the microphone into an internal buffer.
    fn start_recording(&self) -> Result<(), PlatformError>;

    /// Stop recording and return the captured audio encoded as a 16kHz mono 16-bit WAV byte buffer.
    fn stop_recording(&self) -> Result<Vec<u8>, PlatformError>;

    /// Return true if recording is currently in progress.
    fn is_recording(&self) -> bool;
}

struct ActiveStream {
    _stream: Stream,
    samples: Arc<Mutex<Vec<f32>>>,
    source_sample_rate: u32,
    source_channels: u16,
}

/// Native audio recorder backed by `cpal` (WASAPI on Windows, CoreAudio on macOS).
pub struct CpalAudioCapture {
    active_stream: Arc<Mutex<Option<ActiveStream>>>,
    is_recording_flag: Arc<AtomicBool>,
}

impl Default for CpalAudioCapture {
    fn default() -> Self {
        Self::new()
    }
}

impl CpalAudioCapture {
    pub fn new() -> Self {
        Self {
            active_stream: Arc::new(Mutex::new(None)),
            is_recording_flag: Arc::new(AtomicBool::new(false)),
        }
    }
}

impl AudioCapture for CpalAudioCapture {
    fn is_microphone_available(&self) -> bool {
        #[cfg(target_os = "macos")]
        {
            let status = macos_mic::check_mic_permission();
            if status == macos_mic::MacMicPermission::Denied
                || status == macos_mic::MacMicPermission::Restricted
            {
                return false;
            }
        }
        let host = cpal::default_host();
        host.default_input_device().is_some()
    }

    fn start_recording(&self) -> Result<(), PlatformError> {
        #[cfg(target_os = "macos")]
        {
            let status = macos_mic::ensure_mic_permission();
            if status == macos_mic::MacMicPermission::Denied
                || status == macos_mic::MacMicPermission::Restricted
            {
                tracing::error!("macOS microphone permission denied or restricted");
                return Err(PlatformError::PermissionDenied(
                    "Microphone access denied. Please grant microphone access in System Settings -> Privacy & Security -> Microphone.".to_string(),
                ));
            }
        }

        let host = cpal::default_host();
        let device = host.default_input_device().ok_or_else(|| {
            tracing::error!("No audio input device detected by CPAL");
            PlatformError::SystemApi("No audio input device detected".to_string())
        })?;

        let device_name = device.description().map(|d| d.name().to_string()).unwrap_or_else(|_| device.to_string());
        tracing::info!(device = %device_name, "Selected default audio input device");

        let supported_config = device.default_input_config().map_err(|e| {
            tracing::error!(error = %e, "Failed to query default input config");
            PlatformError::SystemApi(format!("Failed to get default input config: {}", e))
        })?;

        let sample_rate = supported_config.sample_rate();
        let channels = supported_config.channels();
        let sample_format = supported_config.sample_format();

        tracing::info!(
            rate = sample_rate,
            channels = channels,
            format = ?sample_format,
            "Creating CPAL audio input stream"
        );

        let samples_buf = Arc::new(Mutex::new(Vec::<f32>::with_capacity(
            sample_rate as usize * 10,
        )));
        let samples_clone = samples_buf.clone();

        let err_fn = |err| {
            tracing::error!(error = %err, "Audio input stream callback error");
        };

        let stream_config: cpal::StreamConfig = supported_config.into();

        let stream = match sample_format {
            SampleFormat::F32 => device
                .build_input_stream(
                    stream_config.clone(),
                    move |data: &[f32], _: &_| {
                        if let Ok(mut buf) = samples_clone.lock() {
                            buf.extend_from_slice(data);
                        }
                    },
                    err_fn,
                    None,
                )
                .map_err(|e| {
                    PlatformError::SystemApi(format!("Failed to build f32 input stream: {}", e))
                })?,
            SampleFormat::I16 => device
                .build_input_stream(
                    stream_config.clone(),
                    move |data: &[i16], _: &_| {
                        if let Ok(mut buf) = samples_clone.lock() {
                            buf.extend(data.iter().map(|&s| (s as f32) / 32768.0));
                        }
                    },
                    err_fn,
                    None,
                )
                .map_err(|e| {
                    PlatformError::SystemApi(format!("Failed to build i16 input stream: {}", e))
                })?,
            SampleFormat::U16 => device
                .build_input_stream(
                    stream_config,
                    move |data: &[u16], _: &_| {
                        if let Ok(mut buf) = samples_clone.lock() {
                            buf.extend(data.iter().map(|&s| (s as f32 - 32768.0) / 32768.0));
                        }
                    },
                    err_fn,
                    None,
                )
                .map_err(|e| {
                    PlatformError::SystemApi(format!("Failed to build u16 input stream: {}", e))
                })?,
            _ => {
                return Err(PlatformError::Unsupported(format!(
                    "Unsupported audio format: {:?}",
                    sample_format
                )));
            }
        };

        stream.play().map_err(|e| {
            tracing::error!(error = %e, "Failed to start CPAL audio stream");
            PlatformError::SystemApi(format!("Failed to start audio stream: {}", e))
        })?;

        let mut lock = self.active_stream.lock().unwrap();
        *lock = Some(ActiveStream {
            _stream: stream,
            samples: samples_buf,
            source_sample_rate: sample_rate,
            source_channels: channels,
        });

        self.is_recording_flag.store(true, Ordering::SeqCst);
        tracing::info!(
            rate = sample_rate,
            channels = channels,
            "CPAL audio recording started"
        );
        Ok(())
    }

    fn stop_recording(&self) -> Result<Vec<u8>, PlatformError> {
        self.is_recording_flag.store(false, Ordering::SeqCst);

        let active = {
            let mut lock = self.active_stream.lock().unwrap();
            lock.take()
        };

        let Some(active) = active else {
            tracing::info!("stop_recording called with no active audio stream");
            return Ok(Vec::new());
        };

        let raw_samples = {
            let lock = active.samples.lock().unwrap();
            lock.clone()
        };

        tracing::info!(
            sample_count = raw_samples.len(),
            source_rate = active.source_sample_rate,
            source_channels = active.source_channels,
            "CPAL audio recording stopped"
        );

        let wav_bytes = encode_to_16k_mono_wav(
            &raw_samples,
            active.source_sample_rate,
            active.source_channels,
        )?;

        tracing::info!(
            sample_count = raw_samples.len(),
            byte_count = wav_bytes.len(),
            "Finalized audio buffer encoded as 16kHz mono WAV"
        );

        Ok(wav_bytes)
    }

    fn is_recording(&self) -> bool {
        self.is_recording_flag.load(Ordering::SeqCst)
    }
}

/// Convert arbitrary multi-channel f32 samples to 16kHz mono 16-bit PCM WAV.
pub fn encode_to_16k_mono_wav(
    samples: &[f32],
    source_rate: u32,
    source_channels: u16,
) -> Result<Vec<u8>, PlatformError> {
    if samples.is_empty() || source_channels == 0 || source_rate == 0 {
        return Ok(Vec::new());
    }

    // 1. Downmix multi-channel audio to mono
    let channels = source_channels as usize;
    let frame_count = samples.len() / channels;
    let mut mono_samples = Vec::with_capacity(frame_count);

    for frame_idx in 0..frame_count {
        let base = frame_idx * channels;
        let mut sum = 0.0f32;
        for c in 0..channels {
            sum += samples[base + c];
        }
        mono_samples.push(sum / channels as f32);
    }

    // 2. Resample to 16,000 Hz if necessary using linear interpolation
    let target_rate = 16_000u32;
    let resampled: Vec<f32> = if source_rate == target_rate {
        mono_samples
    } else {
        let ratio = source_rate as f64 / target_rate as f64;
        let target_len = ((mono_samples.len() as f64) / ratio).round() as usize;
        let mut output = Vec::with_capacity(target_len);

        for i in 0..target_len {
            let src_idx = (i as f64) * ratio;
            let idx0 = src_idx.floor() as usize;
            let idx1 = (idx0 + 1).min(mono_samples.len().saturating_sub(1));
            let fract = (src_idx - idx0 as f64) as f32;

            let s0 = mono_samples.get(idx0).copied().unwrap_or(0.0);
            let s1 = mono_samples.get(idx1).copied().unwrap_or(0.0);
            output.push(s0 + fract * (s1 - s0));
        }
        output
    };

    // 3. Write into WAV buffer using Hound (16-bit signed PCM, 16kHz, 1 channel)
    let spec = WavSpec {
        channels: 1,
        sample_rate: target_rate,
        bits_per_sample: 16,
        sample_format: HoundSampleFormat::Int,
    };

    let mut cursor = Cursor::new(Vec::with_capacity(resampled.len() * 2 + 44));
    {
        let mut writer = WavWriter::new(&mut cursor, spec)
            .map_err(|e| PlatformError::SystemApi(format!("Failed to create WAV writer: {}", e)))?;

        for &sample in &resampled {
            // Convert f32 [-1.0, 1.0] to i16
            let clamped = sample.clamp(-1.0, 1.0);
            let s16 = (clamped * 32767.0).round() as i16;
            writer.write_sample(s16).map_err(|e| {
                PlatformError::SystemApi(format!("Failed to write WAV sample: {}", e))
            })?;
        }

        writer.finalize().map_err(|e| {
            PlatformError::SystemApi(format!("Failed to finalize WAV audio: {}", e))
        })?;
    }

    Ok(cursor.into_inner())
}

/// Simulated audio capture for testing and headless environments.
pub struct MockAudioCapture {
    is_recording_flag: Arc<AtomicBool>,
}

impl Default for MockAudioCapture {
    fn default() -> Self {
        Self::new()
    }
}

impl MockAudioCapture {
    pub fn new() -> Self {
        Self {
            is_recording_flag: Arc::new(AtomicBool::new(false)),
        }
    }
}

impl AudioCapture for MockAudioCapture {
    fn is_microphone_available(&self) -> bool {
        true
    }

    fn start_recording(&self) -> Result<(), PlatformError> {
        self.is_recording_flag.store(true, Ordering::SeqCst);
        Ok(())
    }

    fn stop_recording(&self) -> Result<Vec<u8>, PlatformError> {
        self.is_recording_flag.store(false, Ordering::SeqCst);

        // Generate 0.5s of 440Hz sine wave at 16kHz mono
        let sample_rate = 16_000;
        let num_samples = 8_000; // 0.5 seconds
        let mut samples = Vec::with_capacity(num_samples);
        for i in 0..num_samples {
            let t = i as f32 / sample_rate as f32;
            let sample = (2.0 * std::f32::consts::PI * 440.0 * t).sin() * 0.25;
            samples.push(sample);
        }

        encode_to_16k_mono_wav(&samples, sample_rate, 1)
    }

    fn is_recording(&self) -> bool {
        self.is_recording_flag.load(Ordering::SeqCst)
    }
}
