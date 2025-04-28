use std::path::PathBuf;

use egui::DroppedFile;

pub struct DroppedFilesWidget {
    dropped_files: Vec<DroppedFile>,
}

impl DroppedFilesWidget {
    pub fn new() -> Self {
        DroppedFilesWidget {
            dropped_files: vec![],
        }
    }

    pub fn run(&mut self, ctx: &egui::Context, can_process_file: &impl Fn(PathBuf) -> bool) {
        self.preview_files_being_dropped(ctx, can_process_file);
        ctx.input(|i| {
            if !i.raw.dropped_files.is_empty() {
                self.dropped_files.clone_from(&i.raw.dropped_files);
            }
        });
    }

    pub fn pull_dropped_file(&mut self) -> Vec<DroppedFile> {
        let dropped_files = self.dropped_files.clone();
        self.dropped_files.clear();
        dropped_files
    }

    fn preview_files_being_dropped(
        &self,
        ctx: &egui::Context,
        can_process_file: &impl Fn(PathBuf) -> bool,
    ) {
        use egui::*;
        use std::fmt::Write as _;

        let mut is_unsupported_file_exist = false;

        if !ctx.input(|i| i.raw.hovered_files.is_empty()) {
            let text = ctx.input(|i| {
                let mut text = "Dropping files:\n".to_owned();
                for file in &i.raw.hovered_files {
                    if let Some(path) = &file.path {
                        write!(text, "\n{}", path.display()).ok();
                        if !can_process_file(path.to_path_buf()) {
                            is_unsupported_file_exist = true;
                        }
                    } else {
                        text += "\n???";
                    }
                }
                text
            });

            let painter =
                ctx.layer_painter(LayerId::new(Order::Foreground, Id::new("file_drop_target")));

            let screen_rect = ctx.screen_rect();
            painter.rect_filled(screen_rect, 0.0, Color32::from_black_alpha(192));
            painter.text(
                screen_rect.center(),
                Align2::CENTER_CENTER,
                text,
                TextStyle::Heading.resolve(&ctx.style()),
                Color32::WHITE,
            );
            let mut stroke = (20.0, Color32::DARK_GREEN);
            if is_unsupported_file_exist {
                stroke = (20.0, Color32::DARK_RED)
            }
            painter.rect_stroke(screen_rect, 0.0, stroke, StrokeKind::Middle);
        }
    }
}
