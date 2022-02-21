#![feature(thread_is_running)]
use std::{
    iter::zip,
    sync::{
        atomic::{AtomicBool, AtomicU8, AtomicUsize, Ordering},
        mpsc::{sync_channel, Receiver, SyncSender},
        Arc, Mutex,
    },
    thread::{self, JoinHandle},
};

use cpal::{
    traits::{DeviceTrait, HostTrait, StreamTrait},
    StreamConfig,
};
use itertools::Itertools;
use rubato::{InterpolationParameters, Resampler, SincFixedIn};

use rb::*;
use symphonia::core::audio::Signal;
use symphonia::core::codecs::{DecoderOptions, CODEC_TYPE_NULL};
use symphonia::core::errors::Error;
use symphonia::core::formats::FormatOptions;
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;

enum InStreamControl {
    Pause,
    Resume,
    Stop,
}

enum StreamControl {
    Pause,
    Resume,
}

struct Stream {
    stream_control_rx: Receiver<StreamControl>,
    stream: Box<dyn StreamTrait>,
    paused: Arc<AtomicBool>,
}

impl Stream {
    fn start(
        stream_control_rx: Receiver<StreamControl>,
    ) -> (
        JoinHandle<()>,
        SyncSender<f32>,
        Arc<Mutex<StreamConfig>>,
        Arc<AtomicU8>,
        Arc<AtomicBool>,
    ) {
        let (temp_tx, temp_rx) = sync_channel(2);
        let h = thread::spawn(move || {
            let (stream, tx, config, volume) = stream_setup_for(|o| match o.try_recv() {
                Ok(v) => v / 2000000000.0,
                Err(_) => 0.0,
            })
            .unwrap();
            let paused = Arc::new(AtomicBool::new(false));
            let config = Arc::new(Mutex::new(config));
            let stream = Stream {
                stream_control_rx,
                stream: Box::new(stream),
                paused: paused.clone(),
            };
            temp_tx.send((tx, config, volume, paused)).unwrap();
            stream.stream.play().unwrap();
            loop {
                if let Ok(command) = stream.stream_control_rx.recv() {
                    match command {
                        StreamControl::Pause => {
                            stream.stream.pause().unwrap();
                            stream.paused.store(true, Ordering::SeqCst)
                        }
                        StreamControl::Resume => {
                            stream.stream.play().unwrap();
                            stream.paused.store(false, Ordering::SeqCst)
                        }
                    }
                }
            }
        });
        let (tx, config, volume, paused) = temp_rx.recv().unwrap();
        drop(temp_rx);
        (h, tx, config, volume, paused)
    }
}

enum InSourceControl {
    StopStream,
    Seek,
}

pub enum SourceResponse {}

struct SourceHandler {
    source_control_rx: Receiver<SourceControl>,
    stream_config: Arc<Mutex<StreamConfig>>,
    sample_tx: SyncSender<f32>,
    in_source_control_tx: Option<SyncSender<InSourceControl>>,
    current_source: Option<JoinHandle<()>>,
    current_timer: Arc<AtomicUsize>,
}

impl SourceHandler {
    fn start(mut self) -> JoinHandle<()> {
        thread::spawn(move || loop {
            if let Ok(control) = self.source_control_rx.recv() {
                match control {
                    SourceControl::AddTrack(track_path, skip) => {
                        let (tx, rx) = sync_channel(3);
                        self.in_source_control_tx = Some(tx);
                        let ttx = self.sample_tx.clone();
                        let stream_config = self.stream_config.lock().unwrap().clone();
                        // let mut current = Source::from_path(track_path);
                        let src = std::fs::File::open(track_path.trim().trim_matches('"')).unwrap();
                        let mss = MediaSourceStream::new(Box::new(src), Default::default());
                        let hint = Hint::new();
                        let meta_opts: MetadataOptions = Default::default();
                        let fmt_opts: FormatOptions = Default::default();
                        let probed = symphonia::default::get_probe()
                            .format(&hint, mss, &fmt_opts, &meta_opts)
                            .expect("unsupported format");
                        let mut format = probed.format;

                        let track = format
                            .tracks()
                            .iter()
                            .find(|t| t.codec_params.codec != CODEC_TYPE_NULL)
                            .expect("no supported audio tracks");
                        let dec_opts: DecoderOptions = Default::default();
                        let mut decoder = symphonia::default::get_codecs()
                            .make(&track.codec_params, &dec_opts)
                            .expect("unsupported codec");
                        let track_id = track.id;
                        // let bps = track.clone().codec_params.bits_per_sample.unwrap();
                        let sample_rate = track.clone().codec_params.sample_rate.unwrap();
                        let chans = track.clone().codec_params.channels.unwrap().count();
                        let timer = self.current_timer.clone();
                        if format
                            .seek(
                                symphonia::core::formats::SeekMode::Accurate,
                                symphonia::core::formats::SeekTo::Time {
                                    time: symphonia::core::units::Time {
                                        seconds: skip as u64,
                                        frac: 0.0,
                                    },
                                    track_id: None,
                                },
                            )
                            .is_ok()
                        {
                            timer.store(skip * 4, Ordering::Relaxed);
                        } else {
                            timer.store(0, Ordering::Relaxed);
                            continue;
                        }

                        self.current_source = Some(thread::spawn(move || {
                            let (lprod, lcon) = sync_channel(sample_rate as usize);
                            let (rprod, rcon) = sync_channel(sample_rate as usize);
                            let packetsthread = thread::spawn(move || {
                                while let Ok(packet) = format.next_packet() {
                                    while !format.metadata().is_latest() {
                                        // Pop the old head of the metadata queue.
                                        format.metadata().pop();

                                        // Consume the new metadata at the head of the metadata queue.
                                    }
                                    if packet.track_id() != track_id {
                                        continue;
                                    }
                                    match decoder.decode(&packet) {
                                        Ok(decoded) => match decoded {
                                            symphonia::core::audio::AudioBufferRef::S32(buf) => {
                                                let left = buf.chan(0).to_vec();
                                                let right = buf.chan(1).to_vec();
                                                // TODO разберись с bits_per_sample
                                                // let left = left
                                                //     .iter()
                                                //     .map(|e| match bps.cmp(&24) {
                                                //         std::cmp::Ordering::Equal => *e as f32,
                                                //         std::cmp::Ordering::Greater => {
                                                //             (*e >> (bps - 24)) as f32
                                                //         }
                                                //         std::cmp::Ordering::Less => {
                                                //             (*e << (24 - bps)) as f32
                                                //         }
                                                //     })
                                                //     .collect_vec();
                                                for e in zip(left, right) {
                                                    match lprod.send(e.0 as f32) {
                                                        Ok(_) => {}
                                                        Err(_) => break,
                                                    };
                                                    rprod.send(e.1 as f32).unwrap();
                                                }
                                            }
                                            symphonia::core::audio::AudioBufferRef::U8(buf) => {
                                                todo!()
                                            }
                                            symphonia::core::audio::AudioBufferRef::U16(buf) => {
                                                todo!()
                                            }
                                            symphonia::core::audio::AudioBufferRef::U24(buf) => {
                                                todo!()
                                            }
                                            symphonia::core::audio::AudioBufferRef::U32(buf) => {
                                                todo!()
                                            } // TODO Сделать все варианты инпутов
                                            symphonia::core::audio::AudioBufferRef::S8(buf) => {
                                                let left = buf.chan(0).to_vec();
                                                let right = buf.chan(1).to_vec();
                                                for e in zip(left, right) {
                                                    match lprod.send(((e.0 as i32) << 24) as f32) {
                                                        Ok(_) => {}
                                                        Err(_) => break,
                                                    };
                                                    rprod
                                                        .send(((e.1 as i32) << 24) as f32)
                                                        .unwrap();
                                                }
                                            }
                                            symphonia::core::audio::AudioBufferRef::S16(buf) => {
                                                let left = buf.chan(0).to_vec();
                                                let right = buf.chan(1).to_vec();
                                                for e in zip(left, right) {
                                                    match lprod.send((e.0 as i32) as f32) {
                                                        Ok(_) => {}
                                                        Err(_) => break,
                                                    };
                                                    rprod.send((e.1 as i32) as f32).unwrap();
                                                }
                                            }
                                            symphonia::core::audio::AudioBufferRef::S24(buf) => {
                                                let left = buf.chan(0).to_vec();
                                                let right = buf.chan(1).to_vec();
                                                for e in zip(left, right) {
                                                    match lprod.send((e.0.into_i32()) as f32) {
                                                        Ok(_) => {}
                                                        Err(_) => break,
                                                    };
                                                    rprod.send((e.1.into_i32()) as f32).unwrap();
                                                }
                                            }
                                            symphonia::core::audio::AudioBufferRef::F32(buf) => {
                                                let left = buf.chan(0).to_vec();
                                                let right = buf.chan(1).to_vec();
                                                for e in zip(left, right) {
                                                    match lprod.send(e.0 * 2000000000.0) {
                                                        Ok(_) => {}
                                                        Err(_) => break,
                                                    };
                                                    rprod.send(e.1 * 2000000000.0).unwrap();
                                                }
                                            }
                                            symphonia::core::audio::AudioBufferRef::F64(buf) => {
                                                let left = buf.chan(0).to_vec();
                                                let right = buf.chan(1).to_vec();
                                                for e in zip(left, right) {
                                                    match lprod.send((e.0 * 2000000000.0) as f32) {
                                                        Ok(_) => {}
                                                        Err(_) => break,
                                                    };
                                                    rprod
                                                        .send((e.1 * 2000000000.0) as f32)
                                                        .unwrap();
                                                }
                                            }
                                        },
                                        Err(Error::IoError(_)) => {
                                            // The packet failed to decode due to an IO error, skip the packet.
                                            continue;
                                        }
                                        Err(Error::DecodeError(_)) => {
                                            // The packet failed to decode due to invalid data, skip the packet.
                                            continue;
                                        }
                                        Err(err) => {
                                            // An unrecoverable error occured, halt decoding.
                                            panic!("{}", err);
                                        }
                                    }
                                }
                            });

                            // rubato
                            let params = InterpolationParameters {
                                sinc_len: 2048,
                                f_cutoff: 0.95,
                                interpolation: rubato::InterpolationType::Cubic,
                                oversampling_factor: 1024,
                                window: rubato::WindowFunction::BlackmanHarris2,
                            };
                            let mut resampler = SincFixedIn::<f32>::new(
                                stream_config.sample_rate.0 as f64 / sample_rate as f64,
                                params,
                                sample_rate as usize / 4,
                                chans,
                            );
                            let mut seek = false;
                            while packetsthread.is_running() {
                                if let Ok(command) = rx.try_recv() {
                                    match command {
                                        InSourceControl::StopStream => {
                                            timer.store(0, Ordering::Relaxed);
                                            break;
                                        }
                                        InSourceControl::Seek => {
                                            // TODO часы сбрасываются при перемотке
                                            seek = true;
                                            break;
                                        }
                                    }
                                }
                                let mut left = vec![];
                                let mut right = vec![];

                                for _ in 0..(sample_rate / 4) {
                                    left.push(lcon.recv().unwrap());
                                    right.push(rcon.recv().unwrap());
                                }

                                let chunk = vec![left, right];
                                let out = resampler.process(&chunk).unwrap();
                                let out = out[0].iter().interleave(out[1].iter()).collect_vec();
                                for sample in out {
                                    ttx.send(*sample).unwrap();
                                }
                                timer.store(timer.load(Ordering::Relaxed) + 1, Ordering::Relaxed);
                            }
                            if !seek {
                                timer.store(0, Ordering::Relaxed);
                            }
                        }))
                    }
                    SourceControl::Stop => match &self.in_source_control_tx {
                        Some(sender) => {
                            sender.send(InSourceControl::StopStream).unwrap();
                        }
                        None => {}
                    },
                    SourceControl::Seek => match &self.in_source_control_tx {
                        Some(sender) => {
                            sender.send(InSourceControl::Seek).unwrap_or(());
                        }
                        None => {}
                    },
                }
            };
        })
    }
}
// TODO normal Source struct
pub struct Source {}

pub enum Control {
    Resume,
    Pause,
    AddTrack(String, usize),
    NextTrack,
    StopCurTrack,
    Seek,
    Todo,
}

enum SourceControl {
    AddTrack(String, usize),
    Stop,
    Seek,
}

struct Backend {
    control_rx: Receiver<Control>,
    stream_control_tx: SyncSender<StreamControl>,
    source_control_tx: SyncSender<SourceControl>,
    _stream: JoinHandle<()>,
    _source_processor: JoinHandle<()>,
}

impl Backend {
    fn start(self) -> JoinHandle<()> {
        thread::spawn(move || loop {
            if let Ok(command) = self.control_rx.recv() {
                match command {
                    Control::AddTrack(track_path, skip) => self
                        .source_control_tx
                        .send(SourceControl::AddTrack(track_path, skip))
                        .unwrap(),

                    Control::Todo => todo!(),
                    Control::Resume => self.stream_control_tx.send(StreamControl::Resume).unwrap(),
                    Control::Pause => self.stream_control_tx.send(StreamControl::Pause).unwrap(),
                    Control::StopCurTrack => {
                        self.source_control_tx.send(SourceControl::Stop).unwrap()
                    }
                    Control::NextTrack => todo!(),
                    Control::Seek => self.source_control_tx.send(SourceControl::Seek).unwrap(),
                }
            }
        })
    }
}

pub enum InputMode {
    Normal,
    AddTrack,
}

pub struct Ui {
    // pub source_rx: Receiver<SourceResponse>,
    pub queue: Vec<String>,
    pub current: Option<String>,
    pub timer: Arc<AtomicUsize>,
    pub paused: Arc<AtomicBool>,
    pub volume: Arc<AtomicU8>,
    pub control_tx: SyncSender<Control>,
    pub cursor: u16,
    pub tmp_add_track: Vec<char>,
    pub add_track: bool,
    pub ui_state: InputMode,
}

pub struct Rpc {
    pub front_tx: SyncSender<Control>,
    pub ui: Ui,
    _back: JoinHandle<()>,
}

impl Rpc {
    pub fn new() -> Rpc {
        let (control_tx, control_rx) = sync_channel(3);
        let (stream_control_tx, stream_control_rx) = sync_channel(3);
        let (source_control_tx, source_control_rx) = sync_channel(3);
        let (stream, sample_tx, stream_config, volume, paused) = Stream::start(stream_control_rx);
        // let (source_response_tx, source_response_rx) = sync_channel(3);
        let timer = Arc::new(AtomicUsize::new(0));
        let source_processor = SourceHandler {
            source_control_rx,
            sample_tx,
            in_source_control_tx: None,
            current_source: None,
            stream_config,
            current_timer: timer.clone(),
        };
        let backend = Backend {
            control_rx,
            stream_control_tx,
            source_control_tx,
            _stream: stream,
            _source_processor: source_processor.start(),
        };
        let front = Ui {
            queue: vec![],
            current: None,
            timer,
            paused,
            volume,
            control_tx: control_tx.clone(),
            cursor: 0,
            tmp_add_track: vec![],
            add_track: false,
            ui_state: InputMode::Normal,
        };

        let backh = backend.start();

        Rpc {
            front_tx: control_tx,
            ui: front,
            _back: backh,
        }
    }

    pub fn set_volume(&mut self, volume: i8) {
        if volume > 100 {
            self.ui.volume.store(100, Ordering::Relaxed)
        } else if volume < 0 {
            self.ui.volume.store(0, Ordering::Relaxed)
        } else {
            self.ui.volume.store(volume as u8, Ordering::Relaxed)
        }
    }

    pub fn volume(&self) -> u8 {
        self.ui.volume.load(Ordering::Relaxed)
    }
}

impl Default for Rpc {
    fn default() -> Self {
        Rpc::new()
    }
}

fn stream_setup_for<F>(
    on_sample: F,
) -> Result<(
    cpal::Stream,
    SyncSender<f32>,
    cpal::StreamConfig,
    Arc<AtomicU8>,
)>
where
    F: FnMut(&Receiver<f32>) -> f32 + std::marker::Send + 'static + Copy,
{
    let (_host, device, config) = host_device_setup()?;

    match config.sample_format() {
        cpal::SampleFormat::F32 => stream_make::<f32, _>(&device, &config.into(), on_sample),
        cpal::SampleFormat::I16 => stream_make::<i16, _>(&device, &config.into(), on_sample),
        cpal::SampleFormat::U16 => stream_make::<u16, _>(&device, &config.into(), on_sample),
    }
}

fn host_device_setup() -> Result<(cpal::Host, cpal::Device, cpal::SupportedStreamConfig)> {
    let host = cpal::default_host();

    let device = host
        .default_output_device()
        .ok_or_else(|| anyhow::Error::msg("Default output device is not available"))
        .unwrap();
    // println!("Output device : {}", device.name()?);

    let config = device.default_output_config().unwrap();
    // println!("Default output config : {:?}", config);

    Ok((host, device, config))
}

fn stream_make<T, F>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    on_sample: F,
) -> Result<(
    cpal::Stream,
    SyncSender<f32>,
    cpal::StreamConfig,
    Arc<AtomicU8>,
)>
where
    T: cpal::Sample,
    F: FnMut(&Receiver<f32>) -> f32 + std::marker::Send + 'static + Copy,
{
    // let (tx, rx) = sync_channel(config.sample_rate.0 as usize + 1);
    let (tx, rx) = sync_channel((config.sample_rate.0 as usize) / 4);

    let err_fn = |err| eprintln!("Error building output sound stream: {}", err);

    let volume = Arc::new(AtomicU8::new(20));
    let vvolume = volume.clone();

    let stream = device
        .build_output_stream(
            config,
            move |output: &mut [T], _: &cpal::OutputCallbackInfo| {
                on_window(output, &rx, on_sample, vvolume.clone())
            },
            err_fn,
        )
        .unwrap();

    Ok((stream, tx, config.clone(), volume))
}

fn on_window<T, F>(
    output: &mut [T],
    request: &Receiver<f32>,
    mut on_sample: F,
    volume: Arc<AtomicU8>,
) where
    T: cpal::Sample,
    F: FnMut(&Receiver<f32>) -> f32 + std::marker::Send + 'static,
{
    for frame in output.chunks_mut(1) {
        let value: T = cpal::Sample::from::<f32>(
            &(on_sample(request) * ((volume.load(Ordering::Relaxed) as f32) / 100.0)),
        );
        for sample in frame.iter_mut() {
            *sample = value;
        }
    }
}
