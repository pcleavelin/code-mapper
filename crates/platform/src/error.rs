use std::error::Error;
use std::fmt;

use wgpu::{CreateSurfaceError, RequestAdapterError, RequestDeviceError};
use winit::error::{EventLoopError, OsError};

#[derive(Clone, Copy, Debug)]
pub struct Reason(&'static str);

impl Reason {
    pub(crate) const fn new(text: &'static str) -> Self {
        Self(text)
    }
}

impl fmt::Display for Reason {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.0)
    }
}

#[derive(Debug)]
pub enum StartError {
    EventLoop(EventLoopError),
    Window(OsError),
    Surface(CreateSurfaceError),
    Adapter(RequestAdapterError),
    Device(RequestDeviceError),
    SurfaceConfiguration,
    Font(Reason),
}

impl fmt::Display for StartError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EventLoop(error) => write!(formatter, "event loop: {error}"),
            Self::Window(error) => write!(formatter, "window: {error}"),
            Self::Surface(error) => write!(formatter, "surface: {error}"),
            Self::Adapter(error) => write!(formatter, "no GPU adapter: {error}"),
            Self::Device(error) => write!(formatter, "device: {error}"),
            Self::SurfaceConfiguration => {
                formatter.write_str("surface config: unsupported surface")
            }
            Self::Font(reason) => write!(formatter, "font: {reason}"),
        }
    }
}

impl Error for StartError {}
