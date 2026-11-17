mod common;

use bevy_krkr::{script::Tjs2Engine, sys};
use std::ffi::CString;

static ENGINE_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[test]
fn motionplayer_alias_registers_motion_api() {
    let _guard = ENGINE_LOCK.lock().unwrap();
    common::initialize_renderer();
    let path = CString::new(format!("{}/assets/game", env!("CARGO_MANIFEST_DIR"))).unwrap();
    assert_eq!(
        unsafe { sys::krkr_engine_init(path.as_ptr()) },
        sys::KRKR_OK
    );
    let engine = Tjs2Engine::new().unwrap();
    assert_eq!(engine.eval("typeof global.Motion").unwrap(), "undefined");
    engine
        .exec_script(
            "Plugins.link('motionplayer.dll'); Plugins.link('emoteplayer.dll');",
            "Emote alias",
        )
        .unwrap();
    for member in ["ResourceManager", "EmotePlayer", "Player", "D3DAdaptor"] {
        assert_eq!(
            engine.eval(&format!("typeof Motion.{member}")).unwrap(),
            "Object"
        );
    }
    drop(engine);
    unsafe { sys::krkr_engine_shutdown() };
}

#[test]
#[ignore = "requires local test_games/snbk/data720.xp3"]
fn snbk_motion_resource_renders() {
    let _guard = ENGINE_LOCK.lock().unwrap();
    common::initialize_renderer();
    let mut previous_session = None;
    for _ in 0..2 {
        let path = CString::new(format!("{}/assets/game", env!("CARGO_MANIFEST_DIR"))).unwrap();
        assert_eq!(
            unsafe { sys::krkr_engine_init(path.as_ptr()) },
            sys::KRKR_OK
        );
        let engine = Tjs2Engine::new().unwrap();
        let run = |s: &str| engine.exec_script(s, "snbk motion").unwrap();
        run(
            "Plugins.link('xp3filter.dll'); Plugins.link('motionplayer.dll'); global.manager = new Motion.ResourceManager(null, 0);",
        );
        let sample = format!(
            "{}/test_games/snbk/data720.xp3>sd001.mtn",
            env!("CARGO_MANIFEST_DIR")
        );
        run(&format!("global.root = manager.load({sample:?});"));
        run(
            "global.w = new Window(); w.setInnerSize(1280,720); w.visible=true; global.layer = new Layer(w,null); layer.setImageSize(1280,720); layer.setSizeToImageSize(); layer.type=ltAlpha; layer.visible=true; global.player = new Motion.Player(manager); player.chara='SD001'; player.play('SD001AA',0); player.progress(0); player.draw(layer);",
        );
        assert_eq!(engine.eval("player.playing").unwrap(), "1");
        let frame = || {
            let (mut width, mut height) = (0, 0);
            let id =
                unsafe { sys::krkr_scene_build(std::ptr::null_mut(), &mut width, &mut height) };
            assert_ne!(id, 0);
            bevy_krkr::plugin::render::read_pixels(id).unwrap()
        };
        let pixels = frame();
        assert!(
            pixels
                .chunks_exact(4)
                .any(|p| p[0] != 0 || p[1] != 0 || p[2] != 0)
        );
        run("invalidate player; player=void; root=void;");
        run(&format!(
            "manager.unload({sample:?}); manager.load({sample:?}); player=new Motion.Player(manager); player.chara='SD001'; player.play('SD001AA',0); player.progress(0); player.draw(layer);"
        ));
        assert_eq!(
            frame(),
            pixels,
            "resource reload must reproduce the original pixels"
        );
        if let Some(previous) = &previous_session {
            assert_eq!(
                &pixels, previous,
                "engine restart must reproduce the original pixels"
            );
        }
        previous_session = Some(pixels.clone());
        run("player.progress(100); player.setCoord(100,0); player.draw(layer);");
        assert!(
            frame() != pixels,
            "moving the player must update rendered pixels"
        );
        run("invalidate player; player=void; manager.unloadAll();");
        drop(engine);
        unsafe { sys::krkr_engine_shutdown() };
    }
}

#[test]
fn emote_controls_and_shapes_survive_restore_and_restart() {
    let _guard = ENGINE_LOCK.lock().unwrap();
    common::initialize_renderer();
    for session in 0..2 {
        let path = CString::new(format!("{}/assets/game", env!("CARGO_MANIFEST_DIR"))).unwrap();
        assert_eq!(
            unsafe { sys::krkr_engine_init(path.as_ptr()) },
            sys::KRKR_OK
        );
        let engine = Tjs2Engine::new().unwrap();
        let run = |script: &str| engine.exec_script(script, "Emote controls").unwrap();
        let fixture = format!(
            "{}/tests/fixtures/emote-controls.psb",
            env!("CARGO_MANIFEST_DIR")
        );
        run(&format!(
            "Plugins.link('emoteplayer.dll'); global.manager = new Motion.ResourceManager(null,0); manager.load({fixture:?}); global.player = new Motion.EmotePlayer(manager); player.motionKey={fixture:?}; player.play('idle',0);"
        ));
        let number = |expression: &str| engine.eval(expression).unwrap().parse::<f64>().unwrap();
        let close = |expression: &str, expected: f64| {
            let actual = number(expression);
            assert!(
                (actual - expected).abs() < 0.001,
                "session {session}: {expression}: {actual} != {expected}"
            );
        };
        assert_eq!(
            engine.eval("player.variableKeys.join(',')").unwrap(),
            "pose"
        );
        assert_eq!(
            engine
                .eval("player.getMainTimelineLabelList().join(',')")
                .unwrap(),
            "main"
        );
        assert_eq!(
            engine
                .eval("player.getDiffTimelineLabelList().join(',')")
                .unwrap(),
            "diff"
        );
        close("player.speed", 1000.0 / 60.0);
        run(
            "player.setVariable('pose',0); player.setVariable('pose',100,20); player.setCoord(40,20,20); player.setScale(2,20); player.setRotate(Math.PI/2,20); player.progress(1000.0/6);",
        );
        close("player.tickCount", 10.0);
        close("player.getVariable('pose')", 50.0);
        close("player.serialize().currCoordx", 20.0);
        close("player.serialize().currAngle", 45.0);
        close("player.serialize().currZx", 1.5);
        run(
            "global.saved=player.serialize(); player.progress(1000.0/6); player.unserialize(saved);",
        );
        close("player.getVariable('pose')", 50.0);
        close("player.serialize().currCoordx", 20.0);
        run(&format!(
            "global.restored=new Motion.EmotePlayer(manager); restored.motionKey={fixture:?}; restored.play('idle',0); restored.unserialize(saved); restored.progress(1000.0/6);"
        ));
        close("restored.getVariable('pose')", 100.0);
        close("restored.serialize().currCoordx", 40.0);
        run("invalidate restored; restored=void; player.progress(1000.0/6);");
        close("player.getVariable('pose')", 100.0);
        close("player.serialize().currCoordx", 40.0);
        run(
            "player.setCoord(12,32); player.setRotate(0); player.setScale(1); global.w=new Window(); w.setInnerSize(64,64); global.layer=new Layer(w,null); layer.setImageSize(64,64); layer.setSizeToImageSize(); player.setVariable('pose',0); player.draw(layer);",
        );
        assert_eq!(
            engine.eval("player.hitTest('pose-shape',12,32)").unwrap(),
            "1"
        );
        assert_eq!(
            engine.eval("player.hitTest('pose-shape',33,32)").unwrap(),
            "0"
        );
        run("player.playTimeline('main'); player.progress(1000.0/6); player.draw(layer);");
        assert_eq!(
            engine.eval("player.hitTest('pose-shape',33,32)").unwrap(),
            "1"
        );
        assert_eq!(
            engine.eval("player.hitTest('pose-shape',12,32)").unwrap(),
            "0"
        );
        close("player.serialize().controlState[0]", 30.0);
        close("player.getPlayingTimelineInfoList()[0].blendRatio", 1.0);
        assert_eq!(
            engine.eval("player.getTimelinePlaying('main')").unwrap(),
            "1"
        );
        run(
            "global.timelineSaved=player.serialize(); player.stopTimeline(); player.unserialize(timelineSaved);",
        );
        assert_eq!(
            engine.eval("player.getTimelinePlaying('main')").unwrap(),
            "1"
        );
        run("player.progress(1000.0/6);");
        assert_eq!(
            engine.eval("player.getTimelinePlaying('main')").unwrap(),
            "0"
        );
        close("player.getVariable('pose')", 100.0);
        run(
            "player.setVariable('pose',25); player.playTimeline('diff'); player.setTimelineBlendRatio('diff',0.5); player.progress(1000.0/6); player.draw(layer);",
        );
        close("player.getPlayingTimelineInfoList()[0].blendRatio", 0.5);
        assert_eq!(
            engine.eval("player.hitTest('pose-shape',31,32)").unwrap(),
            "1"
        );
        run("player.fadeOutTimeline('diff',5); player.progress(1000.0/12);");
        assert_eq!(engine.eval("player.getTimelinePlaying()").unwrap(), "0");
        run("player.draw(layer);");
        assert_eq!(
            engine.eval("player.hitTest('pose-shape',31,32)").unwrap(),
            "0"
        );
        run("player.setCoord(32,32); player.setScale(1); player.setRotate(0); player.draw(layer);");
        assert_eq!(engine.eval("player.hitTest(32,32)").unwrap(), "1");
        assert_eq!(engine.eval("player.hitTest('anchor',32,32)").unwrap(), "1");
        assert_eq!(engine.eval("player.hitTest('missing',32,32)").unwrap(), "0");
        assert_eq!(engine.eval("player.hitTest(0,0)").unwrap(), "0");
        run("player.setCoord(48,32); player.draw(layer);");
        assert_eq!(engine.eval("player.hitTest(32,32)").unwrap(), "0");
        assert_eq!(engine.eval("player.hitTest('anchor',48,32)").unwrap(), "1");
        run("player.setDrawAffineTranslateMatrix(1,0,0,1,-16,0); player.draw(layer);");
        assert_eq!(engine.eval("player.hitTest('anchor',32,32)").unwrap(), "1");
        run(
            "player.setCoord(32,32); player.setDrawAffineTranslateMatrix(1,0,0,1,0,0); player.initPhysics(%['bustControl'=>[%['baseLayer'=>'anchor','var_lr'=>'physicsX','var_ud'=>'physicsY','gravity'=>0,'spring'=>0.03125,'friction'=>0.125]],'hairControl'=>[],'partsControl'=>[]]); player.setOuterForce('bust',1,0); player.progress(1000.0/60);",
        );
        let angle = std::f64::consts::PI / 80.0;
        close(
            "player.getVariable('physicsX')",
            (0.875 * 1.15 * angle).atan() / angle,
        );
        close("player.getVariable('physicsY')", 0.0);
        run("player.bustScale=0; player.progress(1000.0/60);");
        close("player.getVariable('physicsX')", 0.0);
        run(&format!(
            r#"
            global.physicsRules=%['bustControl'=>[%['baseLayer'=>'anchor','var_lr'=>'physicsX','var_ud'=>'physicsY','gravity'=>0,'spring'=>0.03125,'friction'=>0.125]],'hairControl'=>[],'partsControl'=>[]];
            global.makePhysicsPlayer=function() {{
                var p=new Motion.EmotePlayer(manager);
                p.motionKey={fixture:?}; p.play('idle',0); p.initPhysics(physicsRules);
                p.setOuterForce('bust',1,0); return p;
            }};
            global.whole=makePhysicsPlayer(); global.halves=makePhysicsPlayer();
            whole.progress(1000.0/6);
            for(var i=0;i<20;i++) halves.progress(1000.0/120);
        "#
        ));
        close(
            "whole.getVariable('physicsX')",
            number("halves.getVariable('physicsX')"),
        );
        let (mut position, mut velocity) = (0.0, 0.0);
        for _ in 0..10 {
            velocity = (velocity + 1.0 - position / 32.0) * 0.875;
            position += velocity;
        }
        close(
            "whole.getVariable('physicsX')",
            (position * 1.15 * angle).atan() / angle,
        );
        run(r#"
            global.chain=%['baseLayer'=>'anchor','var_lr'=>'chainX','var_lrm'=>'chainMid','var_ud'=>'chainY','gravity'=>0,'b_rate'=>0.125,'length'=>[0],'scale_x'=>[1],'scale_y'=>[1], 'param'=>%['bp'=>[%['x'=>0,'y'=>0,'z'=>0]],'p'=>[%['x'=>0,'y'=>0,'z'=>0]],'pv'=>[%['x'=>0,'y'=>0,'z'=>0]]]];
            whole.initPhysics(%['bustControl'=>[],'hairControl'=>[chain],'partsControl'=>[]]);
            halves.initPhysics(%['bustControl'=>[],'hairControl'=>[],'partsControl'=>[chain]]);
            whole.setOuterForce('hair',1,0); halves.setOuterForce('parts',1,0);
            whole.progress(1000.0/60); halves.progress(1000.0/60);
        "#);
        close(
            "whole.getVariable('chainX')",
            (0.96875 * 1.15 * angle).atan() / angle,
        );
        close(
            "halves.getVariable('chainX')",
            number("whole.getVariable('chainX')"),
        );
        close("whole.getVariable('chainY')", 0.0);
        run(r#"
            whole.initPhysics(%['bustControl'=>[],'hairControl'=>[chain],'partsControl'=>[]]);
            whole.setOuterForce('hair',0,0); whole.startWind(-32,32,1,8,8);
            for(var i=0;i<128;i++) whole.progress(1000.0/60);
        "#);
        assert!(number("whole.getVariable('chainX')").abs() > 0.01);
        run("whole.stopWind(); for(var i=0;i<1024;i++) whole.progress(1000.0/60);");
        close("whole.getVariable('chainX')", 0.0);
        run("whole.setVariable('pose',75);");
        close("halves.getVariable('pose')", 0.0);
        run(
            "invalidate whole; invalidate halves; whole=void; halves=void; makePhysicsPlayer=void;",
        );
        run(&format!(
            "invalidate player; player=void; manager.unload({fixture:?}); manager.load({fixture:?}); player=new Motion.EmotePlayer(manager); player.motionKey={fixture:?}; player.play('idle',0); player.setCoord(32,32); player.draw(layer);"
        ));
        close("player.tickCount", 0.0);
        close("player.getVariable('pose')", 0.0);
        assert_eq!(engine.eval("player.hitTest('anchor',32,32)").unwrap(), "1");
        run("invalidate player; player=void; manager.unloadAll();");
        drop(engine);
        unsafe { sys::krkr_engine_shutdown() };
    }
}
