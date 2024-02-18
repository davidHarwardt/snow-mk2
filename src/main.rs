#![allow(unused)]

use objc2::rc::Id;
use raw_window_handle::{HasRawWindowHandle, RawWindowHandle};
use winit::{
    event_loop::{EventLoopBuilder, ControlFlow},
    window::{WindowBuilder, WindowLevel},
    event::{Event, WindowEvent},
    platform::macos::EventLoopBuilderExtMacOS, dpi::{LogicalPosition, PhysicalPosition},
};
use tracing_subscriber::prelude::*;

use icrate::{
    Foundation::{MainThreadMarker, ns_string},
    AppKit::{NSApplication, NSStatusBar, NSImage, NSView, NSScreen, NSWindow, self}
};

mod gfx;
mod snow;
mod utils;

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_env("wgpu=warn"))
    .init();

    let event_loop = EventLoopBuilder::new()
    .build()?;

    let main_thread = MainThreadMarker::new().expect("not on main thread");
    let app = NSApplication::sharedApplication(main_thread);
    let main_screen = NSScreen::mainScreen(main_thread).expect("no main screen");
    
    let win2 = WindowBuilder::new()
        .with_transparent(true)
        .with_active(false)
        .with_decorations(false)
        // .with_window_level(WindowLevel::AlwaysOnBottom)
        .with_window_level(WindowLevel::AlwaysOnTop)
    .build(&event_loop)?;

    // let win2 = WindowBuilder::new()
    //     // .with_transparent(true)
    //     // .with_blur(true)
    //     .with_active(false)
    //     .with_decorations(false)
    //     .with_window_level(WindowLevel::AlwaysOnTop)
    // .build(&event_loop)?;

    let win2_raw = win2.raw_window_handle();
    let win2_nsview: Id<NSView> = match win2_raw {
        RawWindowHandle::AppKit(handle) => unsafe {
            Id::new(handle.ns_view as *mut NSView)
                .expect("could not get ns_view")
        },
        _ => panic!("did not get the appropriate window handle"),
    };
    let win2_nswindow = win2_nsview.window().expect("could not get window");
    win2_nswindow.setTitle(ns_string!("test"));

    win2_nswindow.setMovable(false);
    win2_nswindow.setFrame_display(main_screen.frame(), true);
    unsafe {
        win2_nswindow.setCollectionBehavior(
            // moves with current space + can overlay fullscreen windows
              AppKit::NSWindowCollectionBehaviorCanJoinAllSpaces
            // cant tab to window
            | AppKit::NSWindowCollectionBehaviorIgnoresCycle
            // stays even while mission control
            | AppKit::NSWindowCollectionBehaviorStationary
            // dont show in fullscreen
            | AppKit::NSWindowCollectionBehaviorFullScreenNone
        );
    }
    win2.set_cursor_hittest(false)?;

    unsafe {
        let status_bar = NSStatusBar::systemStatusBar();
        let status_item = status_bar.statusItemWithLength(AppKit::NSSquareStatusItemLength);
        if let Some(btn) = status_item.button(main_thread) {
            btn.setImage(NSImage::imageWithSystemSymbolName_accessibilityDescription(
                ns_string!("mic"),
                None,
            ).as_deref());
        }

        if let Some(btn) = status_item.button(main_thread) {
            btn.setImage(NSImage::imageWithSystemSymbolName_accessibilityDescription(
                ns_string!("snowflake"),
                None,
            ).as_deref());
        }
    }

    let mut state = pollster::block_on(
        gfx::State::new(main_thread, &event_loop)
    )?;

    event_loop.set_control_flow(ControlFlow::Poll);

    // add:
    // - collision field (land on windows)
    // - velocity field (interaction with cursor)

    event_loop.set_control_flow(ControlFlow::Poll);

    event_loop.run(move |ev, target| {
        match ev {
            Event::WindowEvent { event, window_id } => match event {
                WindowEvent::CloseRequested => target.exit(),
                WindowEvent::RedrawRequested => {
                    state.update();
                    match state.render() {
                        Ok(_) => (),
                        // Err(wgpu::SurfaceError::Lost) => state.resize(state.size),
                        Err(wgpu::SurfaceError::OutOfMemory) => {
                            tracing::error!("out of memory");
                            target.exit();
                        },
                        Err(e) => tracing::error!("render error: {e:?}"),
                    }
                },
                WindowEvent::Occluded(v) => tracing::info!("occluded: {v}"),
                event => state.event(&window_id, event),
                _ => (),
            },
            Event::AboutToWait => state.redraw(),
            _ => (),
        }
    })?;
    Ok(())
}

