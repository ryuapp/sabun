use std::{
    collections::HashSet,
    sync::mpsc::{Receiver, TryRecvError, channel},
    time::Duration,
};

use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};

use super::{Context, DiffViewer, FileViewData, Input, WatchRequest, point, px};

const WATCH_DEBOUNCE: Duration = Duration::from_millis(250);

impl DiffViewer {
    pub(super) fn start_watch(
        &self,
        request: WatchRequest,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let (sender, receiver) = channel();
        let mut watcher = notify::recommended_watcher(move |event: notify::Result<Event>| {
            if event.as_ref().is_ok_and(should_reload) {
                let _ = sender.send(());
            }
        })
        .map_err(|error| format!("Could not create filesystem watcher: {error}"))?;

        for path in request.paths() {
            watcher
                .watch(path, RecursiveMode::Recursive)
                .map_err(|error| format!("Could not watch {}: {error}", path.display()))?;
        }

        cx.spawn(async move |viewer, cx| {
            let _watcher: RecommendedWatcher = watcher;
            loop {
                cx.background_executor().timer(WATCH_DEBOUNCE).await;
                if !take_pending_event(&receiver) {
                    continue;
                }
                let reload = request.clone();
                let result = cx
                    .background_executor()
                    .spawn(async move { reload.reload() })
                    .await;
                if viewer
                    .update(cx, |viewer, cx| {
                        if let Ok(input) = result {
                            viewer.apply_watched_input(input, cx);
                        }
                    })
                    .is_err()
                {
                    break;
                }
            }
        })
        .detach();
        Ok(())
    }

    fn apply_watched_input(&mut self, input: Input, cx: &mut Context<Self>) {
        if self.diff == input.diff
            && self.path_root == input.path_root
            && self.source_name == input.source_name
            && self.comparison_label == input.comparison_label
            && self.target_label == input.target_label
        {
            return;
        }

        let selected_path = self
            .diff
            .files
            .get(self.selected_file)
            .map(|file| file.display_path().to_owned());
        let collapsed_paths = self
            .collapsed_files
            .iter()
            .filter_map(|index| self.diff.files.get(*index))
            .map(|file| file.display_path().to_owned())
            .collect::<HashSet<_>>();

        self.diff = input.diff;
        self.path_root = input.path_root;
        self.source_name = input.source_name;
        self.comparison_label = input.comparison_label;
        self.target_label = input.target_label;
        self.empty_title = input.empty_title;
        self.empty_detail = input.empty_detail;
        self.selected_file = selected_path
            .as_deref()
            .and_then(|path| {
                self.diff
                    .files
                    .iter()
                    .position(|file| file.display_path() == path)
            })
            .unwrap_or(0)
            .min(self.diff.files.len().saturating_sub(1));
        self.collapsed_files = self
            .diff
            .files
            .iter()
            .enumerate()
            .filter_map(|(index, file)| {
                collapsed_paths
                    .contains(file.display_path())
                    .then_some(index)
            })
            .collect();
        self.context_expansions.clear();
        self.text_selection = None;
        self.header_text_selection = None;
        self.text_context_menu = None;
        self.path_context_menu = None;
        self.copy_path_feedback = None;
        let file_view_data = FileViewData::from_files(&self.diff.files, self.path_root.as_deref());
        self.file_meta = file_view_data.meta;
        self.file_stats = file_view_data.stats;
        self.total_additions = file_view_data.total_additions;
        self.total_deletions = file_view_data.total_deletions;
        self.rebuild_file_tree();
        self.cancel_diff_layout_zoom();
        self.rebuild_diff_row_data();
        self.retain_valid_syntax_cache();
        self.diff_smooth_scroll.reset();
        self.pending_scroll_file = (!self.diff.files.is_empty()).then_some(self.selected_file);
        if self.diff.files.is_empty() {
            self.diff_scroll.set_offset(point(px(0.), px(0.)));
        }
        cx.notify();
    }
}

const fn should_reload(event: &Event) -> bool {
    !matches!(event.kind, EventKind::Access(_))
}

fn take_pending_event(receiver: &Receiver<()>) -> bool {
    match receiver.try_recv() {
        Ok(()) => {
            while receiver.try_recv().is_ok() {}
            true
        }
        Err(TryRecvError::Empty | TryRecvError::Disconnected) => false,
    }
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        sync::mpsc::channel,
        time::{Duration, Instant, SystemTime, UNIX_EPOCH},
    };

    use notify::{Event, RecursiveMode, Watcher};

    use super::should_reload;

    #[test]
    fn filesystem_change_emits_a_reload_event() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!("sabun-watch-{unique}"));
        fs::create_dir(&root).unwrap();

        let (sender, receiver) = channel::<notify::Result<Event>>();
        let mut watcher = notify::recommended_watcher(sender).unwrap();
        watcher.watch(&root, RecursiveMode::Recursive).unwrap();
        let changed = root.join("changed.txt");
        fs::write(&changed, "changed\n").unwrap();

        let deadline = Instant::now() + Duration::from_secs(5);
        let saw_change = loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                break false;
            }
            let Ok(event) = receiver.recv_timeout(remaining) else {
                break false;
            };
            if event.as_ref().is_ok_and(|event| {
                should_reload(event)
                    && event
                        .paths
                        .iter()
                        .any(|path| path.file_name() == changed.file_name())
            }) {
                break true;
            }
        };

        drop(watcher);
        fs::remove_dir_all(&root).unwrap();
        assert!(saw_change, "no filesystem change event received");
    }
}
