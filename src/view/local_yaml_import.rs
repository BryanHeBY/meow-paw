use super::{BLUE, Context, Div, ExternalPaths, FAINT, LINE, Locale, Message, Paw, SOFT_BLUE};
use gpui_kit::*;
use meow_paw::profile::local_import::{StagedYaml, stage_yaml};
use std::path::{Path, PathBuf};

const MAX_BATCH: usize = 16;

#[derive(Default)]
pub(super) enum State {
    #[default]
    Idle,
    Picking,
    Importing,
    PickerFailed,
    SaveFailed,
    Finished {
        added: usize,
        rejected: usize,
    },
}

impl State {
    fn busy(&self) -> bool {
        matches!(self, Self::Picking | Self::Importing)
    }
}

fn stage_batch(paths: Vec<PathBuf>, directory: &Path) -> (Vec<StagedYaml>, usize) {
    let count = paths.len();
    let staged: Vec<_> = paths
        .into_iter()
        .take(MAX_BATCH)
        .filter_map(|path| stage_yaml(&path, directory).ok())
        .collect();
    let rejected = count - staged.len();
    (staged, rejected)
}

impl Paw {
    pub(super) fn local_yaml_import_panel(&self, cx: &mut Context<Self>) -> Option<Div> {
        if !self.preferences.has_persistent_file() {
            return None;
        }
        let busy = self.local_yaml_import.busy() || self.quit_pending();
        let mut zone = div()
            .flex()
            .flex_wrap()
            .items_center()
            .justify_between()
            .gap_2()
            .px_3()
            .py_2()
            .rounded_md()
            .border_1()
            .border_color(rgb(LINE))
            .child(
                div()
                    .text_sm()
                    .text_color(rgb(FAINT))
                    .child(self.locale.message(Message::DropYamlCopies)),
            )
            .child(
                super::secondary_button(
                    "import-yaml-copies",
                    self.locale.message(Message::ImportYamlCopies),
                )
                .disabled(busy)
                .on_click(cx.listener(|this, _, _, cx| this.choose_yaml_copies(cx))),
            );
        if !busy {
            zone = zone
                .drag_over::<ExternalPaths>(|style, _, _, _| {
                    style.bg(rgb(SOFT_BLUE)).border_color(rgb(BLUE))
                })
                .on_drop(cx.listener(|this, drop: &ExternalPaths, _, cx| {
                    this.import_yaml_copies(drop.paths().to_vec(), cx);
                }));
        }
        let notice = match self.local_yaml_import {
            State::Idle | State::Picking => None,
            State::Importing => Some(self.locale.message(Message::ImportingYamlCopies).into()),
            State::PickerFailed => Some(self.locale.message(Message::LocalYamlPickerFailed).into()),
            State::SaveFailed => Some(self.locale.message(Message::LocalYamlImportFailed).into()),
            State::Finished { added, rejected } => Some(match self.locale {
                Locale::En => format!("Imported: {added}. Not imported: {rejected}."),
                Locale::ZhCn => format!("已导入 {added}。未导入 {rejected}。"),
            }),
        };
        Some(
            div()
                .flex()
                .flex_col()
                .gap_1()
                .child(zone)
                .children(notice.map(|text| div().text_sm().text_color(rgb(FAINT)).child(text))),
        )
    }

    fn choose_yaml_copies(&mut self, cx: &mut Context<Self>) {
        if self.local_yaml_import.busy() || self.quit_pending() {
            return;
        }
        self.local_yaml_import = State::Picking;
        cx.notify();
        let reply = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: true,
            prompt: Some(self.locale.message(Message::PickYamlCopies).into()),
        });
        cx.spawn(async move |weak, cx| {
            let reply = reply.await;
            let _ = weak.update(cx, |paw, cx| {
                if !matches!(paw.local_yaml_import, State::Picking) {
                    return;
                }
                match reply {
                    Ok(Ok(paths)) => {
                        paw.local_yaml_import = State::Idle;
                        if let Some(paths) = paths {
                            paw.import_yaml_copies(paths, cx);
                        }
                    }
                    _ => paw.local_yaml_import = State::PickerFailed,
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn import_yaml_copies(&mut self, paths: Vec<PathBuf>, cx: &mut Context<Self>) {
        if paths.is_empty() || self.local_yaml_import.busy() || self.quit_pending() {
            return;
        }
        let Some(directory) = self
            .preferences
            .subscription_dir()
            .map(|p| p.with_file_name("local-profiles"))
        else {
            return;
        };
        self.local_yaml_import = State::Importing;
        cx.notify();
        cx.spawn(async move |weak, cx| {
            let (staged, rejected) = cx
                .background_executor()
                .spawn(async move { stage_batch(paths, &directory) })
                .await;
            let _ = weak.update(cx, |paw, cx| {
                if paw.quit_pending() {
                    return;
                }
                let added = staged.len();
                paw.local_yaml_import = match paw.preferences.register_local_imports(staged) {
                    Ok(()) => State::Finished { added, rejected },
                    Err(_) => State::SaveFailed,
                };
                cx.notify();
            });
        })
        .detach();
    }
}

#[cfg(test)]
mod tests {
    use super::{MAX_BATCH, stage_batch};

    #[test]
    fn local_drop_counts_mixed_and_excess_items_without_touching_sources() {
        let root = tempfile::tempdir().unwrap();
        let source = root.path().join("A \u{6d4b}\u{8bd5}.yaml");
        let invalid = root.path().join("unsupported.txt");
        std::fs::write(&source, "mode: rule").unwrap();
        std::fs::write(&invalid, "mode: rule").unwrap();
        let mut paths = vec![invalid; MAX_BATCH + 1];
        paths[0] = source.clone();
        paths[MAX_BATCH] = source.clone();
        let (staged, rejected) = stage_batch(paths, &root.path().join("imports"));
        assert_eq!(staged.len(), 1);
        assert_eq!(rejected, MAX_BATCH);
        assert_eq!(staged[0].path().file_name(), source.file_name());
        assert_eq!(std::fs::read_to_string(source).unwrap(), "mode: rule");
    }
}
