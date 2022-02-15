use std::io;
use std::time::Duration;

use crossterm::{
    event::{self, poll, DisableMouseCapture, EnableMouseCapture, Event, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use rpc::*;
use tui::{
    backend::{Backend, CrosstermBackend},
    layout::{Constraint, Direction, Layout},
    style::{Color, Style},
    text::{Spans, Text},
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
    Frame, Terminal,
};

fn main() -> anyhow::Result<()> {
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
    loop {
        // match &rpc.current {
        //     Some(current) => {
        //         if !current.is_running() {
        //             rpc.current = None;
        //             rpc.curtimer = Some(Arc::new(PausableClock::default()));
        //             rpc.curtimer.as_ref().unwrap().pause();
        //         }
        //     }
        //     None => {
        //         if !rpc.queue.is_empty() {
        //             rpc.start_next_track()
        //         }
        //     }
        // }
        terminal.draw(|f| ui(f, &rpc))?;
        if let Ok(true) = poll(Duration::from_millis(100)) {
            if let Event::Key(key) = event::read()? {
                let volume = rpc.volume();
                match rpc.ui.ui_state {
                    InputMode::Normal => match key.code {
                        KeyCode::Char('q') | KeyCode::Char('й') => {
                            return Ok(());
                        }
                        KeyCode::Char('+') => rpc.set_volume(volume as i8 + 5),
                        KeyCode::Char('-') => rpc.set_volume(volume as i8 - 5),
                        KeyCode::Char('a') => match rpc.ui.add_track {
                            true => {
                                rpc.ui.add_track = false;
                            }
                            false => {
                                rpc.ui.add_track = true;
                                rpc.ui.ui_state = InputMode::AddTrack;
                            }
                        },
                        KeyCode::Char(' ') => {
                            match rpc.ui.paused.load(std::sync::atomic::Ordering::Relaxed) {
                                true => {
                                    rpc.ui.control_tx.send(Control::Resume).unwrap();
                                    rpc.ui
                                        .paused
                                        .store(false, std::sync::atomic::Ordering::SeqCst);
                                }
                                false => {
                                    rpc.ui.control_tx.send(Control::Pause).unwrap();
                                    rpc.ui
                                        .paused
                                        .store(true, std::sync::atomic::Ordering::SeqCst);
                                }
                            }
                        }
                        _ => {}
                    },
                    InputMode::AddTrack => match key.code {
                        KeyCode::Enter => {
                            let track_path = rpc.ui.tmp_add_track.drain(..).collect();
                            rpc.ui
                                .control_tx
                                .send(Control::AddTrack(track_path))
                                .unwrap();
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
                            rpc.ui.ui_state = InputMode::Normal;
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
        .margin(1)
        .constraints([Constraint::Length(1)].as_ref())
        .split(size);
    let msg = format!(
        "cur vol: {}, cur time: {}:{:02}",
        rpc.volume(),
        rpc.ui.timer.load(std::sync::atomic::Ordering::Relaxed) / 60,
        rpc.ui.timer.load(std::sync::atomic::Ordering::Relaxed) % 60,
    );
    let mut text = Text::from(Spans::from(msg));
    text.patch_style(Style::default());
    f.render_widget(Paragraph::new(text), chunks[0]);
    if rpc.ui.add_track {
        let block = Block::default().title("Add track").borders(Borders::ALL);
        let area = centered_rect(60, 20, size);
        let text = String::from_iter(rpc.ui.tmp_add_track.clone());
        let text = Paragraph::new(text)
            .block(block)
            .style(match rpc.ui.ui_state {
                InputMode::Normal => Style::default(),
                InputMode::AddTrack => Style::default().fg(Color::Yellow),
            })
            .alignment(tui::layout::Alignment::Left)
            .wrap(Wrap { trim: false });
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
