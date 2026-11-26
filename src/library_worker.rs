//! SQLite is owned exclusively by this worker; no database calls run in the event loop.
use rust_browser::{
    library::{Action, Data, Database, Filter},
    suggestions::Suggestions,
};
use std::{
    path::PathBuf,
    sync::mpsc::{self, SyncSender},
    thread::JoinHandle,
};

pub struct Job {
    pub revision: u64,
    pub action: Action,
    pub filter: Filter,
}
#[derive(Debug)]
pub struct Reply {
    pub revision: u64,
    pub data: Option<Box<Data>>,
    pub message: Option<String>,
    /// What an import did, shown as a notice (not a failure).
    pub summary: Option<String>,
    /// The answer to `Action::Suggest`; such a reply carries nothing else.
    pub suggestions: Option<Suggestions>,
}
pub struct Worker {
    sender: Option<SyncSender<Job>>,
    thread: Option<JoinHandle<()>>,
}

impl Worker {
    /// Open the database at `path` on its own thread. Every answer goes to
    /// `notify`, on that thread (the caller forwards it to its event loop).
    pub fn start(path: PathBuf, notify: impl Fn(Reply) + Send + 'static) -> std::io::Result<Self> {
        let (sender, receiver) = mpsc::sync_channel::<Job>(64);
        let thread = std::thread::Builder::new().name("library-database".into()).spawn(move || {
            let (mut database, recovered) = match Database::open(&path) {
                Ok(result) => result,
                Err(_) => { notify(Reply { revision: 0, data: None, summary: None, suggestions: None,
                    message: Some("Library database is unavailable or uses a newer schema. Its files were preserved; check profile access or use the newer browser.".into()) }); return; }
            };
            if recovered { notify(Reply { revision: 0, data: None, summary: None, suggestions: None, message: Some("Corrupted library was backed up; a new database was created.".into()) }); }
            while let Ok(job) = receiver.recv() {
                if let Action::ImportBookmarks(parsed) = &job.action {
                    let reply = match database.import_bookmarks(parsed) {
                        Ok(summary) => Reply { revision: job.revision, data: database.query(&job.filter).ok().map(Box::new), message: None, summary: Some(summary.message()), suggestions: None },
                        Err(error) => {
                            let text = error.to_string();
                            Reply { revision: job.revision, data: None, summary: None, suggestions: None, message: Some(if text.contains("nothing was imported") { text } else { "Bookmark import failed, so nothing was imported. Check profile access and disk space.".into() }) }
                        }
                    };
                    notify(reply);
                    continue;
                }
                if let Action::Suggest { text, window } = &job.action {
                    // A failed lookup shows nothing rather than a notice: the
                    // field still works without suggestions.
                    let mut found = database.suggest(text, rust_browser::library::now()).unwrap_or_default();
                    found.text.clone_from(text);
                    found.window = *window;
                    notify(Reply { revision: job.revision, data: None, message: None, summary: None, suggestions: Some(found) });
                    continue;
                }
                let record = matches!(job.action, Action::Record { .. });
                let result = database.apply(job.action).and_then(|()| if record { Ok(None) } else { database.query(&job.filter).map(|d| Some(Box::new(d))) });
                let reply = match result {
                    Ok(data) => Reply { revision: job.revision, data, message: None, summary: None, suggestions: None },
                    Err(_) => Reply { revision: job.revision, data: None, summary: None, suggestions: None, message: Some("Library action failed. Check the values, duplicate URLs/folder names, date range and profile access.".into()) },
                };
                if !record || reply.message.is_some() { notify(reply); }
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
