#![feature(thread_is_running)]
#![feature(cell_update)]
pub mod source_handler;
pub mod stream;
pub mod timer;

use stream::{Current, TrackMetadata};

use std::sync::{
    atomic::{AtomicU8, Ordering},
    Arc,
};

pub enum InputMode {
    Normal,
    AddTrack,
}
pub struct Ui {
    pub ui_state: InputMode,
    pub add_track: bool,
    pub paused: bool,
    pub cursor: u16,
    pub tmp_add_track: Vec<char>,
}

pub struct Rpc {
    pub ui: Ui,
    pub current: Option<Current>,
    pub queue: Vec<String>,
    pub library: Vec<TrackMetadata>,
    pub volume: Arc<AtomicU8>,
}

impl Rpc {
    pub fn new() -> Self {
        Rpc {
            ui: Ui {
                ui_state: InputMode::Normal,
                add_track: false,
                paused: false,
                cursor: 0,
                tmp_add_track: vec![],
            },
            current: None,
            queue: vec![],
            library: vec![],
            volume: Arc::new(AtomicU8::new(50)),
        }
    }

    pub fn volume(&self) -> u8 {
        self.volume.load(Ordering::Relaxed)
    }

    pub fn set_volume(&mut self, volume: i8) {
        if volume > 100 {
            self.volume.store(100, Ordering::Relaxed)
        } else if volume < 0 {
            self.volume.store(0, Ordering::Relaxed)
        } else {
            self.volume.store(volume as u8, Ordering::Relaxed)
        }
    }

    pub fn time_as_secs(&mut self) -> u64 {
        // self.current..timer.as_secs()
        if let Some(cur) = &mut self.current {
            cur.timer.as_secs()
        } else {
            0
        }
    }
}

impl Default for Rpc {
    fn default() -> Self {
        Self::new()
    }
}
