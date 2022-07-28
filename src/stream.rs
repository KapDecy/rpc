use anyhow::Error;
use crossbeam::channel::{self, Receiver, Sender};
use log::info;
use serde::{Deserialize, Serialize};
use std::{
    str::FromStr,
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

use crate::timer::Timer;

pub enum SourceControl {
    None,
    Stop,
}

// pub struct Streamer {
//     pub stream: todo!(),
//     pub paused: bool,
//     pub stream_config: todo!(),
//     pub source_handler: JoinHandle<()>,
//     pub volume: Arc<AtomicU8>,
//     pub sample_tx: Sender<f32>,
//     pub control_tx: Sender<SourceControl>,
//     pub device: todo!(),
// }

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct TrackMetadata {
    pub full_time_secs: Option<u64>,
    pub path: String,
    pub file_stem: String,
    pub title: Option<String>,
    pub artist: Option<String>,
    pub album: Option<String>,
    pub year: Option<i32>,
}

impl FromStr for TrackMetadata {
    fn from_str(path: &str) -> Result<Self, Self::Err> {
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

        let r = path.to_string();
        let tag = Tag::new().read_from_path(path).unwrap();

        let mut metadata = TrackMetadata {
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
        };
        for tag in meta {
            if let Some(skey) = tag.std_key {
                match skey {
                    symphonia::core::meta::StandardTagKey::Artist => {
                        metadata.artist = Some(tag.value.to_string())
                    }
                    symphonia::core::meta::StandardTagKey::TrackTitle => {
                        metadata.title = Some(tag.value.to_string())
                    }
                    _ => (),
                }
            } else {
                match tag.key.to_uppercase().as_str() {
                    "TITLE" => metadata.title = Some(tag.value.to_string()),
                    "ARTIST" => metadata.artist = Some(tag.value.to_string()),
                    _ => (),
                }
            }
        }
        Ok(metadata)
    }

    type Err = Error;
}
