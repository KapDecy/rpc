use std::{
    cell::RefCell,
    rc::{Rc, Weak},
    str::FromStr,
};

use crate::stream::TrackMetadata;

#[derive(Debug, Default)]
pub struct Node {
    pub parant: Option<Weak<RefCell<Node>>>,
    pub mvec: Vec<TrackMetadata>,
    pub nvec: Vec<Rc<RefCell<Node>>>,
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

    pub fn from_path(path: String, parent: Option<Weak<RefCell<Node>>>) -> Rc<RefCell<Node>> {
        use walkdir::WalkDir;

        let mut node = Node::default();
        node.name = path.split('\\').last().unwrap().to_string();
        let name = node.name.clone();
        if parent.is_some() {
            node.parant = parent;
        }
        let rcnode = Rc::new(RefCell::new(node));

        for entry in WalkDir::new(path)
            .max_depth(1)
            .into_iter()
            .filter_map(|e| e.ok())
            .skip(1)
        {
            match entry.metadata().unwrap().is_file() {
                true => {
                    // println!(
                    //     "{} added to {}",
                    //     entry.path().to_string_lossy(),
                    //     name.clone()
                    // );
                    match entry.path().extension().unwrap().to_str().unwrap() {
                        "flac" => {
                            rcnode.borrow_mut().mvec.push(
                                TrackMetadata::from_str(entry.path().to_str().unwrap()).unwrap(),
                            );
                        }
                        "mp3" => {
                            rcnode.borrow_mut().mvec.push(
                                TrackMetadata::from_str(entry.path().to_str().unwrap()).unwrap(),
                            );
                        }
                        _ => {}
                    }
                }
                false => {
                    rcnode.borrow_mut().nvec.push(Node::from_path(
                        entry.path().to_str().unwrap().to_string(),
                        Some(Rc::downgrade(&rcnode)),
                    ));
                }
            }
        }

        rcnode
    }
}
