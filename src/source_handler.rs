// use std::{iter::zip, thread, time::Duration};

// use cpal::StreamConfig;
// use crossbeam::channel::{self, Receiver, Sender};
// use log::info;
// use symphonia::core::codecs::{DecoderOptions, CODEC_TYPE_NULL};
// use symphonia::core::errors::Error;
// use symphonia::core::{audio::Signal, formats::FormatReader};

// use rubato::{InterpolationParameters, Resampler, SincFixedIn};

// use crate::stream::SourceControl;

// pub fn new_source_handle(
//     mut format: Box<dyn FormatReader>,
//     dur: Duration,
//     sample_tx: Sender<f32>,
//     control_rx: Receiver<SourceControl>,
//     stream_config: StreamConfig,
// ) -> thread::JoinHandle<()> {
//     // info!("new handle created: {}", thread::current().id().as_u64());
//     let track = format
//         .tracks()
//         .iter()
//         .find(|t| t.codec_params.codec != CODEC_TYPE_NULL)
//         .expect("no supported audio tracks");
//     let sample_rate = track.codec_params.sample_rate.unwrap();
//     let dec_opts: DecoderOptions = Default::default();
//     let mut decoder = symphonia::default::get_codecs()
//         .make(&track.codec_params, &dec_opts)
//         .expect("unsupported codec");
//     let track_id = track.id;
//     format
//         .seek(
//             symphonia::core::formats::SeekMode::Coarse,
//             symphonia::core::formats::SeekTo::Time {
//                 time: symphonia::core::units::Time {
//                     seconds: dur.as_secs(),
//                     frac: 0.0,
//                 },
//                 track_id: None,
//             },
//         )
//         .unwrap();
//     // info!("starting main thread: {}", thread::current().id().as_u64());

//     thread::spawn(move || {
//         let (ch_s_tx, ch_s_rx) = channel::bounded(0);
//         let packet_control = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
//         let pacc = packet_control.clone();
//         let (lprod, lcon) = channel::bounded(sample_rate as usize);
//         let (rprod, rcon) = channel::bounded(sample_rate as usize);
//         let packet_thread = thread::spawn(move || {
//             // info!(
//             //     "starting packet thread: {}",
//             //     thread::current().id().as_u64()
//             // );
//             while let Ok(packet) = format.next_packet() {
//                 if pacc.load(std::sync::atomic::Ordering::Relaxed) {
//                     // info!(
//                     //     "loaded stop command in packet thread: {}",
//                     //     thread::current().id().as_u64()
//                     // );
//                     break;
//                 }
//                 while !format.metadata().is_latest() {
//                     // Pop the old head of the metadata queue.
//                     format.metadata().pop();

//                     // Consume the new metadata at the head of the metadata queue.
//                 }
//                 if packet.track_id() != track_id {
//                     continue;
//                 }

//                 match decoder.decode(&packet) {
//                     Ok(decoded) => {
//                         // info!("decoded next packet: {}", thread::current().id().as_u64());
//                         ch_s_tx.try_send(decoded.capacity()).unwrap_or(());
//                         match decoded {
//                             symphonia::core::audio::AudioBufferRef::S32(buf) => {
//                                 let left = buf.chan(0).to_vec();
//                                 let right = buf.chan(1).to_vec();
//                                 for e in zip(left, right) {
//                                     match lprod.send(e.0 as f32) {
//                                         Ok(_) => {}
//                                         Err(_) => break,
//                                     };
//                                     rprod.send(e.1 as f32).unwrap();
//                                 }
//                             }
//                             symphonia::core::audio::AudioBufferRef::U8(_buf) => {
//                                 todo!()
//                             }
//                             symphonia::core::audio::AudioBufferRef::U16(_buf) => {
//                                 todo!()
//                             }
//                             symphonia::core::audio::AudioBufferRef::U24(_buf) => {
//                                 todo!()
//                             }
//                             symphonia::core::audio::AudioBufferRef::U32(_buf) => {
//                                 todo!()
//                             } // TODO Сделать все варианты инпутов
//                             symphonia::core::audio::AudioBufferRef::S8(buf) => {
//                                 let left = buf.chan(0).to_vec();
//                                 let right = buf.chan(1).to_vec();
//                                 for e in zip(left, right) {
//                                     match lprod.send(((e.0 as i32) << 24) as f32) {
//                                         Ok(_) => {}
//                                         Err(_) => break,
//                                     };
//                                     rprod.send(((e.1 as i32) << 24) as f32).unwrap();
//                                 }
//                             }
//                             symphonia::core::audio::AudioBufferRef::S16(buf) => {
//                                 let left = buf.chan(0).to_vec();
//                                 let right = buf.chan(1).to_vec();
//                                 for e in zip(left, right) {
//                                     match lprod.send((e.0 as i32) as f32) {
//                                         Ok(_) => {}
//                                         Err(_) => break,
//                                     };
//                                     rprod.send((e.1 as i32) as f32).unwrap();
//                                 }
//                             }
//                             symphonia::core::audio::AudioBufferRef::S24(buf) => {
//                                 let left = buf.chan(0).to_vec();
//                                 let right = buf.chan(1).to_vec();
//                                 for e in zip(left, right) {
//                                     match lprod.send((e.0.into_i32()) as f32) {
//                                         Ok(_) => {}
//                                         Err(_) => break,
//                                     };
//                                     rprod.send((e.1.into_i32()) as f32).unwrap();
//                                 }
//                             }
//                             symphonia::core::audio::AudioBufferRef::F32(buf) => {
//                                 let left = buf.chan(0).to_vec();
//                                 let right = buf.chan(1).to_vec();
//                                 for e in zip(left, right) {
//                                     match lprod.send(e.0 * 2000000000.0) {
//                                         Ok(_) => {}
//                                         Err(_) => break,
//                                     };
//                                     rprod.send(e.1 * 2000000000.0).unwrap();
//                                 }
//                             }
//                             symphonia::core::audio::AudioBufferRef::F64(buf) => {
//                                 let left = buf.chan(0).to_vec();
//                                 let right = buf.chan(1).to_vec();
//                                 for e in zip(left, right) {
//                                     match lprod.send((e.0 * 2000000000.0) as f32) {
//                                         Ok(_) => {}
//                                         Err(_) => break,
//                                     };
//                                     rprod.send((e.1 * 2000000000.0) as f32).unwrap();
//                                 }
//                             }
//                         }
//                     }
//                     Err(Error::IoError(_)) => {
//                         // The packet failed to decode due to an IO error, skip the packet.
//                         continue;
//                     }
//                     Err(Error::DecodeError(_)) => {
//                         // The packet failed to decode due to invalid data, skip the packet.
//                         continue;
//                     }
//                     Err(err) => {
//                         // An unrecoverable error occured, halt decoding.
//                         panic!("{}", err);
//                     }
//                 }
//             }
//             info!("stoping packet thread: {}", thread::current().id().as_u64());
//         });
//         let chunk_size = ch_s_rx.recv().unwrap();
//         drop(ch_s_rx);
//         // rubato
//         let params = InterpolationParameters {
//             sinc_len: 2048,
//             f_cutoff: 0.95,
//             interpolation: rubato::InterpolationType::Cubic,
//             oversampling_factor: 1024,
//             window: rubato::WindowFunction::BlackmanHarris2,
//         };
//         let mut resampler = SincFixedIn::<f32>::new(
//             stream_config.sample_rate.0 as f64 / sample_rate as f64,
//             2.0,
//             params,
//             chunk_size as usize,
//             2,
//         )
//         .unwrap();
//         while packet_thread.is_running() {
//             if let Ok(com) = control_rx.try_recv() {
//                 match com {
//                     SourceControl::Stop => {
//                         packet_control.store(true, std::sync::atomic::Ordering::Relaxed);
//                         break;
//                     }
//                     SourceControl::None => (),
//                 }
//             }
//             let mut left = vec![];
//             let mut right = vec![];

//             for _ in 0..(chunk_size) {
//                 left.push(lcon.recv().unwrap());
//                 right.push(rcon.recv().unwrap());
//             }

//             let chunk = vec![left, right];
//             let mut out = resampler.process(&chunk, None).unwrap();
//             let right = out.pop().unwrap();
//             let left = out.pop().unwrap();
//             for sample in zip(left, right) {
//                 match sample_tx.send(sample.0) {
//                     Ok(_) => {}
//                     Err(_) => break,
//                 };
//                 match sample_tx.send(sample.1) {
//                     Ok(_) => {}
//                     Err(_) => break,
//                 };
//             }
//             // FIXME жрет память при/после перемотки
//             // FIXME жрет цпу во время перемотки
//         }
//     })
// }
