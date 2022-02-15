use std::{
    fs::File,
    io::BufReader,
    sync::{
        atomic::{AtomicBool, AtomicU8, AtomicUsize, Ordering},
        mpsc::{sync_channel, Receiver, SyncSender},
        Arc, Mutex,
    },
    thread::{self, JoinHandle},
};

use claxon::FlacReader;
use cpal::{
    traits::{DeviceTrait, HostTrait, StreamTrait},
    StreamConfig,
};
use itertools::Itertools;
use rubato::{InterpolationParameters, Resampler, SincFixedIn};

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
                Ok(v) => v / 10000000.0,
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
                    SourceControl::AddTrack(track_path) => {
                        let (tx, rx) = sync_channel(3);
                        self.in_source_control_tx = Some(tx);
                        let ttx = self.sample_tx.clone();
                        let stream_config = self.stream_config.lock().unwrap().clone();
                        let mut current = Source::from_path(track_path);
                        let timer = self.current_timer.clone();
                        self.current_source = Some(thread::spawn(move || {
                            let samples = current.reader.samples().map(|e| e.unwrap());
                            // println!("started next track");
                            //
                            // rubato
                            let params = InterpolationParameters {
                                sinc_len: 2048,
                                f_cutoff: 1.0,
                                interpolation: rubato::InterpolationType::Cubic,
                                oversampling_factor: 1024,
                                window: rubato::WindowFunction::BlackmanHarris2,
                            };
                            let mut resampler = SincFixedIn::<f32>::new(
                                stream_config.sample_rate.0 as f64 / current.sample_rate as f64,
                                params,
                                current.sample_rate as usize,
                                current.nchannels,
                            );
                            for chunk in &samples.chunks(current.sample_rate as usize * 2) {
                                if let Ok(command) = rx.try_recv() {
                                    match command {
                                        InSourceControl::StopStream => return,
                                    }
                                }
                                let mut left = vec![];
                                let mut right = vec![];
                                let mut f = false;
                                for el in chunk {
                                    if !f {
                                        match current.bits_per_sample.cmp(&24) {
                                            std::cmp::Ordering::Equal => left.push(el as f32),
                                            std::cmp::Ordering::Greater => left.push(
                                                (el >> (current.bits_per_sample - 24)) as f32,
                                            ),
                                            std::cmp::Ordering::Less => left.push(
                                                (el << (24 - current.bits_per_sample)) as f32,
                                            ),
                                        }
                                        f = true;
                                    } else {
                                        match current.bits_per_sample.cmp(&24) {
                                            std::cmp::Ordering::Equal => right.push(el as f32),
                                            std::cmp::Ordering::Greater => right.push(
                                                (el >> (current.bits_per_sample - 24)) as f32,
                                            ),
                                            std::cmp::Ordering::Less => right.push(
                                                (el << (24 - current.bits_per_sample)) as f32,
                                            ),
                                        };
                                        f = false;
                                    }
                                }
                                left.resize(current.sample_rate as usize, 0.0);
                                right.resize(current.sample_rate as usize, 0.0);
                                let chunk = vec![left, right];

                                let out = resampler.process(&chunk).unwrap();
                                let out = out[0].iter().interleave(out[1].iter()).collect_vec();
                                for sample in out {
                                    ttx.send(*sample).unwrap();
                                }
                                timer
                                    .clone()
                                    .store(timer.load(Ordering::Relaxed) + 1, Ordering::Relaxed);
                            }
                            timer.store(0, Ordering::Relaxed);
                        }))
                    }
                    SourceControl::Stop => match &self.in_source_control_tx {
                        Some(sender) => {
                            sender.send(InSourceControl::StopStream).unwrap();
                        }
                        None => {}
                    },
                }
            };
        })
    }
}
struct Source {
    activated: bool,
    finished: bool,
    reader: FlacReader<BufReader<File>>,
    sample_rate: f32,
    bits_per_sample: u8,
    nchannels: usize,
    samples_count: usize,
}

impl Source {
    fn from_path(track_path: String) -> Source {
        let track_path = track_path.trim().trim_matches('"');
        let reader = FlacReader::new(BufReader::new(File::open(track_path).unwrap())).unwrap();
        let bits_per_sample = reader.streaminfo().bits_per_sample as u8;
        let sample_rate = reader.streaminfo().sample_rate as f32;
        let nchannels = reader.streaminfo().channels as usize;
        let samples_count = reader.streaminfo().samples.unwrap() as usize;

        Source {
            activated: false,
            finished: false,
            reader,
            bits_per_sample,
            sample_rate,
            nchannels,
            samples_count,
        }
    }
}

pub enum Control {
    Resume,
    Pause,
    AddTrack(String),
    NextTrack,
    Todo,
}

enum SourceControl {
    AddTrack(String),
    Stop,
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
                    Control::AddTrack(track_path) => self
                        .source_control_tx
                        .send(SourceControl::AddTrack(track_path))
                        .unwrap(),

                    Control::Todo => todo!(),
                    Control::Resume => self.stream_control_tx.send(StreamControl::Resume).unwrap(),
                    Control::Pause => self.stream_control_tx.send(StreamControl::Pause).unwrap(),
                    Control::NextTrack => todo!(),
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
            control_tx: control_tx.clone(),
            cursor: 0,
            tmp_add_track: vec![],
            add_track: false,
            ui_state: InputMode::Normal,
            paused,
            volume,
            timer,
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
) -> Result<
    (
        cpal::Stream,
        SyncSender<f32>,
        cpal::StreamConfig,
        Arc<AtomicU8>,
    ),
    anyhow::Error,
>
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

fn host_device_setup(
) -> Result<(cpal::Host, cpal::Device, cpal::SupportedStreamConfig), anyhow::Error> {
    let host = cpal::default_host();

    let device = host
        .default_output_device()
        .ok_or_else(|| anyhow::Error::msg("Default output device is not available"))?;
    // println!("Output device : {}", device.name()?);

    let config = device.default_output_config()?;
    // println!("Default output config : {:?}", config);

    Ok((host, device, config))
}

fn stream_make<T, F>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    on_sample: F,
) -> Result<
    (
        cpal::Stream,
        SyncSender<f32>,
        cpal::StreamConfig,
        Arc<AtomicU8>,
    ),
    anyhow::Error,
>
where
    T: cpal::Sample,
    F: FnMut(&Receiver<f32>) -> f32 + std::marker::Send + 'static + Copy,
{
    // let (tx, rx) = sync_channel(config.sample_rate.0 as usize + 1);
    let (tx, rx) = sync_channel((config.sample_rate.0 as usize) / 4);

    let err_fn = |err| eprintln!("Error building output sound stream: {}", err);

    let volume = Arc::new(AtomicU8::new(50));
    let vvolume = volume.clone();

    let stream = device.build_output_stream(
        config,
        move |output: &mut [T], _: &cpal::OutputCallbackInfo| {
            on_window(output, &rx, on_sample, vvolume.clone())
        },
        err_fn,
    )?;

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
