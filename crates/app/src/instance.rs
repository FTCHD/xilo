use std::io::Write as _;

use gpui_kit::*;
use tokio::sync::mpsc;
use xilo_store::Paths;

use crate::state::AppState;

/// Signals that another launch asked to show the window.
pub type Wake = mpsc::UnboundedReceiver<()>;

/// Makes this the only running Xilo. Returns `None` when another
/// instance is running; that instance is asked to show its window.
#[cfg(unix)]
pub fn claim(paths: &Paths) -> Option<Wake> {
    use std::io::Read as _;
    use std::os::unix::net::{UnixListener, UnixStream};

    let _ = std::fs::create_dir_all(&paths.data);
    let socket = paths.data.join("xilo.sock");
    if let Ok(mut stream) = UnixStream::connect(&socket) {
        let _ = stream.write_all(b"show\n");
        return None;
    }
    let _ = std::fs::remove_file(&socket);
    let (tx, rx) = mpsc::unbounded_channel();
    match UnixListener::bind(&socket) {
        Ok(listener) => {
            std::thread::Builder::new()
                .name("xilo-instance".into())
                .spawn(move || {
                    for mut stream in listener.incoming().flatten() {
                        let mut buf = [0u8; 8];
                        let _ = stream.read(&mut buf);
                        if tx.send(()).is_err() {
                            break;
                        }
                    }
                })
                .ok()?;
        }
        Err(err) => tracing::warn!("single instance lock unavailable: {err}"),
    }
    Some(rx)
}

#[cfg(not(unix))]
pub fn claim(_paths: &Paths) -> Option<Wake> {
    Some(mpsc::unbounded_channel().1)
}

pub fn listen(mut wake: Wake, state: Entity<AppState>, cx: &mut App) {
    cx.spawn(async move |cx: &mut AsyncApp| {
        while wake.recv().await.is_some() {
            let state = state.clone();
            cx.update(|cx| crate::open_main_window(state, cx));
        }
    })
    .detach();
}
