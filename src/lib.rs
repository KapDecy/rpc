#![feature(thread_is_running)]
mod source_handler;
mod stream;

use pausable_clock::*;
use std::{
    sync::{
        atomic::{AtomicBool, AtomicU8, AtomicUsize, Ordering},
        mpsc::{sync_channel, Receiver, SyncSender},
        Arc, Mutex,
    },
    thread::{self, JoinHandle},
    time::Duration,
};

use source_handler::*;
use stream::*;

// TODO normal Source struct
pub struct Source {}

pub enum Control {
    Resume,
    Pause,
    AddTrack(String, Duration),
    NextTrack,
    StopCurTrack,
    Seek,
    Todo,
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
                    Control::AddTrack(track_path, skip) => self
                        .source_control_tx
                        .send(SourceControl::AddTrack(track_path, skip))
                        .unwrap(),

                    Control::Todo => todo!(),
                    Control::Resume => self.stream_control_tx.send(StreamControl::Resume).unwrap(),
                    Control::Pause => self.stream_control_tx.send(StreamControl::Pause).unwrap(),
                    Control::StopCurTrack => {
                        self.source_control_tx.send(SourceControl::Stop).unwrap()
                    }
                    Control::NextTrack => todo!(),
                    Control::Seek => self.source_control_tx.send(SourceControl::Seek).unwrap(),
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
    pub queue: Vec<String>,
    pub current: Option<String>,
    pub timer: Arc<Mutex<PausableClock>>,
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
        let timer = Arc::new(Mutex::new(PausableClock::new(
            Duration::from_secs(0),
            paused.load(Ordering::Relaxed),
        )));
        let source_processor = SourceHandler {
            source_control_rx,
            stream_config,
            sample_tx,
            in_source_control_tx: None,
            current_source: None,
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
            queue: vec![],
            current: None,
            timer,
            paused,
            volume,
            control_tx: control_tx.clone(),
            cursor: 0,
            tmp_add_track: vec![],
            add_track: false,
            ui_state: InputMode::Normal,
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
