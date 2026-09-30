use winit::event_loop::EventLoopBuilder;

#[cfg(target_os = "macos")]
pub(crate) fn launch_without_focus(builder: &mut EventLoopBuilder<()>) {
    use winit::platform::macos::{ActivationPolicy, EventLoopBuilderExtMacOS};
    builder
        .with_activation_policy(ActivationPolicy::Prohibited)
        .with_activate_ignoring_other_apps(false);
}

#[cfg(not(target_os = "macos"))]
pub(crate) fn launch_without_focus(_builder: &mut EventLoopBuilder<()>) {}
