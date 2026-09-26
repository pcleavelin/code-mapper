use std::sync::mpsc::{Receiver, TryRecvError, channel};
use std::thread;

pub(crate) struct Job<T>(Receiver<T>);

impl<T: Send + 'static> Job<T> {
    pub(crate) fn start(work: impl FnOnce() -> T + Send + 'static) -> Self {
        let (sender, receiver) = channel();
        spawn(move || drop(sender.send(work())));
        Self(receiver)
    }
}

pub(crate) enum Landing<T> {
    Waiting,
    Landed(T),
    Failed,
}

pub(crate) fn landed<T>(job: &mut Option<Job<T>>) -> Landing<T> {
    let Some(running) = job.as_ref() else {
        return Landing::Waiting;
    };
    let landing = match running.0.try_recv() {
        Err(TryRecvError::Empty) => return Landing::Waiting,
        Ok(result) => Landing::Landed(result),
        Err(TryRecvError::Disconnected) => Landing::Failed,
    };
    *job = None;
    landing
}

pub(crate) fn service(body: impl FnOnce() + Send + 'static) {
    spawn(body);
}

#[expect(
    clippy::disallowed_methods,
    reason = "the window's background jobs and services start their threads here"
)]
fn spawn(body: impl FnOnce() + Send + 'static) {
    drop(thread::spawn(body));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_job_that_panics_lands_as_a_failure() {
        let mut job = Some(Job::start(|| -> u32 { panic!("the job failed") }));
        loop {
            match landed(&mut job) {
                Landing::Waiting => thread::yield_now(),
                Landing::Landed(_) => panic!("a panicking job landed a result"),
                Landing::Failed => break,
            }
        }
        assert!(job.is_none());
    }

    #[test]
    fn a_job_lands_its_result_once() {
        let mut job = Some(Job::start(|| 7_u32));
        let result = loop {
            match landed(&mut job) {
                Landing::Waiting => thread::yield_now(),
                Landing::Landed(result) => break result,
                Landing::Failed => panic!("the job failed"),
            }
        };
        assert_eq!(result, 7);
        assert!(matches!(landed(&mut job), Landing::Waiting));
    }
}
