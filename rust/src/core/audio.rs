use crate::core::settings::VolumeSettings;
use crate::core::units::Seconds;
use godot::classes::{
    AudioServer, AudioStream, AudioStreamMp3, AudioStreamOggVorbis, AudioStreamPlayer,
    AudioStreamWav, Node, audio_stream_wav,
};
use godot::global::linear_to_db;
use godot::prelude::*;
use std::cell::Cell;
use std::io::Cursor;
use std::rc::Rc;

/// The game's audio buses. Music and sound effects each get their own so
/// the volume settings apply live to everything playing on them.
pub const MUSIC_BUS: &str = "Music";
pub const SFX_BUS: &str = "Sfx";

/// Creates the game's buses once; safe to call again.
pub fn ensure_buses() {
    let mut server = AudioServer::singleton();
    for name in [MUSIC_BUS, SFX_BUS] {
        if server.get_bus_index(name) < 0 {
            let index = server.get_bus_count();
            server.add_bus();
            server.set_bus_name(index, name);
        }
    }
}

/// Applies the volume settings to the buses: `master` on the master bus,
/// the rest on their own.
pub fn apply_volumes(volume: &VolumeSettings) {
    let mut server = AudioServer::singleton();
    let mut set = |name: &str, linear: f32| {
        let index = server.get_bus_index(name);
        if index >= 0 {
            server.set_bus_volume_db(index, linear_to_db(linear as f64) as f32);
        }
    };
    set("Master", volume.master);
    set(MUSIC_BUS, volume.music);
    set(SFX_BUS, volume.sfx);
}

#[derive(Debug, Clone, Copy)]
pub struct SoundOptions {
    pub timeline: SoundTimeline,
    pub paused: bool,
    /// Muting silences the channel without touching its bus.
    pub muted: bool,
    /// The bus the sound plays on.
    pub bus: &'static str,
}

impl Default for SoundOptions {
    fn default() -> SoundOptions {
        SoundOptions {
            timeline: SoundTimeline::WholeFile,
            paused: false,
            muted: false,
            bus: SFX_BUS,
        }
    }
}

/// How playback traverses the sound's own timeline.
#[derive(Debug, Clone, Copy)]
pub enum SoundTimeline {
    /// The whole file, once, from the top.
    WholeFile,
    /// The whole file, once, from this position.
    From(Seconds),
    /// This `[start, start+length)` window, looping forever — a looping
    /// window never finishes.
    LoopWindow { start: Seconds, length: Seconds },
}

/// One playing sound: an [`AudioStreamPlayer`] owned by this handle and
/// parented under the opener. The owner calls [`poll`](SoundChannel::poll)
/// every frame — it starts playback once the player has entered the tree,
/// wraps loop windows, and keeps the finish flag honest — and dropping the
/// channel stops and frees the player.
pub struct SoundChannel {
    player: Gd<AudioStreamPlayer>,
    timeline: SoundTimeline,
    muted: bool,
    paused: bool,
    /// Set by the player's `finished` signal; loop windows rearm instead.
    finished: Rc<Cell<bool>>,
    /// Playback began (playback can only start inside the scene tree, and
    /// an initially paused channel holds at its start).
    started: bool,
}

impl SoundChannel {
    /// Decodes `bytes` (by `file_name`'s extension: ogg, mp3, or wav) and
    /// starts playback under `parent`.
    pub fn open(
        parent: &mut Node,
        bytes: &[u8],
        file_name: &str,
        options: SoundOptions,
    ) -> Result<SoundChannel, String> {
        let extension = file_name.rsplit('.').next().unwrap_or_default();
        let stream: Gd<AudioStream> = match extension.to_lowercase().as_str() {
            "ogg" => AudioStreamOggVorbis::load_from_buffer(&PackedByteArray::from(bytes))
                .ok_or("failed to decode ogg")?
                .upcast(),
            "mp3" => AudioStreamMp3::load_from_buffer(&PackedByteArray::from(bytes))
                .ok_or("failed to decode mp3")?
                .upcast(),
            "wav" => wav_stream(bytes)?.upcast(),
            other => return Err(format!("unsupported sound format {other:?}")),
        };
        Self::open_stream(parent, &stream, options)
    }

    /// Starts a pre-rendered mono 16-bit PCM buffer (the tick track).
    pub fn open_pcm(
        parent: &mut Node,
        samples: &[i16],
        sample_rate: u32,
        options: SoundOptions,
    ) -> Result<SoundChannel, String> {
        let stream = pcm_stream(samples, sample_rate);
        Self::open_stream(parent, &stream.upcast(), options)
    }

    fn open_stream(
        parent: &mut Node,
        stream: &Gd<AudioStream>,
        options: SoundOptions,
    ) -> Result<SoundChannel, String> {
        let mut player = AudioStreamPlayer::new_alloc();
        player.set_stream(stream);
        player.set_bus(options.bus);
        parent.add_child(&player);

        let finished = Rc::new(Cell::new(false));
        let flag = Rc::clone(&finished);
        player.signals().finished().connect(move || flag.set(true));

        let mut channel = SoundChannel {
            player,
            timeline: options.timeline,
            muted: options.muted,
            paused: options.paused,
            finished,
            started: false,
        };
        channel.apply_gain();
        channel.ensure_started();
        Ok(channel)
    }

    /// Starts the underlying player once it can (inside the tree),
    /// honoring the requested pause state.
    fn ensure_started(&mut self) {
        if self.started || !self.player.is_inside_tree() {
            return;
        }
        let start = match self.timeline {
            SoundTimeline::WholeFile => 0.0,
            SoundTimeline::From(start) | SoundTimeline::LoopWindow { start, .. } => {
                start.0.max(0.0)
            }
        };
        self.player.play_ex().from_position(start as f32).done();
        if self.paused {
            self.player.set_stream_paused(true);
        }
        self.started = true;
    }

    /// Starts pending playback, wraps loop windows, and restarts a
    /// windowed file that ran out early; the owner calls this every frame.
    pub fn poll(&mut self) {
        self.ensure_started();
        let SoundTimeline::LoopWindow { start, length } = self.timeline else {
            return;
        };
        let start = start.0.max(0.0);
        let end = start + length.0.max(0.0);
        if self.finished.get() {
            // The file ended inside the window; loop back to its start.
            self.finished.set(false);
            self.player.play_ex().from_position(start as f32).done();
            return;
        }
        let position = self.player.get_playback_position() as f64;
        if position >= end {
            let wrapped = start + (position - end).rem_euclid(length.0.max(1e-6));
            self.player.seek(wrapped as f32);
        }
    }

    /// Whether the sound is decoded and playback obeys this channel;
    /// buffers decode synchronously, so an open channel is always ready.
    pub fn is_ready(&self) -> bool {
        true
    }

    pub fn set_paused(&mut self, paused: bool) {
        self.paused = paused;
        if self.started {
            self.player.set_stream_paused(paused);
        } else if !paused {
            self.ensure_started();
        }
    }

    pub fn set_muted(&mut self, muted: bool) {
        self.muted = muted;
        self.apply_gain();
    }

    pub fn is_muted(&self) -> bool {
        self.muted
    }

    /// Seconds into the sound's own timeline, on the mixer-queue clock:
    /// the player's position plus what the mixer consumed since its last
    /// report. Runs ahead of the speakers by the output latency, which the
    /// timing settings compensate for.
    pub fn position(&self) -> Seconds {
        if !self.started {
            let start = match self.timeline {
                SoundTimeline::WholeFile => 0.0,
                SoundTimeline::From(start) | SoundTimeline::LoopWindow { start, .. } => {
                    start.0.max(0.0)
                }
            };
            return Seconds(start);
        }
        let mut position = self.player.get_playback_position() as f64;
        if self.player.is_playing() && !self.player.get_stream_paused() {
            position += AudioServer::singleton().get_time_since_last_mix();
        }
        Seconds(position)
    }

    /// The sound ran out; looping windows never finish.
    pub fn is_finished(&self) -> bool {
        match self.timeline {
            SoundTimeline::LoopWindow { .. } => false,
            _ => self.finished.get(),
        }
    }

    fn apply_gain(&mut self) {
        let volume = if self.muted { -80.0 } else { 0.0 };
        self.player.set_volume_db(volume);
    }
}

impl Drop for SoundChannel {
    /// The player usually dies with the opener's subtree before this handle
    /// drops; only a still-live player needs freeing.
    fn drop(&mut self) {
        if self.player.is_instance_valid() {
            self.player.queue_free();
        }
    }
}

/// Decodes a WAV via hound into an [`AudioStreamWav`] — deterministic
/// across platforms, and the same decoding the tick-track renderer uses.
pub fn wav_stream(bytes: &[u8]) -> Result<Gd<AudioStreamWav>, String> {
    let mut reader = hound::WavReader::new(Cursor::new(bytes)).map_err(|e| e.to_string())?;
    let spec = reader.spec();
    let samples: Vec<i16> = match spec.sample_format {
        hound::SampleFormat::Int => {
            let shift = spec.bits_per_sample.saturating_sub(16) as u32;
            reader
                .samples::<i32>()
                .map(|sample| sample.map(|sample| (sample >> shift) as i16))
                .collect::<Result<_, _>>()
                .map_err(|e| e.to_string())?
        }
        hound::SampleFormat::Float => reader
            .samples::<f32>()
            .map(|sample| sample.map(|sample| (sample.clamp(-1.0, 1.0) * i16::MAX as f32) as i16))
            .collect::<Result<_, _>>()
            .map_err(|e| e.to_string())?,
    };
    let mut data = Vec::with_capacity(samples.len() * 2);
    for sample in samples {
        data.extend_from_slice(&sample.to_le_bytes());
    }
    let mut stream = AudioStreamWav::new_gd();
    stream.set_data(&PackedByteArray::from(data.as_slice()));
    stream.set_format(audio_stream_wav::Format::FORMAT_16_BITS);
    stream.set_mix_rate(spec.sample_rate as i32);
    stream.set_stereo(spec.channels >= 2);
    Ok(stream)
}

fn pcm_stream(samples: &[i16], sample_rate: u32) -> Gd<AudioStreamWav> {
    let mut data = Vec::with_capacity(samples.len() * 2);
    for sample in samples {
        data.extend_from_slice(&sample.to_le_bytes());
    }
    let mut stream = AudioStreamWav::new_gd();
    stream.set_data(&PackedByteArray::from(data.as_slice()));
    stream.set_format(audio_stream_wav::Format::FORMAT_16_BITS);
    stream.set_mix_rate(sample_rate as i32);
    stream.set_stereo(false);
    stream
}
