struct MonitorSmoke;

impl winit::application::ApplicationHandler for MonitorSmoke {
    fn resumed(&mut self, event_loop: &winit::event_loop::ActiveEventLoop) {
        for monitor in event_loop.available_monitors() {
            let _ = monitor.scale_factor();
        }
        event_loop.exit();
    }

    fn window_event(
        &mut self,
        _event_loop: &winit::event_loop::ActiveEventLoop,
        _window_id: winit::window::WindowId,
        _event: winit::event::WindowEvent,
    ) {
    }
}

fn main() -> Result<(), winit::error::EventLoopError> {
    let event_loop = winit::event_loop::EventLoop::new()
        .expect("EventLoop must be created on main thread");
    let mut app = MonitorSmoke;
    event_loop.run_app(&mut app)
}
