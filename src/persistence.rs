//! Ordered background saves, with a bounded delay and a flush on normal exit.

use std::{io, path::Path, thread::JoinHandle};

/// One writer per file. A newer snapshot never races an older one's rename.
#[derive(Default)]
pub(crate) struct Save {
    since: Option<f64>,
    worker: Option<JoinHandle<io::Result<()>>>,
}

impl Save {
    pub(crate) fn update(
        &mut self,
        path: &Path,
        now: f64,
        changed: bool,
        exiting: bool,
        text: impl FnOnce() -> String,
    ) {
        if changed {
            // Measure from the first unsaved change: continuous changes must
            // not postpone saving forever.
            self.since.get_or_insert(now);
        }
        if self
            .worker
            .as_ref()
            .is_some_and(|w| exiting || w.is_finished())
        {
            let result = self
                .worker
                .take()
                .unwrap()
                .join()
                .unwrap_or_else(|_| Err(io::Error::other("save thread panicked")));
            if let Err(error) = result {
                bevy::log::warn!("cannot save {}: {error}", path.display());
                self.since.get_or_insert(now);
            }
        }
        if self.worker.is_some()
            || !self
                .since
                .is_some_and(|since| exiting || now - since >= 1.0)
        {
            return;
        }
        let text = text();
        let result = if exiting {
            write(path, &text)
        } else {
            let path = path.to_path_buf();
            std::thread::Builder::new()
                .name("save".into())
                .spawn(move || write(&path, &text))
                .map(|worker| self.worker = Some(worker))
        };
        self.since = if let Err(error) = result {
            bevy::log::warn!("cannot save {}: {error}", path.display());
            Some(now)
        } else {
            None
        };
    }
}

impl Drop for Save {
    fn drop(&mut self) {
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

fn write(path: &Path, text: &str) -> io::Result<()> {
    std::fs::create_dir_all(path.parent().unwrap_or(path))?;
    let beside = path.with_extension("writing");
    std::fs::write(&beside, text)?;
    std::fs::rename(beside, path)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn folder(name: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!("todora-save-{}-{name}", std::process::id()))
    }

    #[test]
    fn quitting_before_the_delay_saves_the_latest_choice() {
        let folder = folder("quit");
        let path = folder.join("settings.json");
        let mut save = Save::default();
        save.update(&path, 0.0, true, false, || panic!("not due yet"));
        save.update(&path, 0.1, true, true, || "latest".into());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "latest");
        std::fs::remove_dir_all(folder).unwrap();
    }

    #[test]
    fn continuous_changes_are_saved_and_exit_waits_for_the_previous_write() {
        let folder = folder("continuous");
        let path = folder.join("achievements.json");
        let mut save = Save::default();
        for step in 0..10 {
            save.update(&path, f64::from(step) / 10.0, true, false, || {
                panic!("not due yet")
            });
        }
        save.update(&path, 1.0, true, false, || "first".into());
        assert!(
            save.worker.is_some(),
            "continuous changes still start a save"
        );
        save.update(&path, 1.1, true, true, || "latest".into());
        assert!(save.worker.is_none());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "latest");
        std::fs::remove_dir_all(folder).unwrap();
    }

    #[test]
    fn failed_writes_retry_without_another_change() {
        let folder = folder("retry");
        std::fs::write(&folder, "a file blocks the directory").unwrap();
        let path = folder.join("settings.json");
        let mut save = Save::default();
        save.update(&path, 0.0, true, true, || "latest".into());
        assert!(save.since.is_some());
        std::fs::remove_file(&folder).unwrap();
        save.update(&path, 1.0, false, true, || "latest".into());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "latest");
        assert!(save.since.is_none());
        std::fs::remove_dir_all(folder).unwrap();
    }
}
