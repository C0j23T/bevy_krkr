use std::sync::{Mutex, MutexGuard};

use bevy::{
    app::SubApp,
    ecs::{
        schedule::ScheduleCleanupPolicy,
        system::{Adapt, IntoAdapterSystem, RunSystemError, System, SystemIn},
    },
    prelude::*,
    render::{
        Render, RenderSystems,
        view::window::{create_surfaces, need_surface_configuration, prepare_windows},
    },
};

// Surface::configure must not overlap Queue::submit, even for unrelated textures.
// Never acquire the Kirikiri renderer mutex while holding this lock.
static SUBMISSION: Mutex<()> = Mutex::new(());

pub(super) fn lock() -> MutexGuard<'static, ()> {
    SUBMISSION.lock().unwrap_or_else(|error| error.into_inner())
}

struct SurfaceAccess;

impl<S: System<In = ()>> Adapt<S> for SurfaceAccess {
    type In = ();
    type Out = S::Out;

    fn adapt(
        &mut self,
        input: (),
        run_system: impl FnOnce(SystemIn<'_, S>) -> Result<S::Out, RunSystemError>,
    ) -> Result<Self::Out, RunSystemError> {
        let _surface = lock();
        run_system(input)
    }
}

pub(super) fn install(app: &mut SubApp) {
    for removed in [
        app.remove_systems_in_set(
            Render,
            create_surfaces,
            ScheduleCleanupPolicy::RemoveSystemsOnly,
        ),
        app.remove_systems_in_set(
            Render,
            prepare_windows,
            ScheduleCleanupPolicy::RemoveSystemsOnly,
        ),
    ] {
        assert_eq!(removed.expect("Bevy window render systems must exist"), 1);
    }
    app.add_systems(
        Render,
        (
            IntoAdapterSystem::new(SurfaceAccess, create_surfaces)
                .run_if(need_surface_configuration)
                .before(prepare_windows),
            IntoAdapterSystem::new(SurfaceAccess, prepare_windows)
                .in_set(RenderSystems::PrepareViews),
        ),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::ecs::system::IntoSystem;

    #[test]
    fn surface_system_excludes_submissions_and_releases_lock() {
        let mut system = IntoSystem::into_system(IntoAdapterSystem::new(SurfaceAccess, || {
            std::thread::scope(|scope| {
                scope.spawn(|| assert!(SUBMISSION.try_lock().is_err()));
            });
        }));
        let mut world = World::new();
        system.initialize(&mut world);
        system.run((), &mut world).unwrap();
        assert!(SUBMISSION.try_lock().is_ok());

        let mut failing = IntoSystem::into_system(IntoAdapterSystem::new(
            SurfaceAccess,
            || -> Result<(), &'static str> { Err("surface system failed") },
        ));
        failing.initialize(&mut world);
        assert!(failing.run((), &mut world).unwrap().is_err());
        assert!(SUBMISSION.try_lock().is_ok());
    }

    #[test]
    fn window_system_order_survives_wrapping() {
        use bevy::ecs::schedule::IntoSystemSet;

        let mut app = SubApp::new();
        app.init_schedule(Render).add_systems(
            Render,
            (
                create_surfaces
                    .run_if(need_surface_configuration)
                    .before(prepare_windows),
                prepare_windows.in_set(RenderSystems::PrepareViews),
            ),
        );
        install(&mut app);
        app.world_mut().schedule_scope(Render, |world, schedule| {
            schedule.initialize(world).unwrap();
            let positions: Vec<_> = [
                create_surfaces.into_system_set().intern(),
                prepare_windows.into_system_set().intern(),
            ]
            .into_iter()
            .map(|set| {
                let members = schedule.graph().systems_in_set(set).unwrap();
                assert_eq!(members.len(), 1);
                schedule
                    .systems()
                    .unwrap()
                    .position(|(key, _)| members.contains(&key))
                    .unwrap()
            })
            .collect();
            assert!(positions[0] < positions[1]);
        });
    }
}
