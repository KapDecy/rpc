#![feature(cell_update)]
#![feature(duration_constants)]
#![feature(thread_id_value)]
pub mod basslib;
pub mod source_handler;
pub mod stream;
pub mod timer;

use basslib::{MediaStream, BASS_POS_BYTE};
use stream::TrackMetadata;


use basslib::*;

pub enum InputMode {
    Default,
    AddTrack,
}

pub enum UiState {
    Queue,
    Library,
}
pub struct Ui {
    pub repeat: bool,
    pub ui_counter: u32,
    pub input_state: InputMode,
    pub ui_state: UiState,
    pub add_track: bool,
    pub paused: bool,
    pub cursor: u16,
    pub tmp_add_track: Vec<char>,
}

pub struct Rpc {
    pub ui: Ui,
    pub current: Option<MediaStream>,
    pub queue: Vec<TrackMetadata>,
    pub library: Vec<String>,
    pub volume: u8,
    pub device: u8,
}

impl Rpc {
    pub fn new() -> Self {
        Rpc {
            ui: Ui {
                repeat: false,
                ui_counter: 0,
                input_state: InputMode::Default,
                ui_state: UiState::Queue,
                add_track: false,
                paused: false,
                cursor: 0,
                tmp_add_track: vec![],
            },
            current: None,
            queue: vec![],
            library: vec![],
            volume: 20,
            device: 1,
        }
    }

    pub fn volume(&self) -> u8 {
        self.volume
    }

    pub fn set_volume(&mut self, volume: u8) {
        self.volume = volume.clamp(0, 100);
        if let Some(cur) = self.current.as_ref() {
            basslib::BSetVolume(cur, (self.volume as f32) / 100.0);
        }
        // if volume > 100 {
        //     self.volume= 100
        // } else if volume < 0 {
        //     self.volume.store(0, Ordering::Relaxed)
        // } else {
        //     self.volume.store(volume as u8, Ordering::Relaxed)
        // }
    }

    pub fn time_as_secs(&mut self) -> f64 {
        if let Some(cur) = &mut self.current {
            basslib::BChannelBytes2Seconds(cur, basslib::BChannelGetPosition(cur, BASS_POS_BYTE))
        } else {
            0.0
        }
    }

    pub fn new_media_stream(&self, path: String) -> MediaStream {
        // TODO проверять, что предыдущий стрим завершен, или закрыть принудительно
        let ms = MediaStream::new(path);
        basslib::BSetVolume(&ms, (self.volume as f32) / 100.0);
        if self.ui.paused {
            BChannelPause(&ms);
        } else {
            BChannelPlay(&ms, 0);
        };
        ms
    }
}

impl Default for Rpc {
    fn default() -> Self {
        Self::new()
    }
}
