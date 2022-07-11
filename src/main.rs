use std::net::ToSocketAddrs;
use std::ops::Add;
use std::path::Path;
use std::str::FromStr;
use std::time::Duration;
use std::{io, sync::Arc};

use cpal::traits::{DeviceTrait, HostTrait};
use crossterm::{
    event::{self, poll, DisableMouseCapture, EnableMouseCapture, Event, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use itertools::Itertools;
use log::info;
use rpc::stream::TrackMetadata;
// use flexi_logger::{FileSpec, Logger, WriteMode};
use rpc::{
    stream::{Current, SourceControl},
    InputMode, Rpc,
};
use tui::widgets::{Cell, Row};
use tui::{
    backend::{Backend, CrosstermBackend},
    layout::{Constraint, Direction, Layout},
    style::{Color, Style},
    text::{Span, Spans, Text},
    widgets::{Block, Borders, Clear, List, ListItem, Paragraph, Table, TableState, Wrap},
    Frame, Terminal,
};

fn main() -> anyhow::Result<()> {
    // let _logger = Logger::try_with_str("info, my::critical::module=trace")?
    //     .log_to_file(
    //         FileSpec::default()
    //             .directory("log_files")
    //             .basename("rpc")
    //             .discriminant("def")
    //             .suffix("log"),
    //     )
    //     .write_mode(WriteMode::BufferAndFlush)
    //     .start()?;
    // debug!("started");
    info!("started");
    // error!("started");
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    // println!("start");
    let rpc = Rpc::new();
    let res = run_app(&mut terminal, rpc);
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    if let Err(err) = res {
        println!("{:?}", err)
    }
    Ok(())
}

fn run_app<B: Backend>(terminal: &mut Terminal<B>, mut rpc: Rpc) -> io::Result<()> {
    rpc.library = rpc::library::load(".library".to_string());
    loop {
        rpc.ui.ui_counter += 1;
        if rpc.ui.ui_counter >= 100 {
            rpc.ui.ui_counter = 0;
            rpc.device = Arc::new(cpal::default_host().default_output_device().unwrap());
            if let Some(cur) = rpc.current.as_mut() {
                if cur.streamer.device.name().unwrap() != rpc.device.name().unwrap() {
                    rpc.current = cur.change_device(rpc.device.clone());
                }
            }
        }
        if let Some(cur) = rpc.current.as_mut() {
            if cur.timer.as_secs() > cur.metadata.full_time_secs.unwrap() {
                match rpc.ui.repeat {
                    true => {
                        rpc.current = Some(Current::new(
                            rpc.current.unwrap().metadata.path,
                            rpc.volume.clone(),
                            rpc.ui.paused,
                            rpc.device.clone(),
                        ));
                        if !rpc.ui.paused {
                            rpc.current.as_mut().unwrap().timer.start();
                        }
                    }
                    false => match rpc.queue.is_empty() {
                        false => {
                            let track_path = rpc.queue.remove(0);
                            rpc.current = Some(Current::new(
                                track_path.path,
                                rpc.volume.clone(),
                                rpc.ui.paused,
                                rpc.device.clone(),
                            ));
                            if !rpc.ui.paused {
                                rpc.current.as_mut().unwrap().timer.start();
                            }
                        }
                        true => rpc.current = None,
                    },
                }
            }
        } else {
            match rpc.queue.is_empty() {
                false => {
                    let track_path = rpc.queue.remove(0);
                    rpc.current = Some(Current::new(
                        track_path.path,
                        rpc.volume.clone(),
                        rpc.ui.paused,
                        rpc.device.clone(),
                    ));
                    if !rpc.ui.paused {
                        rpc.current.as_mut().unwrap().timer.start();
                    }
                }
                true => (),
            }
        }

        terminal.draw(|f| ui(f, &rpc))?;
        if let Ok(true) = poll(Duration::from_millis(10)) {
            if let Event::Key(key) = event::read()? {
                let volume = rpc.volume();

                match rpc.ui.input_state {
                    InputMode::Default => match key.code {
                        KeyCode::Char('q') | KeyCode::Char('й') => {
                            return Ok(());
                        }
                        KeyCode::Char('+') => rpc.set_volume(volume as i8 + 5),
                        KeyCode::Char('-') => rpc.set_volume(volume as i8 - 5),
                        KeyCode::Char('s') | KeyCode::Char('ы') => {
                            if let Some(cur) = &rpc.current {
                                cur.streamer.control_tx.send(SourceControl::Stop).unwrap();
                            }
                            rpc.current = None;
                        }
                        KeyCode::Char('a') | KeyCode::Char('ф') => match rpc.ui.add_track {
                            true => {
                                rpc.ui.add_track = false;
                            }
                            false => {
                                rpc.ui.add_track = true;
                                rpc.ui.input_state = InputMode::AddTrack;
                            }
                        },
                        KeyCode::Char('r') => {
                            rpc.ui.repeat = !rpc.ui.repeat;
                        }
                        KeyCode::Char(' ') => match rpc.ui.paused {
                            true => {
                                rpc.ui.paused = false;
                                match &mut rpc.current {
                                    Some(cur) => {
                                        cur.streamer.paused = false;
                                        cur.timer.resume();
                                        cur.streamer.stream.play().unwrap();
                                    }
                                    None => (),
                                }
                            }
                            false => {
                                rpc.ui.paused = true;
                                match &mut rpc.current {
                                    Some(cur) => {
                                        cur.streamer.paused = true;
                                        cur.timer.pause();
                                        cur.streamer.stream.pause().unwrap();
                                    }
                                    None => (),
                                }
                            }
                        },
                        KeyCode::Right => {
                            if let Some(cur) = &mut rpc.current {
                                rpc.current = cur.seek_forward(Duration::from_secs(15));
                            }
                        }
                        KeyCode::Left => {
                            if let Some(cur) = &mut rpc.current {
                                rpc.current = cur.seek_backward(Duration::from_secs(5));
                            }
                        }
                        KeyCode::Tab => match rpc.ui.ui_state {
                            rpc::UiState::Queue => rpc.ui.ui_state = rpc::UiState::Library,
                            rpc::UiState::Library => rpc.ui.ui_state = rpc::UiState::Queue,
                        },
                        _ => {}
                    },
                    InputMode::AddTrack => match key.code {
                        KeyCode::Enter => {
                            let track_path: String = rpc.ui.tmp_add_track.drain(..).collect();
                            match Path::new(track_path.trim_matches('"')).exists() {
                                true => {
                                    rpc.queue
                                        .push(TrackMetadata::from_str(&track_path).unwrap());
                                }
                                false => (),
                            }
                            rpc.ui.cursor = 0;
                        }
                        KeyCode::Char(c) => {
                            rpc.ui.tmp_add_track.insert((rpc.ui.cursor) as usize, c);
                            rpc.ui.cursor += 1;
                        }
                        KeyCode::Backspace => match rpc.ui.cursor {
                            0 => (),
                            _ => {
                                rpc.ui.tmp_add_track.remove(rpc.ui.cursor as usize - 1);
                                rpc.ui.cursor -= 1;
                            }
                        },
                        KeyCode::Esc => {
                            rpc.ui.input_state = InputMode::Default;
                            rpc.ui.add_track = false;
                        }
                        KeyCode::Left => {
                            rpc.ui.cursor = match rpc.ui.cursor {
                                0 => 0,
                                v => v - 1,
                            };
                        }
                        KeyCode::Right => {
                            rpc.ui.cursor =
                                match rpc.ui.cursor.cmp(&(rpc.ui.tmp_add_track.len() as u16)) {
                                    std::cmp::Ordering::Less => rpc.ui.cursor + 1,
                                    std::cmp::Ordering::Equal => rpc.ui.cursor,
                                    std::cmp::Ordering::Greater => {
                                        rpc.ui.tmp_add_track.len() as u16
                                    }
                                }
                            // TODO переработать
                        }
                        _ => {}
                    },
                }
            }
        }
    }
}

fn ui<B: Backend>(f: &mut Frame<B>, rpc: &Rpc) {
    let size = f.size();
    f.render_widget(Clear, size);
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(0)
        .constraints(
            [
                Constraint::Length(2),
                // Constraint::Length(1),
                Constraint::Min(5),
                Constraint::Length(2),
            ]
            .as_ref(),
        )
        .split(size);
    // let now = rpc.ui.timer.lock().unwrap().now().elapsed_millis() / 1000;
    let now = match &rpc.current {
        Some(cur) => cur.timer.as_secs(),
        None => 0,
    };
    let cur_full_time = match &rpc.current {
        Some(cur) => cur.metadata.full_time_secs.unwrap(),
        None => 0,
    };
    let msg = format!(
        "cur vol: {}, {}:{:02}/{}:{:02} {}, {}, {}",
        rpc.volume(),
        now / 60,
        now % 60,
        cur_full_time / 60,
        cur_full_time % 60,
        match rpc.ui.repeat {
            true => "r",
            false => "s",
        },
        match rpc.ui.paused {
            true => "paused",
            false => "resumed",
        },
        match &rpc.current {
            Some(cur) => {
                let mut s = cur.metadata.file_stem.clone();
                if s.chars().count() > 50 {
                    s.truncate(50);
                    s.push_str("...");
                }
                s
            }
            None => "None".to_string(),
        }
    );
    let mut text = Text::from(Spans::from(msg));
    text.patch_style(Style::default());
    f.render_widget(
        Paragraph::new(text).block(Block::default().borders(Borders::NONE)),
        chunks[0],
    );
    match rpc.ui.ui_state {
        rpc::UiState::Queue => {
            // let queue: Vec<ListItem> = rpc
            //     .queue
            //     .iter()
            //     .enumerate()
            //     .map(|(i, m)| {
            //         let content = vec![Spans::from(Span::raw(format!("{}: {}", i, m.file_stem)))];
            //         ListItem::new(content)
            //     })
            //     .collect();
            // let queue = List::new(queue).block(
            //     Block::default()
            //         .borders(Borders::TOP)
            //         .title("Queue")
            //         .title_alignment(tui::layout::Alignment::Right),
            // );
            // f.render_widget(queue, chunks[1]);

            let selected_style = Style::default().add_modifier(tui::style::Modifier::REVERSED);
            let header_cells = ["title", "artist", "dur"]
                .iter()
                .map(|h| Cell::from(*h).style(Style::default()));
            let header = Row::new(header_cells).height(1).bottom_margin(1);
            let rows = rpc
                .queue
                .iter()
                .map(|item| {
                    let cells = [
                        Cell::from(item.title.clone().unwrap_or_else(|| item.file_stem.clone())),
                        Cell::from(item.artist.clone().unwrap_or_else(|| "None".to_string())),
                        // Cell::from(item.full_time_secs.unwrap().to_string()),
                        Cell::from(format!(
                            "{}:{:02}",
                            item.full_time_secs.unwrap() / 60,
                            item.full_time_secs.unwrap() % 60
                        )),
                    ];
                    Row::new(cells)
                })
                .collect_vec();
            let t = Table::new(rows)
                .header(header)
                .block(Block::default().borders(Borders::TOP).title("queue"))
                .highlight_style(selected_style)
                .widths(&[
                    Constraint::Percentage(33),
                    Constraint::Percentage(33),
                    Constraint::Percentage(34),
                ]);
            f.render_widget(t, chunks[1]);
        }
        rpc::UiState::Library => todo!(),
    }

    let block = Block::default().title("").borders(Borders::TOP);
    f.render_widget(block, chunks[2]);
    if rpc.ui.add_track {
        let block = Block::default()
            .title("Add track ")
            .borders(Borders::TOP)
            .title_alignment(tui::layout::Alignment::Right);
        let area = chunks[2];
        let mut chars = rpc.ui.tmp_add_track.clone();
        chars.insert(0, '>');
        let text = String::from_iter(chars);
        let text = Paragraph::new(text)
            .block(block)
            .style(match rpc.ui.input_state {
                InputMode::Default => Style::default(),
                InputMode::AddTrack => Style::default().fg(Color::Yellow),
            })
            .alignment(tui::layout::Alignment::Left)
            .wrap(Wrap {
                trim: false,
                break_word: true,
            });
        f.render_widget(tui::widgets::Clear, area); //this clears out the background
        f.render_widget(text, area);
        f.set_cursor(
            area.x + (rpc.ui.cursor as u16 % (area.width - 2)) + 1,
            area.y + (rpc.ui.cursor as u16 / (area.width - 2)) + 1,
        );
    }
}

fn centered_rect(percent_x: u16, percent_y: u16, r: tui::layout::Rect) -> tui::layout::Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints(
            [
                Constraint::Percentage((100 - percent_y) / 2),
                Constraint::Percentage(percent_y),
                Constraint::Percentage((100 - percent_y) / 2),
            ]
            .as_ref(),
        )
        .split(r);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints(
            [
                Constraint::Percentage((100 - percent_x) / 2),
                Constraint::Percentage(percent_x),
                Constraint::Percentage((100 - percent_x) / 2),
            ]
            .as_ref(),
        )
        .split(popup_layout[1])[1]
}
