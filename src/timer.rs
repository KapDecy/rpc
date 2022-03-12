use std::{
    cell::Cell,
    ops,
    time::{Duration, Instant},
};

#[derive(Debug, Clone)]
pub struct Timer {
    times: Cell<Duration>,
    current: Cell<Option<Instant>>,
    paused: bool,
}

impl Timer {
    pub fn new() -> Timer {
        Timer {
            times: Cell::new(Duration::ZERO),
            current: Cell::new(None),
            paused: true,
        }
    }

    pub fn start(&mut self) {
        self.current.set(Some(Instant::now()));
        self.paused = false
    }

    pub fn pause(&mut self) {
        if let Some(inst) = self.current.get() {
            // self.times += inst.elapsed();
            self.times.update(|x| x + inst.elapsed());
            self.current.set(None);
            self.paused = true;
        }
    }

    pub fn resume(&mut self) {
        match self.current.get() {
            Some(_) => (),
            None => {
                self.current.set(Some(Instant::now()));
                self.paused = false;
            }
        }
    }

    pub fn reset(&mut self) {
        self.times = Cell::new(Duration::ZERO);
        self.current.set(None);
        self.paused = true;
    }

    pub fn update(&self) {
        if !self.paused {
            if let Some(inst) = self.current.get() {
                let end = Instant::now();
                // self.times += end - inst;
                self.times.update(|x| x + (end - inst));
                self.current.set(Some(end));
            }
        }
    }

    pub fn as_secs(&self) -> u64 {
        self.update();
        self.times.get().as_secs()
    }

    pub fn add(&mut self, dur: Duration) {
        self.update();
        self.times.update(|x| x + dur);
    }

    pub fn sub(&mut self, dur: Duration) {
        self.update();
        if self.times.get() >= dur {
            // self.times -= dur;
            self.times.update(|x| x - dur);
        } else {
            self.times.set(Duration::ZERO);
        }
    }
}

impl Default for Timer {
    fn default() -> Self {
        Self::new()
    }
}

impl ops::AddAssign<Duration> for Timer {
    fn add_assign(&mut self, rhs: Duration) {
        self.add(rhs);
    }
}

impl ops::SubAssign<Duration> for Timer {
    fn sub_assign(&mut self, rhs: Duration) {
        self.sub(rhs);
    }
}
