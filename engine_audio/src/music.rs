//! Bounded Vorbis streaming. Only the producer thread touches files or codecs.
use super::*;
use crossbeam_queue::ArrayQueue;
use lewton::{inside_ogg::OggStreamReader, samples::InterleavedSamples};
use std::{
    fs::File,
    io::BufReader,
    path::Path,
    sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering},
    thread::{self, JoinHandle},
    time::Duration,
};

const BUFFER_SECONDS: usize = 4;

struct Shared {
    frames: ArrayQueue<[f32; 2]>,
    ready: AtomicBool,
    eof: AtomicBool,
    failed: AtomicBool,
    cancel: AtomicBool,
    started: AtomicBool,
    ended: AtomicBool,
    position: AtomicU64,
    audible: AtomicU64,
    underruns: AtomicU64,
    error: Mutex<Option<String>>,
    rate: u32,
}

/// Prepared music, owned by the application. Dropping the last handle cancels its decoder;
/// worker joins and buffer deallocations occur on the application thread, never the callback.
#[derive(Clone)]
pub struct MusicStream(Arc<Worker>);
struct Worker {
    shared: Arc<Shared>,
    thread: Mutex<Option<JoinHandle<()>>>,
    slots: Arc<AtomicUsize>,
}
impl Drop for Worker {
    fn drop(&mut self) {
        self.shared.cancel.store(true, Ordering::Release);
        self.slots.fetch_sub(1, Ordering::AcqRel);
        if let Ok(slot) = self.thread.get_mut() {
            if let Some(worker) = slot.take() {
                let _ = worker.join();
            }
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct MusicStatus {
    pub position_seconds: f64,
    pub audible_seconds: f64,
    pub ready: bool,
    pub started: bool,
    pub ended: bool,
    pub failed: bool,
    pub underruns: u64,
}
impl MusicStream {
    pub fn ready(&self) -> bool {
        self.status().ready
    }
    pub fn status(&self) -> MusicStatus {
        let s = &self.0.shared;
        MusicStatus {
            position_seconds: s.position.load(Ordering::Acquire) as f64 / s.rate as f64,
            audible_seconds: s.audible.load(Ordering::Acquire) as f64 / s.rate as f64,
            ready: s.ready.load(Ordering::Acquire),
            started: s.started.load(Ordering::Acquire),
            ended: s.ended.load(Ordering::Acquire),
            failed: s.failed.load(Ordering::Acquire),
            underruns: s.underruns.load(Ordering::Relaxed),
        }
    }
    pub fn error(&self) -> Option<String> {
        self.0.shared.error.lock().ok()?.clone()
    }
}

pub(super) struct MusicVoice {
    shared: Arc<Shared>,
    /// Start when predecessor reaches this decoded output frame. No wall clock timing.
    after: Option<(Arc<Shared>, u64)>,
    gain: f32,
    fade_frames: u64,
    age: u64,
    fade_out: Option<(u64, u64)>,
    armed: bool,
}
impl MusicVoice {
    pub(super) fn active(&self) -> bool {
        self.armed
            && !self.shared.ended.load(Ordering::Acquire)
            && !self.shared.cancel.load(Ordering::Acquire)
    }
    pub(super) fn stop(&self) {
        self.shared.cancel.store(true, Ordering::Release);
        self.shared.ended.store(true, Ordering::Release);
    }
    pub(super) fn begin_frame(&self) {
        let s = &self.shared;
        if !self.active() {
            return;
        }
        if !s.started.load(Ordering::Relaxed) {
            if !s.ready.load(Ordering::Acquire) {
                return;
            }
            if self.after.as_ref().is_some_and(|(p, at)| {
                p.position.load(Ordering::Acquire) < *at && !p.ended.load(Ordering::Acquire)
            }) {
                return;
            }
            s.started.store(true, Ordering::Release);
        }
    }
    pub(super) fn sample(&mut self, audible: bool) -> [f32; 2] {
        let s = &self.shared;
        if s.cancel.load(Ordering::Acquire) || s.ended.load(Ordering::Relaxed) {
            return [0.0; 2];
        }
        if !self.armed {
            return [0.0; 2];
        }
        if !s.started.load(Ordering::Relaxed) {
            return [0.0; 2];
        }
        if let Some((start, length)) = self.fade_out {
            if self.age >= start.saturating_add(length) {
                s.ended.store(true, Ordering::Release);
                s.cancel.store(true, Ordering::Release);
                return [0.0; 2];
            }
        }
        let Some(frame) = s.frames.pop() else {
            if s.eof.load(Ordering::Acquire) {
                s.ended.store(true, Ordering::Release);
            } else {
                s.underruns.fetch_add(1, Ordering::Relaxed);
            }
            return [0.0; 2];
        };
        let fade_in = if self.fade_frames == 0 {
            1.0
        } else {
            ((self.age as f32 / self.fade_frames as f32).min(1.0) * std::f32::consts::FRAC_PI_2)
                .sin()
        };
        let fade_out = self.fade_out.map_or(1.0, |(start, len)| {
            if self.age < start {
                1.0
            } else {
                (((self.age - start) as f32 / len.max(1) as f32).min(1.0)
                    * std::f32::consts::FRAC_PI_2)
                    .cos()
            }
        });
        let gain = self.gain * fade_in * fade_out;
        self.age += 1;
        s.position.store(self.age, Ordering::Release);
        if audible && gain > 0.0001 {
            s.audible.fetch_add(1, Ordering::Relaxed);
        }
        [frame[0] * gain, frame[1] * gain]
    }
}

impl Audio {
    /// Background decode, resampling to the output device rate. Metadata validates exact source
    /// frame count and rate; malformed/truncated files fail without affecting the SFX mixer.
    pub fn prepare_music(
        &self,
        path: impl AsRef<Path>,
        source_frames: u64,
        source_rate: u32,
    ) -> Result<MusicStream, AudioError> {
        prepare_stream(
            path.as_ref(),
            source_frames,
            source_rate,
            self.sample_rate,
            self.music_workers.clone(),
        )
    }

    /// Stream a bounded engine resource without extracting it to a temporary file.
    pub fn prepare_music_reader<R: std::io::Read + std::io::Seek + Send + 'static>(
        &self, reader: R, source_frames: u64, source_rate: u32,
    ) -> Result<MusicStream, AudioError> {
        prepare_reader(Box::new(move || Ok(Box::new(reader))), source_frames, source_rate,
            self.sample_rate, self.music_workers.clone())
    }

    /// Arms a preloaded source and equal-power handoff on a predecessor's accurate position.
    /// At most two voices are attached. Return false if not ready, invalid, or both slots busy.
    pub fn transition_music(
        &self,
        stream: &MusicStream,
        after: Option<(&MusicStream, f64)>,
        fade_seconds: f32,
        gain: f32,
    ) -> bool {
        if !stream.ready() || stream.status().failed || stream.status().started {
            return false;
        }
        let Ok(mut m) = self.mixer.lock() else {
            return false;
        };
        let Some(slot) = m.music.iter().position(|v| {
            v.as_ref().is_none_or(|v| {
                v.shared.ended.load(Ordering::Acquire) || v.shared.cancel.load(Ordering::Acquire)
            })
        }) else {
            return false;
        };
        let fade = (fade_seconds.clamp(0.0, 10.0) * self.sample_rate as f32) as u64;
        let after = after.map(|(p, seconds)| {
            (
                p.0.shared.clone(),
                (seconds.max(0.0) * self.sample_rate as f64).round() as u64,
            )
        });
        if let Some((ref previous, at)) = after {
            if let Some(v) = m
                .music
                .iter_mut()
                .flatten()
                .find(|v| Arc::ptr_eq(&v.shared, previous))
            {
                v.fade_out = Some((at.max(v.age), fade));
            }
        } else {
            for v in m.music.iter_mut().flatten() {
                v.fade_out = Some((v.age, fade));
            }
        }
        m.music[slot] = Some(MusicVoice {
            shared: stream.0.shared.clone(),
            after,
            gain: finite_gain(gain),
            fade_frames: fade,
            age: 0,
            fade_out: None,
            armed: true,
        });
        true
    }

    /// Atomically cancels only a not-yet-audible target. False means the callback
    /// already started it; the caller must reconcile it as current rather than drop it.
    pub fn cancel_pending_music(&self, stream: &MusicStream) -> bool {
        let Ok(mut mixer) = self.mixer.lock() else {
            return false;
        };
        cancel_locked(&mut mixer, &stream.0.shared, true)
    }

    /// Cancels a source, including an already playing failed stream.
    pub fn cancel_music(&self, stream: &MusicStream) {
        if let Ok(mut mixer) = self.mixer.lock() {
            cancel_locked(&mut mixer, &stream.0.shared, false);
        }
    }
    pub fn stop_music(&self) {
        if let Ok(mut m) = self.mixer.lock() {
            for v in m.music.iter_mut().flatten() {
                v.shared.cancel.store(true, Ordering::Release);
                v.shared.ended.store(true, Ordering::Release);
            }
            m.music = [None, None];
        }
    }
}

fn cancel_locked(mixer: &mut Mixer, s: &Arc<Shared>, pending_only: bool) -> bool {
    let started = s.started.load(Ordering::Acquire);
    if pending_only && started {
        return false;
    }
    let previous = mixer
        .music
        .iter()
        .flatten()
        .find(|v| Arc::ptr_eq(&v.shared, s))
        .and_then(|v| v.after.as_ref().map(|(p, _)| p.clone()));
    s.cancel.store(true, Ordering::Release);
    s.ended.store(true, Ordering::Release);
    if !started {
        if let Some(previous) = previous {
            if let Some(voice) = mixer
                .music
                .iter_mut()
                .flatten()
                .find(|v| Arc::ptr_eq(&v.shared, &previous))
            {
                voice.fade_out = None;
            }
        }
    }
    true
}

fn decode(reader: Box<dyn MusicReader>, source_frames: u64, source_rate: u32, s: &Shared) -> Result<(), String> {
    let mut decoder =
        OggStreamReader::new(BufReader::new(reader))
            .map_err(|e| e.to_string())?;
    if decoder.ident_hdr.audio_channels != 2 || decoder.ident_hdr.audio_sample_rate != source_rate {
        return Err("music format disagrees with manifest (stereo/rate)".into());
    }
    let mut source_index = 0u64;
    let mut output_index = 0u64;
    let output_frames = ((source_frames as u128 * s.rate as u128) / source_rate as u128) as u64;
    let step = source_rate as f64 / s.rate as f64;
    let mut previous = [0.0; 2];
    while let Some(packet) = decoder
        .read_dec_packet_generic::<InterleavedSamples<f32>>()
        .map_err(|e| e.to_string())?
    {
        for frame in packet.samples.chunks_exact(2) {
            if s.cancel.load(Ordering::Acquire) {
                return Ok(());
            }
            if source_index >= source_frames {
                break;
            }
            let b = [frame[0], frame[1]];
            while output_index < output_frames && output_index as f64 * step <= source_index as f64
            {
                let fraction = (output_index as f64 * step - source_index.saturating_sub(1) as f64)
                    .clamp(0.0, 1.0) as f32;
                let a = if source_index == 0 { b } else { previous };
                let value = [
                    a[0] + (b[0] - a[0]) * fraction,
                    a[1] + (b[1] - a[1]) * fraction,
                ];
                if !push(s, value) {
                    return Ok(());
                }
                output_index += 1;
            }
            source_index += 1;
            previous = b;
        }
        if source_index >= source_frames {
            break;
        }
    }
    if source_index < source_frames {
        return Err(format!(
            "truncated music: expected {source_frames} frames, decoded {source_index}"
        ));
    }
    while output_index < output_frames {
        if !push(s, previous) {
            return Ok(());
        }
        output_index += 1;
    }
    Ok(())
}
fn push(s: &Shared, mut frame: [f32; 2]) -> bool {
    loop {
        if s.cancel.load(Ordering::Acquire) {
            return false;
        }
        match s.frames.push(frame) {
            Ok(()) => {
                if s.frames.len() >= s.rate as usize {
                    s.ready.store(true, Ordering::Release);
                }
                return true;
            }
            Err(value) => {
                frame = value;
                thread::sleep(Duration::from_millis(2));
            }
        }
    }
}

trait MusicReader: std::io::Read + std::io::Seek + Send {}
impl<T: std::io::Read + std::io::Seek + Send> MusicReader for T {}

fn prepare_stream(
    path: &Path,
    source_frames: u64,
    source_rate: u32,
    output_rate: u32,
    slots: Arc<AtomicUsize>,
) -> Result<MusicStream, AudioError> {
    let path = path.to_owned();
    prepare_reader(Box::new(move || Ok(Box::new(File::open(path)?))), source_frames, source_rate, output_rate, slots)
}

fn prepare_reader(
    source: Box<dyn FnOnce() -> std::io::Result<Box<dyn MusicReader>> + Send>,
    source_frames: u64, source_rate: u32, output_rate: u32, slots: Arc<AtomicUsize>,
) -> Result<MusicStream, AudioError> {
    if source_frames == 0 || source_rate == 0 {
        return Err(AudioError::Backend("invalid music timing".into()));
    }
    slots
        .fetch_update(Ordering::AcqRel, Ordering::Acquire, |count| {
            (count < 2).then_some(count + 1)
        })
        .map_err(|_| AudioError::Backend("both music decoder slots are occupied".into()))?;
    let shared = Arc::new(Shared {
        frames: ArrayQueue::new(output_rate as usize * BUFFER_SECONDS),
        ready: AtomicBool::new(false),
        eof: AtomicBool::new(false),
        failed: AtomicBool::new(false),
        cancel: AtomicBool::new(false),
        started: AtomicBool::new(false),
        ended: AtomicBool::new(false),
        position: AtomicU64::new(0),
        audible: AtomicU64::new(0),
        underruns: AtomicU64::new(0),
        error: Mutex::new(None),
        rate: output_rate,
    });
    let producer = shared.clone();
    let worker = thread::Builder::new()
        .name("dogwood-vorbis".into())
        .spawn(move || {
            if let Err(error) = source().map_err(|e| e.to_string()).and_then(|reader| decode(reader, source_frames, source_rate, &producer)) {
                if let Ok(mut message) = producer.error.lock() {
                    *message = Some(error);
                }
                producer.failed.store(true, Ordering::Release);
            }
            producer.eof.store(true, Ordering::Release);
            producer.ready.store(true, Ordering::Release);
        })
        .map_err(|e| {
            slots.fetch_sub(1, Ordering::AcqRel);
            AudioError::Backend(e.to_string())
        })?;
    Ok(MusicStream(Arc::new(Worker {
        shared,
        thread: Mutex::new(Some(worker)),
        slots,
    })))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn slots() -> Arc<AtomicUsize> {
        Arc::new(AtomicUsize::new(0))
    }
    fn wait_ready(stream: &MusicStream) {
        let until = std::time::Instant::now() + Duration::from_secs(10);
        while !stream.ready() {
            assert!(
                std::time::Instant::now() < until,
                "decoder readiness timeout"
            );
            thread::sleep(Duration::from_millis(1));
        }
    }
    fn voice(shared: Arc<Shared>) -> MusicVoice {
        MusicVoice {
            shared,
            after: None,
            gain: 0.4,
            fade_frames: 0,
            age: 0,
            fade_out: None,
            armed: true,
        }
    }
    #[test]
    fn missing_file_is_async_error_and_decoder_count_is_bounded() {
        let slots = slots();
        let a = prepare_stream(
            Path::new("missing-music.ogg"),
            100,
            48000,
            48000,
            slots.clone(),
        )
        .unwrap();
        let b = prepare_stream(
            Path::new("missing-music.ogg"),
            100,
            48000,
            48000,
            slots.clone(),
        )
        .unwrap();
        assert!(
            prepare_stream(
                Path::new("missing-music.ogg"),
                100,
                48000,
                48000,
                slots.clone()
            )
            .is_err()
        );
        wait_ready(&a);
        assert!(a.status().failed);
        assert!(a.error().is_some());
        drop(a);
        drop(b);
        assert_eq!(slots.load(Ordering::Acquire), 0);
    }
    #[test]
    fn sample_clock_fade_and_pending_start_are_exact() {
        // A failed worker supplies an empty completed queue; tests replace it with known PCM.
        let a = prepare_stream(Path::new("missing-music.ogg"), 100, 48000, 48000, slots()).unwrap();
        let b = prepare_stream(Path::new("missing-music.ogg"), 100, 48000, 48000, slots()).unwrap();
        wait_ready(&a);
        wait_ready(&b);
        for stream in [&a, &b] {
            for _ in 0..100 {
                stream.0.shared.frames.push([0.5, -0.25]).unwrap();
            }
        }
        let mut old = voice(a.0.shared.clone());
        old.fade_out = Some((10, 10));
        let mut new = voice(b.0.shared.clone());
        new.after = Some((a.0.shared.clone(), 10));
        new.fade_frames = 10;
        for frame in 0..30 {
            old.begin_frame();
            new.begin_frame();
            let x = old.sample(true);
            let y = new.sample(true);
            if frame < 10 {
                assert_eq!(y, [0.0; 2]);
                assert!(!b.status().started);
            }
            assert!(x[0].is_finite() && y[0].is_finite());
            assert!(x[0].abs() + y[0].abs() <= 0.4);
        }
        assert!(a.status().ended);
        assert!(b.status().started);
        assert_eq!(a.status().position_seconds, 20.0 / 48000.0);
        assert_eq!(b.status().position_seconds, 20.0 / 48000.0);
    }
    #[test]
    fn zero_fade_loop_boundary_has_neither_gap_nor_double_frame() {
        let a = prepare_stream(Path::new("missing-music.ogg"), 4, 48000, 48000, slots()).unwrap();
        let b = prepare_stream(Path::new("missing-music.ogg"), 4, 48000, 48000, slots()).unwrap();
        wait_ready(&a);
        wait_ready(&b);
        for _ in 0..4 {
            a.0.shared.frames.push([0.1; 2]).unwrap();
            b.0.shared.frames.push([0.2; 2]).unwrap();
        }
        let mut old = voice(a.0.shared.clone());
        old.fade_out = Some((4, 0));
        let mut next = voice(b.0.shared.clone());
        next.after = Some((a.0.shared.clone(), 4));
        let mut mixer = Mixer {
            voices: Vec::new(),
            master_volume: 1.0,
            counter: 0,
            buses: [1.0; 2],
            music: [Some(old), Some(next)],
        };
        let mut output = [0.0f32; 16];
        mixer.mix(&mut output, 2);
        for (i, frame) in output.chunks_exact(2).enumerate() {
            let expected = if i < 4 { 0.04 } else { 0.08 };
            assert!(
                (frame[0] - expected).abs() < 1e-6,
                "frame {i} is {:?}",
                frame
            );
        }
    }

    #[test]
    fn pending_cancel_restores_predecessor_but_started_target_is_preserved() {
        let a = prepare_stream(Path::new("missing-music.ogg"), 100, 48000, 48000, slots()).unwrap();
        let b = prepare_stream(Path::new("missing-music.ogg"), 100, 48000, 48000, slots()).unwrap();
        wait_ready(&a);
        wait_ready(&b);
        let mut old = voice(a.0.shared.clone());
        old.fade_out = Some((50, 10));
        let mut next = voice(b.0.shared.clone());
        next.after = Some((a.0.shared.clone(), 50));
        let mut mixer = Mixer {
            voices: Vec::new(),
            master_volume: 1.0,
            counter: 0,
            buses: [1.0; 2],
            music: [Some(old), Some(next)],
        };
        assert!(cancel_locked(&mut mixer, &b.0.shared, true));
        assert!(mixer.music[0].as_ref().unwrap().fade_out.is_none());
        assert!(b.0.shared.cancel.load(Ordering::Acquire));
        b.0.shared.cancel.store(false, Ordering::Release);
        b.0.shared.ended.store(false, Ordering::Release);
        b.0.shared.started.store(true, Ordering::Release);
        mixer.music[0].as_mut().unwrap().fade_out = Some((50, 10));
        assert!(!cancel_locked(&mut mixer, &b.0.shared, true));
        assert!(!b.0.shared.cancel.load(Ordering::Acquire));
        assert_eq!(mixer.music[0].as_ref().unwrap().fade_out, Some((50, 10)));
    }

    /// Real default-device smoke test; verifies callback positions and a scheduled handoff.
    #[test]
    #[ignore = "opens real audio device"]
    fn real_device_streams_and_crossfades() {
        let audio = Audio::new().expect("default output device");
        audio.set_master_volume(0.2);
        audio.set_bus_volume(Bus::Music, 0.25);
        let root = std::path::PathBuf::from(std::env::var_os("DOGWOOD_TEST_MUSIC_DIR").expect("set DOGWOOD_TEST_MUSIC_DIR to the authored music fixture directory"));
        let a = audio
            .prepare_music(root.join("01-the-calm-before.ogg"), 8640000, 48000)
            .unwrap();
        wait_ready(&a);
        assert!(audio.transition_music(&a, None, 0.1, 0.4));
        thread::sleep(Duration::from_secs(1));
        assert!(a.status().started && a.status().position_seconds > 0.5);
        let b = audio
            .prepare_music(root.join("15-getting-ugly.ogg"), 7936000, 48000)
            .unwrap();
        wait_ready(&b);
        let at = a.status().position_seconds + 0.5;
        assert!(audio.transition_music(&b, Some((&a, at)), 0.4, 0.4));
        thread::sleep(Duration::from_secs(2));
        assert!(a.status().ended && b.status().started && b.status().position_seconds > 0.5);
        assert_eq!(a.status().underruns, 0);
        assert_eq!(b.status().underruns, 0);
        audio.stop_music();
        assert!(b.status().ended);
        println!("real device: old {:?}, target {:?}", a.status(), b.status());
    }

    /// Full authored soundtrack through background decoder, resampler, and actual mixer.
    /// Explicit because it decodes 42 minutes of music; no output device is opened.
    #[test]
    #[ignore = "full soundtrack technical validation"]
    fn all_runtime_ogg_decode_and_mix_with_exact_duration() {
        let root = std::path::PathBuf::from(std::env::var_os("DOGWOOD_TEST_MUSIC_DIR").expect("set DOGWOOD_TEST_MUSIC_DIR to the authored music fixture directory"));
        let manifest: serde_json::Value =
            serde_json::from_slice(&std::fs::read(root.join("manifest.json")).unwrap()).unwrap();
        let tracks = manifest["tracks"].as_array().unwrap();
        assert_eq!(tracks.len(), 15);
        for (index, track) in tracks.iter().enumerate() {
            let frames = track["frames"].as_u64().unwrap();
            let source_rate = track["sample_rate"].as_u64().unwrap() as u32;
            // Validate resampling as well as native stereo; alternating output rates.
            let output_rate = if index % 2 == 0 { 44100 } else { 48000 };
            let stream = prepare_stream(
                &root.join(track["file"].as_str().unwrap()),
                frames,
                source_rate,
                output_rate,
                slots(),
            )
            .unwrap();
            wait_ready(&stream);
            assert!(!stream.status().failed, "{:?}", stream.error());
            let mut mixer = Mixer {
                voices: Vec::new(),
                master_volume: 0.8,
                counter: 0,
                buses: [1.0, 0.8],
                music: [Some(voice(stream.0.shared.clone())), None],
            };
            let mut output = [0.0f32; 2048];
            let mut energy = [0.0f64; 2];
            let mut stereo_difference = 0.0f64;
            while !stream.status().ended {
                // Offline renderer goes faster than realtime: wait for producer without
                // intentionally testing starvation, while retaining the same callback path.
                while stream.0.shared.frames.len() < 1024
                    && !stream.0.shared.eof.load(Ordering::Acquire)
                {
                    thread::sleep(Duration::from_micros(100));
                }
                mixer.mix(&mut output, 2);
                for frame in output.chunks_exact(2) {
                    assert!(frame.iter().all(|v| v.is_finite() && v.abs() <= 1.0));
                    energy[0] += (frame[0] as f64).powi(2);
                    energy[1] += (frame[1] as f64).powi(2);
                    stereo_difference += (frame[0] as f64 - frame[1] as f64).powi(2);
                }
                assert!(
                    !stream.status().failed,
                    "{} {:?}",
                    index + 1,
                    stream.error()
                );
            }
            let expected = ((frames as u128 * output_rate as u128) / source_rate as u128) as u64;
            assert_eq!(
                stream.0.shared.position.load(Ordering::Acquire),
                expected,
                "track {}",
                index + 1
            );
            assert_eq!(stream.status().underruns, 0);
            assert!(energy[0] > 1.0 && energy[1] > 1.0 && stereo_difference > 1.0);
            println!(
                "track {:02}: {} output frames @ {}, stereo energy {:?}, no underruns",
                index + 1,
                expected,
                output_rate,
                energy
            );
        }
    }
}
