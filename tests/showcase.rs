//! Runs `test_games/extrans_showcase`: every registered transition must reach
//! its completion callback, and `scanline` / `wave` must place every pixel where
//! their integer geometry says.
//!
//! ```sh
//! cargo test --test showcase -- --ignored --nocapture   # per-effect frame cost
//! ```

mod common;
use bevy_krkr::{
    plugin::render::read_pixels,
    script::{last_error, Tjs2Engine},
    sys,
};
use std::{
    ffi::CString,
    time::{Duration, Instant},
};

const W: u32 = 1920;
const H: u32 = 1080;

fn build_frame() -> u64 {
    let (mut width, mut height) = (0, 0);
    let id = unsafe { sys::krkr_scene_build(std::ptr::null_mut(), &mut width, &mut height) };
    assert_ne!(id, 0, "{}", last_error());
    assert_eq!((width as u32, height as u32), (W, H));
    id
}

fn frame() -> Vec<u8> {
    read_pixels(build_frame()).unwrap()
}

fn red(pixels: &[u8], x: u32, y: u32) -> i32 {
    pixels[((y * W + x) * 4) as usize] as i32
}

/// Red channel of ramp column `x`; never zero, so a black gap is
/// distinguishable from the ramp.
fn ramp(x: i32) -> i32 {
    ((x % 255) + 1) as i32
}

struct Showcase {
    engine: Tjs2Engine,
    effects: Vec<String>,
}

impl Showcase {
    fn new() -> Self {
        common::initialize_renderer();
        let root = CString::new(format!(
            "{}/test_games/extrans_showcase",
            env!("CARGO_MANIFEST_DIR")
        ))
        .unwrap();
        assert_eq!(unsafe { sys::krkr_engine_init(root.as_ptr()) }, sys::KRKR_OK);
        let engine = Tjs2Engine::new().unwrap();
        let run = |s: &str| engine.exec_script(s, "showcase").unwrap();
        run(include_str!("../test_games/extrans_showcase/startup.tjs"));
        run("showcaseTimer.enabled=false;");
        let effects = engine.eval("showcaseEffects.join(',')").unwrap();
        Self {
            engine,
            effects: effects.split(',').map(str::to_owned).collect(),
        }
    }

    fn run(&self, script: &str) {
        self.engine.exec_script(script, "showcase").unwrap();
    }

    /// Starts `effect` with the transition clock driven by `tick`, so frames do
    /// not depend on wall time. `override_options` runs after the effect's own
    /// options are selected and before the transition begins.
    fn start(&self, effect: &str, time: u32) {
        self.start_with(effect, time, "");
    }

    fn start_with(&self, effect: &str, time: u32, override_options: &str) {
        self.run(&format!(
            r#"
            try {{ showcaseTo.stopTransition(); }} catch(e) {{}}
            showcaseFrom.visible=false; showcaseTo.visible=true;
            global.freeze=0;
            global.completed=false;
            global.options=showcaseOptions["{effect}"];
            options.time={time}; options.selfupdate=true;
            options.callback=function(){{return freeze;}};
            showcaseTo.onTransitionCompleted=function(dest,src){{completed=true;}};
            {override_options}
            showcaseTo.beginTransition("{effect}",false,showcaseFrom,options);
            "#
        ));
    }

    fn tick(&self, tick: u32) {
        self.run(&format!("freeze={tick};"));
    }

    /// Rule images decode on the engine's worker thread; the transition holds
    /// at phase 0 until the pixels arrive.
    fn wait_for_decode(&self) {
        std::thread::sleep(Duration::from_millis(300));
    }

    fn completed(&self) -> bool {
        self.engine.eval("completed?'1':'0'").unwrap() == "1"
    }

    /// Both layers hold the same horizontal ramp, so a pixel's red channel
    /// reads back the x coordinate it was sampled from.
    fn ramp(&self) {
        self.run(&format!(
            r#"
            for(var i=0;i<{W};i++) {{
                var c = 0xff000000 | (((i%255)+1)<<16);
                showcaseFrom.fillRect(i,0,1,{H},c);
                showcaseTo.fillRect(i,0,1,{H},c);
            }}
            "#
        ));
    }
}

/// Every registered transition must reach its completion callback, and the two
/// effects with exact integer geometry must place every pixel where their
/// algorithm says. The engine initialises once per process, so both checks
/// share this test.
#[test]
fn effects_complete_and_geometry_is_exact() {
    let showcase = Showcase::new();
    let mut pending = Vec::new();
    for effect in &showcase.effects {
        showcase.start(effect, 400);
        showcase.wait_for_decode();
        for tick in [0, 80, 160, 240, 320, 399] {
            showcase.tick(tick);
            frame();
        }
        showcase.tick(400);
        frame();
        if !showcase.completed() {
            pending.push(effect.clone());
        }
    }
    assert!(
        pending.is_empty(),
        "transitions that never completed: {pending:?}"
    );

    showcase.ramp();

    showcase.start("scanline", 1000);
    showcase.tick(500);
    let pixels = frame();
    let pos = W / 2;
    for y in [0u32, 1, 400, 401, 1078, 1079] {
        for x in (0..W).step_by(31) {
            let source = if y % 2 == 0 {
                if x < pos { x + (W - pos) } else { x - pos }
            } else if x < W - pos {
                x + pos
            } else {
                x - (W - pos)
            };
            assert_eq!(
                red(&pixels, x, y),
                ramp(source as i32),
                "scanline ({x},{y})"
            );
        }
    }

    showcase.start("wave", 1000);
    showcase.tick(500);
    let pixels = frame();
    let (maxh, maxomega) = (18.0f32, 0.2f32);
    for y in (0..H).step_by(37) {
        let shift = ((maxomega * (y as f32 - H as f32 / 2.0)).sin() * maxh).trunc() as i32;
        for x in (0..W).step_by(31) {
            let source = x as i32 - shift;
            let expected = if (0..W as i32).contains(&source) {
                ramp(source)
            } else {
                0
            };
            assert_eq!(
                red(&pixels, x, y),
                expected,
                "wave ({x},{y}) shift {shift}"
            );
        }
    }

    // zoomfade shrinks both images early in the transition, so the edges map
    // outside the source. Those pixels must stay empty instead of repeating the
    // border pixel; the layer is `ltOpaque`, so empty composites as black.
    showcase.start("zoomfade", 1000);
    showcase.tick(500);
    let pixels = frame();
    for (x, y) in [(0u32, 0u32), (0, 540), (47, 540), (W - 1, H - 1)] {
        assert_eq!(
            red(&pixels, x, y),
            0,
            "zoomfade sampled outside the source at ({x},{y})"
        );
    }
    assert_eq!(red(&pixels, 960, 540), ramp(960), "zoomfade centre");

    // imagewipe takes its rule either as a file or as a layer. The layer path
    // loads the image in the engine, so matching it pins the decoded file path.
    // imagewipe takes its rule either as a file or as a layer, and samples it as a
    // texture. The layer path loads it in the engine, so matching it proves the
    // decoded rule reaches the shader. Both runs get a frame at tick 0 first, so
    // they share the same start tick.
    showcase.run(
        r#"
        global.showcaseRule = new Layer(showcaseWindow, showcaseBack);
        showcaseRule.loadImages("rule1080.png");
        showcaseRule.setSizeToImageSize();
        showcaseRule.visible = false;
        "#,
    );
    let mut wipe = |start: &dyn Fn()| {
        start();
        showcase.wait_for_decode();
        showcase.tick(0);
        frame();
        showcase.tick(500);
        frame()
    };
    let from_file = wipe(&|| showcase.start("imagewipe", 1000));
    let from_layer = wipe(&|| showcase.start_with("imagewipe", 1000, "options.rule=showcaseRule;"));
    let differing = from_file
        .chunks_exact(4)
        .zip(from_layer.chunks_exact(4))
        .filter(|(file, layer)| file != layer)
        .count();
    assert_eq!(differing, 0, "imagewipe differs between a file and a layer rule");
    assert_ne!(
        from_file.chunks_exact(4).map(|p| p[0]).max(),
        Some(0),
        "imagewipe drew nothing"
    );
}

#[test]
#[ignore = "timing, run with --ignored --nocapture"]
fn frame_spikes() {
    let showcase = Showcase::new();
    showcase.ramp();
    for effect in ["crossfade", "wave", "universal"] {
        showcase.run("try { showcaseTo.stopTransition(); } catch(e) {}");
        frame();
        showcase.start(effect, 5000);
        let mut costs = Vec::new();
        for tick in 0..240u32 {
            showcase.tick(tick * 20);
            let started = Instant::now();
            let id = build_frame();
            let build = started.elapsed().as_secs_f64() * 1000.0;
            let started = Instant::now();
            read_pixels(id).unwrap();
            costs.push((build, started.elapsed().as_secs_f64() * 1000.0));
        }
        let (mut build, mut present): (Vec<f64>, Vec<f64>) = costs.iter().copied().unzip();
        build.sort_by(|a, b| a.partial_cmp(b).unwrap());
        present.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let stats = |v: &[f64]| {
            (
                v[(v.len() - 1) * 5 / 10],
                v[(v.len() - 1) * 9 / 10],
                v[v.len() - 1],
            )
        };
        let (bp50, bp90, bmax) = stats(&build);
        let (pp50, pp90, pmax) = stats(&present);
        println!(
            "{effect:10} build p50 {bp50:6.3} p90 {bp90:6.3} max {bmax:6.3} | present p50 {pp50:6.3} p90 {pp90:6.3} max {pmax:6.3} ms"
        );
    }
}

#[test]
#[ignore = "timing, run with --ignored --nocapture"]
fn frame_cost() {
    let showcase = Showcase::new();
    showcase.ramp();
    println!(
        "{:14} {:>9} {:>9} {:>9} {:>9} {:>9} {:>9}",
        "effect", "begin", "begin_again", "build", "build_max", "read", "read_max"
    );
    for effect in &showcase.effects {
        showcase.run("try { showcaseTo.stopTransition(); } catch(e) {}");
        frame();

        let started = Instant::now();
        showcase.start(effect, 1000);
        let begin = started.elapsed();

        let (mut build, mut read) = (Vec::new(), Vec::new());
        for tick in [100u32, 300, 500, 700, 900] {
            showcase.tick(tick);
            let started = Instant::now();
            let id = build_frame();
            build.push(started.elapsed());
            let started = Instant::now();
            read_pixels(id).unwrap();
            read.push(started.elapsed());
        }

        let started = Instant::now();
        showcase.start(effect, 1000);
        let begin_again = started.elapsed();

        let mean = |v: &[Duration]| v.iter().sum::<Duration>() / v.len() as u32;
        println!(
            "{effect:14} {:>8.2?} {:>8.2?} {:>8.2?} {:>8.2?} {:>8.2?} {:>8.2?}",
            begin,
            begin_again,
            mean(&build),
            mean(&read),
            build.iter().max().unwrap(),
            read.iter().max().unwrap()
        );
    }
}
