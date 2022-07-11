struct Library {
    items: Folder,
}

struct Folder {
    parant: Option<Weak<Folder>>,
    items: Vec<Rc<LibObject>>,
}

enum LibObject {
    Folder(Folder),
    Audio(TrackMetadata),
    Cue(CueMetadata),
}

struct CueMetadata {}

pub fn parse_directory(path: String, parant: Weak<Folder>) -> Folder {
    let mut dir = LibObject::Folder(Folder {
        parant: Some(parant),
        items: Vec::new(),
    });
    if let LibObject::Folder(ref mut v) = dir {
        for entry in WalkDir::new(path).into_iter().filter_map(|e| e.ok()) {
            if entry.file_type().is_dir() {
                v.push(parse_directory(
                    entry.path().as_os_str().to_string_lossy().to_string(),
                ));
            } else if entry.file_type().is_file()
                && ["flac", "mp3", "wav"].contains(
                    entry
                        .path()
                        .to_string_lossy()
                        .to_string()
                        .split('.')
                        .collect_vec()
                        .last()
                        .unwrap(),
                )
            {
                v.push(LibObject::Audio(
                    TrackMetadata::from_str(&entry.path().to_string_lossy().to_string()).unwrap(),
                ));
            }
        }
    }
    dir
}

// use crate::stream::TrackMetadata;
// use itertools::Itertools;
// use serde::{Deserialize, Serialize};
// use tui::widgets::ListState;
// use std::{fs, str::FromStr};
// use walkdir::WalkDir;

// struct Library {
//     items: Vec<LibObject>,
//     selected: LibObject,
//     state: ListState,
// }

// #[derive(Serialize, Deserialize)]
// pub struct CueMetadata {}

// #[derive(Serialize, Deserialize)]
// pub enum LibObject {
//     Folder(Vec<LibObject>),
//     Audio(TrackMetadata),
//     Cue(CueMetadata),
// }

// pub fn load(path: String) -> Vec<LibObject> {
//     let data = match fs::read_to_string(path.clone()) {
//         Ok(d) => d,
//         Err(_) => {
//             fs::File::create(path).unwrap();
//             String::new()
//         }
//     };
//     let v: Vec<LibObject> = serde_json::from_str(data.as_str()).unwrap();
//     v
// }

// pub fn write(path: String, data: Vec<LibObject>) {
//     let s = serde_json::to_string_pretty(&data).unwrap();
//     fs::write(path, s).unwrap();
// }

//     dir
// }

use std::rc::{Rc, Weak};

use walkdir::WalkDir;

use crate::stream::TrackMetadata;
