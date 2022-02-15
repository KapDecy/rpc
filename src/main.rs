// use std::io;
use std::time::Duration;

// use crossterm::{
//     event::{self, poll, DisableMouseCapture, EnableMouseCapture, Event, KeyCode},
//     execute,
//     terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
// };
use rpc::*;
// use tui::backend::CrosstermBackend;
// use tui::Terminal;

fn main() -> anyhow::Result<()> {
    // enable_raw_mode()?;
    // let mut stdout = io::stdout();
    // execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    // let backend = CrosstermBackend::new(stdout);
    // let mut terminal = Terminal::new(backend)?;
    println!("start");
    let rpc = Rpc::new();
    // std::thread::sleep(Duration::from_secs(1));
    println!("rpc created");
    rpc.front_tx
        .send(Control::AddTrack(
            r"D:\From Torrent\Музыка\tydal\Album\Doja Cat\Hot Pink\05 - Doja Cat - Say So.flac"
                .to_string(),
        ))
        .unwrap();
    println!("rpc: track_path sended");
    rpc.front_tx.send(Control::NextTrack).unwrap();
    println!("rpc: nextTrackCommand sended");
    std::thread::sleep(Duration::from_secs(600));
    // rpc.front_tx.send(Control::NextTrack).unwrap();
    // let mut rpc = Rpc::new();
    // let res = run_app(&mut terminal, rpc);
    // rpc.start();
    // disable_raw_mode()?;
    // execute!(
    //     terminal.backend_mut(),
    //     LeaveAlternateScreen,
    //     DisableMouseCapture
    // )?;
    // terminal.show_cursor()?;

    // if let Err(err) = res {
    //     println!("{:?}", err)
    // }
    Ok(())
}
