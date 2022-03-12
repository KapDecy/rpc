#![feature(thread_is_running)]
#![feature(cell_update)]
// mod source_handler;
// mod stream;
mod source_handler;
mod timer;

use crossterm::style::Stylize;
use portaudio::{self as pa, NonBlocking, Output, Stream};
use source_handler::new_source_handle;
use timer::*;

use crossbeam::channel::{self, Sender};
use std::{
    sync::{
        atomic::{AtomicU8, Ordering},
        Arc,
    },
    thread::JoinHandle,
    time::Duration,
};

use itertools::Itertools;
use symphonia::core::codecs::{DecoderOptions, CODEC_TYPE_NULL};
use symphonia::core::errors::Error;
use symphonia::core::formats::FormatOptions;
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;
use symphonia::core::{audio::Signal, meta::Tag};

pub enum SourceControl {
    Stop,
}

pub struct Streamer {
    pub stream: Stream<NonBlocking, Output<f32>>,
    pub source_handler: JoinHandle<()>,
    pub volume: Arc<AtomicU8>,
    pub sample_tx: Sender<f32>,
    pub control_tx: Sender<SourceControl>,
}

pub struct SongMetadata {
    pub full_time_secs: Option<u64>,
    pub path: String,
    pub title: Option<String>,
    pub artist: Option<String>,
}

pub struct Current {
    pub streamer: Streamer,
    pub metadata: SongMetadata,
    pub timer: Timer,
}

impl Current {
    pub fn new(path: String, volume: Arc<AtomicU8>, paused_on_start: bool) -> Self {
        let src = std::fs::File::open(path.trim().trim_matches('"')).unwrap();
        let mss = MediaSourceStream::new(Box::new(src), Default::default());
        let hint = Hint::new();
        let meta_opts: MetadataOptions = Default::default();
        let fmt_opts: FormatOptions = Default::default();
        let probed = symphonia::default::get_probe()
            .format(&hint, mss, &fmt_opts, &meta_opts)
            .expect("unsupported format");

        let mut format = probed.format;
        let meta = match format.metadata().current() {
            Some(data) => data.tags().to_owned(),
            None => vec![],
        };
        let track = format.default_track().unwrap();
        let full_time_secs =
            track.codec_params.n_frames.unwrap() / track.codec_params.sample_rate.unwrap() as u64;

        let (stream, sample_tx) = make_stream(
            track.codec_params.channels.unwrap().count() as i32,
            track.codec_params.sample_rate.unwrap() as f64,
            volume.clone(),
            paused_on_start,
        )
        .unwrap();
        let (control_tx, control_rx) = channel::bounded(3);
        let r = path.clone();

        let mut cur = Current {
            streamer: Streamer {
                stream,
                source_handler: new_source_handle(
                    format,
                    Duration::ZERO,
                    sample_tx.clone(),
                    control_rx,
                ),
                volume,
                sample_tx,
                control_tx,
            },
            metadata: SongMetadata {
                full_time_secs: Some(full_time_secs),
                path,
                title: None,
                artist: None,
            },
            timer: Timer::new(),
        };
        for tag in meta {
            if let Some(skey) = tag.std_key {
                match skey {
                    symphonia::core::meta::StandardTagKey::Artist => {
                        cur.metadata.artist = Some(tag.value.to_string())
                    }
                    symphonia::core::meta::StandardTagKey::TrackTitle => {
                        cur.metadata.title = Some(tag.value.to_string())
                    }
                    _ => (),
                }
            } else {
                match tag.key.to_uppercase().as_str() {
                    "TITLE" => cur.metadata.title = Some(tag.value.to_string()),
                    "ARTIST" => cur.metadata.artist = Some(tag.value.to_string()),
                    _ => (),
                }
            }
        }
        // TODO разобраться с метадатой
        // ниже затычка
        if cur.metadata.title == None {
            cur.metadata.title = Some(
                std::path::Path::new(&r)
                    .file_stem()
                    .unwrap()
                    .to_str()
                    .unwrap()
                    .to_string(),
            );
        }
        cur
    }

    fn seek(&mut self, dur: Duration) -> Option<Self> {
        self.streamer.control_tx.send(SourceControl::Stop).unwrap();
        if dur > Duration::from_secs(self.metadata.full_time_secs.unwrap()) {
            return None;
        }
        let src = std::fs::File::open(self.metadata.path.trim().trim_matches('"')).unwrap();
        let mss = MediaSourceStream::new(Box::new(src), Default::default());
        let hint = Hint::new();
        let meta_opts: MetadataOptions = Default::default();
        let fmt_opts: FormatOptions = Default::default();
        let probed = symphonia::default::get_probe()
            .format(&hint, mss, &fmt_opts, &meta_opts)
            .expect("unsupported format");

        let mut format = probed.format;
        let meta = match format.metadata().current() {
            Some(data) => data.tags().to_owned(),
            None => vec![],
        };
        let track = format.default_track().unwrap();
        let full_time_secs =
            track.codec_params.n_frames.unwrap() / track.codec_params.sample_rate.unwrap() as u64;

        let (stream, sample_tx) = make_stream(
            track.codec_params.channels.unwrap().count() as i32,
            track.codec_params.sample_rate.unwrap() as f64,
            self.streamer.volume.clone(),
            self.streamer.stream.is_stopped().unwrap(),
        )
        .unwrap();
        let (control_tx, control_rx) = channel::bounded(3);
        let r = self.metadata.path.clone();

        let mut cur = Current {
            streamer: Streamer {
                stream,
                source_handler: new_source_handle(format, dur, sample_tx.clone(), control_rx),
                volume: self.streamer.volume.clone(),
                sample_tx,
                control_tx,
            },
            metadata: SongMetadata {
                full_time_secs: Some(full_time_secs),
                path: self.metadata.path.clone(),
                title: None,
                artist: None,
            },
            timer: self.timer.clone(),
        };
        for tag in meta {
            if let Some(skey) = tag.std_key {
                match skey {
                    symphonia::core::meta::StandardTagKey::Artist => {
                        cur.metadata.artist = Some(tag.value.to_string())
                    }
                    symphonia::core::meta::StandardTagKey::TrackTitle => {
                        cur.metadata.title = Some(tag.value.to_string())
                    }
                    _ => (),
                }
            } else {
                match tag.key.to_uppercase().as_str() {
                    "TITLE" => cur.metadata.title = Some(tag.value.to_string()),
                    "ARTIST" => cur.metadata.artist = Some(tag.value.to_string()),
                    _ => (),
                }
            }
        }
        // TODO разобраться с метадатой
        // ниже затычка
        if cur.metadata.title == None {
            cur.metadata.title = Some(
                std::path::Path::new(&r)
                    .file_stem()
                    .unwrap()
                    .to_str()
                    .unwrap()
                    .to_string(),
            );
        }
        Some(cur)
    }

    pub fn seek_forward(&mut self, dur: Duration) -> Option<Self> {
        self.timer += dur;
        self.seek(Duration::from_secs(self.timer.as_secs()))
    }

    pub fn seek_backward(&mut self, dur: Duration) -> Option<Self> {
        self.timer -= dur;
        self.seek(Duration::from_secs(self.timer.as_secs()))
    }
}

// impl Current {
//     pub fn seek_to(&mut self, dur: Duration) {
//         self.streamer.control_tx.send(SourceControl::SeekTo(dur)).unwrap();
//     }

//     pub fn seek_forward(&mut self)
// }

pub enum InputMode {
    Normal,
    AddTrack,
}
pub struct Ui {
    pub ui_state: InputMode,
    pub add_track: bool,
    pub paused: bool,
    pub cursor: u16,
    pub tmp_add_track: Vec<char>,
}

pub struct Rpc {
    pub ui: Ui,
    pub current: Option<Current>,
    pub queue: Vec<String>,
    pub library: Vec<SongMetadata>,
    pub volume: Arc<AtomicU8>,
}

fn make_stream(
    channels: i32,
    sample_rate: f64,
    volume: Arc<AtomicU8>,
    paused: bool,
) -> Result<(Stream<NonBlocking, Output<f32>>, channel::Sender<f32>), pa::Error> {
    let pa = pa::PortAudio::new()?;

    let settings = pa.default_output_stream_settings(channels, sample_rate, 1)?;
    // we won't output out of range samples so don't bother clipping them.

    // This routine will be called by the PortAudio engine when audio is needed. It may called at
    // interrupt level on some machines so don't do anything that could mess up the system like
    // dynamic resource allocation or IO.
    let (tx, rx) = channel::bounded(4096);
    let callback = move |pa::OutputStreamCallbackArgs {
                             buffer, frames: _, ..
                         }| {
        buffer[0] = rx.try_recv().unwrap_or(0.0) / 2000000000.0
            * (volume.load(std::sync::atomic::Ordering::Relaxed) as f32 / 100.0);
        buffer[1] = rx.try_recv().unwrap_or(0.0) / 2000000000.0
            * (volume.load(std::sync::atomic::Ordering::Relaxed) as f32 / 100.0);

        pa::Continue
    };

    let mut stream = pa.open_non_blocking_stream(settings, callback)?;
    if !paused {
        stream.start().unwrap();
    }

    Ok((stream, tx))
}

impl Rpc {
    pub fn new() -> Self {
        Rpc {
            ui: Ui {
                ui_state: InputMode::Normal,
                add_track: false,
                paused: false,
                cursor: 0,
                tmp_add_track: vec![],
            },
            current: None,
            queue: vec![],
            library: vec![],
            volume: Arc::new(AtomicU8::new(50)),
        }
    }

    pub fn volume(&self) -> u8 {
        self.volume.load(Ordering::Relaxed)
    }

    pub fn set_volume(&mut self, volume: i8) {
        if volume > 100 {
            self.volume.store(100, Ordering::Relaxed)
        } else if volume < 0 {
            self.volume.store(0, Ordering::Relaxed)
        } else {
            self.volume.store(volume as u8, Ordering::Relaxed)
        }
    }

    pub fn time_as_secs(&mut self) -> u64 {
        // self.current..timer.as_secs()
        if let Some(cur) = &mut self.current {
            cur.timer.as_secs()
        } else {
            0
        }
    }
}

impl Default for Rpc {
    fn default() -> Self {
        Self::new()
    }
}
