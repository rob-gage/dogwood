+++
title = "Audio"
description = "Stereo sound playback, mixing buses, panning, and persistent voices through engine::audio."
weight = 110
[extra]
sources = ["engine_audio/src/lib.rs", "engine_audio/Cargo.toml", "engine/src/lib.rs"]
+++

`engine::audio` re-exports `engine_audio`. `Audio` owns a `cpal` output stream and a bounded stereo mixer; keep it alive on its creating thread.

## Loading and playback

- `Audio::new()` opens the default output device and returns `Result<Audio, AudioError>`. `AudioError::NoDevice` means no device exists.
- `Sound::from_wav(bytes, sample_rate)` decodes 16-bit PCM WAV without opening a device. Mono recordings are duplicated to stereo; stereo recordings retain their channels. Resampling is linear, and `Sound` is cheaply cloneable.
- `Audio::load_wav(bytes)` decodes a `Sound` at the device rate.
- `Audio::play(sound, volume, pitch)` starts a centered SFX one-shot at priority 128. Pitch 1.0 is normal speed.
- `Audio::start(sound, volume, pitch, pan, bus, looping, priority)` returns `Option<VoiceId>`. Pan ranges from -1 left to +1 right; `Bus::Music` is always centered. The game chooses its own panning and distance policy.
- `Audio::update_voice(id, volume, pan)` smooths a voice's gain and returns false if it ended or was stolen. `stop(id)` fades it out; `stop_bus(bus)` fades that bus's voices; `stop_all()` clears all voices immediately.
- `Audio::active_voices()` reports the current count, including fading voices.

## Mixing

`set_master_volume` scales both buses before the limiter. `set_bus_volume(Bus::Sfx, gain)` and `set_bus_volume(Bus::Music, gain)` independently affect existing and future voices. Gains are linear and clamped to 0–2; nonfinite gains become zero. Voice gain changes smooth over about 20 ms at 48 kHz; loop starts fade in from zero. Source pitch is clamped to 0.1–8.

The callback mixes at most 32 voices behind a short `Mutex`, with at most four copies of one decoded `Sound`. Global voice stealing chooses the lowest priority, then oldest voice. The copy limit replaces the oldest copy. Neither path replaces a higher-priority voice with a lower-priority request. Larger priority values take precedence; rejected requests return `None`.

Panning uses square-root left/right gains. Mono devices receive the mean stereo signal; other output channels alternate left/right. Each channel passes peaks unchanged below 0.8 of full scale and bends louder peaks toward full scale with a `tanh` knee. Supported output formats are `f32`, `i16`, `u16`, and `i32`.

> **Limitation:** Sounds are decoded in memory. Streaming, compressed audio, non-16-bit WAV decoding, automatic music scheduling, and sample-loop seam preparation are not implemented. Games prepare loop assets and schedule persistent voices.
