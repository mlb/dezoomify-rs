use std::fmt;
use std::fmt::Formatter;
use std::time::{Instant, Duration};

#[derive(Debug)]
pub struct Progress {
    current : usize,
    finish : usize,
    modulo : usize,
    last_progress : usize,
    start : Instant,
    pub elapsed : Duration,
    finished : bool
}

impl Progress {
    pub fn new(finish : usize, modulo : usize) -> Progress {
        Progress {
            current : 0,
            finish,
            modulo,
            last_progress : 0,
            start : Instant::now(),
            elapsed : Duration::from_secs(0),
            finished : false
        }
    }

    pub fn start(&mut self) {
        self.start = Instant::now();
    }

    pub fn advance(&mut self, current : usize) -> bool {
        self.current = current;
        self.elapsed = self.start.elapsed();

        let progress = (self.current as f32 / self.finish as f32) * 100.0;
        if progress as usize % self.modulo == 0 {
            if progress as usize > self.last_progress {
                self.last_progress = progress as usize;
                return true
            }
        }
        false
    }

    pub fn finish(&mut self) -> Duration {
        self.finished = true;
        self.elapsed = self.start.elapsed();
        self.elapsed
    }
}

impl fmt::Display for Progress {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let est = ((self.elapsed.as_secs() as f32 / self.current as f32) * (self.finish as f32 - self.current as f32)) as u64;
        let est_h = est / 60 / 60;
        let est_m = (est - (est_h * 60 * 60)) / 60;
        let est_s = est - (est_m * 60);

        let tot = self.elapsed.as_secs();
        let tot_h = tot / 60 / 60;
        let tot_m = (tot - (tot_h * 60 * 60)) / 60;
        let tot_s = tot - (tot_m * 60);

        let mut progress = String::new();

        if self.finished {
            progress.push_str("% 100");
        } else {
            progress.push_str(&*format!("% {:3.0}", self.current as f32 / self.finish as f32 * 100.0));
        }
        if est_h == 0 {
            progress.push_str(&*format!("  Time: {:02}:{:02}", tot_m, tot_s));
        } else {
            progress.push_str(&*format!("  Time: {:02}:{:02}:{:02}", tot_h, tot_m, tot_s));
        }
        if !self.finished {
            if est_h == 0 {
                progress.push_str(&*format!("  Left: {:02}:{:02}", est_m, est_s));
            } else {
                progress.push_str(&*format!("  Left: {:02}:{:02}:{:02}", est_h, est_m, est_s));
            }
        }

        write!(f, "{}", progress)
    }
}
