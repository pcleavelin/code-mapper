use std::env;
use std::sync::Arc;
use std::time::{Duration, Instant};

use ui::{
    Button, Color, Coordinate, Count, DrawList, Extent, Glyph, Id, Input, Key, Mods, Point, Press,
    Px, Scale, Vector,
};
use winit::application::ApplicationHandler;
use winit::dpi::{LogicalSize, PhysicalSize};
use winit::event::{ElementState, KeyEvent, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{Key as WindowKey, NamedKey};
use winit::window::{CursorIcon, Window, WindowId};

use crate::error::StartError;
use crate::renderer::Renderer;
use crate::report::report;
use crate::script::{Outcome, Script, ScriptLine};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Exit {
    Stay,
    Quit,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cursor {
    Default,
    ColumnResize,
    RowResize,
    Grab,
    Grabbing,
}

impl Cursor {
    const fn icon(self) -> CursorIcon {
        match self {
            Self::Default => CursorIcon::Default,
            Self::ColumnResize => CursorIcon::ColResize,
            Self::RowResize => CursorIcon::RowResize,
            Self::Grab => CursorIcon::Grab,
            Self::Grabbing => CursorIcon::Grabbing,
        }
    }
}

pub struct Frame {
    pub redraw_after: Duration,
    pub exit: Exit,
    pub clear: Color,
    pub cursor: Cursor,
    pub drawing: DrawList,
}

pub trait App {
    fn frame(&mut self, renderer: &mut Renderer, input: &mut Input) -> Frame;
    fn script(&mut self, line: &ScriptLine) -> Outcome;
    fn locate(&mut self, id: Id) -> Option<Point>;
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Title(String);

impl Title {
    pub fn new(text: impl Into<String>) -> Self {
        Self(text.into())
    }

    fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum WindowMode {
    Fixed,
    Maximised,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Redraw {
    Pending,
    Idle,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
struct Click {
    at: Instant,
    button: Button,
    mouse: Point,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub(crate) struct FrameStatistics {
    frames: Count,
    longest: Duration,
    slow: Count,
}

impl FrameStatistics {
    const SLOW: Duration = Duration::from_millis(16);

    fn record(&mut self, took: Duration) {
        self.frames += Count::new(1);
        self.longest = self.longest.max(took);
        if took > Self::SLOW {
            self.slow += Count::new(1);
        }
    }

    pub(crate) fn dump(&mut self, start: Instant) {
        report(format_args!(
            "DUMP frames n={} max={:.1} over16={} t={:.1}",
            self.frames,
            self.longest.as_secs_f64() * 1000.0,
            self.slow,
            start.elapsed().as_secs_f64() * 1000.0
        ));
        *self = Self::default();
    }
}

const FIXED_WIDTH: Px = Px::new(1600);
const FIXED_HEIGHT: Px = Px::new(1000);
const DOUBLE_CLICK: Duration = Duration::from_millis(350);
const DOUBLE_CLICK_REACH: Px = Px::new(4);
const LINE_SCROLL: Coordinate = Coordinate::new(40.0);
const SCRIPT_FRAME: Duration = Duration::from_millis(8);

pub(crate) struct Runner<Application: App> {
    pub(crate) app: Application,
    title: Title,
    renderer: Option<Renderer>,
    pub(crate) input: Input,
    pub(crate) mods: Mods,
    last_click: Option<Click>,
    redraw: Redraw,
    next_redraw: Instant,
    pub(crate) start: Instant,
    cursor: Cursor,
    pub(crate) script: Option<Script>,
    mode: WindowMode,
    pub(crate) statistics: FrameStatistics,
    failure: Option<StartError>,
}

impl<Application: App> Runner<Application> {
    fn open_window(&mut self, event_loop: &ActiveEventLoop) -> Result<Renderer, StartError> {
        let attributes = Window::default_attributes().with_title(self.title.as_str());
        let attributes = match self.mode {
            WindowMode::Fixed => attributes
                .with_inner_size(PhysicalSize::new(
                    FIXED_WIDTH.unsigned(),
                    FIXED_HEIGHT.unsigned(),
                ))
                .with_resizable(false),
            WindowMode::Maximised => attributes
                .with_inner_size(LogicalSize::new(
                    f64::from(FIXED_WIDTH.get()),
                    f64::from(FIXED_HEIGHT.get()),
                ))
                .with_maximized(true),
        };
        let window = Arc::new(
            event_loop
                .create_window(attributes)
                .map_err(StartError::Window)?,
        );
        let mut renderer = Renderer::new(window)?;
        if self.mode == WindowMode::Fixed {
            renderer.scale = Scale::ONE;
        }
        Ok(renderer)
    }

    fn mouse_button(&mut self, state: ElementState, button: MouseButton) {
        let button = match button {
            MouseButton::Left => Button::Left,
            MouseButton::Right => Button::Right,
            MouseButton::Middle => Button::Middle,
            MouseButton::Back | MouseButton::Forward => {
                if state == ElementState::Pressed {
                    let navigation = if button == MouseButton::Back {
                        Button::Back
                    } else {
                        Button::Forward
                    };
                    self.input.pointer.pressed.insert(navigation);
                }
                self.redraw = Redraw::Pending;
                return;
            }
            MouseButton::Other(_) => return,
        };
        match state {
            ElementState::Pressed => {
                let pointer = &mut self.input.pointer;
                pointer.down.insert(button);
                pointer.pressed.insert(button);
                let now = Instant::now();
                let mouse = pointer.mouse;
                let double = self.last_click.is_some_and(|click| {
                    click.button == button
                        && now.duration_since(click.at) < DOUBLE_CLICK
                        && (click.mouse.horizontal - mouse.horizontal).absolute()
                            < DOUBLE_CLICK_REACH
                        && (click.mouse.vertical - mouse.vertical).absolute() < DOUBLE_CLICK_REACH
                });
                pointer.clicks.set(button, if double { 2 } else { 1 });
                self.last_click = if double {
                    None
                } else {
                    Some(Click {
                        at: now,
                        button,
                        mouse,
                    })
                };
            }
            ElementState::Released => self.input.pointer.down.remove(button),
        }
        self.redraw = Redraw::Pending;
    }

    fn wheel(&mut self, delta: MouseScrollDelta) {
        let amount = match delta {
            MouseScrollDelta::LineDelta(across, down) => Vector::new(
                Coordinate::new(across * LINE_SCROLL.get()),
                Coordinate::new(down * LINE_SCROLL.get()),
            ),
            MouseScrollDelta::PixelDelta(position) => Vector::new(
                Coordinate::narrow(position.x),
                Coordinate::narrow(position.y),
            ),
        };
        let wheel = &mut self.input.pointer.wheel;
        wheel.horizontal = Coordinate::new(wheel.horizontal.get() + amount.horizontal.get());
        wheel.vertical = Coordinate::new(wheel.vertical.get() + amount.vertical.get());
        self.redraw = Redraw::Pending;
    }

    fn keyboard(&mut self, event: &KeyEvent) {
        if event.state == ElementState::Pressed {
            let key = match &event.logical_key {
                WindowKey::Named(NamedKey::Enter) => Some(Key::Enter),
                WindowKey::Named(NamedKey::Escape) => Some(Key::Escape),
                WindowKey::Named(NamedKey::Backspace) => Some(Key::Backspace),
                WindowKey::Named(NamedKey::Delete) => Some(Key::Remove),
                WindowKey::Named(NamedKey::ArrowLeft) => Some(Key::Left),
                WindowKey::Named(NamedKey::ArrowRight) => Some(Key::Right),
                WindowKey::Named(NamedKey::ArrowUp) => Some(Key::Up),
                WindowKey::Named(NamedKey::ArrowDown) => Some(Key::Down),
                WindowKey::Named(NamedKey::Home) => Some(Key::Home),
                WindowKey::Named(NamedKey::End) => Some(Key::End),
                WindowKey::Character(text) => text
                    .chars()
                    .next()
                    .map(|character| Key::Character(Glyph::new(character))),
                _ => None,
            };
            if let Some(key) = key {
                self.input.keys.push(Press {
                    key,
                    mods: self.mods,
                });
            }
            if let Some(text) = &event.text
                && !self.mods.ctrl()
                && !self.mods.alt()
            {
                for character in text.chars().filter(|character| !character.is_control()) {
                    self.input.typed.push(character);
                }
            }
        }
        self.redraw = Redraw::Pending;
    }

    fn redraw(&mut self, event_loop: &ActiveEventLoop) {
        self.input.time = self.start.elapsed();
        let scripted = self.step_script();
        let Some(renderer) = self.renderer.as_mut() else {
            return;
        };
        let began = Instant::now();
        let frame = self.app.frame(renderer, &mut self.input);
        renderer.render(&frame.drawing, frame.clear);
        if frame.cursor != self.cursor {
            self.cursor = frame.cursor;
            renderer.window.set_cursor(frame.cursor.icon());
        }
        self.statistics.record(began.elapsed());
        self.input.end_frame();
        self.redraw = Redraw::Idle;
        self.next_redraw = Instant::now()
            + if self.script.is_some() {
                SCRIPT_FRAME
            } else {
                frame.redraw_after
            };
        if frame.exit == Exit::Quit || scripted == Exit::Quit {
            event_loop.exit();
        }
    }
}

fn real_input(event: &WindowEvent) -> bool {
    matches!(
        event,
        WindowEvent::ModifiersChanged(_)
            | WindowEvent::CursorMoved { .. }
            | WindowEvent::CursorLeft { .. }
            | WindowEvent::MouseInput { .. }
            | WindowEvent::MouseWheel { .. }
            | WindowEvent::KeyboardInput { .. }
    )
}

impl<Application: App> ApplicationHandler for Runner<Application> {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.renderer.is_some() {
            return;
        }
        match self.open_window(event_loop) {
            Ok(renderer) => {
                self.input.size = renderer.size;
                self.renderer = Some(renderer);
            }
            Err(error) => {
                self.failure = Some(error);
                event_loop.exit();
            }
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        if self.renderer.is_none() || (self.script.is_some() && real_input(&event)) {
            return;
        }
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => {
                if let Some(renderer) = self.renderer.as_mut() {
                    renderer.resize(Extent::new(
                        Px::new(i32::try_from(size.width).unwrap_or(0)),
                        Px::new(i32::try_from(size.height).unwrap_or(0)),
                    ));
                    self.input.size = renderer.size;
                }
                self.redraw = Redraw::Pending;
            }
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                if self.mode == WindowMode::Maximised
                    && let Some(renderer) = self.renderer.as_mut()
                {
                    renderer.scale = Scale::new(scale_factor);
                }
                self.redraw = Redraw::Pending;
            }
            WindowEvent::ModifiersChanged(modifiers) => {
                let state = modifiers.state();
                self.mods = Mods::new(state.control_key(), state.shift_key(), state.alt_key());
                self.input.pointer.mods = self.mods;
            }
            WindowEvent::CursorMoved { position, .. } => {
                self.input.pointer.mouse = Point::new(
                    Coordinate::truncate_wide(position.x),
                    Coordinate::truncate_wide(position.y),
                );
                self.redraw = Redraw::Pending;
            }
            WindowEvent::CursorLeft { .. } => {
                self.input.pointer.mouse = Point::new(Px::new(-1), Px::new(-1));
                self.redraw = Redraw::Pending;
            }
            WindowEvent::MouseInput { state, button, .. } => self.mouse_button(state, button),
            WindowEvent::MouseWheel { delta, .. } => self.wheel(delta),
            WindowEvent::KeyboardInput { event, .. } => self.keyboard(&event),
            WindowEvent::RedrawRequested => self.redraw(event_loop),
            _ => {}
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        let Some(renderer) = self.renderer.as_ref() else {
            return;
        };
        let due = Instant::now() >= self.next_redraw;
        if self.redraw == Redraw::Pending || due {
            renderer.window.request_redraw();
        }
        event_loop.set_control_flow(ControlFlow::WaitUntil(self.next_redraw));
    }

    fn exiting(&mut self, _event_loop: &ActiveEventLoop) {
        self.renderer = None;
    }
}

pub fn run<Application: App>(title: Title, app: Application) -> Result<(), StartError> {
    let event_loop = EventLoop::new().map_err(StartError::EventLoop)?;
    let scripted = env::var_os("CODEMAP_SCRIPT").is_some();
    let mode = if scripted || env::var_os("CODEMAP_SHOT").is_some() {
        WindowMode::Fixed
    } else {
        WindowMode::Maximised
    };
    let mut runner = Runner {
        app,
        title,
        renderer: None,
        input: Input::default(),
        mods: Mods::NONE,
        last_click: None,
        redraw: Redraw::Pending,
        next_redraw: Instant::now(),
        start: Instant::now(),
        cursor: Cursor::Default,
        script: Script::load(),
        mode,
        statistics: FrameStatistics::default(),
        failure: None,
    };
    event_loop
        .run_app(&mut runner)
        .map_err(StartError::EventLoop)?;
    match runner.failure {
        Some(error) => Err(error),
        None => Ok(()),
    }
}
