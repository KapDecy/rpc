#![feature(cell_update)]
#![feature(duration_constants)]
#![feature(thread_id_value)]
pub mod basslib;
pub mod stream;
pub mod timer;
use std::{
    fs,
    path::Path,
    thread,
    time::{Duration, Instant},
};

use eframe::{
    egui::{self},
    emath::Rect,
    epaint::ColorImage,
};
use egui_extras::{Size, StripBuilder};

use basslib::{MediaStream, BASS_POS_BYTE};
use itertools::enumerate;
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
    pub dropped_files: Vec<egui::DroppedFile>,
    pub no_lyrics_color_image: Option<ColorImage>,
    pub no_lyrics_texture_handle: Option<egui::TextureHandle>,
}

enum Pointer {
    Start,
    Idx(usize),
    End,
}

pub struct Queue {
    qu_vec: Vec<TrackMetadata>,
    pointer: Pointer,
}

impl Default for Queue {
    fn default() -> Self {
        Self {
            qu_vec: Vec::from([]),
            pointer: Pointer::Start,
        }
    }
}

pub struct Rpc {
    pub ui: Ui,
    pub current: Option<MediaStream>,
    pub queue: Queue,
    pub library: Vec<TrackMetadata>,
    pub volume: u8,
    pub device: u8,
}

impl Rpc {
    pub fn new() -> Self {
        BSetConfig(42, 1);
        BInit(1, 192000, 0, 0);
        BStart();
        Rpc {
            ui: Ui {
                repeat: false,
                ui_counter: 0,
                input_state: InputMode::Default,
                ui_state: UiState::Queue,
                add_track: false,
                paused: false,
                cursor: 0,
                tmp_add_track: String::from(""),
                dropped_files: vec![],
                no_lyrics_color_image: None,
                no_lyrics_texture_handle: None,
            },
            current: None,
            queue: Queue::default(),
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
            BSetVolume(cur, (self.volume as f32) / 100.0);
        }
    }

    pub fn time_as_secs(&mut self) -> f64 {
        if let Some(cur) = &mut self.current {
            BChannelBytes2Seconds(cur, BChannelGetPosition(cur, BASS_POS_BYTE))
        } else {
            0.0
        }
    }

    pub fn new_media_stream(&self, path: String) -> MediaStream {
        let ms = MediaStream::new(path);
        BSetVolume(&ms, (self.volume as f32) / 100.0);
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
        if self.ui.no_lyrics_color_image.is_none() {
            let image = image::io::Reader::open("src/no_lyrics.png")
                .unwrap()
                .decode()
                .unwrap();
            let size = [image.width() as _, image.height() as _];
            let image_buffer = image.to_rgba8();
            let pixels = image_buffer.as_flat_samples();
            let color_image = egui::ColorImage::from_rgba_unmultiplied(size, pixels.as_slice());

            self.ui.no_lyrics_color_image = Some(color_image);
        }
        let tsp = Instant::now(); // tsp = time start point
        match &self.current {
            Some(cur) => {
                let lpath = cur.metadata.path.replace(".flac", ".lrc");
                let lpath = Path::new(&lpath);
                match lpath.exists() {
                    true => {
                        let mut lyrics = fs::read_to_string(lpath).unwrap();
                        egui::SidePanel::right("lyrics")
                            .min_width(300.0)
                            .resizable(false)
                            .show(ctx, |ui| {
                                egui::containers::ScrollArea::vertical().show(ui, |ui| {
                                    ui.add_sized(
                                        [ui.available_width(), ui.available_height()],
                                        egui::widgets::TextEdit::multiline(&mut lyrics)
                                            .interactive(false),
                                    );
                                });
                            });
                    }
                    false => {
                        egui::SidePanel::right("lyrics")
                            .min_width(150.0)
                            .resizable(false)
                            .show(ctx, |ui| {
                                // let (size, texture) = self.ui.no_lyrics_image.unwrap();
                                let color_image = self.ui.no_lyrics_color_image.clone().unwrap();
                                let texture: &egui::TextureHandle =
                                    self.ui.no_lyrics_texture_handle.get_or_insert_with(|| {
                                        ui.ctx().load_texture(
                                            "no_lyrics",
                                            color_image,
                                            egui::TextureFilter::Linear,
                                        )
                                    });
                                // ui.image(texture, texture.size_vec2());
                                // let size = texture.size();
                                // let fsize = [(size[0] / 2) as f32, (size[1] / 2) as f32];
                                let fsize = ui.available_size();
                                ui.add_sized(fsize, egui::widgets::Image::new(texture, fsize));
                            });
                    }
                }
            }
            None => (),
        }

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

            let sth = 21.0; // sth = standard height
            ui.style_mut().spacing.item_spacing.x = 5.0;
            StripBuilder::new(ui)
                .size(Size::exact(sth))
                .size(Size::initial(5.0).at_least(5.0))
                // .size(Size::exact(sth))
                .size(Size::remainder())
                .vertical(|mut strip| {
                    strip.strip(|strip_builder| {
                        strip_builder
                            .size(Size::exact(21.0))
                            .size(Size::exact(21.0))
                            .size(Size::exact(40.0))
                            .size(Size::remainder().at_least(150.0))
                            .size(Size::exact(40.0))
                            .size(Size::exact(80.0))
                            .size(Size::exact(36.0))
                            .horizontal(|mut strip| {
                                // pause button
                                strip.cell(|ui| {
                                    let pausebn = ui.add_sized(
                                        [ui.available_width(), sth],
                                        egui::Button::new(if self.ui.paused {
                                            "▶"
                                        } else {
                                            "⏸"
                                        }),
                                    );
                                    if pausebn.clicked() {
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
                                });
                                strip.cell(|ui| {
                                    let nextbn = ui.add_sized(
                                        [ui.available_width(), sth],
                                        egui::Button::new("⏭"),
                                    );
                                    if nextbn.clicked() {
                                        match self.queue.qu_vec.is_empty() {
                                            true => (),
                                            false => match &mut self.queue.pointer {
                                                Pointer::Start => {
                                                    self.current = None;
                                                    self.queue.pointer = Pointer::Idx(0);
                                                    if (self.queue.qu_vec.len() - 1) > 0 {
                                                        let file_path = &self.queue.qu_vec[0].path;
                                                        self.current = Some(self.new_media_stream(
                                                            file_path.to_string(),
                                                        ));
                                                    }
                                                }
                                                Pointer::Idx(idx) => {
                                                    self.current = None;
                                                    if (self.queue.qu_vec.len() - 1) >= (*idx + 1) {
                                                        *idx += 1;
                                                        let file_path =
                                                            &self.queue.qu_vec[*idx].path;
                                                        self.current = Some(self.new_media_stream(
                                                            file_path.to_string(),
                                                        ));
                                                    } else {
                                                        self.queue.pointer = Pointer::End;
                                                    }
                                                }
                                                Pointer::End => {
                                                    // self.current = None;
                                                    // self.queue.pointer = Pointer::Idx(0);
                                                    // if (self.queue.qu_vec.len() - 1) > 0 {
                                                    //     let file_path = &self.queue.qu_vec[0].path;
                                                    //     self.current = Some(self.new_media_stream(
                                                    //         file_path.to_string(),
                                                    //     ));
                                                    // } else {
                                                    self.queue.pointer = Pointer::Start;
                                                    // }
                                                }
                                            },
                                        }
                                    }
                                });
                                let mut now = match &self.current {
                                    Some(cur) => cur.as_secs(),
                                    None => 0.0,
                                };
                                let cur_full_time = match &self.current {
                                    Some(cur) => cur.metadata.full_time_secs.unwrap(),
                                    None => 0,
                                };
                                // curtime label
                                strip.cell(|ui| {
                                    ui.add_sized(
                                        [ui.available_width(), sth],
                                        egui::Label::new(format!(
                                            "{}:{:02}",
                                            (now / 60.0) as u8,
                                            (now % 60.0) as u8
                                        )),
                                    );
                                });
                                // playback slider
                                strip.cell(|ui| {
                                    let oldnow = now;
                                    ui.style_mut().spacing.slider_width = ui.available_width();
                                    ui.add_sized(
                                        [ui.available_width(), sth],
                                        egui::Slider::new(
                                            &mut now,
                                            0.0..=(cur_full_time as i64 - 1).max(0) as f64,
                                        )
                                        .show_value(false),
                                    );
                                    ui.style_mut().spacing.slider_width = 100.0; // default value
                                    if (-1.0 >= (now - oldnow)) || ((now - oldnow) >= 1.0) {
                                        self.current
                                            .as_ref()
                                            .unwrap()
                                            .seek_to(Duration::from_secs_f64(now));
                                    }
                                });
                                // fulltime label
                                strip.cell(|ui| {
                                    ui.add_sized(
                                        [ui.available_width(), sth],
                                        egui::Label::new(format!(
                                            "{}:{:02}",
                                            cur_full_time / 60,
                                            cur_full_time % 60
                                        )),
                                    );
                                });
                                let mut vol = self.volume() as i8;
                                let oldvol = vol;
                                let mut rect = Rect::NOTHING;
                                strip.cell(|ui| {
                                    ui.style_mut().spacing.slider_width = 80.0;
                                    rect = rect.union(
                                        ui.add_sized(
                                            [80.0, sth],
                                            egui::Slider::new(&mut vol, 0..=100).show_value(false),
                                        )
                                        .rect,
                                    );
                                    ui.style_mut().spacing.slider_width = 100.0;
                                    // default value
                                });
                                strip.cell(|ui| {
                                    rect = rect.union(
                                        ui.add_sized(
                                            [ui.available_width(), sth],
                                            egui::DragValue::new(&mut vol),
                                        )
                                        .rect,
                                    );
                                });
                                let scroll_delta = ctx.input().scroll_delta.y;
                                let mouse_pos = ctx.pointer_hover_pos();
                                if let Some(mouse_pos) = mouse_pos {
                                    if (scroll_delta != 0.0) && rect.contains(mouse_pos) {
                                        if scroll_delta > 0.0 {
                                            self.set_volume((self.volume() + 2).clamp(0, 100));
                                        }
                                        if scroll_delta < 0.0 {
                                            self.set_volume(
                                                (self.volume() as i8 - 2).clamp(0, 100) as u8,
                                            );
                                        }
                                    }
                                }
                                if (-1 >= (vol - oldvol)) || ((vol - oldvol) >= 1) {
                                    self.set_volume(vol as u8)
                                }
                            });
                    });
                    strip.cell(|ui| {
                        ui.separator();
                    });
                    if self.current.is_some()
                        && matches!(
                            basslib::BChannelIsActive(self.current.as_ref().unwrap()),
                            basslib::channel_state::BASS_ACTIVE_STOPPED
                        )
                    {
                        self.current = None;
                    }
                    strip.cell(|ui| {
                        // TODO вынести коммент в отдельную функцию
                        // if ui
                        //     .add_sized(
                        //         ui.available_size(),
                        //         egui::TextEdit::singleline(&mut self.ui.tmp_add_track),
                        //     )
                        //     .lost_focus()
                        // {
                        //     match Path::new(self.ui.tmp_add_track.trim_matches('"')).exists() {
                        //         true => {
                        //             self.queue.push_back(
                        //                 <TrackMetadata as std::str::FromStr>::from_str(
                        //                     &self.ui.tmp_add_track,
                        //                 )
                        //                 .unwrap(),
                        //             );
                        //             self.ui.tmp_add_track = String::from("");
                        //         }
                        //         false => (),
                        //     }
                        // }

                        for (idx, track) in enumerate(&self.queue.qu_vec) {
                            if let Pointer::Idx(pidx) = self.queue.pointer && idx == pidx {
                                ui.add(egui::Label::new(format!(">{}", track.file_stem.clone())));
                            } else {
                                ui.add(egui::Label::new(track.file_stem.clone()));
                            }
                            ui.end_row();
                        }
                    });

                    match self.current.is_some() {
                        true => (),
                        false => match self.queue.qu_vec.is_empty() {
                            true => (),
                            false => match &mut self.queue.pointer {
                                Pointer::Start => {
                                    self.current = None;
                                    self.queue.pointer = Pointer::Idx(0);
                                    let file_path = &self.queue.qu_vec[0].path;
                                    self.current =
                                        Some(self.new_media_stream(file_path.to_string()));
                                }
                                Pointer::Idx(idx) => {
                                    self.current = None;
                                    if (self.queue.qu_vec.len() - 1) >= (*idx + 1) {
                                        *idx += 1;
                                        let file_path = &self.queue.qu_vec[*idx].path;
                                        self.current =
                                            Some(self.new_media_stream(file_path.to_string()));
                                    } else {
                                        // self.queue.pointer = Pointer::End;
                                    }
                                }
                                Pointer::End => {
                                    // self.current = None;
                                    // self.queue.pointer = Pointer::Idx(0);
                                    // if (self.queue.qu_vec.len() - 1) > 0 {
                                    //     let file_path = &self.queue.qu_vec[0].path;
                                    //     self.current =
                                    //         Some(self.new_media_stream(file_path.to_string()));
                                    // }
                                }
                            },
                        },
                    }

                    preview_files_being_dropped(ctx);

                    if !ctx.input().raw.dropped_files.is_empty() {
                        self.ui.dropped_files = ctx.input().raw.dropped_files.clone();
                    }
                    if !self.ui.dropped_files.is_empty() {
                        self.ui.dropped_files.reverse();
                        for file in &self.ui.dropped_files {
                            // self.current = Some(self.new_media_stream(
                            //     file.path.as_ref().unwrap().display().to_string(),
                            // ));
                            if file
                                .path
                                .as_ref()
                                .unwrap()
                                .display()
                                .to_string()
                                .ends_with(".flac")
                                || file
                                    .path
                                    .as_ref()
                                    .unwrap()
                                    .display()
                                    .to_string()
                                    .ends_with(".mp3")
                                || file
                                    .path
                                    .as_ref()
                                    .unwrap()
                                    .display()
                                    .to_string()
                                    .ends_with(".m4a")
                            {
                                self.queue.qu_vec.push(
                                    <TrackMetadata as std::str::FromStr>::from_str(
                                        &file.path.as_ref().unwrap().display().to_string(),
                                    )
                                    .unwrap(),
                                );
                            }
                        }
                        self.ui.dropped_files = vec![];
                    }
                });
        });

        let tbrp = 1.0 / 100.0; // tbrp = time before repaint
        if tsp.elapsed().as_secs_f64() < tbrp {
            thread::sleep(Duration::from_secs_f64(tbrp - tsp.elapsed().as_secs_f64()));
        }
        ctx.request_repaint();
    }

    fn save(&mut self, _storage: &mut dyn eframe::Storage) {}

    fn on_exit(&mut self, _gl: std::option::Option<&eframe::glow::Context>) {}

    fn auto_save_interval(&self) -> Duration {
        Duration::from_secs(30)
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

    fn on_close_event(&mut self) -> bool {
        true
    }

    fn post_rendering(&mut self, _window_size_px: [u32; 2], _frame: &eframe::Frame) {}

    // fn post_rendering(&mut self, _window_size_px: [u32; 2], _frame: &eframe::Frame) {}3
}

fn preview_files_being_dropped(ctx: &egui::Context) {
    use egui::*;
    use std::fmt::Write as _;

    if !ctx.input().raw.hovered_files.is_empty() {
        let mut text = "Dropping files:\n".to_owned();
        for file in &ctx.input().raw.hovered_files {
            if let Some(path) = &file.path {
                write!(text, "\n{}", path.display()).ok();
            } else if !file.mime.is_empty() {
                write!(text, "\n{}", file.mime).ok();
            } else {
                text += "\n???";
            }
        }

        let painter =
            ctx.layer_painter(LayerId::new(Order::Foreground, Id::new("file_drop_target")));

        let screen_rect = ctx.input().screen_rect();
        painter.rect_filled(screen_rect, 0.0, Color32::from_black_alpha(192));
        painter.text(
            screen_rect.center(),
            Align2::CENTER_CENTER,
            text,
            TextStyle::Heading.resolve(&ctx.style()),
            Color32::WHITE,
        );
    }
}
