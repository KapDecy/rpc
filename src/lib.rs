extern crate libsoxr;

use std::{
    fs::File,
    io::BufReader,
    sync::{
        atomic::{AtomicU8, Ordering},
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
use libsoxr::{Datatype, Soxr};
use pausable_clock::PausableClock;

enum InStreamControl {
    Pause,
    Resume,
    Stop,
}

enum StreamControl {
    Pause,
    Resume,
    SetVolume(u8),
}

struct Stream {
    stream_control_rx: Receiver<StreamControl>,
    stream: Box<dyn StreamTrait>,
    volume: Arc<AtomicU8>,
    paused: bool,
    stream_config: Arc<Mutex<StreamConfig>>,
    timer: Arc<PausableClock>,
}

impl Stream {
    fn start(
        stream_control_rx: Receiver<StreamControl>,
    ) -> (
        JoinHandle<()>,
        SyncSender<f32>,
        Arc<Mutex<StreamConfig>>,
        Arc<AtomicU8>,
    ) {
        let (temp_tx, temp_rx) = sync_channel(2);
        let h = thread::spawn(move || {
            let (stream, tx, config, volume) = stream_setup_for(|o| match o.try_recv() {
                Ok(v) => v / 10000000.0,
                Err(_) => 0.0,
            })
            .unwrap();
            let config = Arc::new(Mutex::new(config));
            let stream = Stream {
                stream_control_rx,
                stream: Box::new(stream),
                volume: volume.clone(),
                paused: false,
                stream_config: config.clone(),
                timer: Arc::new(PausableClock::default()),
            };
            temp_tx.send((tx, config, volume)).unwrap();
            stream.stream.play().unwrap();
            stream.timer.as_ref().pause();
            loop {
                if let Ok(command) = stream.stream_control_rx.recv() {
                    match command {
                        StreamControl::Pause => {
                            stream.stream.pause().unwrap();
                            stream.timer.pause();
                        }
                        StreamControl::Resume => {
                            stream.stream.play().unwrap();
                            stream.timer.resume();
                        }
                        StreamControl::SetVolume(v) => stream.volume.store(v, Ordering::SeqCst),
                    }
                }
            }
        });
        let (tx, config, volume) = temp_rx.recv().unwrap();
        drop(temp_rx);
        (h, tx, config, volume)
    }
}

enum InSourceControl {
    StopStream,
    Todo,
}

struct SourceHandler {
    source_control_rx: Receiver<SourceControl>,
    stream_config: Arc<Mutex<StreamConfig>>,
    sample_tx: SyncSender<f32>,
    in_source_control_tx: Option<SyncSender<InSourceControl>>,
    current_source: Option<JoinHandle<()>>,
    queue: Vec<Source>,
}

impl SourceHandler {
    fn start(mut self) -> JoinHandle<()> {
        thread::spawn(move || loop {
            if let Ok(control) = self.source_control_rx.recv() {
                match control {
                    SourceControl::Todo => todo!(),
                    SourceControl::NextTrack => {
                        if !self.queue.is_empty() {
                            let (tx, rx) = sync_channel(3);
                            self.in_source_control_tx = Some(tx);
                            let ttx = self.sample_tx.clone();
                            let stream_config = self.stream_config.lock().unwrap().clone();
                            let mut current = self.queue.remove(0);
                            self.current_source = Some(thread::spawn(move || {
                                // println!("started next track");
                                let samples = current.reader.samples().map(|e| e.unwrap());
                                let soxr = Soxr::create(
                                    current.sample_rate as f64,
                                    stream_config.sample_rate.0 as f64,
                                    2,
                                    Some(&libsoxr::IOSpec::new(Datatype::Int32I, Datatype::Int32I)),
                                    Some(&libsoxr::QualitySpec::new(
                                        // use
                                        // &libsoxr::QualityRecipe::Low,
                                        // or
                                        &libsoxr::QualityRecipe::Quick,
                                        // to remove click-sound at the first second
                                        // &libsoxr::QualityRecipe::VeryHigh,
                                        libsoxr::QualityFlags::DOUBLE_PRECISION,
                                    )),
                                    // None,
                                    Some(&libsoxr::RuntimeSpec::new(16)),
                                )
                                .unwrap();
                                // for chunk in &samples.chunks(ctrack.sample_rate as usize * 2) {
                                for chunk in &samples
                                    // .skip(current.sample_rate as usize * 150 * 2)
                                    .chunks(current.sample_rate as usize * 2)
                                {
                                    if let Ok(command) = rx.try_recv() {
                                        match command {
                                            InSourceControl::Todo => {}
                                            InSourceControl::StopStream => return,
                                        }
                                    }
                                    let chunk = chunk.collect_vec();
                                    let mut output =
                                        vec![0; stream_config.sample_rate.0 as usize * 2];
                                    soxr.process(Some(&chunk), &mut output).unwrap();
                                    for sample in &output {
                                        match current.bits_per_sample.cmp(&24) {
                                            std::cmp::Ordering::Equal => {
                                                ttx.send(*sample as f32).unwrap()
                                            }
                                            std::cmp::Ordering::Greater => ttx
                                                .send(
                                                    (*sample >> (current.bits_per_sample - 24))
                                                        as f32,
                                                )
                                                .unwrap(),
                                            std::cmp::Ordering::Less => ttx
                                                .send(
                                                    (*sample << (24 - current.bits_per_sample))
                                                        as f32,
                                                )
                                                .unwrap(),
                                        }
                                    }
                                }
                                soxr.process::<f32, _>(None, &mut [0; 100]).unwrap();
                            }))
                        }
                    }
                    SourceControl::Stop => match &self.in_source_control_tx {
                        Some(sender) => {
                            sender.send(InSourceControl::StopStream).unwrap();
                        }
                        None => {}
                    },
                    SourceControl::AddTrack(track_path) => {
                        self.queue.push(Source::from_path(track_path))
                    }
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
    AddTrack(String),
    NextTrack,
    Todo,
}

enum SourceControl {
    Todo,
    NextTrack,
    AddTrack(String),
    Stop,
}

struct Backend {
    control_rx: Receiver<Control>,
    stream_control_tx: SyncSender<StreamControl>,
    source_control_tx: SyncSender<SourceControl>,
    stream: JoinHandle<()>,
    source_processor: JoinHandle<()>,
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
                    Control::NextTrack => self
                        .source_control_tx
                        .send(SourceControl::NextTrack)
                        .unwrap(),
                    Control::Todo => todo!(),
                }
            }
        })
    }
}

struct Ui {
    control_tx: SyncSender<Control>,
}

pub struct Rpc {
    pub front_tx: SyncSender<Control>,
    front: JoinHandle<()>,
    back: JoinHandle<()>,
}

impl Rpc {
    pub fn new() -> Rpc {
        let (control_tx, control_rx) = sync_channel(3);
        let (stream_control_tx, stream_control_rx) = sync_channel(3);
        let (source_control_tx, source_control_rx) = sync_channel(3);
        let (stream, sample_tx, stream_config, volume) = Stream::start(stream_control_rx);
        let source_processor = SourceHandler {
            source_control_rx,
            sample_tx,
            in_source_control_tx: None,
            current_source: None,
            queue: vec![],
            stream_config,
        };
        let backend = Backend {
            control_rx,
            stream_control_tx,
            source_control_tx,
            stream,
            source_processor: source_processor.start(),
        };
        let front = Ui {
            control_tx: control_tx.clone(),
        };
        // let rpc = Rpc {
        //     front: todo!(),
        //     back: todo!(),
        // };

        // test
        let fronth = thread::spawn(|| {});
        let backh = backend.start();

        Rpc {
            front_tx: control_tx,
            front: fronth,
            back: backh,
        }
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
    let (tx, rx) = sync_channel((config.sample_rate.0 as usize) * 3);

    let err_fn = |err| eprintln!("Error building output sound stream: {}", err);

    let volume = Arc::new(AtomicU8::new(35));
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
