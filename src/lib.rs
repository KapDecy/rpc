#![feature(cell_update)]
#![feature(duration_constants)]
#![feature(thread_id_value)]
pub mod basslib;
pub mod source_handler;
pub mod stream;
pub mod timer;
use std::{
    path::Path,
    thread,
    time::{Duration, Instant},
};

use eframe::egui;

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
    pub tmp_add_track: String,
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
        let r = Rpc {
            ui: Ui {
                repeat: false,
                ui_counter: 0,
                input_state: InputMode::Default,
                ui_state: UiState::Queue,
                add_track: false,
                paused: false,
                cursor: 0,
                tmp_add_track: String::from(""),
            },
            current: None,
            queue: vec![],
            library: vec![],
            volume: 20,
            device: 1,
        };

        BSetConfig(42, 1);
        BInit(1, 192000, 0, 0);
        BStart();

        r
    }

    pub fn volume(&self) -> u8 {
        self.volume
    }

    pub fn set_volume(&mut self, volume: u8) {
        self.volume = volume.clamp(0, 100);
        if let Some(cur) = self.current.as_ref() {
            basslib::BSetVolume(cur, (self.volume as f32) / 100.0);
        }
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

impl eframe::App for Rpc {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let tsp = Instant::now(); // tsp = time start point
        egui::CentralPanel::default().show(ctx, |ui| {
            if !ctx.wants_keyboard_input() {
                let inp = &ctx.input().events;
                for ev in inp {
                    if let egui::Event::Key {
                        key,
                        pressed: true,
                        modifiers:
                            egui::Modifiers {
                                alt: false,
                                ctrl: false,
                                shift: false,
                                mac_cmd: false,
                                command: false,
                            },
                    } = ev
                    {
                        match *key {
                            egui::Key::Space => match self.ui.paused {
                                true => {
                                    self.ui.paused = false;
                                    match &mut self.current {
                                        Some(cur) => {
                                            BChannelPlay(cur, 0);
                                        }
                                        None => (),
                                    }
                                }
                                false => {
                                    self.ui.paused = true;
                                    match &mut self.current {
                                        Some(cur) => {
                                            BChannelPause(cur);
                                        }
                                        None => (),
                                    }
                                }
                            },
                            egui::Key::ArrowRight => {
                                if let Some(cur) = &mut self.current {
                                    cur.seek_forward(Duration::from_secs(5));
                                }
                            }
                            egui::Key::ArrowLeft => {
                                if let Some(cur) = &mut self.current {
                                    cur.seek_backward(Duration::from_secs(5));
                                }
                            }
                            egui::Key::ArrowUp => {
                                self.set_volume((self.volume() + 5).clamp(0, 100));
                            }
                            egui::Key::ArrowDown => {
                                self.set_volume((self.volume() - 5).clamp(0, 100));
                            }
                            egui::Key::R => self.ui.repeat = !self.ui.repeat,
                            _ => (),
                        }
                    }
                }
            }

            let awd = ui.available_width(); // awd = avalible width
            ui.horizontal(|ui| {
                // let buttonsymbol = ;
                let pausebr = ui.add_sized(
                    [21.0, 20.0],
                    egui::Button::new(if self.ui.paused { "⏸" } else { "▶" }),
                );
                if pausebr.clicked() {
                    match self.ui.paused {
                        true => {
                            self.ui.paused = false;
                            match &mut self.current {
                                Some(cur) => {
                                    BChannelPlay(cur, 0);
                                }
                                None => (),
                            }
                        }
                        false => {
                            self.ui.paused = true;
                            match &mut self.current {
                                Some(cur) => {
                                    BChannelPause(cur);
                                }
                                None => (),
                            }
                        }
                    }
                }
                let mut now = match &self.current {
                    Some(cur) => cur.as_secs(),
                    None => 0.0,
                };

                let cur_full_time = match &self.current {
                    Some(cur) => cur.metadata.full_time_secs.unwrap(),
                    None => 0,
                };
                ui.add_sized(
                    [30.0, 20.0],
                    egui::Label::new(format!("{}:{:02}", (now / 60.0) as u8, (now % 60.0) as u8)),
                );
                let oldnow = now;
                ui.style_mut().spacing.slider_width = awd - 21.0 - 30.0 - 30.0 - 80.0 - 30.0 - 48.0;
                // 48 is padding, 6 items, 8 pixels per item
                ui.add(
                    egui::Slider::new(&mut now, 0.0..=(cur_full_time as i64 - 1).max(0) as f64)
                        .show_value(false),
                );
                ui.style_mut().spacing.slider_width = 100.0; // default value
                if (-1.0 >= (now - oldnow)) || ((now - oldnow) >= 1.0) {
                    self.current
                        .as_ref()
                        .unwrap()
                        .seek_to(Duration::from_secs_f64(now));
                }
                ui.add_sized(
                    [30.0, 20.0],
                    egui::Label::new(format!("{}:{:02}", cur_full_time / 60, cur_full_time % 60)),
                );
                let mut vol = self.volume() as i8;
                let oldvol = vol;
                ui.style_mut().spacing.slider_width = 80.0;
                let volrect = ui
                    .add(egui::Slider::new(&mut vol, 0..=100).show_value(false))
                    .rect;

                ui.style_mut().spacing.slider_width = 100.0; // default value
                let volrect = volrect.union(
                    ui.add_sized([30.0, 20.0], egui::DragValue::new(&mut vol))
                        .rect,
                );
                let scroll_delta = ctx.input().scroll_delta.y;
                let mouse_pos = ctx.pointer_hover_pos();
                if let Some(mouse_pos) = mouse_pos {
                    if (scroll_delta != 0.0) && volrect.contains(mouse_pos) {
                        if scroll_delta > 0.0 {
                            self.set_volume((self.volume() + 2).clamp(0, 100));
                        }
                        if scroll_delta < 0.0 {
                            self.set_volume((self.volume() as i8 - 2).clamp(0, 100) as u8);
                        }
                    }
                }
                if (-1 >= (vol - oldvol)) || ((vol - oldvol) >= 1) {
                    self.set_volume(vol as u8)
                }
            });
            ui.separator();
            if ui
                .add(egui::TextEdit::singleline(&mut self.ui.tmp_add_track))
                .lost_focus()
            {
                match Path::new(self.ui.tmp_add_track.trim_matches('"')).exists() {
                    true => {
                        // self.queue
                        //     .push(TrackMetadata::from_str(&self.ui.tmp_add_track).unwrap());
                        {
                            // TODO переработать
                            // BFree();
                            self.current =
                                Some(self.new_media_stream(self.ui.tmp_add_track.clone()));
                            self.ui.tmp_add_track = String::from("");
                        }
                    }
                    false => (),
                }
            }
        });
        let tbrp = 1.0 / 100.0; // tbrp = time before repaint
        if tsp.elapsed().as_secs_f64() < tbrp {
            thread::sleep(Duration::from_secs_f64(tbrp - tsp.elapsed().as_secs_f64()));
        }
        ctx.request_repaint();
    }

    fn save(&mut self, _storage: &mut dyn eframe::Storage) {}

    fn on_exit_event(&mut self) -> bool {
        true
    }

    fn on_exit(&mut self, _gl: &eframe::glow::Context) {}

    fn auto_save_interval(&self) -> std::time::Duration {
        std::time::Duration::from_secs(30)
    }

    fn max_size_points(&self) -> egui::Vec2 {
        egui::Vec2::INFINITY
    }

    fn clear_color(&self, _visuals: &egui::Visuals) -> egui::Rgba {
        // NOTE: a bright gray makes the shadows of the windows look weird.
        // We use a bit of transparency so that if the user switches on the
        // `transparent()` option they get immediate results.
        egui::Color32::from_rgba_unmultiplied(12, 12, 12, 180).into()

        // _visuals.window_fill() would also be a natural choice
    }

    fn persist_native_window(&self) -> bool {
        true
    }

    fn persist_egui_memory(&self) -> bool {
        true
    }

    fn warm_up_enabled(&self) -> bool {
        false
    }

    // fn post_rendering(&mut self, _window_size_px: [u32; 2], _frame: &eframe::Frame) {}
}
