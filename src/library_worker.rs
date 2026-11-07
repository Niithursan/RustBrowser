//! SQLite is owned exclusively by this worker; no database calls run in the event loop.
use crate::app::Event;
use rust_browser::library::{Action, Data, Database, Filter};
use std::{
    path::PathBuf,
    sync::mpsc::{self, SyncSender},
    thread::JoinHandle,
};
use winit::event_loop::EventLoopProxy;

pub struct Job {
    pub revision: u64,
    pub action: Action,
    pub filter: Filter,
}
#[derive(Debug)]
pub struct Reply {
    pub revision: u64,
    pub data: Option<Data>,
    pub message: Option<String>,
}
pub struct Worker {
    sender: Option<SyncSender<Job>>,
    thread: Option<JoinHandle<()>>,
}

impl Worker {
    pub fn start(path: PathBuf, proxy: EventLoopProxy<Event>) -> std::io::Result<Self> {
        let (sender, receiver) = mpsc::sync_channel::<Job>(64);
        let thread = std::thread::Builder::new().name("library-database".into()).spawn(move || {
            let (mut database, recovered) = match Database::open(&path) {
                Ok(result) => result,
                Err(_) => { let _ = proxy.send_event(Event::Library(Reply { revision: 0, data: None,
                    message: Some("Library database is unavailable or uses a newer schema. Its files were preserved; check profile access or use the newer browser.".into()) })); return; }
            };
            if recovered { let _ = proxy.send_event(Event::Library(Reply { revision: 0, data: None, message: Some("Corrupted library was backed up; a new database was created.".into()) })); }
            while let Ok(job) = receiver.recv() {
                let record = matches!(job.action, Action::Record { .. });
                let result = database.apply(job.action).and_then(|()| if record { Ok(None) } else { database.query(&job.filter).map(Some) });
                let reply = match result {
                    Ok(data) => Reply { revision: job.revision, data, message: None },
                    Err(_) => Reply { revision: job.revision, data: None, message: Some("Library action failed. Check the values, duplicate URLs/folder names, date range and profile access.".into()) },
                };
                if !record || reply.message.is_some() { let _ = proxy.send_event(Event::Library(reply)); }
            }
        })?;
        Ok(Self {
            sender: Some(sender),
            thread: Some(thread),
        })
    }

    pub fn submit(&self, job: Job) -> Result<(), &'static str> {
        self.sender
            .as_ref()
            .ok_or("Library database is closed.")?
            .try_send(job)
            .map_err(|_| "Library is busy or unavailable. Try again.")
    }
}

impl Drop for Worker {
    fn drop(&mut self) {
        self.sender.take();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}
