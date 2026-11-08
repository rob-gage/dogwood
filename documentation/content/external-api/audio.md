+++
title = "Audio"
description = "One-shot sound playback with overlapping voices through engine::audio."
weight = 110
[extra]
sources = ["engine_audio/src/lib.rs", "engine_audio/Cargo.toml", "engine/src/lib.rs"]
+++

`engine::audio` re-exports the `engine_audio` crate. It plays short one-shot sounds through the default output device using `cpal`. This page is for game programmers.

## Types

- `Audio::new()` opens the default output device and returns `Result<Audio, AudioError>`. `AudioError::NoDevice` means no device exists. `Audio` owns the output stream, so keep it alive and on its creating thread.
- `Audio::load_wav(&[u8])` decodes 16-bit PCM WAV (mono or stereo, any sample rate), mixes it to mono, and resamples it linearly to the device rate. It returns a cheaply cloneable `Sound`.
- `Audio::play(&Sound, volume, pitch)` starts a voice. `pitch` 1.0 is normal speed; other values resample the playback rate.
- `Audio::set_master_volume(volume)` scales the mix before the limiter.

## Mixing

The output callback sums up to 32 voices behind a short `Mutex`, then passes the sum unchanged below 0.8 of full scale and bends louder peaks smoothly toward full scale with a `tanh` knee. When all voices are in use, the oldest voice is dropped. Output formats `f32`, `i16`, `u16`, and `i32` are supported.

> **Limitation:** There is no streaming, looping, panning, or non-16-bit WAV decoding.
