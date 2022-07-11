#![feature(thread_is_running)]
#![feature(cell_update)]
#![feature(duration_constants)]
#![feature(thread_id_value)]
pub mod library;
pub mod source_handler;
pub mod stream;
pub mod timer;

use cpal::{traits::HostTrait, Device};
use stream::{Current, TrackMetadata};

use std::{
    any::Any,
    sync::{
        atomic::{AtomicU8, Ordering},
        Arc,
    },
};

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
    pub current: Option<Current>,
    pub queue: Vec<TrackMetadata>,
    pub library: Box<dyn Any>,
    pub volume: Arc<AtomicU8>,
    pub device: Arc<Device>,
}

impl Rpc {
    pub fn new() -> Self {
        let host = cpal::default_host();
        let device = Arc::new(
            host.default_output_device()
                .expect("no output device available"),
        );
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
            library: todo!(),
            volume: Arc::new(AtomicU8::new(20)),
            device,
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
