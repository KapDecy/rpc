use cpal::{
    traits::{DeviceTrait, HostTrait, StreamTrait},
    StreamConfig,
};
use rb::*;
use std::{
    sync::{
        atomic::{AtomicBool, AtomicU8, Ordering},
        mpsc::{sync_channel, Receiver, SyncSender},
        Arc, Mutex,
    },
    thread::{self, JoinHandle},
};

pub(crate) enum InStreamControl {
    Pause,
    Resume,
    Stop,
}

pub(crate) enum StreamControl {
    Pause,
    Resume,
}

pub(crate) struct Stream {
    pub(crate) stream_control_rx: Receiver<StreamControl>,
    pub(crate) stream: Box<dyn StreamTrait>,
    pub(crate) paused: Arc<AtomicBool>,
}

impl Stream {
    pub(crate) fn start(
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
            let paused = Arc::new(AtomicBool::new(true));
            let config = Arc::new(Mutex::new(config));
            let stream = Stream {
                stream_control_rx,
                stream: Box::new(stream),
                paused: paused.clone(),
            };
            temp_tx.send((tx, config, volume, paused)).unwrap();
            stream.stream.pause().unwrap();
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
    for frame in output.iter_mut() {
        let value: T = cpal::Sample::from::<f32>(
            &(on_sample(request) * ((volume.load(Ordering::Relaxed) as f32) / 100.0)),
        );
        *frame = value;
    }
}
