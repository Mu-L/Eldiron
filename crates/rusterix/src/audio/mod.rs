use std::f32::consts::TAU;
use std::io::Cursor;
use std::sync::Arc as StdArc;
use std::sync::{Arc, Mutex};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

#[derive(Debug, Clone, Copy)]
pub struct AudioConfig {
    pub sample_rate: Option<u32>,
    pub channels: Option<u16>,
    pub master_volume: f32,
}

impl Default for AudioConfig {
    fn default() -> Self {
        Self {
            sample_rate: None,
            channels: None,
            master_volume: 1.0,
        }
    }
}

#[derive(Debug, Clone)]
pub struct OutputInfo {
    pub device_name: String,
    pub sample_rate: u32,
    pub channels: u16,
}

#[derive(Debug)]
pub enum AudioError {
    NoOutputDevice,
    DefaultOutputConfig(cpal::DefaultStreamConfigError),
    BuildStream(cpal::BuildStreamError),
    PlayStream(cpal::PlayStreamError),
    Decode(String),
}

impl std::fmt::Display for AudioError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoOutputDevice => write!(f, "no default output audio device found"),
            Self::DefaultOutputConfig(e) => {
                write!(f, "failed to query default output stream config: {e}")
            }
            Self::BuildStream(e) => write!(f, "failed to build audio output stream: {e}"),
            Self::PlayStream(e) => write!(f, "failed to start audio output stream: {e}"),
            Self::Decode(e) => write!(f, "failed to decode audio clip: {e}"),
        }
    }
}

impl std::error::Error for AudioError {}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub struct SineVoiceId(u64);

#[derive(Debug)]
struct Voice {
    id: SineVoiceId,
    phase: f32,
    phase_inc: f32,
    gain: f32,
    samples_left: usize,
}

#[derive(Debug, Clone)]
struct DecodedClip {
    sample_rate: u32,
    samples: StdArc<[f32]>,
}

#[derive(Debug)]
struct ClipVoice {
    samples: StdArc<[f32]>,
    pos: f32,
    step: f32,
    gain: f32,
    bus: String,
    looping: bool,
}

#[derive(Debug)]
struct MixerState {
    next_id: u64,
    master_volume: f32,
    voices: Vec<Voice>,
    clips: std::collections::HashMap<String, DecodedClip>,
    clip_voices: Vec<ClipVoice>,
    bus_volumes: std::collections::HashMap<String, f32>,
}

impl MixerState {
    fn new(master_volume: f32) -> Self {
        let mut bus_volumes = std::collections::HashMap::default();
        bus_volumes.insert("master".to_string(), 1.0);
        bus_volumes.insert("music".to_string(), 1.0);
        bus_volumes.insert("sfx".to_string(), 1.0);
        bus_volumes.insert("ui".to_string(), 1.0);
        bus_volumes.insert("ambience".to_string(), 1.0);
        bus_volumes.insert("voice".to_string(), 1.0);
        Self {
            next_id: 1,
            master_volume,
            voices: Vec::new(),
            clips: std::collections::HashMap::default(),
            clip_voices: Vec::new(),
            bus_volumes,
        }
    }

    fn set_master_volume(&mut self, volume: f32) {
        self.master_volume = volume.clamp(0.0, 4.0);
    }

    fn add_sine_voice(
        &mut self,
        sample_rate: u32,
        hz: f32,
        duration_seconds: f32,
        gain: f32,
    ) -> SineVoiceId {
        let id = SineVoiceId(self.next_id);
        self.next_id += 1;

        let hz = hz.max(1.0);
        let seconds = duration_seconds.max(0.0);
        let gain = gain.clamp(0.0, 1.0);
        let samples_left = (seconds * sample_rate as f32) as usize;
        let phase_inc = (hz / sample_rate as f32) * TAU;

        self.voices.push(Voice {
            id,
            phase: 0.0,
            phase_inc,
            gain,
            samples_left,
        });
        id
    }

    fn stop_voice(&mut self, id: SineVoiceId) {
        self.voices.retain(|v| v.id != id);
    }

    fn clear_clips(&mut self) {
        self.clips.clear();
        self.clip_voices.clear();
    }

    fn insert_clip(&mut self, name: String, clip: DecodedClip) {
        self.clips.insert(name, clip);
    }

    fn set_bus_volume(&mut self, bus: &str, volume: f32) {
        self.bus_volumes
            .insert(bus.to_string(), volume.clamp(0.0, 4.0));
    }

    fn get_bus_volume(&self, bus: &str) -> f32 {
        self.bus_volumes.get(bus).copied().unwrap_or(1.0)
    }

    fn clear_bus(&mut self, bus: &str) {
        self.clip_voices.retain(|v| v.bus != bus);
    }

    fn clear_all_buses(&mut self) {
        self.clip_voices.clear();
    }

    fn play_clip(
        &mut self,
        output_sample_rate: u32,
        name: &str,
        bus: &str,
        gain: f32,
        looping: bool,
    ) -> bool {
        let Some(clip) = self.clips.get(name).cloned() else {
            return false;
        };
        if clip.samples.is_empty() || clip.sample_rate == 0 || output_sample_rate == 0 {
            return false;
        }
        let step = clip.sample_rate as f32 / output_sample_rate as f32;
        self.clip_voices.push(ClipVoice {
            samples: clip.samples,
            pos: 0.0,
            step,
            gain: gain.clamp(0.0, 4.0),
            bus: bus.to_string(),
            looping,
        });
        if !self.bus_volumes.contains_key(bus) {
            self.bus_volumes.insert(bus.to_string(), 1.0);
        }
        true
    }

    fn mix_next_sample(&mut self) -> f32 {
        let mut out = 0.0f32;
        for voice in &mut self.voices {
            if voice.samples_left > 0 {
                out += voice.phase.sin() * voice.gain;
                voice.phase += voice.phase_inc;
                if voice.phase > TAU {
                    voice.phase -= TAU;
                }
                voice.samples_left -= 1;
            }
        }
        let bus_volumes = self.bus_volumes.clone();
        for voice in &mut self.clip_voices {
            let bus_volume = bus_volumes.get(&voice.bus).copied().unwrap_or(1.0);
            let i0 = voice.pos.floor() as usize;
            let i1 = i0.saturating_add(1);
            if i0 < voice.samples.len() {
                let s0 = voice.samples[i0];
                let s1 = if i1 < voice.samples.len() {
                    voice.samples[i1]
                } else {
                    s0
                };
                let frac = voice.pos - i0 as f32;
                out += (s0 + (s1 - s0) * frac) * voice.gain * bus_volume;
                voice.pos += voice.step;
            } else if voice.looping && !voice.samples.is_empty() {
                voice.pos = 0.0;
            }
        }
        self.voices.retain(|v| v.samples_left > 0);
        self.clip_voices
            .retain(|v| v.looping || (v.pos.floor() as usize) < v.samples.len());
        (out * self.master_volume).clamp(-1.0, 1.0)
    }
}

pub struct AudioEngine {
    _stream: cpal::Stream,
    mixer: Arc<Mutex<MixerState>>,
    output: OutputInfo,
}

impl AudioEngine {
    pub fn new() -> Result<Self, AudioError> {
        Self::with_config(AudioConfig::default())
    }

    pub fn with_config(config: AudioConfig) -> Result<Self, AudioError> {
        let host = cpal::default_host();
        let device = host
            .default_output_device()
            .ok_or(AudioError::NoOutputDevice)?;
        let default_cfg = device
            .default_output_config()
            .map_err(AudioError::DefaultOutputConfig)?;

        let mut stream_config = default_cfg.config();
        if let Some(sample_rate) = config.sample_rate {
            stream_config.sample_rate = sample_rate.max(1);
        }
        if let Some(channels) = config.channels {
            stream_config.channels = channels.max(1);
        }

        let output = OutputInfo {
            device_name: device
                .description()
                .map(|d| d.name().to_string())
                .unwrap_or_else(|_| "unknown output".to_string()),
            sample_rate: stream_config.sample_rate,
            channels: stream_config.channels,
        };

        let mixer = Arc::new(Mutex::new(MixerState::new(config.master_volume)));
        let stream = build_stream(
            &device,
            &stream_config,
            default_cfg.sample_format(),
            mixer.clone(),
        )
        .map_err(AudioError::BuildStream)?;
        stream.play().map_err(AudioError::PlayStream)?;

        Ok(Self {
            _stream: stream,
            mixer,
            output,
        })
    }

    pub fn output_info(&self) -> &OutputInfo {
        &self.output
    }

    pub fn set_master_volume(&self, volume: f32) {
        if let Ok(mut m) = self.mixer.lock() {
            m.set_master_volume(volume);
        }
    }

    pub fn play_sine(&self, hz: f32, duration_seconds: f32, gain: f32) -> Option<SineVoiceId> {
        if let Ok(mut m) = self.mixer.lock() {
            return Some(m.add_sine_voice(self.output.sample_rate, hz, duration_seconds, gain));
        }
        None
    }

    pub fn stop_voice(&self, id: SineVoiceId) {
        if let Ok(mut m) = self.mixer.lock() {
            m.stop_voice(id);
        }
    }

    pub fn clear_clips(&self) {
        if let Ok(mut m) = self.mixer.lock() {
            m.clear_clips();
        }
    }

    pub fn load_clip_from_bytes(&self, name: &str, bytes: &[u8]) -> Result<(), AudioError> {
        let clip = decode_clip(bytes)?;
        if let Ok(mut m) = self.mixer.lock() {
            m.insert_clip(name.to_string(), clip);
        }
        Ok(())
    }

    pub fn play_one_shot(&self, name: &str, gain: f32) -> bool {
        if let Ok(mut m) = self.mixer.lock() {
            return m.play_clip(self.output.sample_rate, name, "sfx", gain, false);
        }
        false
    }

    pub fn play_on_bus(&self, name: &str, bus: &str, gain: f32, looping: bool) -> bool {
        if let Ok(mut m) = self.mixer.lock() {
            return m.play_clip(self.output.sample_rate, name, bus, gain, looping);
        }
        false
    }

    pub fn set_bus_volume(&self, bus: &str, volume: f32) {
        if let Ok(mut m) = self.mixer.lock() {
            m.set_bus_volume(bus, volume);
        }
    }

    pub fn bus_volume(&self, bus: &str) -> f32 {
        if let Ok(m) = self.mixer.lock() {
            return m.get_bus_volume(bus);
        }
        1.0
    }

    pub fn clear_bus(&self, bus: &str) {
        if let Ok(mut m) = self.mixer.lock() {
            m.clear_bus(bus);
        }
    }

    pub fn clear_all_buses(&self) {
        if let Ok(mut m) = self.mixer.lock() {
            m.clear_all_buses();
        }
    }
}

fn build_stream(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    sample_format: cpal::SampleFormat,
    mixer: Arc<Mutex<MixerState>>,
) -> Result<cpal::Stream, cpal::BuildStreamError> {
    let err_fn = |err| {
        eprintln!("[audio] output stream error: {err}");
    };

    match sample_format {
        cpal::SampleFormat::F32 => build_stream_t::<f32>(device, config, mixer, err_fn),
        cpal::SampleFormat::I16 => build_stream_t::<i16>(device, config, mixer, err_fn),
        cpal::SampleFormat::U16 => build_stream_t::<u16>(device, config, mixer, err_fn),
        _ => build_stream_t::<f32>(device, config, mixer, err_fn),
    }
}

fn build_stream_t<T>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    mixer: Arc<Mutex<MixerState>>,
    err_fn: impl FnMut(cpal::StreamError) + Send + 'static,
) -> Result<cpal::Stream, cpal::BuildStreamError>
where
    T: cpal::SizedSample + cpal::FromSample<f32>,
{
    let channels = config.channels as usize;
    device.build_output_stream(
        config,
        move |data: &mut [T], _| {
            if let Ok(mut m) = mixer.lock() {
                for frame in data.chunks_mut(channels) {
                    let v: T = T::from_sample(m.mix_next_sample());
                    for sample in frame {
                        *sample = v;
                    }
                }
            } else {
                let zero: T = T::from_sample(0.0);
                for sample in data {
                    *sample = zero;
                }
            }
        },
        err_fn,
        None,
    )
}

fn decode_clip(bytes: &[u8]) -> Result<DecodedClip, AudioError> {
    if is_wav(bytes) {
        return decode_wav(bytes);
    }
    if is_ogg(bytes) {
        return decode_ogg(bytes);
    }
    decode_wav(bytes).or_else(|_| decode_ogg(bytes))
}

fn is_wav(bytes: &[u8]) -> bool {
    bytes.len() >= 12 && &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"WAVE"
}

fn is_ogg(bytes: &[u8]) -> bool {
    bytes.len() >= 4 && &bytes[0..4] == b"OggS"
}

fn decode_wav(bytes: &[u8]) -> Result<DecodedClip, AudioError> {
    let cursor = Cursor::new(bytes);
    let mut reader = hound::WavReader::new(cursor)
        .map_err(|e| AudioError::Decode(format!("wav reader error: {e}")))?;
    let spec = reader.spec();
    let channels = spec.channels.max(1) as usize;
    let sample_rate = spec.sample_rate.max(1);

    let mut interleaved = Vec::<f32>::new();
    match spec.sample_format {
        hound::SampleFormat::Float => {
            for s in reader.samples::<f32>() {
                interleaved.push(
                    s.map_err(|e| AudioError::Decode(format!("wav float sample error: {e}")))?,
                );
            }
        }
        hound::SampleFormat::Int => {
            let bits = spec.bits_per_sample.clamp(1, 32) as i32;
            let denom = (1_i64 << (bits - 1)) as f32;
            for s in reader.samples::<i32>() {
                let v = s.map_err(|e| AudioError::Decode(format!("wav int sample error: {e}")))?;
                interleaved.push((v as f32 / denom).clamp(-1.0, 1.0));
            }
        }
    }

    let mono = downmix_to_mono(&interleaved, channels);
    Ok(DecodedClip {
        sample_rate,
        samples: mono.into(),
    })
}

fn decode_ogg(bytes: &[u8]) -> Result<DecodedClip, AudioError> {
    let cursor = Cursor::new(bytes);
    let mut reader = lewton::inside_ogg::OggStreamReader::new(cursor)
        .map_err(|e| AudioError::Decode(format!("ogg reader error: {e}")))?;
    let channels = reader.ident_hdr.audio_channels.max(1) as usize;
    let sample_rate = reader.ident_hdr.audio_sample_rate.max(1);

    let mut interleaved = Vec::<f32>::new();
    while let Some(packet) = reader
        .read_dec_packet_itl()
        .map_err(|e| AudioError::Decode(format!("ogg packet decode error: {e}")))?
    {
        interleaved.extend(packet.into_iter().map(|s| s as f32 / 32768.0));
    }

    let mono = downmix_to_mono(&interleaved, channels);
    Ok(DecodedClip {
        sample_rate,
        samples: mono.into(),
    })
}

fn downmix_to_mono(interleaved: &[f32], channels: usize) -> Vec<f32> {
    if channels <= 1 {
        return interleaved.to_vec();
    }
    let mut out = Vec::with_capacity(interleaved.len() / channels);
    for frame in interleaved.chunks(channels) {
        let mut sum = 0.0f32;
        for s in frame {
            sum += *s;
        }
        out.push(sum / channels as f32);
    }
    out
}
