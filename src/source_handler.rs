use std::{
    iter::zip,
    thread::{self},
    time::Duration,
};

use crossbeam::channel::{Receiver, Sender};
use symphonia::core::codecs::{DecoderOptions, CODEC_TYPE_NULL};
use symphonia::core::errors::Error;
use symphonia::core::{audio::Signal, formats::FormatReader};

use crate::SourceControl;

pub fn new_source_handle(
    mut format: Box<dyn FormatReader>,
    dur: Duration,
    sample_tx: Sender<f32>,
    control_rx: Receiver<SourceControl>,
) -> thread::JoinHandle<()> {
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
    format
        .seek(
            symphonia::core::formats::SeekMode::Coarse,
            symphonia::core::formats::SeekTo::Time {
                time: symphonia::core::units::Time {
                    seconds: dur.as_secs(),
                    frac: 0.0,
                },
                track_id: None,
            },
        )
        .unwrap();

    thread::spawn(move || {
        while let Ok(packet) = format.next_packet() {
            if let Ok(SourceControl::Stop) = control_rx.try_recv() {
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
                Ok(decoded) => {
                    match decoded {
                        symphonia::core::audio::AudioBufferRef::S32(buf) => {
                            let left = buf.chan(0).to_vec();
                            let right = buf.chan(1).to_vec();
                            for e in zip(left, right) {
                                match sample_tx.send(e.0 as f32) {
                                    Ok(_) => {}
                                    Err(_) => break,
                                };
                                sample_tx.send(e.1 as f32).unwrap();
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
                                match sample_tx.send(((e.0 as i32) << 24) as f32) {
                                    Ok(_) => {}
                                    Err(_) => break,
                                };
                                sample_tx.send(((e.1 as i32) << 24) as f32).unwrap();
                            }
                        }
                        symphonia::core::audio::AudioBufferRef::S16(buf) => {
                            let left = buf.chan(0).to_vec();
                            let right = buf.chan(1).to_vec();
                            for e in zip(left, right) {
                                match sample_tx.send((e.0 as i32) as f32) {
                                    Ok(_) => {}
                                    Err(_) => break,
                                };
                                sample_tx.send((e.1 as i32) as f32).unwrap();
                            }
                        }
                        symphonia::core::audio::AudioBufferRef::S24(buf) => {
                            let left = buf.chan(0).to_vec();
                            let right = buf.chan(1).to_vec();
                            for e in zip(left, right) {
                                match sample_tx.send((e.0.into_i32()) as f32) {
                                    Ok(_) => {}
                                    Err(_) => break,
                                };
                                sample_tx.send((e.1.into_i32()) as f32).unwrap();
                            }
                        }
                        symphonia::core::audio::AudioBufferRef::F32(buf) => {
                            let left = buf.chan(0).to_vec();
                            let right = buf.chan(1).to_vec();
                            for e in zip(left, right) {
                                match sample_tx.send(e.0 * 2000000000.0) {
                                    Ok(_) => {}
                                    Err(_) => break,
                                };
                                sample_tx.send(e.1 * 2000000000.0).unwrap();
                            }
                        }
                        symphonia::core::audio::AudioBufferRef::F64(buf) => {
                            let left = buf.chan(0).to_vec();
                            let right = buf.chan(1).to_vec();
                            for e in zip(left, right) {
                                match sample_tx.send((e.0 * 2000000000.0) as f32) {
                                    Ok(_) => {}
                                    Err(_) => break,
                                };
                                sample_tx.send((e.1 * 2000000000.0) as f32).unwrap();
                            }
                        }
                    }
                }
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
    })
}
