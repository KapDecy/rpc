use std::{
    str::FromStr,
    sync::{Arc, Mutex, Weak},
};

use crate::stream::TrackMetadata;

use rayon::prelude::*;

#[derive(Debug, Default)]
pub struct Node {
    pub parant: Option<Weak<Mutex<Node>>>,
    pub mvec: Vec<TrackMetadata>,
    pub nvec: Vec<Arc<Mutex<Node>>>,
    pub name: String,
}

impl Node {
    pub fn root() -> Node {
        Node {
            parant: None,
            mvec: vec![],
            nvec: vec![],
            name: "root".to_string(),
        }
    }

    pub fn from_path(path: String, parent: Option<Weak<Mutex<Node>>>) -> Arc<Mutex<Node>> {
        use walkdir::WalkDir;

        let mut node = Node::default();
        node.name = path.split('\\').last().unwrap().to_string();
        let name = node.name.clone();
        if parent.is_some() {
            node.parant = parent;
        }
        let rcnode = Arc::new(Mutex::new(node));

        let _v: Vec<_> = WalkDir::new(path)
            .max_depth(1)
            .into_iter()
            .filter_map(|e| e.ok())
            .skip(1)
            .par_bridge()
            .map(|entry| {
                match entry.metadata().unwrap().is_file() {
                    true => {
                        // println!(
                        //     "{} added to {}",
                        //     entry.path().to_string_lossy(),
                        //     name.clone()
                        // );
                        match entry.path().extension().unwrap().to_str().unwrap() {
                            "flac" => {
                                rcnode.lock().unwrap().mvec.push(
                                    TrackMetadata::from_str(entry.path().to_str().unwrap())
                                        .unwrap(),
                                );
                            }
                            "mp3" => {
                                rcnode.lock().unwrap().mvec.push(
                                    TrackMetadata::from_str(entry.path().to_str().unwrap())
                                        .unwrap(),
                                );
                            }
                            _ => {}
                        }
                    }
                    false => {
                        rcnode.lock().unwrap().nvec.push(Node::from_path(
                            entry.path().to_str().unwrap().to_string(),
                            Some(Arc::downgrade(&rcnode)),
                        ));
                    }
                }
            })
            .collect();

        rcnode
    }
}
