#[cfg(not(target_os = "android"))]
fn main() {}

#[cfg(target_os = "android")]
mod video {
    use bevy::prelude::*;
    use bevy_inspector_egui::bevy_egui::{EguiContexts, EguiPlugin, EguiPrimaryContextPass, egui};
    use bevy_krkr::{prelude::*, script::Tjs2Engine};
    use std::{io::Read, path::PathBuf};

    #[derive(Resource)]
    struct Session {
        root: PathBuf,
        started: bool,
        hardware: bool,
        error: String,
    }

    #[bevy_main]
    fn main() {
        let android = bevy::android::ANDROID_APP.get().expect("Android app");
        let root = android
            .internal_data_path()
            .expect("internal data path")
            .join("video-test");
        std::fs::create_dir_all(&root).expect("create media directory");
        let mut movie = Vec::new();
        android
            .asset_manager()
            .open(c"fd_op01.mpg")
            .expect("bundled movie")
            .read_to_end(&mut movie)
            .expect("read bundled movie");
        std::fs::write(root.join("fd_op01.mpg"), movie).expect("extract bundled movie");
        bevy_krkr::storage::set_filesystem(std::sync::Arc::new(
            bevy_krkr::storage::AndroidFileSystem::new(root.clone()),
        ));
        let mut app = App::new();
        app.add_plugins(KrkrVideoPlugin::default())
            .add_plugins(
                DefaultPlugins
                    .set(bevy::render::RenderPlugin {
                        synchronous_pipeline_compilation: true,
                        ..default()
                    })
                    .set(WindowPlugin {
                        primary_window: Some(Window {
                            title: "Fate video test".into(),
                            ..default()
                        }),
                        ..default()
                    }),
            )
            .add_plugins(EguiPlugin::default())
            .add_plugins(BevyKrkrPlugin {
                screen_size: UVec2::new(800, 600),
            })
            .insert_resource(Session {
                root,
                started: false,
                hardware: true,
                error: String::new(),
            })
            .add_systems(Startup, setup)
            .add_systems(Update, start)
            .add_systems(Update, report)
            .add_systems(Update, capture)
            .add_systems(EguiPrimaryContextPass, controls);
        app.run();
    }

    fn setup(
        mut commands: Commands,
        session: Res<Session>,
        mut enter: MessageWriter<KirikiriEnterEvent>,
    ) {
        if let Err(error) = report_codec_capabilities() {
            warn!("video codec capabilities unavailable: {error}");
        }
        commands.spawn((
            Camera2d,
            KirikiriCamera,
            bevy::render::view::NoIndirectDrawing,
        ));
        if session.root.join("defer-video").exists() {
            commands.spawn((
                Sprite::from_color(Color::srgb(0.1, 0.8, 0.2), Vec2::new(1000.0, 500.0)),
                Transform::from_xyz(0.0, 0.0, 10.0),
                bevy::camera::visibility::RenderLayers::layer(27),
            ));
        }
        enter.write(KirikiriEnterEvent {
            data_xp3_path: session.root.clone(),
        });
    }

    fn report_codec_capabilities() -> jni::errors::Result<()> {
        use jni::{
            JavaVM,
            objects::{JObject, JObjectArray, JString, JValue},
        };

        if android_activity::AndroidApp::sdk_version() < 29 {
            info!("video codec: hardware classification requires Android 10+");
            return Ok(());
        }
        let app = bevy::android::ANDROID_APP.get().expect("Android app");
        let vm = unsafe { JavaVM::from_raw(app.vm_as_ptr().cast()) }?;
        let mut env = vm.attach_current_thread()?;
        env.with_local_frame(64, |env| {
            let list = env.new_object("android/media/MediaCodecList", "(I)V", &[JValue::Int(1)])?;
            let codecs = JObjectArray::from(env.call_method(&list, "getCodecInfos", "()[Landroid/media/MediaCodecInfo;", &[])?.l()?);
            for index in 0..env.get_array_length(&codecs)? {
                env.with_local_frame(32, |env| {
                    let codec = env.get_object_array_element(&codecs, index)?;
                    if env.call_method(&codec, "isEncoder", "()Z", &[])?.z()? {
                        return Ok::<_, jni::errors::Error>(JObject::null());
                    }
                    let types = JObjectArray::from(env.call_method(&codec, "getSupportedTypes", "()[Ljava/lang/String;", &[])?.l()?);
                    let mut video = false;
                    for index in 0..env.get_array_length(&types)? {
                        let mime = JString::from(env.get_object_array_element(&types, index)?);
                        video |= env.get_string(&mime)?.to_string_lossy().starts_with("video/");
                    }
                    if video {
                        let name = JString::from(env.call_method(&codec, "getName", "()Ljava/lang/String;", &[])?.l()?);
                        let name: String = env.get_string(&name)?.into();
                        let hardware = env.call_method(&codec, "isHardwareAccelerated", "()Z", &[])?.z()?;
                        let software = env.call_method(&codec, "isSoftwareOnly", "()Z", &[])?.z()?;
                        info!("video codec: name={name} hardware_accelerated={hardware} software_only={software}");
                    }
                    Ok(JObject::null())
                })?;
            }
            Ok(())
        })
    }

    fn capture(
        mut commands: Commands,
        session: Res<Session>,
        time: Res<Time>,
        mut done: Local<bool>,
    ) {
        if *done || time.elapsed_secs() < 12.0 {
            return;
        }
        *done = true;
        use bevy::render::view::screenshot::{Screenshot, save_to_disk};
        commands
            .spawn(Screenshot::primary_window())
            .observe(save_to_disk(session.root.join("display.png")));
    }

    fn report(
        time: Res<Time>,
        mut last: Local<f32>,
        scene: Res<krkr_plugin::render::KirikiriScene>,
        cameras: Query<(&Camera, &Projection), With<KirikiriCamera>>,
        screens: Query<(&Transform, &ViewVisibility), With<krkr_plugin::render::KirikiriScreen>>,
    ) {
        if time.elapsed_secs() - *last < 5.0 {
            return;
        }
        *last = time.elapsed_secs();
        for (camera, projection) in &cameras {
            info!(
                "Video display: scene={:?}, viewport={:?}, projection={projection:?}",
                scene.size,
                camera.physical_viewport_size()
            );
        }
        for (transform, visibility) in &screens {
            info!("Video screen: {transform:?}, visible={}", visibility.get());
        }
    }

    const OPEN: &str = r#"
        global.w = new Window(); w.setInnerSize(800,600); w.visible=true;
        global.base = new Layer(w,null); base.setImageSize(800,600); base.setSizeToImageSize();
        base.fillRect(0,0,800,600,0xff000000);
        global.v = new VideoOverlay(w); v.open("fd_op01.mpg");
        v.setBounds(0,0,800,600); v.visible=true; v.loop=true; v.play();
    "#;

    fn start(engine: Option<NonSend<Tjs2Engine>>, mut session: ResMut<Session>) {
        let Some(engine) = engine else { return };
        if session.started {
            return;
        }
        if session.root.join("defer-video").exists() {
            return;
        }
        session.started = true;
        if let Err(error) = engine.exec_script(OPEN, "Android video test") {
            session.error = error.to_string();
            error!("{}", session.error);
        }
    }

    fn controls(
        mut contexts: EguiContexts,
        engine: Option<NonSend<Tjs2Engine>>,
        mut session: ResMut<Session>,
    ) -> Result {
        let Some(engine) = engine else { return Ok(()) };
        egui::Window::new("Fate video test")
            .default_pos([8.0, 8.0])
            .show(contexts.ctx_mut()?, |ui| {
                ui.label("fd_op01.mpg | H.264 + AAC | 800 x 600");
                if let Ok(state) =
                    engine.eval("'Time: '+v.position+' / '+v.totalTime+' ms, frame: '+v.frame")
                {
                    ui.label(state);
                }
                let mut action = None;
                ui.horizontal(|ui| {
                    if ui.button("Play").clicked() {
                        action = Some("v.play();");
                    }
                    if ui.button("Pause").clicked() {
                        action = Some("v.pause();");
                    }
                    if ui.button("-10s").clicked() {
                        action = Some("v.position=Math.max(0,v.position-10000);");
                    }
                    if ui.button("+10s").clicked() {
                        action = Some("v.position=Math.min(v.totalTime-100,v.position+10000);");
                    }
                    if ui.button("Restart").clicked() {
                        action = Some("v.position=0; v.play();");
                    }
                });
                if ui
                    .checkbox(&mut session.hardware, "Hardware decoding (reopens video)")
                    .changed()
                {
                    match engine.exec_script("invalidate v; v=void;", "close video") {
                        Ok(()) => {
                            bevy_krkr::plugin::video::initialize(session.hardware);
                            action = Some(
                                "global.v=new VideoOverlay(w); v.open('fd_op01.mpg'); v.setBounds(0,0,800,600); v.visible=true; v.loop=true; v.play();",
                            );
                        }
                        Err(error) => session.error = error.to_string(),
                    }
                }
                if let Some(action) = action {
                    info!("Android video test: {action}");
                    if let Err(error) = engine.exec_script(action, "video controls") {
                        session.error = error.to_string();
                    }
                }
                if !session.error.is_empty() {
                    ui.colored_label(egui::Color32::RED, &session.error);
                }
            });
        Ok(())
    }
}
