//! Directory-tree controller. Shares the live window, source scenes and GPU with 12D.
use super::*;
pub(super) struct TreeView {
    pub state: UiState,
    pub camera: Camera,
    target_camera: Camera,
    pub size: [f32; 2],
    pub dpi: f32,
    window: Arc<Window>,
    pub cursor: [f32; 2],
    fitted: bool,
    pub dragging: bool,
    drag_start: [f32; 2],
    drag_distance: f32,
    last_click: Instant,
    pub modifiers: ModifiersState,
    search_generation: u64,
    search_epoch: Arc<std::sync::atomic::AtomicU64>,
    search_tx: mpsc::Sender<(u64, Arc<[ui::Match]>)>,
    search_rx: mpsc::Receiver<(u64, Arc<[ui::Match]>)>,
    pub pending: Option<Action>,
}
impl TreeView {
    pub fn new(l: &Loaded, window: Arc<Window>, size: [f32; 2], dpi: f32, tests: bool) -> Self {
        let (search_tx, search_rx) = mpsc::channel();
        let mut camera = Camera::default();
        camera.fit(l.scenes[tests as usize].bounds, ui::viewport(size));
        Self {
            state: UiState {
                tests,
                ..Default::default()
            },
            camera,
            target_camera: camera,
            window,
            size,
            dpi,
            cursor: [0.; 2],
            fitted: true,
            dragging: false,
            drag_start: [0.; 2],
            drag_distance: 0.,
            last_click: Instant::now() - Duration::from_secs(1),
            modifiers: ModifiersState::default(),
            search_generation: 0,
            search_epoch: Arc::new(std::sync::atomic::AtomicU64::new(0)),
            search_tx,
            search_rx,
            pending: None,
        }
    }
    pub fn set_tests(&mut self, l: &Loaded, tests: bool) {
        if self.state.tests != tests {
            self.act(l, Action::Tests);
        }
    }
    pub fn resize(&mut self, l: &Loaded, size: [f32; 2], dpi: f32) {
        self.size = size;
        self.dpi = dpi;
        if self.fitted {
            self.target_camera.fit(
                l.scenes[self.state.tests as usize].bounds,
                ui::viewport(size),
            );
            self.camera = self.target_camera;
        }
    }
    pub fn draw(
        &mut self,
        l: &Loaded,
        dt: f64,
        bytes: u64,
        fps: f32,
        backend: &str,
    ) -> Vec<codebush::geometry::Quad> {
        while let Ok((generation, matches)) = self.search_rx.try_recv() {
            if generation == self.search_generation {
                replace_matches(&mut self.state, matches);
                self.state.searching = false;
            } else {
                rayon::spawn(move || drop(matches));
            }
        }
        let blend = 1. - (-24. * dt.min(0.1) as f32).exp();
        self.camera.zoom += (self.target_camera.zoom - self.camera.zoom) * blend;
        for k in 0..2 {
            self.camera.center[k] += (self.target_camera.center[k] - self.camera.center[k]) * blend;
        }
        self.state.gpu_bytes = bytes;
        self.state.fps = fps;
        self.state.frame_ms = 1000. / fps.max(1.);
        self.state.gpu_backend = backend.into();
        self.state.draw(
            &l.base,
            &l.scenes[self.state.tests as usize],
            &l.atlas,
            self.camera,
            self.size,
        )
    }
    pub fn event(&mut self, l: &Loaded, event: &WindowEvent) {
        let r = self;
        match event {
            WindowEvent::KeyboardInput { event, .. } if event.state == ElementState::Pressed => {
                r.key(l, event.logical_key.clone(), event.text.as_deref());
            }
            WindowEvent::CursorMoved { position, .. } => {
                let p = [position.x as f32 / r.dpi, position.y as f32 / r.dpi];
                if r.dragging {
                    r.fitted = false;
                    let delta = [p[0] - r.cursor[0], p[1] - r.cursor[1]];
                    r.drag_distance += delta[0].abs() + delta[1].abs();
                    r.target_camera.pan(delta);
                    r.camera = r.target_camera;
                }
                r.cursor = p;
                let vp = ui::viewport(r.size);
                r.state.hover = if vp.contains(p) {
                    l.scenes[r.state.tests as usize].hit(r.camera.world(p, vp))
                } else {
                    None
                };
                r.window.set_cursor(if r.dragging {
                    winit::window::CursorIcon::Grabbing
                } else if r.state.hit(p).is_some() {
                    winit::window::CursorIcon::Pointer
                } else {
                    winit::window::CursorIcon::Default
                });
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let dy = match delta {
                    MouseScrollDelta::LineDelta(_, y) => y * 60.,
                    MouseScrollDelta::PixelDelta(p) => p.y as f32,
                };
                if r.cursor[0] < ui::SIDE && r.cursor[1] > ui::TOP && r.cursor[1] < r.size[1] - 345.
                {
                    r.state.scroll = (r.state.scroll - dy).max(0.);
                } else if ui::viewport(r.size).contains(r.cursor) && !r.state.help {
                    r.fitted = false;
                    r.target_camera
                        .zoom_at((dy * 0.0025).exp(), r.cursor, ui::viewport(r.size));
                }
            }
            WindowEvent::MouseInput { state, button, .. }
                if *button == MouseButton::Left || *button == MouseButton::Middle =>
            {
                if *state == ElementState::Pressed {
                    if let Some(action) = r.state.hit(r.cursor) {
                        r.act(l, action);
                    } else if ui::viewport(r.size).contains(r.cursor) && !r.state.help {
                        r.state.search_active = false;
                        r.dragging = true;
                        r.drag_start = r.cursor;
                        r.drag_distance = 0.;
                    }
                } else if r.dragging {
                    r.dragging = false;
                    if r.drag_distance < 5. {
                        let id = l.scenes[r.state.tests as usize]
                            .hit(r.camera.world(r.cursor, ui::viewport(r.size)));
                        let double = r.last_click.elapsed() < Duration::from_millis(350)
                            && id == r.state.selected;
                        r.state.selected = id;
                        r.last_click = Instant::now();
                        if double {
                            r.act(l, Action::Read);
                        }
                    }
                }
            }
            _ => {}
        }
    }
    fn search(&mut self, l: &Loaded) {
        self.search_generation += 1;
        let generation = self.search_generation;
        self.search_epoch
            .store(generation, std::sync::atomic::Ordering::Release);
        let query = self.state.query.clone();
        let tests = self.state.tests;
        self.state.searching = !query.is_empty();
        self.state.active_match = 0;
        self.state.search_navigated = false;
        self.state.scroll = 0.;
        if query.is_empty() {
            replace_matches(&mut self.state, Arc::from([]));
            self.state.searching = false;
            return;
        }
        let base = l.base.clone();
        let tx = self.search_tx.clone();
        let epoch = self.search_epoch.clone();
        std::thread::spawn(move || {
            // Coalesce a burst of typing before scanning millions of source lines.
            std::thread::sleep(Duration::from_millis(80));
            if epoch.load(std::sync::atomic::Ordering::Acquire) != generation {
                return;
            }
            let matches: Arc<[ui::Match]> = ui::search(&base, &query, tests).into();
            if epoch.load(std::sync::atomic::Ordering::Acquire) == generation {
                let _ = tx.send((generation, matches));
            }
        });
    }
    fn act(&mut self, l: &Loaded, action: Action) {
        let idx = self.state.tests as usize;
        if matches!(
            action,
            Action::Read
                | Action::File(_)
                | Action::Directory(_)
                | Action::Hit(_)
                | Action::Zoom(_)
                | Action::Minimap
        ) {
            self.fitted = false;
        }
        match action {
            Action::Tests => {
                let t = Instant::now();
                let previous = l.scenes[idx].bounds;
                self.state.tests = !self.state.tests;
                let next = &l.scenes[self.state.tests as usize];
                if self
                    .state
                    .selected
                    .is_some_and(|id| next.file(id).is_none())
                {
                    self.state.selected = None;
                    self.fitted = true;
                }
                if self.fitted {
                    self.target_camera.fit(next.bounds, ui::viewport(self.size));
                } else if let Some((old, new)) = self
                    .state
                    .selected
                    .and_then(|id| l.scenes[idx].file(id).zip(next.file(id)))
                {
                    // Keep the selected source line on screen when filtering tests
                    // repacks the directory tree, including inline test blocks.
                    let row = old
                        .rows
                        .iter()
                        .find(|r| r.y >= self.target_camera.center[1]);
                    let dy = row
                        .and_then(|r| {
                            new.rows
                                .iter()
                                .find(|n| n.line >= r.line)
                                .map(|n| n.y - r.y)
                        })
                        .unwrap_or(new.bounds.y - old.bounds.y);
                    self.target_camera.center[0] += new.bounds.x - old.bounds.x;
                    self.target_camera.center[1] += dy;
                    self.target_camera.zoom *= old.text_scale / new.text_scale;
                } else {
                    self.target_camera.center[0] *= next.bounds.w / previous.w;
                    self.target_camera.center[1] *= next.bounds.h / previous.h;
                }
                self.camera = self.target_camera;
                self.state.scroll = 0.;
                self.state.last_tests_ms = t.elapsed().as_secs_f64() * 1000.;
                if !self.state.query.is_empty() {
                    self.search(l);
                }
            }
            Action::Fit => {
                self.fitted = true;
                self.target_camera
                    .fit(l.scenes[idx].bounds, ui::viewport(self.size));
            }
            Action::Zoom(z) => self.target_camera.zoom_at(
                z,
                ui::viewport(self.size).center(),
                ui::viewport(self.size),
            ),
            Action::Read => {
                if let Some(id) = self.state.selected {
                    focus(&mut self.target_camera, id, &l.scenes[idx], self.size, true)
                }
            }
            Action::File(id) => {
                self.state.selected = Some(id);
                focus(
                    &mut self.target_camera,
                    id,
                    &l.scenes[idx],
                    self.size,
                    false,
                );
            }
            Action::Directory(path) => {
                if let Some(d) = l.scenes[idx].dirs.iter().find(|d| d.path == path) {
                    self.target_camera.fit(d.bounds, ui::viewport(self.size));
                }
            }
            Action::Collapse(path) => {
                if !self.state.collapsed.remove(&path) {
                    self.state.collapsed.insert(path);
                }
            }
            Action::Search => {
                self.state.search_active = true;
            }
            Action::ClearSearch => {
                self.state.query.clear();
                self.state.search_active = false;
                self.search(l);
            }
            Action::Hit(i) => goto_match(
                &mut self.state,
                &mut self.target_camera,
                &l.scenes[idx],
                self.size,
                i,
            ),
            Action::Diagnostics => {
                let _ = std::process::Command::new("notepad.exe")
                    .arg(
                        native_report_path()
                            .parent()
                            .unwrap()
                            .join("12d-latest/load-report.json"),
                    )
                    .spawn();
            }
            Action::Help => self.state.help = !self.state.help,
            Action::CloseHelp => self.state.help = false,
            Action::Reload | Action::Open | Action::Mode => self.pending = Some(action),
            Action::Minimap => {
                let map = self.state.map;
                if map.w > 0. && map.h > 0. {
                    self.target_camera.center = [
                        ((self.cursor[0] - map.x) / map.w).clamp(0., 1.) * self.state.map_world.w,
                        ((self.cursor[1] - map.y) / map.h).clamp(0., 1.) * self.state.map_world.h,
                    ];
                }
            }
        }
    }
    fn key(&mut self, l: &Loaded, key: Key, text: Option<&str>) {
        if self.state.help {
            if key == Key::Named(NamedKey::Escape) {
                self.state.help = false;
            }
            return;
        }
        let ctrl = self.modifiers.control_key();
        let shift = self.modifiers.shift_key();
        if let Key::Character(c) = &key {
            if ctrl {
                match c.to_lowercase().as_str() {
                    "k" | "f" => self.act(l, Action::Search),
                    "o" => self.act(l, Action::Open),
                    "r" => self.act(l, Action::Reload),
                    "a" if self.state.search_active => {
                        self.state.query.clear();
                        self.search(l);
                    }
                    "v" if self.state.search_active => {
                        #[cfg(windows)]
                        if let Ok(mut clip) = arboard::Clipboard::new() {
                            if let Ok(s) = clip.get_text() {
                                self.state.query.push_str(&s.replace(['\r', '\n'], ""));
                                self.search(l);
                            }
                        }
                    }
                    _ => {}
                }
                return;
            }
        }
        if self.state.search_active {
            match key {
                Key::Named(NamedKey::Escape) => self.act(l, Action::ClearSearch),
                Key::Named(NamedKey::Backspace) => {
                    self.state.query.pop();
                    self.search(l);
                }
                Key::Named(NamedKey::Enter) => {
                    let len = self.state.matches.len();
                    if len > 0 {
                        let i = if !self.state.search_navigated {
                            if shift { len - 1 } else { 0 }
                        } else if shift {
                            (self.state.active_match + len - 1) % len
                        } else {
                            (self.state.active_match + 1) % len
                        };
                        self.act(l, Action::Hit(i));
                    }
                }
                Key::Character(_) => {
                    if let Some(t) = text {
                        self.state.query.push_str(t);
                        self.search(l);
                    }
                }
                _ => {}
            }
            return;
        }
        if matches!(
            key,
            Key::Named(
                NamedKey::ArrowLeft
                    | NamedKey::ArrowRight
                    | NamedKey::ArrowUp
                    | NamedKey::ArrowDown
            )
        ) {
            self.fitted = false;
        }
        match key {
            Key::Named(NamedKey::Home) => self.act(l, Action::Fit),
            Key::Named(NamedKey::Enter) => self.act(l, Action::Read),
            Key::Named(NamedKey::F11) => {
                self.window
                    .set_fullscreen(if self.window.fullscreen().is_some() {
                        None
                    } else {
                        Some(Fullscreen::Borderless(None))
                    })
            }
            Key::Named(NamedKey::ArrowLeft) => self.target_camera.pan([90., 0.]),
            Key::Named(NamedKey::ArrowRight) => self.target_camera.pan([-90., 0.]),
            Key::Named(NamedKey::ArrowUp) => self.target_camera.pan([0., 90.]),
            Key::Named(NamedKey::ArrowDown) => self.target_camera.pan([0., -90.]),
            Key::Character(c) => match c.as_str() {
                "f" | "F" => self.act(l, Action::Fit),
                "t" | "T" => self.act(l, Action::Tests),
                "+" | "=" => self.act(l, Action::Zoom(1.25)),
                "-" => self.act(l, Action::Zoom(0.8)),
                "?" => self.act(l, Action::Help),
                _ => {}
            },
            _ => {}
        }
    }
}
impl Drop for TreeView {
    fn drop(&mut self) {
        self.search_epoch
            .fetch_add(1, std::sync::atomic::Ordering::Release);
        replace_matches(&mut self.state, Arc::from([]));
    }
}
