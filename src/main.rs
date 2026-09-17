#![cfg_attr(windows, windows_subsystem = "windows")]
mod hyper_app;
mod tree_view;

use anyhow::{Context, Result};
use clap::Parser;
use codebush::{
    camera::Camera,
    font::Atlas,
    gpu::{Gpu, GpuScene},
    model::Codebase,
    scene::Scene,
    ui::{self, Action, UiState},
};
use std::{
    path::{Path, PathBuf},
    sync::{Arc, mpsc},
    time::{Duration, Instant},
};
use winit::{
    application::ApplicationHandler,
    event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent},
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
    keyboard::{Key, ModifiersState, NamedKey},
    window::{Fullscreen, Window, WindowId},
};
#[derive(Parser, Clone)]
#[command(version, about = "A GPU-resident 2D source atlas for the RTX 5090")]
struct Args {
    /// Open the original directory atlas instead of the 12D reference projections.
    #[arg(long)]
    tree: bool,
    /// Source directory (also accepts a Windows UNC path to WSL).
    #[arg(default_value = ".")]
    path: PathBuf,
    /// Save overview, reading, tests-off, search and help captures without a window.
    #[arg(long)]
    capture: Option<PathBuf>,
    /// Measure complete 8K frames, including CPU submission and GPU completion.
    #[arg(long)]
    benchmark: bool,
    #[arg(long, default_value_t = 240)]
    frames: usize,
    /// Pace benchmark starts at this rate (0 measures unbounded throughput).
    #[arg(long, default_value_t = 0.)]
    benchmark_hz: f64,
    /// Frames in flight for the benchmark; 2 matches the native presentation queue.
    #[arg(long, default_value_t = 1, value_parser = clap::value_parser!(u32).range(1..=3))]
    pipeline_depth: u32,
    /// Capture or benchmark resolution, e.g. 7680x4320.
    #[arg(long, default_value = "1920x1080")]
    size: String,
    /// Allow a different adapter for development. Never used for 5090 evidence.
    #[arg(long)]
    allow_other_gpu: bool,
    /// Maximum application GPU allocations in decimal GB, at most 32.
    #[arg(long, default_value_t = 30.)]
    vram_gb: f64,
    #[arg(long)]
    no_tests: bool,
    /// Export the exact indexed file manifest.
    #[arg(long)]
    manifest: Option<PathBuf>,
    /// Save a native screenshot and close after N frames (automation).
    #[arg(long)]
    smoke_frames: Option<usize>,
    /// Save native scheduling, UI, acquire, submit and present-call timings on exit.
    #[arg(long)]
    native_timing: Option<PathBuf>,
}
struct Loaded {
    base: Arc<Codebase>,
    atlas: Atlas,
    scenes: [Scene; 2],
    load_ms: f64,
}
fn prepare(path: &Path) -> Result<Loaded> {
    let start = Instant::now();
    eprintln!("Indexing {}", path.display());
    let base = Arc::new(Codebase::scan(path)?);
    eprintln!(
        "{} source files · {} lines · {:.0} ms scan",
        base.stats.files, base.stats.lines, base.stats.scan_ms
    );
    let atlas = Atlas::new(base.chars());
    let (off, on) = rayon::join(
        || Scene::build(&base, &atlas, false),
        || Scene::build(&base, &atlas, true),
    );
    Ok(Loaded {
        base,
        atlas,
        scenes: [off, on],
        load_ms: start.elapsed().as_secs_f64() * 1000.,
    })
}
fn budget(args: &Args, loaded: &Loaded, gpu: &Gpu) -> Result<u64> {
    anyhow::ensure!(
        args.vram_gb.is_finite() && args.vram_gb > 0. && args.vram_gb <= 32.,
        "--vram-gb must be in (0, 32]"
    );
    let bytes = loaded
        .scenes
        .iter()
        .map(|s| s.quads.len() as u64 * 48)
        .sum::<u64>()
        + gpu.atlas_bytes
        + 512 * 1024 * 1024;
    anyhow::ensure!(
        bytes as f64 <= args.vram_gb * 1e9,
        "Scene needs {:.2} GB including render-target reserve, exceeding {:.2} GB budget. No files were silently dropped.",
        bytes as f64 / 1e9,
        args.vram_gb
    );
    Ok(bytes)
}
fn manifest(args: &Args, l: &Loaded) -> Result<()> {
    if let Some(path) = &args.manifest {
        if let Some(p) = path.parent() {
            std::fs::create_dir_all(p)?;
        }
        let j = serde_json::json!({"root":l.base.root,"stats":l.base.stats,"files":l.base.files.iter().map(|f|serde_json::json!({"path":f.path,"language":f.language,"lines":f.lines.len(),"test":f.is_test,"inline_test_lines":f.hidden_lines})).collect::<Vec<_>>(),"load_ms":l.load_ms});
        std::fs::write(path, serde_json::to_vec_pretty(&j)?)?;
    }
    Ok(())
}
fn focus(camera: &mut Camera, id: usize, scene: &Scene, size: [f32; 2], read: bool) {
    if let Some(f) = scene.file(id) {
        let vp = ui::viewport(size);
        if read {
            camera.zoom = 1.25 / f.text_scale;
            camera.center = [
                f.bounds.x + (vp.w / 2. - 42.) / camera.zoom,
                f.bounds.y + (vp.h / 2. - 32.) / camera.zoom,
            ];
        } else {
            camera.fit(f.bounds, vp);
        }
    }
}
fn goto_match(state: &mut UiState, camera: &mut Camera, scene: &Scene, size: [f32; 2], i: usize) {
    if state.matches.is_empty() {
        return;
    }
    state.active_match = i % state.matches.len();
    state.search_navigated = true;
    let m = &state.matches[state.active_match];
    state.selected = Some(m.file);
    if m.line == 0 {
        focus(camera, m.file, scene, size, true);
    } else if let Some(f) = scene.file(m.file) {
        if let Some(row) = f.rows.iter().find(|r| r.line == m.line) {
            camera.zoom = 1.2 / f.text_scale;
            camera.center = [row.x + 300. * f.text_scale, row.y];
        }
    }
}
fn parse_size(s: &str) -> Result<[u32; 2]> {
    let (w, h) = s.split_once('x').context("Size must be WIDTHxHEIGHT")?;
    let size = [w.parse()?, h.parse()?];
    anyhow::ensure!(
        size[0] >= 960 && size[1] >= 640 && size[0] <= 16384 && size[1] <= 16384,
        "Resolution must be between 960x640 and 16384x16384"
    );
    Ok(size)
}
fn headless(args: &Args, l: Loaded) -> Result<()> {
    let gpu_start = Instant::now();
    let size = parse_size(&args.size)?;
    let dpi = size[0] as f32 / 1920.;
    let logical = [size[0] as f32 / dpi, size[1] as f32 / dpi];
    let (mut gpu, _) = pollster::block_on(Gpu::new(&l.atlas, None, args.allow_other_gpu))?;
    budget(args, &l, &gpu)?;
    let mut residents = [gpu.upload(&l.scenes[0]), gpu.upload(&l.scenes[1])];
    let remaining = (args.vram_gb * 1e9) as u64 - budget(args, &l, &gpu)?;
    for i in 0..2 {
        gpu.cache_scene(&l.scenes[i], &mut residents[i], remaining / 2)?;
    }
    let resident_bytes = residents.iter().map(|r| r.bytes).sum::<u64>() + gpu.atlas_bytes;
    gpu.wait(None)?;
    let mut state = UiState {
        tests: !args.no_tests,
        gpu_name: gpu.info.name.clone(),
        gpu_backend: format!("{:?}", gpu.info.backend).to_uppercase(),
        gpu_bytes: resident_bytes,
        ..Default::default()
    };
    let mut camera = Camera::default();
    camera.fit(l.scenes[state.tests as usize].bounds, ui::viewport(logical));
    let target = gpu.target(size[0], size[1]);
    let view = target.create_view(&Default::default());
    let ready_ms = l.load_ms + gpu_start.elapsed().as_secs_f64() * 1000.;
    if let Some(dir) = &args.capture {
        std::fs::create_dir_all(dir)?;
        let mut capture = |name: &str, state: &mut UiState, camera: Camera| -> Result<()> {
            let i = state.tests as usize;
            let ui = state.draw(&l.base, &l.scenes[i], &l.atlas, camera, logical);
            let (sub, _) = gpu.render(
                &view,
                size,
                dpi,
                ui::viewport(logical),
                camera,
                &l.scenes[i],
                &residents[i],
                &ui,
                false,
            );
            gpu.wait(Some(sub))?;
            gpu.save_png(&target, &dir.join(format!("{name}.png")))?;
            Ok(())
        };
        capture("01-overview", &mut state, camera)?;
        if let Some(f) = l.scenes[state.tests as usize]
            .files
            .iter()
            .find(|p| l.base.files[p.file].path.ends_with("camera.rs"))
            .or_else(|| l.scenes[state.tests as usize].files.first())
        {
            state.selected = Some(f.file);
            focus(
                &mut camera,
                f.file,
                &l.scenes[state.tests as usize],
                logical,
                true,
            );
            capture("02-read-source", &mut state, camera)?;
        }
        state.tests = false;
        state.selected = None;
        camera.fit(l.scenes[0].bounds, ui::viewport(logical));
        capture("03-tests-hidden", &mut state, camera)?;
        state.tests = true;
        state.query = "pub".into();
        state.search_active = true;
        state.matches = ui::search(&l.base, &state.query, true).into();
        goto_match(&mut state, &mut camera, &l.scenes[1], logical, 0);
        capture("04-search", &mut state, camera)?;
        let reading_camera = camera;
        camera.fit(l.scenes[1].bounds, ui::viewport(logical));
        capture("09-overview-search", &mut state, camera)?;
        camera = reading_camera;
        state.query.clear();
        state.matches = Arc::from([]);
        state.help = true;
        capture("05-navigation", &mut state, camera)?;
        state.help = false;
        state.search_active = false;
        state.selected = None;
        if let Some(dir) = l.scenes[1]
            .dirs
            .iter()
            .filter(|d| d.depth == 1)
            .max_by(|a, b| (a.bounds.w * a.bounds.h).total_cmp(&(b.bounds.w * b.bounds.h)))
        {
            camera.fit(dir.bounds, ui::viewport(logical));
            camera.zoom *= 2.;
            capture("06-directory-detail", &mut state, camera)?;
        }
        camera.fit(l.scenes[1].bounds, ui::viewport(logical));
        camera.zoom = residents[1].cache_scale() / dpi * 1.04;
        capture("07-cache-to-glyph", &mut state, camera)?;
        // Both sides of the cached-image handoff expose contrast or seam jumps
        // that a single intermediate screenshot cannot establish.
        camera.zoom = residents[1].cache_scale() / dpi * 0.99;
        capture("07a-cache-side", &mut state, camera)?;
        camera.zoom = residents[1].cache_scale() / dpi * 1.01;
        capture("07b-glyph-side", &mut state, camera)?;
        if let Some(file) = l.scenes[1].files.iter().max_by_key(|p| p.count) {
            state.selected = Some(file.file);
            camera.zoom = 1.2 / file.text_scale;
            if let Some(row) = file.rows.iter().min_by(|a, b| {
                (a.y - file.bounds.center()[1])
                    .abs()
                    .total_cmp(&(b.y - file.bounds.center()[1]).abs())
            }) {
                camera.center = [row.x + 400. * file.text_scale, row.y];
            }
            capture("08-long-file", &mut state, camera)?;
        }
        if let Some(file) = l.scenes[1]
            .files
            .iter()
            .filter(|f| f.count > 0)
            .min_by(|a, b| a.text_scale.total_cmp(&b.text_scale))
        {
            state.selected = Some(file.file);
            focus(&mut camera, file.file, &l.scenes[1], logical, true);
            capture("10-smallest-text-reading", &mut state, camera)?;
        }
        if let Some(file) = l.scenes[1].files.first() {
            state.query = l.base.files[file.file].path.clone();
            state.search_active = true;
            state.matches = ui::search(&l.base, &state.query, true).into();
            anyhow::ensure!(
                state
                    .matches
                    .first()
                    .is_some_and(|m| m.file == file.file && m.line == 0),
                "An exact file path must be the first navigation result"
            );
            goto_match(&mut state, &mut camera, &l.scenes[1], logical, 0);
            capture("11-exact-file-search", &mut state, camera)?;
        }
    }
    if args.benchmark {
        anyhow::ensure!(args.frames >= 60, "Use at least 60 measured frames");
        state.query.clear();
        state.matches = Arc::from([]);
        state.selected = None;
        state.tests = true;
        state.status = "8K performance measurement · animated pan and zoom".into();
        let mut frame_ms = Vec::new();
        let mut gpu_ms = Vec::new();
        let mut draw_calls = Vec::new();
        let mut ui_ms = Vec::new();
        let mut submit_ms = Vec::new();
        let mut completion_ms = Vec::new();
        let vp = ui::viewport(logical);
        let warmup = 120;
        let search_start = Instant::now();
        let search_results: [Arc<[ui::Match]>; 2] = [
            ui::search(&l.base, "return", false).into(),
            ui::search(&l.base, "return", true).into(),
        ];
        let search_ms = search_start.elapsed().as_secs_f64() * 1000.;
        let mut profiles = Vec::new();
        let mut search_overviews = Vec::new();
        let mut active_search_mode = None;
        anyhow::ensure!(
            args.benchmark_hz.is_finite() && args.benchmark_hz >= 0. && args.benchmark_hz <= 1000.,
            "--benchmark-hz must be between 0 and 1000"
        );
        let interval =
            (args.benchmark_hz > 0.).then(|| Duration::from_secs_f64(1. / args.benchmark_hz));
        let pipelined = args.pipeline_depth > 1;
        let total_frames = args.frames + warmup;
        if pipelined {
            gpu.configure_timing(
                u32::try_from(total_frames).context("Too many benchmark frames")?,
            )?;
        }
        let completed = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let (complete_tx, complete_rx) = mpsc::channel();
        let mut measurement_started = Instant::now();
        let mut due = Instant::now();
        let mut start_lateness_ms = Vec::new();
        let mut deadline_ms = Vec::new();
        for frame in 0..args.frames + warmup {
            let profile = (frame / 30) % 5;
            let idx = ((frame / 15 + frame / 150) % 2) as usize;
            let searching = profile == 4;
            if active_search_mode != Some((searching, idx)) {
                state.query = if searching {
                    "return".into()
                } else {
                    String::new()
                };
                state.matches = if searching {
                    search_results[idx].clone()
                } else {
                    Arc::from([])
                };
                active_search_mode = Some((searching, idx));
            }
            if pipelined {
                let wait_started = Instant::now();
                while frame.saturating_sub(completed.load(std::sync::atomic::Ordering::Acquire))
                    >= args.pipeline_depth as usize
                    || (interval.is_some() && Instant::now() < due)
                {
                    gpu.device.poll(wgpu::PollType::Poll)?;
                    anyhow::ensure!(
                        wait_started.elapsed().as_secs() < 30,
                        "GPU submission queue timed out"
                    );
                    std::hint::spin_loop();
                }
                gpu.query_slot = frame as u32;
            } else if interval.is_some() {
                loop {
                    let now = Instant::now();
                    if now >= due {
                        break;
                    }
                    let remaining = due - now;
                    if remaining > Duration::from_millis(2) {
                        std::thread::sleep(remaining - Duration::from_millis(1));
                    } else {
                        std::hint::spin_loop();
                    }
                }
            }
            let start = Instant::now();
            if frame == warmup {
                measurement_started = start;
            }
            let lateness = if interval.is_some() {
                start.saturating_duration_since(due).as_secs_f64() * 1000.
            } else {
                0.
            };
            if let Some(interval) = interval {
                due += interval;
            }
            let progress = (frame % 30) as f32 / 30.;
            state.tests = idx == 1;
            camera.fit(l.scenes[idx].bounds, vp);
            let fit = camera.zoom;
            match profile {
                0 => {
                    camera.zoom = fit;
                    camera.center[0] += l.scenes[idx].bounds.w * 0.025 * (progress * 6.).sin();
                }
                1 => {
                    camera.zoom = fit * (1. + progress * 4.);
                    camera.center[1] += l.scenes[idx].bounds.h * 0.1 * (progress * 4.).sin();
                }
                2 => {
                    camera.zoom = residents[idx].cache_scale() / dpi * (0.8 + progress * 0.6);
                }
                3 => {
                    if let Some(file) = l.scenes[idx].files.get(l.scenes[idx].files.len() / 2) {
                        focus(&mut camera, file.file, &l.scenes[idx], logical, true);
                        camera.center[1] += progress * 240.;
                    }
                }
                _ => {
                    if progress < 0.5 {
                        camera.fit(l.scenes[idx].bounds, vp);
                    } else if !state.matches.is_empty() {
                        goto_match(&mut state, &mut camera, &l.scenes[idx], logical, 0);
                    } else if let Some(file) = l.scenes[idx].files.first() {
                        focus(&mut camera, file.file, &l.scenes[idx], logical, true);
                    }
                }
            }
            let ui_start = Instant::now();
            let ui = state.draw(&l.base, &l.scenes[idx], &l.atlas, camera, logical);
            let ui_elapsed = ui_start.elapsed().as_secs_f64() * 1000.;
            let submit_start = Instant::now();
            let (_sub, calls) = gpu.render(
                &view,
                size,
                dpi,
                vp,
                camera,
                &l.scenes[idx],
                &residents[idx],
                &ui,
                true,
            );
            let submit_elapsed = submit_start.elapsed().as_secs_f64() * 1000.;
            let completion_start = Instant::now();
            let (elapsed, completion_elapsed, gm) = if pipelined {
                let tx = complete_tx.clone();
                let done = completed.clone();
                gpu.queue.on_submitted_work_done(move || {
                    let elapsed = start.elapsed().as_secs_f64() * 1000.;
                    let wait = completion_start.elapsed().as_secs_f64() * 1000.;
                    let _ = tx.send((frame, elapsed, wait));
                    done.fetch_add(1, std::sync::atomic::Ordering::Release);
                });
                (0., 0., None)
            } else {
                gpu.finish_frame()?;
                (
                    start.elapsed().as_secs_f64() * 1000.,
                    completion_start.elapsed().as_secs_f64() * 1000.,
                    gpu.gpu_ms()?,
                )
            };
            if frame >= warmup {
                profiles.push(profile);
                search_overviews.push(searching && progress < 0.5);
                frame_ms.push(elapsed);
                start_lateness_ms.push(lateness);
                deadline_ms.push(elapsed + lateness);
                if let Some(gm) = gm {
                    gpu_ms.push(gm);
                }
                draw_calls.push(calls);
                ui_ms.push(ui_elapsed);
                submit_ms.push(submit_elapsed);
                completion_ms.push(completion_elapsed);
            }
            if frame % 60 == 0 {
                if pipelined {
                    eprintln!("Benchmark {frame}/{}: submitted", args.frames + warmup);
                } else {
                    eprintln!(
                        "Benchmark {frame}/{}: {:.2} ms",
                        args.frames + warmup,
                        elapsed
                    );
                }
            }
        }
        if pipelined {
            let wait_started = Instant::now();
            while completed.load(std::sync::atomic::Ordering::Acquire) < total_frames {
                gpu.device.poll(wgpu::PollType::Poll)?;
                anyhow::ensure!(
                    wait_started.elapsed().as_secs() < 30,
                    "GPU benchmark completion timed out"
                );
                std::hint::spin_loop();
            }
            for (frame, elapsed, wait) in complete_rx.try_iter().filter(|(f, _, _)| *f >= warmup) {
                let i = frame - warmup;
                frame_ms[i] = elapsed;
                completion_ms[i] = wait;
                deadline_ms[i] = elapsed + start_lateness_ms[i];
            }
        }
        let measured_cadence_fps = args.frames as f64 / measurement_started.elapsed().as_secs_f64();
        if pipelined {
            gpu_ms = gpu.gpu_frame_times()?.into_iter().skip(warmup).collect();
        }
        let mut sorted = frame_ms.clone();
        sorted.sort_by(f64::total_cmp);
        let mean = frame_ms.iter().sum::<f64>() / frame_ms.len() as f64;
        let pct = |p: f64| sorted[((sorted.len() - 1) as f64 * p) as usize];
        let errors = gpu.errors.lock().unwrap().clone();
        let mut sorted_deadlines = deadline_ms.clone();
        sorted_deadlines.sort_by(f64::total_cmp);
        let p99_deadline = sorted_deadlines[((sorted_deadlines.len() - 1) as f64 * 0.99) as usize];
        let mut report = serde_json::Map::new();
        for section in [
            serde_json::json!({
                "adapter":gpu.info.name,
                "backend":format!("{:?}",gpu.info.backend),
                "driver":gpu.info.driver_info,
                "resolution":size,
                "frames":args.frames,
                "warmup_frames":warmup,
                "workload":"overview pan, zoom, cache-to-glyph transition, reading pan, broad overview and focused source search; both test modes in every profile",
                "profile_names":["overview","zoom","cache-transition","read","search-overview-and-read"],
                "search_overview_frames":search_overviews,
                "frame_profiles":profiles,
                "benchmark_hz":args.benchmark_hz,
                "pipeline_depth":args.pipeline_depth,
                "measured_cadence_fps":measured_cadence_fps,
                "timing_mode":if pipelined {"Asynchronous GPU completion callbacks; timestamp readback after all frames"} else {"Serialized GPU fence and timestamp readback per frame"},
                "start_lateness_ms":start_lateness_ms,
                "deadline_ms":deadline_ms,
                "p99_deadline_ms":p99_deadline,
            }),
            serde_json::json!({
                "search_precompute_ms":search_ms,
                "root":l.base.root,
                "source_files":l.base.stats.files,
                "source_lines":l.base.stats.lines,
                "resident_bytes":resident_bytes,
                "cpu_prepare_ms":l.load_ms,
                "ready_ms":ready_ms,
                "mean_frame_ms":mean,
                "p50_frame_ms":pct(0.50),
                "p95_frame_ms":pct(0.95),
                "p99_frame_ms":pct(0.99),
                "max_frame_ms":sorted.last(),
                "mean_fps":measured_cadence_fps,
                "inverse_mean_latency_fps":1000./mean,
                "frames_over_8_333_ms":frame_ms.iter().filter(|t|**t>1000./120.).count(),
                "mean_gpu_ms":if gpu_ms.is_empty(){None}else{Some(gpu_ms.iter().sum::<f64>()/gpu_ms.len() as f64)},
            }),
            serde_json::json!({
                "mean_ui_cpu_ms":ui_ms.iter().sum::<f64>()/ui_ms.len() as f64,
                "max_draw_calls":draw_calls.iter().max(),
                "rendering_errors":errors,
                "frame_ms":frame_ms,
                "gpu_ms":gpu_ms,
                "ui_cpu_ms":ui_ms,
                "submit_cpu_ms":submit_ms,
                "completion_wait_ms":completion_ms,
                "presentation":"Offscreen, GPU-completion-synchronized rendering; does not measure monitor presentation",
                "passes_120fps_p99":p99_deadline<=1000./120.&&errors.is_empty(),
            }),
        ] {
            if let serde_json::Value::Object(fields) = section {
                report.extend(fields);
            }
        }
        let out = args
            .capture
            .clone()
            .unwrap_or_else(|| PathBuf::from("evidence"));
        std::fs::create_dir_all(&out)?;
        std::fs::write(
            out.join("benchmark.json"),
            serde_json::to_vec_pretty(&report)?,
        )?;
        eprintln!(
            "Mean completion latency {:.2} ms; cadence {:.1} fps; p99 {:.2} ms; {} GPU errors",
            mean,
            measured_cadence_fps,
            pct(0.99),
            errors.len()
        );
    }
    if let Some(dir) = &args.capture {
        std::fs::write(
            dir.join("load-report.json"),
            serde_json::to_vec_pretty(
                &serde_json::json!({"stats":l.base.stats,"cpu_prepare_ms":l.load_ms,"ready_ms":ready_ms,"gpu":gpu.info.name,"gpu_resident_bytes":resident_bytes,"atlas_missing_glyphs":l.atlas.missing,"tree_layout_tests_off":l.scenes[0].layout_stats(),"tree_layout_tests_on":l.scenes[1].layout_stats(),"validation_errors":gpu.errors.lock().unwrap().clone()}),
            )?,
        )?;
    }
    anyhow::ensure!(
        gpu.errors.lock().unwrap().is_empty(),
        "GPU validation errors were recorded"
    );
    Ok(())
}
fn native_report_path() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|p| {
            p.parent()
                .and_then(|p| p.parent())
                .map(|p| p.join("evidence/latest-load.json"))
        })
        .unwrap_or_else(|| PathBuf::from("load-report.json"))
}
fn replace_matches(state: &mut UiState, matches: Arc<[ui::Match]>) {
    let previous = std::mem::replace(&mut state.matches, matches);
    if !previous.is_empty() {
        // Freeing a broad query can release hundreds of thousands of strings.
        // Keep that allocator work off the window's input/render thread.
        rayon::spawn(move || drop(previous));
    }
}

fn main() {
    if let Err(e) = run() {
        eprintln!("CodeBush: {e:#}");
        #[cfg(windows)]
        if !std::env::args().any(|a| a.starts_with("--capture") || a == "--benchmark") {
            rfd::MessageDialog::new()
                .set_title("CodeBush could not continue")
                .set_description(format!("{e:#}"))
                .set_level(rfd::MessageLevel::Error)
                .show();
        }
        std::process::exit(1);
    }
}
#[cfg(windows)]
fn workstation_scheduling() {
    #[link(name = "winmm")]
    unsafe extern "system" {
        fn timeBeginPeriod(ms: u32) -> u32;
    }
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetCurrentProcess() -> *mut std::ffi::c_void;
        fn GetCurrentThread() -> *mut std::ffi::c_void;
        fn SetPriorityClass(process: *mut std::ffi::c_void, class: u32) -> i32;
        fn SetThreadPriority(thread: *mut std::ffi::c_void, priority: i32) -> i32;
    }
    // This dedicated workstation targets low-latency rendering; never use realtime priority.
    unsafe {
        timeBeginPeriod(1);
        SetPriorityClass(GetCurrentProcess(), 0x80);
        SetThreadPriority(GetCurrentThread(), 2);
    }
}
fn run() -> Result<()> {
    #[cfg(windows)]
    workstation_scheduling();
    rayon::ThreadPoolBuilder::new()
        .stack_size(64 * 1024 * 1024)
        .build_global()?;
    let args = Args::parse();
    let loaded = prepare(&args.path)?;
    manifest(&args, &loaded)?;
    if args.tree && (args.capture.is_some() || args.benchmark) {
        return headless(&args, loaded);
    }
    hyper_app::run(args, loaded)
}
