use std::{
    iter::zip,
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        mpsc::{sync_channel, Receiver, SyncSender},
        Arc, Mutex,
    },
    thread::{self, JoinHandle},
};

use cpal::StreamConfig;
use itertools::Itertools;
use rubato::{InterpolationParameters, Resampler, SincFixedIn};

use symphonia::core::audio::Signal;
use symphonia::core::codecs::{DecoderOptions, CODEC_TYPE_NULL};
use symphonia::core::errors::Error;
use symphonia::core::formats::FormatOptions;
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;

pub enum SourceResponse {
    Complete,
}

pub(crate) enum SourceControl {
    AddTrack(String, usize),
    Stop,
    Seek,
}

pub(crate) enum InSourceControl {
    StopStream,
    Seek,
}

pub(crate) struct SourceHandler {
    pub(crate) source_control_rx: Receiver<SourceControl>,
    pub(crate) stream_config: Arc<Mutex<StreamConfig>>,
    pub(crate) sample_tx: SyncSender<f32>,
    pub(crate) in_source_control_tx: Option<SyncSender<InSourceControl>>,
    pub(crate) current_source: Option<JoinHandle<()>>,
    pub(crate) current_timer: Arc<AtomicUsize>,
}

impl SourceHandler {
    pub(crate) fn start(mut self) -> JoinHandle<()> {
        thread::spawn(move || loop {
            if let Ok(control) = self.source_control_rx.recv() {
                match control {
                    SourceControl::AddTrack(track_path, skip) => {
                        // let (out_tx, out_rx) = sync_channel(3);
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
                            let packet_control = Arc::new(AtomicBool::new(false));
                            let pacc = packet_control.clone();
                            let (lprod, lcon) = sync_channel(sample_rate as usize);
                            let (rprod, rcon) = sync_channel(sample_rate as usize);
                            let packetsthread = thread::spawn(move || {
                                while let Ok(packet) = format.next_packet() {
                                    if pacc.load(Ordering::Relaxed) {
                                        break;
                                    }
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
                                                for e in zip(left, right) {
                                                    match lprod.send(e.0 as f32) {
                                                        Ok(_) => {}
                                                        Err(_) => break,
                                                    };
                                                    rprod.send(e.1 as f32).unwrap();
                                                }
                                            }
                                            symphonia::core::audio::AudioBufferRef::U8(_buf) => {
                                                todo!()
                                            }
                                            symphonia::core::audio::AudioBufferRef::U16(_buf) => {
                                                todo!()
                                            }
                                            symphonia::core::audio::AudioBufferRef::U24(_buf) => {
                                                todo!()
                                            }
                                            symphonia::core::audio::AudioBufferRef::U32(_buf) => {
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
                                            packet_control.store(true, Ordering::Relaxed);
                                            break;
                                        }
                                        InSourceControl::Seek => {
                                            // TODO множественная перемотка нагружает процессор
                                            // уже не так сильно, можно подзабить
                                            // TODO при перемотке что-то непонятное с памятью
                                            packet_control.store(true, Ordering::Relaxed);
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
