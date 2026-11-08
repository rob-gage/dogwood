// Copyright Rob Gage 2026

//! Low-latency stereo playback with bounded voices and independent mixing buses.

use cpal::{
    FromSample, SizedSample, Stream, StreamConfig,
    traits::{DeviceTrait, HostTrait, StreamTrait},
};
use std::{
    error::Error,
    fmt::{self, Display, Formatter},
    sync::{Arc, Mutex},
};

/// Maximum simultaneous voices; stealing chooses the lowest priority, then oldest voice.
const MAXIMUM_VOICES: usize = 32;

/// An error from audio output or sound decoding.
#[derive(Debug)]
pub enum AudioError {
    /// No default output device is available.
    NoDevice,
    /// The audio backend failed to configure or start the stream.
    Backend(String),
    /// The device uses a sample format this mixer cannot write.
    UnsupportedFormat(String),
    /// The WAV data is malformed or not 16-bit PCM.
    InvalidWav(&'static str),
}

impl Display for AudioError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            AudioError::NoDevice => write!(formatter, "no audio output device available"),
            AudioError::Backend(message) => write!(formatter, "audio backend error: {message}"),
            AudioError::UnsupportedFormat(format) => {
                write!(formatter, "unsupported audio sample format: {format}")
            }
            AudioError::InvalidWav(reason) => write!(formatter, "invalid WAV data: {reason}"),
        }
    }
}

impl Error for AudioError {}

/// Decoded stereo frames at a selected output rate. Cheap to clone.
#[derive(Clone)]
pub struct Sound(Arc<Vec<[f32; 2]>>);

/// Independently controlled playback buses, both governed by the master gain.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Bus {
    /// Game and interface effects.
    Sfx,
    /// Centered music playback that preserves the recording's stereo channels.
    Music,
}

/// Opaque handle returned when a voice starts; ended or stolen voices invalidate it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VoiceId(u64);

impl Sound {
    /// Decodes a PCM WAV without opening an output device. Stereo recordings retain their image.
    pub fn from_wav(bytes: &[u8], sample_rate: u32) -> Result<Self, AudioError> {
        if sample_rate == 0 {
            return Err(AudioError::InvalidWav("zero output sample rate"));
        }
        let (rate, frames) = decode_wav(bytes)?;
        Ok(Self(Arc::new(resample(&frames, rate, sample_rate))))
    }
}

struct Voice {
    sound: Arc<Vec<[f32; 2]>>,
    position: f64,
    step: f64,
    volume: f32,
    age: u64,
    bus: Bus,
    pan: f32,
    looping: bool,
    target: f32,
    priority: u8,
}

struct Mixer {
    voices: Vec<Voice>,
    master_volume: f32,
    counter: u64,
    buses: [f32; 2],
}

impl Mixer {
    fn mix<T: SizedSample + FromSample<f32>>(&mut self, output: &mut [T], channels: usize) {
        for frame in output.chunks_mut(channels) {
            let mut sum = [0.0f32; 2];
            for voice in &mut self.voices {
                let len = voice.sound.len();
                if len == 0 {
                    continue;
                }
                if voice.looping {
                    voice.position %= len as f64;
                }
                let index = voice.position as usize;
                let Some(&a) = voice.sound.get(index) else {
                    continue;
                };
                let b = voice
                    .sound
                    .get(index + 1)
                    .copied()
                    .unwrap_or(if voice.looping {
                        voice.sound[0]
                    } else {
                        [0.0; 2]
                    });
                let fraction = (voice.position - index as f64) as f32;
                let sample = [
                    a[0] + (b[0] - a[0]) * fraction,
                    a[1] + (b[1] - a[1]) * fraction,
                ];
                // About 20 ms smoothing at 48 kHz, independent of callback size.
                voice.volume += (voice.target - voice.volume) * 0.001;
                let gain = voice.volume * self.buses[voice.bus as usize];
                sum[0] += sample[0] * gain * (1.0 - voice.pan).sqrt();
                sum[1] += sample[1] * gain * (1.0 + voice.pan).sqrt();
                voice.position += voice.step;
            }
            for (channel, sample) in frame.iter_mut().enumerate() {
                let value = if channels == 1 {
                    (sum[0] + sum[1]) * 0.5
                } else {
                    sum[channel % 2]
                };
                *sample = T::from_sample(soft_limit(value * self.master_volume));
            }
        }
        self.voices.retain(|v| {
            (v.looping || (v.position as usize) < v.sound.len())
                && (v.target > 0.0 || v.volume > 0.0001)
        });
    }
}

/// Owns the output stream and mixer.
pub struct Audio {
    _stream: Stream,
    mixer: Arc<Mutex<Mixer>>,
    sample_rate: u32,
}

impl Audio {
    /// Opens the default output device. Fails gracefully when none is available.
    pub fn new() -> Result<Audio, AudioError> {
        let device = cpal::default_host()
            .default_output_device()
            .ok_or(AudioError::NoDevice)?;
        let supported = device
            .default_output_config()
            .map_err(|error| AudioError::Backend(error.to_string()))?;
        let format = supported.sample_format();
        let config: StreamConfig = supported.into();
        let mixer = Arc::new(Mutex::new(Mixer {
            voices: Vec::with_capacity(MAXIMUM_VOICES + 1),
            master_volume: 1.0,
            counter: 0,
            buses: [1.0; 2],
        }));
        let stream = match format {
            cpal::SampleFormat::F32 => build::<f32>(&device, config.clone(), &mixer),
            cpal::SampleFormat::I16 => build::<i16>(&device, config.clone(), &mixer),
            cpal::SampleFormat::U16 => build::<u16>(&device, config.clone(), &mixer),
            cpal::SampleFormat::I32 => build::<i32>(&device, config.clone(), &mixer),
            other => return Err(AudioError::UnsupportedFormat(other.to_string())),
        }?;
        stream
            .play()
            .map_err(|error| AudioError::Backend(error.to_string()))?;
        Ok(Audio {
            _stream: stream,
            mixer,
            sample_rate: config.sample_rate,
        })
    }

    /// Decodes 16-bit PCM WAV bytes (mono or stereo, any rate; preserves stereo) into a [`Sound`] at the device rate.
    pub fn load_wav(&self, bytes: &[u8]) -> Result<Sound, AudioError> {
        Sound::from_wav(bytes, self.sample_rate)
    }

    /// Starts a one-shot voice. `pitch` 1.0 is normal speed.
    pub fn play(&self, sound: &Sound, volume: f32, pitch: f32) {
        self.start(sound, volume, pitch, 0.0, Bus::Sfx, false, 128);
    }

    /// Starts a bounded voice. Panning is -1 left to +1 right; music is centered.
    pub fn start(
        &self,
        sound: &Sound,
        volume: f32,
        pitch: f32,
        pan: f32,
        bus: Bus,
        looping: bool,
        priority: u8,
    ) -> Option<VoiceId> {
        if sound.0.is_empty() {
            return None;
        }
        let Ok(mut mixer) = self.mixer.lock() else {
            return None;
        };
        if mixer
            .voices
            .iter()
            .filter(|v| Arc::ptr_eq(&v.sound, &sound.0))
            .count()
            >= 4
        {
            let victim = mixer
                .voices
                .iter()
                .enumerate()
                .filter(|(_, v)| Arc::ptr_eq(&v.sound, &sound.0))
                .min_by_key(|(_, v)| v.age)
                .map(|(i, _)| i)?;
            if mixer.voices[victim].priority > priority {
                return None;
            }
            mixer.voices.swap_remove(victim);
        }
        if mixer.voices.len() >= MAXIMUM_VOICES {
            let victim = (0..mixer.voices.len())
                .min_by_key(|&i| (mixer.voices[i].priority, mixer.voices[i].age))?;
            if mixer.voices[victim].priority > priority {
                return None;
            }
            mixer.voices.swap_remove(victim);
        }
        mixer.counter += 1;
        let age = mixer.counter;
        let volume = finite_gain(volume);
        mixer.voices.push(Voice {
            sound: sound.0.clone(),
            position: 0.0,
            step: if pitch.is_finite() {
                pitch.clamp(0.1, 8.0) as f64
            } else {
                1.0
            },
            volume: if looping { 0.0 } else { volume },
            target: volume,
            age,
            bus,
            pan: if bus == Bus::Music {
                0.0
            } else {
                finite_pan(pan)
            },
            looping,
            priority,
        });
        Some(VoiceId(age))
    }

    /// Smoothly changes a persistent source; false means it ended or was stolen.
    pub fn update_voice(&self, id: VoiceId, volume: f32, pan: f32) -> bool {
        let Ok(mut mixer) = self.mixer.lock() else {
            return false;
        };
        let Some(v) = mixer.voices.iter_mut().find(|v| v.age == id.0) else {
            return false;
        };
        v.target = finite_gain(volume);
        v.pan = if v.bus == Bus::Music {
            0.0
        } else {
            finite_pan(pan)
        };
        true
    }

    pub fn stop(&self, id: VoiceId) {
        self.update_voice(id, 0.0, 0.0);
    }
    pub fn stop_bus(&self, bus: Bus) {
        if let Ok(mut m) = self.mixer.lock() {
            for v in &mut m.voices {
                if v.bus == bus {
                    v.target = 0.0;
                }
            }
        }
    }
    pub fn stop_all(&self) {
        if let Ok(mut m) = self.mixer.lock() {
            m.voices.clear();
        }
    }
    pub fn active_voices(&self) -> usize {
        self.mixer.lock().map(|m| m.voices.len()).unwrap_or(0)
    }
    pub fn set_bus_volume(&self, bus: Bus, volume: f32) {
        if let Ok(mut m) = self.mixer.lock() {
            m.buses[bus as usize] = finite_gain(volume);
        }
    }

    /// Sets the master volume applied before the limiter.
    pub fn set_master_volume(&self, volume: f32) {
        if let Ok(mut mixer) = self.mixer.lock() {
            mixer.master_volume = finite_gain(volume);
        }
    }
}

fn build<T: SizedSample + FromSample<f32>>(
    device: &cpal::Device,
    config: StreamConfig,
    mixer: &Arc<Mutex<Mixer>>,
) -> Result<Stream, AudioError> {
    let channels = (config.channels as usize).max(1);
    let mixer = mixer.clone();
    device
        .build_output_stream(
            config,
            move |data: &mut [T], _| match mixer.lock() {
                Ok(mut mixer) => mixer.mix(data, channels),
                Err(_) => data.fill(T::EQUILIBRIUM),
            },
            |error| eprintln!("audio stream error: {error}"),
            None,
        )
        .map_err(|error| AudioError::Backend(error.to_string()))
}

/// Returns the sample rate and stereo frames (mono is duplicated) of a 16-bit PCM WAV file.
fn decode_wav(bytes: &[u8]) -> Result<(u32, Vec<[f32; 2]>), AudioError> {
    if bytes.len() < 12 || &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        return Err(AudioError::InvalidWav("missing RIFF/WAVE header"));
    }
    let mut format: Option<(u16, u16, u32, u16)> = None;
    let mut data: Option<&[u8]> = None;
    let mut cursor = 12;
    while cursor + 8 <= bytes.len() {
        let id = &bytes[cursor..cursor + 4];
        let size = u32::from_le_bytes(bytes[cursor + 4..cursor + 8].try_into().unwrap()) as usize;
        let body = &bytes[cursor + 8..(cursor + 8 + size).min(bytes.len())];
        if id == b"fmt " && body.len() >= 16 {
            let tag = u16::from_le_bytes([body[0], body[1]]);
            let channels = u16::from_le_bytes([body[2], body[3]]);
            let rate = u32::from_le_bytes(body[4..8].try_into().unwrap());
            let bits = u16::from_le_bytes([body[14], body[15]]);
            format = Some((tag, channels, rate, bits));
        } else if id == b"data" {
            data = Some(body);
        }
        cursor += 8 + size + (size & 1);
    }
    let (tag, channels, rate, bits) = format.ok_or(AudioError::InvalidWav("missing fmt chunk"))?;
    let data = data.ok_or(AudioError::InvalidWav("missing data chunk"))?;
    if (tag != 1 && tag != 0xFFFE) || bits != 16 {
        return Err(AudioError::InvalidWav("only 16-bit PCM is supported"));
    }
    if channels == 0 || rate == 0 {
        return Err(AudioError::InvalidWav("zero channels or sample rate"));
    }
    let channels = channels as usize;
    let mono = data
        .chunks_exact(2 * channels)
        .map(|frame| {
            let read = |channel: usize| {
                i16::from_le_bytes([frame[channel * 2], frame[channel * 2 + 1]]) as f32 / 32768.0
            };
            [read(0), read(if channels > 1 { 1 } else { 0 })]
        })
        .collect();
    Ok((rate, mono))
}

/// Linearly resamples `samples` from `from` Hz to `to` Hz.
fn resample(samples: &[[f32; 2]], from: u32, to: u32) -> Vec<[f32; 2]> {
    if from == to || samples.is_empty() {
        return samples.to_vec();
    }
    let ratio = from as f64 / to as f64;
    let length = (samples.len() as f64 / ratio).floor() as usize;
    (0..length)
        .map(|i| {
            let position = i as f64 * ratio;
            let index = position as usize;
            let fraction = (position - index as f64) as f32;
            let a = samples[index];
            let b = samples.get(index + 1).copied().unwrap_or(a);
            [
                a[0] + (b[0] - a[0]) * fraction,
                a[1] + (b[1] - a[1]) * fraction,
            ]
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mixer_advances_every_frame_within_a_buffer() {
        let ramp: Vec<f32> = (0..64).map(|index| index as f32 / 200.0).collect();
        let mut mixer = Mixer {
            voices: Vec::new(),
            master_volume: 1.0,
            counter: 0,
            buses: [1.0; 2],
        };
        mixer.voices.push(Voice {
            sound: Arc::new(ramp.iter().map(|&v| [v; 2]).collect()),
            position: 0.0,
            step: 1.0,
            volume: 1.0,
            age: 0,
            bus: Bus::Sfx,
            pan: 0.0,
            looping: false,
            target: 1.0,
            priority: 128,
        });
        let mut output = [0.0f32; 32];
        mixer.mix(&mut output, 1);
        for (index, sample) in output.iter().enumerate() {
            assert!(
                (sample - ramp[index]).abs() < 1e-6,
                "frame {index} held {sample}"
            );
        }
        let mut next = [0.0f32; 32];
        mixer.mix(&mut next, 1);
        assert!((next[0] - ramp[32]).abs() < 1e-6);
    }

    fn wav(rate: u32, channels: u16, samples: &[i16]) -> Vec<u8> {
        let mut out = b"RIFF".to_vec();
        out.extend(((36 + samples.len() * 2) as u32).to_le_bytes());
        out.extend(b"WAVEfmt ");
        out.extend(16u32.to_le_bytes());
        out.extend(1u16.to_le_bytes());
        out.extend(channels.to_le_bytes());
        out.extend(rate.to_le_bytes());
        out.extend((rate * 2 * channels as u32).to_le_bytes());
        out.extend((2 * channels).to_le_bytes());
        out.extend(16u16.to_le_bytes());
        out.extend(b"data");
        out.extend(((samples.len() * 2) as u32).to_le_bytes());
        for s in samples {
            out.extend(s.to_le_bytes());
        }
        out
    }

    #[test]
    fn decodes_and_resamples() {
        let (rate, mono) = decode_wav(&wav(22050, 2, &[16384, 0, 16384, 0])).unwrap();
        assert_eq!(rate, 22050);
        assert_eq!(mono, vec![[0.5, 0.0]; 2]);
        assert_eq!(resample(&[[0.0; 2]; 100], 22050, 44100).len(), 200);
    }

    #[test]
    fn rejects_garbage() {
        assert!(decode_wav(b"nope").is_err());
    }

    /// Plays a beep on the real device; run with `cargo test -p engine_audio -- --ignored`.
    #[test]
    #[ignore]
    fn plays_beep() {
        let audio = Audio::new().expect("open device");
        let samples: Vec<i16> = (0..8820)
            .map(|i| {
                let t = i as f32 / 44100.0;
                ((t * 880.0 * std::f32::consts::TAU).sin() * (1.0 - i as f32 / 8820.0) * 12000.0)
                    as i16
            })
            .collect();
        let sound = audio.load_wav(&wav(44100, 1, &samples)).unwrap();
        audio.play(&sound, 0.5, 1.0);
        audio.play(&sound, 0.5, 1.5);
        std::thread::sleep(std::time::Duration::from_millis(500));
    }
}

/// Level below which the mix bus passes samples unchanged
const LIMITER_KNEE: f32 = 0.8;

/// Passes a sample unchanged below the knee and bends it smoothly toward full scale above it
fn soft_limit(sample: f32) -> f32 {
    let magnitude: f32 = sample.abs();
    if magnitude <= LIMITER_KNEE {
        return sample;
    }
    let headroom: f32 = 1.0 - LIMITER_KNEE;
    sample.signum() * (LIMITER_KNEE + headroom * ((magnitude - LIMITER_KNEE) / headroom).tanh())
}

fn finite_gain(value: f32) -> f32 {
    if value.is_finite() {
        value.clamp(0.0, 2.0)
    } else {
        0.0
    }
}

#[cfg(test)]
mod mixing_tests {
    use super::*;
    fn voice(bus: Bus, pan: f32, looping: bool) -> Voice {
        Voice {
            sound: Arc::new(vec![[0.2; 2]; 8]),
            position: 0.0,
            step: 1.0,
            volume: 1.0,
            age: 1,
            bus,
            pan,
            looping,
            target: 1.0,
            priority: 128,
        }
    }
    fn mixer(voices: Vec<Voice>) -> Mixer {
        Mixer {
            voices,
            master_volume: 1.0,
            counter: 1,
            buses: [1.0; 2],
        }
    }
    #[test]
    fn buses_are_independent_and_master_affects_both() {
        let mut m = mixer(vec![
            voice(Bus::Music, 0.0, false),
            voice(Bus::Sfx, 0.0, false),
        ]);
        m.buses = [0.0, 1.0];
        m.master_volume = 0.5;
        let mut out = [0.0f32; 2];
        m.mix(&mut out, 2);
        assert!((out[0] - 0.1).abs() < 1e-6);
        m.buses = [1.0, 0.0];
        m.mix(&mut out, 2);
        assert!((out[0] - 0.1).abs() < 1e-6);
        m.master_volume = 0.0;
        m.mix(&mut out, 2);
        assert_eq!(out, [0.0; 2]);
    }
    #[test]
    fn loops_survive_buffers_and_pan_is_stereo() {
        let mut m = mixer(vec![voice(Bus::Sfx, 0.3, true)]);
        let mut out = [0.0f32; 100];
        m.mix(&mut out, 2);
        assert_eq!(m.voices.len(), 1);
        assert!(out[1] > out[0]);
        assert!(out[0] > 0.1);
        assert_eq!(out[0], out[98]);
    }
    #[test]
    fn stopped_loop_fades_and_releases() {
        let mut m = mixer(vec![voice(Bus::Sfx, 0.0, true)]);
        m.voices[0].target = 0.0;
        m.mix(&mut vec![0.0f32; 20000], 1);
        assert!(m.voices.is_empty());
    }
}

fn finite_pan(value: f32) -> f32 {
    if value.is_finite() {
        value.clamp(-1.0, 1.0)
    } else {
        0.0
    }
}
