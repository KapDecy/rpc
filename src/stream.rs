use crossbeam::channel::{self, Sender};
use portaudio::{self as pa, NonBlocking, Output, Stream};
use std::{
    sync::{atomic::AtomicU8, Arc},
    thread::JoinHandle, time::Duration,
};

use symphonia::core::formats::FormatOptions;
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;

use audiotags::Tag;

use crate::{timer::Timer, source_handler::new_source_handle};

pub fn make_stream(
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

pub struct TrackMetadata {
    pub full_time_secs: Option<u64>,
    pub path: String,
    pub file_stem: String,
    pub title: Option<String>,
    pub artist: Option<String>,
    pub album: Option<String>,
    pub year: Option<i32>,
}

pub struct Current {
    pub streamer: Streamer,
    pub metadata: TrackMetadata,
    pub timer: Timer,
}

impl Current {
    pub fn new(path: String, volume: Arc<AtomicU8>, paused_on_start: bool) -> Self {
        let path = path.trim().trim_matches('"');
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
        let r = path.to_string();
        let tag = Tag::new().read_from_path(path).unwrap();

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
            metadata: TrackMetadata {
                full_time_secs: Some(full_time_secs),
                path: path.to_string(),
                file_stem: std::path::Path::new(&r)
                    .file_stem()
                    .unwrap()
                    .to_str()
                    .unwrap()
                    .to_string(),
                title: tag.title().map(|x| x.to_string()),
                artist: tag.artist().map(|x| x.to_string()),
                album: tag.album().map(|x| x.title.to_string()),
                year: tag.year(),
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

        if cur.metadata.title == None {
            cur.metadata.title = Some(cur.metadata.file_stem.clone());
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
        let mut cur: _ = Current {
            streamer: Streamer {
                stream,
                source_handler: new_source_handle(format, dur, sample_tx.clone(), control_rx),
                volume: self.streamer.volume.clone(),
                sample_tx,
                control_tx,
            },
            metadata: TrackMetadata {
                full_time_secs: Some(full_time_secs),
                path: self.metadata.path.clone(),
                file_stem: self.metadata.file_stem.clone(),
                title: self.metadata.title.clone(),
                artist: self.metadata.artist.clone(),
                album: self.metadata.album.clone(),
                year: self.metadata.year,
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
        if cur.metadata.title == None {
            cur.metadata.title = Some(self.metadata.file_stem.clone());
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
