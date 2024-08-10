// mod threemf_reader;
mod threemf;
mod widgets;
use eframe::egui_wgpu::{self, wgpu::util::DeviceExt};
use egui_code_editor::{CodeEditor, Syntax};
use threemf::threemf_reader;
use wgpu::{self, ColorTargetState, ColorWrites};
use widgets::tree;

use std::{ffi::OsStr, fs, path::PathBuf};

use anyhow::{anyhow, Result};
use egui::{DroppedFile, Layout};

pub struct MyApp {
    name: String,
    dropped_files: Vec<DroppedFile>,
    file_to_render: Option<String>,
    rendered_file_name: Option<String>,
    font_size: f32,
    trees: Option<Vec<tree::Tree>>,
    show_log: bool,
    show_viewport: bool,
    render: Option<Custom3d>,
}

impl Default for MyApp {
    fn default() -> Self {
        Self {
            name: "AMRUST".to_owned(),
            dropped_files: Vec::new(),
            file_to_render: None,
            rendered_file_name: None,
            font_size: 14.0,
            trees: None,
            show_log: false,
            show_viewport: false,
            render: None,
        }
    }
}

impl eframe::App for MyApp {
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        egui::TopBottomPanel::top("top panel")
            .resizable(false)
            .show(ctx, |ui| {
                egui::menu::bar(ui, |ui| {
                    ui.menu_button("View", |ui| {
                        if ui.button("Show Log").clicked() {
                            self.show_log = !self.show_log;
                        }
                        if ui
                            .add_enabled(
                                self.trees.is_some() && !self.show_viewport,
                                egui::Button::new("Show Viewport"),
                            )
                            .clicked()
                        {
                            self.show_viewport = true;
                        }
                    })
                });
            });
        if let Some(trees) = &self.trees {
            egui::SidePanel::left("left_panel")
                .resizable(true)
                .default_width(100.0)
                .show(ctx, |ui| {
                    egui::ScrollArea::both().auto_shrink(false).show(ui, |ui| {
                        let mut count = 1;
                        for tree in trees {
                            tree.ui(ui, 0, &format!("{} - {}", tree.name, count));
                            count += 1;
                        }
                    });
                });
        }

        if self.show_log {
            egui::TopBottomPanel::bottom("bottom_panel")
                .resizable(true)
                .show_separator_line(true)
                .show(ctx, |ui| {
                    egui_logger::LoggerUi::default().enable_regex(true).show(ui);
                });
        }
        egui::CentralPanel::default().show(ctx, |ui| {
            if let Some(string) = &self.file_to_render {
                let mut text_to_display = string.clone();
                ui.vertical(|ui| {
                    ui.horizontal_top(|ui| {
                        if let Some(file_name) = &self.rendered_file_name {
                            ui.label(file_name);
                        }

                        ui.with_layout(Layout::right_to_left(egui::Align::Min), |ui| {
                            if ui.button("Clear content").clicked() {
                                self.clear_state();
                            }
                            ui.add(
                                egui::Slider::new(&mut self.font_size, 1.0..=120.0)
                                    .fixed_decimals(0)
                                    .integer()
                                    .step_by(1.0),
                            );
                        });
                    });

                    ui.add(
                        egui::Separator::default()
                            .horizontal()
                            .shrink(4.0)
                            .spacing(10.0),
                    );

                    egui::ScrollArea::both()
                        .auto_shrink(false)
                        .scroll_bar_visibility(
                            egui::scroll_area::ScrollBarVisibility::VisibleWhenNeeded,
                        )
                        .show(ui, |ui| {
                            CodeEditor::default()
                                .with_fontsize(self.font_size)
                                .with_syntax(Syntax::simple("xml"))
                                .auto_shrink(false)
                                .with_numlines(false)
                                .show(ui, &mut text_to_display);
                        });
                });
            } else {
                ui.centered_and_justified(|ui| {
                    ui.image(egui::include_image!("../assets/ferris.png"));
                });
            }

            if !self.dropped_files.is_empty() {
                for i in 0..self.dropped_files.len() {
                    let file = self.dropped_files[i].clone();
                    if let Some(path) = &file.path {
                        let processed = self.processed_file_and_update_app(path, frame);
                        match processed {
                            Ok(success) => {
                                if success {
                                    log::debug!("All went well");
                                    //only the first successful file is processed
                                    // break;
                                } else {
                                    let ext = path.extension();
                                    log::error!("File format of type {:?} not supported", ext);
                                }
                            }
                            Err(e) => log::error!("{:?}", e),
                        }
                    }
                }
            }

            self.dropped_files.clear();
            self.preview_files_being_dropped(ctx);
            ctx.input(|i| {
                if !i.raw.dropped_files.is_empty() {
                    self.dropped_files.clone_from(&i.raw.dropped_files);
                }
            });

            if self.show_viewport {
                ctx.show_viewport_immediate(
                    egui::ViewportId::from_hash_of("immediate_viewport"),
                    egui::ViewportBuilder::default()
                        .with_title("Immediate Viewport")
                        .with_inner_size([200.0, 100.0]),
                    |ctx, class| {
                        assert!(
                            class == egui::ViewportClass::Immediate,
                            "This egui backend doesn't support multiple viewports"
                        );

                        egui::CentralPanel::default().show(ctx, |ui| {
                            ui.vertical(|ui| {
                                ui.label("Hello from immediate viewport");

                                egui::Frame::canvas(ui.style()).show(ui, |ui| {
                                    // self.custom_painting(ui);
                                    if let Some(render_3d) = &self.render.as_ref() {
                                        render_3d.custom_painting(ui, 45.0);
                                    }
                                });
                                ui.label("Drag to rotate!");
                            });
                        });

                        if ctx.input(|i| i.viewport().close_requested()) {
                            self.show_viewport = false;
                        }
                    },
                );
            }
        });
    }
}

impl MyApp {
    fn processed_file_and_update_app(
        &mut self,
        path: &PathBuf,
        frame: &eframe::Frame,
    ) -> Result<bool> {
        let processed_file_and_tree = match path.extension().and_then(OsStr::to_str) {
            Some("3mf") => {
                let file = fs::File::open(path)?;
                let file_to_render =
                    threemf_reader::load_threemf_get_root_model_file_as_string(file)?;
                let result = tree::Tree::new_trees_from_xml_string(&file_to_render);
                match result {
                    Ok(trees) => {
                        let trees = Some(trees);
                        self.render = Some(Custom3d::new(frame));
                        Ok((Some(file_to_render), trees))
                    }
                    Err(e) => return Err(e),
                }
            }
            Some("txt") => {
                let file_to_render = Some(fs::read_to_string(path)?);
                Ok((file_to_render, None))
            }
            Some("obj") => {
                let file_to_render = Some(fs::read_to_string(path)?);
                Ok((file_to_render, None))
            }
            Some("xml") => {
                let file_to_render = fs::read_to_string(path)?;
                let result = tree::Tree::new_trees_from_xml_string(&file_to_render);
                match result {
                    Ok(trees) => {
                        let trees = Some(trees);
                        Ok((Some(file_to_render), trees))
                    }
                    Err(e) => return Err(e),
                }
            }
            _ => Err(anyhow!("File format not supported")),
        };

        let status = match processed_file_and_tree {
            Ok((file_to_render, trees)) => {
                self.clear_state();
                self.file_to_render = file_to_render;
                self.trees = trees;
                self.rendered_file_name = match path.file_name().and_then(OsStr::to_str) {
                    Some(file_name) => Some(file_name.to_string()),
                    None => None,
                };
                Ok(true)
            }
            Err(e) => Err(e),
        };

        status
    }

    // Preview hovering files:
    fn preview_files_being_dropped(&self, ctx: &egui::Context) {
        use egui::*;
        use std::fmt::Write as _;

        let mut is_unsupported_file_exist = false;

        if !ctx.input(|i| i.raw.hovered_files.is_empty()) {
            let text = ctx.input(|i| {
                let mut text = "Dropping files:\n".to_owned();
                for file in &i.raw.hovered_files {
                    if let Some(path) = &file.path {
                        write!(text, "\n{}", path.display()).ok();
                        if !self.can_process_file(path.to_path_buf()) {
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
            painter.rect_stroke(screen_rect, 0.0, stroke);
        }
    }

    fn can_process_file(&self, path: PathBuf) -> bool {
        let result = match path.extension().and_then(OsStr::to_str) {
            Some("txt") => true,
            Some("obj") => true,
            Some("3mf") => true,
            Some("xml") => true,
            _ => false,
        };

        result
    }

    fn clear_state(&mut self) {
        self.file_to_render = None;
        self.trees = None;
        self.rendered_file_name = None;
    }
}

pub struct Custom3d {
    angle: f32,
}

impl Custom3d {
    pub fn new<'a>(cc: &'a eframe::Frame) -> Self {
        // Get the WGPU render state from the eframe creation context. This can also be retrieved
        // from `eframe::Frame` when you don't have a `CreationContext` available.
        let binding = cc.wgpu_render_state();
        let render_state = binding.as_ref().expect("WGPU enabled");

        let device = &render_state.device;

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: None,
            source: wgpu::ShaderSource::Wgsl(include_str!("./custom3d_wgpu_shader.wgsl").into()),
        });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: None,
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[],
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: None,
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: "vs_main",
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: "fs_main",
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &[Some(ColorTargetState {
                    format: render_state.target_format,
                    blend: None,
                    write_mask: ColorWrites::all(),
                })],
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
        });

        let uniform_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: None,
            contents: bytemuck::cast_slice(&[0.0]),
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::UNIFORM,
        });

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buffer.as_entire_binding(),
            }],
        });

        // Because the graphics pipeline must have the same lifetime as the egui render pass,
        // instead of storing the pipeline in our `Custom3D` struct, we insert it into the
        // `callback_resources` type map, which is stored alongside the render pass.
        render_state
            .renderer
            .write()
            .callback_resources
            .insert(TriangleRenderResources {
                pipeline,
                bind_group,
                uniform_buffer,
            });

        Self { angle: 0.0 }
    }
}

impl Custom3d {
    fn custom_painting(&self, ui: &mut egui::Ui, angle: f32) {
        let (rect, response) =
            ui.allocate_exact_size(egui::Vec2::splat(300.0), egui::Sense::drag());

        let new_angle = angle + response.drag_delta().x * 0.01;

        let cb = egui_wgpu::Callback::new_paint_callback(
            rect,
            CustomTriangleCallback { angle: new_angle },
        );
        ui.painter().add(cb);
    }
}

// Callbacks in egui_wgpu have 3 stages:
// * prepare (per callback impl)
// * finish_prepare (once)
// * paint (per callback impl)
//
// The prepare callback is called every frame before paint and is given access to the wgpu
// Device and Queue, which can be used, for instance, to update buffers and uniforms before
// rendering.
// If [`egui_wgpu::Renderer`] has [`egui_wgpu::FinishPrepareCallback`] registered,
// it will be called after all `prepare` callbacks have been called.
// You can use this to update any shared resources that need to be updated once per frame
// after all callbacks have been processed.
//
// On both prepare methods you can use the main `CommandEncoder` that is passed-in,
// return an arbitrary number of user-defined `CommandBuffer`s, or both.
// The main command buffer, as well as all user-defined ones, will be submitted together
// to the GPU in a single call.
//
// The paint callback is called after finish prepare and is given access to egui's main render pass,
// which can be used to issue draw commands.
struct CustomTriangleCallback {
    angle: f32,
}

impl egui_wgpu::CallbackTrait for CustomTriangleCallback {
    fn prepare(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        _screen_descriptor: &egui_wgpu::ScreenDescriptor,
        _egui_encoder: &mut wgpu::CommandEncoder,
        resources: &mut egui_wgpu::CallbackResources,
    ) -> Vec<wgpu::CommandBuffer> {
        let resources: &TriangleRenderResources = resources.get().unwrap();
        resources.prepare(device, queue, self.angle);
        Vec::new()
    }

    fn paint<'a>(
        &self,
        _info: egui::PaintCallbackInfo,
        render_pass: &mut wgpu::RenderPass<'a>,
        resources: &'a egui_wgpu::CallbackResources,
    ) {
        let resources: &TriangleRenderResources = resources.get().unwrap();
        resources.paint(render_pass);
    }
}

struct TriangleRenderResources {
    pipeline: wgpu::RenderPipeline,
    bind_group: wgpu::BindGroup,
    uniform_buffer: wgpu::Buffer,
}

impl TriangleRenderResources {
    fn prepare(&self, _device: &wgpu::Device, queue: &wgpu::Queue, angle: f32) {
        // Update our uniform buffer with the angle from the UI
        queue.write_buffer(&self.uniform_buffer, 0, bytemuck::cast_slice(&[angle]));
    }

    fn paint<'rpass>(&'rpass self, rpass: &mut wgpu::RenderPass<'rpass>) {
        // Draw our triangle!
        rpass.set_pipeline(&self.pipeline);
        rpass.set_bind_group(0, &self.bind_group, &[]);
        rpass.draw(0..3, 0..1);
    }
}
