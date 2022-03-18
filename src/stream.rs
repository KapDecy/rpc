use cpal::{
    traits::{DeviceTrait, StreamTrait},
    Device, StreamConfig,
};
use crossbeam::channel::{self, Receiver, Sender};
use log::info;
use std::{
    sync::{
        atomic::{AtomicU8, Ordering},
        Arc,
    },
    thread::JoinHandle,
    time::Duration,
};

use symphonia::core::formats::FormatOptions;
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;

use audiotags::Tag;

use crate::{source_handler::new_source_handle, timer::Timer};

pub enum SourceControl {
    None,
    Stop,
}

pub struct Streamer {
    pub stream: Box<dyn StreamTrait>,
    pub paused: bool,
    pub stream_config: StreamConfig,
    pub source_handler: JoinHandle<()>,
    pub volume: Arc<AtomicU8>,
    pub sample_tx: Sender<f32>,
    pub control_tx: Sender<SourceControl>,
    pub device: Arc<Device>,
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
    pub fn new(
        path: String,
        volume: Arc<AtomicU8>,
        paused_on_start: bool,
        device: Arc<cpal::Device>,
    ) -> Self {
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

        let (stream, sample_tx, stream_config) =
            stream_setup(volume.clone(), paused_on_start, device.clone());
        let (control_tx, control_rx) = channel::bounded(3);
        let r = path.to_string();
        let tag = Tag::new().read_from_path(path).unwrap();

        let mut cur = Current {
            streamer: Streamer {
                stream: Box::new(stream),
                paused: paused_on_start,
                stream_config: stream_config.clone(),
                source_handler: new_source_handle(
                    format,
                    Duration::ZERO,
                    sample_tx.clone(),
                    control_rx,
                    stream_config,
                ),
                volume,
                sample_tx,
                control_tx,
                device,
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

        let (stream, sample_tx, stream_config) = stream_setup(
            self.streamer.volume.clone(),
            self.streamer.paused,
            self.streamer.device.clone(),
        );
        let (control_tx, control_rx) = channel::bounded(3);
        let mut cur: _ = Current {
            streamer: Streamer {
                stream: Box::new(stream),
                source_handler: new_source_handle(
                    format,
                    dur,
                    sample_tx.clone(),
                    control_rx,
                    stream_config.clone(),
                ),
                volume: self.streamer.volume.clone(),
                sample_tx,
                control_tx,
                paused: self.streamer.paused,
                stream_config,
                device: self.streamer.device.clone(),
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

    pub fn change_device(&mut self, device: Arc<Device>) -> Option<Self> {
        self.streamer.control_tx.send(SourceControl::Stop).unwrap();
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

        let (stream, sample_tx, stream_config) = stream_setup(
            self.streamer.volume.clone(),
            self.streamer.paused,
            device.clone(),
        );
        let (control_tx, control_rx) = channel::bounded(3);
        let mut cur: _ = Current {
            streamer: Streamer {
                stream: Box::new(stream),
                source_handler: new_source_handle(
                    format,
                    Duration::from_secs(self.timer.as_secs() - 1),
                    sample_tx.clone(),
                    control_rx,
                    stream_config.clone(),
                ),
                volume: self.streamer.volume.clone(),
                sample_tx,
                control_tx,
                paused: self.streamer.paused,
                stream_config,
                device,
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
        info!(
            "tx discon: {}",
            cur.streamer
                .control_tx
                .try_send(SourceControl::None)
                .is_ok()
        );
        Some(cur)
    }
}

pub fn stream_setup(
    volume: Arc<AtomicU8>,
    paused: bool,
    device: Arc<Device>,
) -> (cpal::Stream, channel::Sender<f32>, cpal::StreamConfig) {
    let config = host_device_setup(device.clone()).unwrap();

    match config.sample_format() {
        cpal::SampleFormat::F32 => {
            stream_make::<f32>(&device, &config.into(), volume, paused).unwrap()
        }
        cpal::SampleFormat::I16 => {
            stream_make::<i16>(&device, &config.into(), volume, paused).unwrap()
        }
        cpal::SampleFormat::U16 => {
            stream_make::<u16>(&device, &config.into(), volume, paused).unwrap()
        }
    }
}

fn stream_make<T>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    volume: Arc<AtomicU8>,
    paused: bool,
) -> Result<(cpal::Stream, Sender<f32>, cpal::StreamConfig), anyhow::Error>
where
    T: cpal::Sample,
{
    // let (tx, rx) = sync_channel(config.sample_rate.0 as usize + 1);
    let (tx, rx) = channel::bounded((config.sample_rate.0 as usize) / 4);

    let err_fn = |err| eprintln!("Error building output sound stream: {}", err);

    let stream = device
        .build_output_stream(
            config,
            move |output: &mut [T], _: &cpal::OutputCallbackInfo| {
                on_window(output, &rx, volume.clone())
            },
            err_fn,
        )
        .unwrap();

    if paused {
        stream.pause().unwrap();
    } else {
        stream.play().unwrap();
    }

    Ok((stream, tx, config.clone()))
}

fn host_device_setup(device: Arc<Device>) -> Result<cpal::SupportedStreamConfig, anyhow::Error> {
    // let host = cpal::default_host();

    // let device = host
    //     .default_output_device()
    //     .ok_or_else(|| anyhow::Error::msg("Default output device is not available"))
    //     .unwrap();
    // println!("Output device : {}", device.name()?);

    let config = device.default_output_config().unwrap();
    // println!("Default output config : {:?}", config);

    Ok(config)
}

fn on_window<T>(output: &mut [T], request: &Receiver<f32>, volume: Arc<AtomicU8>)
where
    T: cpal::Sample,
{
    for frame in output.iter_mut() {
        let v = match request.try_recv() {
            Ok(v) => (v / 2000000000.0 * ((volume.load(Ordering::Relaxed) as f32) / 100.0)),
            Err(_) => 0.0,
        };
        let value: T = cpal::Sample::from::<f32>(
            &v, // &(on_sample(request) * ((volume.load(Ordering::Relaxed) as f32) / 100.0)),
        );
        *frame = value;
    }
}
