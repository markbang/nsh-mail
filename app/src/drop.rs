//! Files dropped on the page, on the web. `onFileDrop` is the desktops'
//! (GTK, Win32, AppKit); a browser has no such event in NSH, and the app's
//! TSX runs in a realm of its own that the page's drag events do not reach.
//! The page's own window does, from Rust (docs/design.md §4 附件).

use nsh_core::prelude::*;

/// Files dragged onto the page.
#[module]
pub struct PageDrop;

#[module]
impl PageDrop {
    /// Listens for files dragged over the page: `dragging` hears a drag of
    /// files start and end, `dropped` each dropped file. Does nothing outside
    /// the web. Resolves with a function that stops listening.
    pub fn listen(
        #[args(over)] dragging: Callback<bool>,
        #[args(name, mime, data)] dropped: Callback<(String, String, Vec<u8>)>,
        // In the reactive runtime: the listener's task is spawned here.
        _cx: &mut ModuleCx,
    ) -> Stop {
        #[cfg(target_arch = "wasm32")]
        return web::listen(dragging, dropped);
        #[cfg(not(target_arch = "wasm32"))]
        {
            let _ = (dragging, dropped);
            Stop::new(|| {})
        }
    }
}

#[cfg(target_arch = "wasm32")]
mod web {
    use std::{cell::Cell, future::poll_fn, pin::Pin, rc::Rc};

    use futures_channel::mpsc;
    use futures_core::Stream;
    use js_sys::Uint8Array;
    use nsh_core::prelude::*;
    use wasm_bindgen::{JsCast, closure::Closure};
    use wasm_bindgen_futures::JsFuture;
    use web_sys::{DragEvent, File};

    enum Seen {
        Over(bool),
        Dropped(File),
    }

    type Listener = Closure<dyn FnMut(DragEvent)>;

    fn has_files(e: &DragEvent) -> bool {
        e.data_transfer()
            .is_some_and(|dt| dt.types().includes(&"Files".into(), 0))
    }

    pub fn listen(dragging: Callback<bool>, dropped: Callback<(String, String, Vec<u8>)>) -> Stop {
        let Some(window) = web_sys::window() else {
            return Stop::new(|| {});
        };
        let (tx, mut rx) = mpsc::unbounded::<Seen>();
        // Enter and leave come for every element the drag crosses.
        let depth = Rc::new(Cell::new(0));
        let listeners: Vec<(&str, Listener)> = vec![
            ("dragenter", {
                let (tx, depth) = (tx.clone(), depth.clone());
                Closure::new(move |e: DragEvent| {
                    if has_files(&e) {
                        depth.set(depth.get() + 1);
                        let _ = tx.unbounded_send(Seen::Over(true));
                    }
                })
            }),
            ("dragover", {
                Closure::new(move |e: DragEvent| {
                    if has_files(&e) {
                        e.prevent_default();
                        if let Some(dt) = e.data_transfer() {
                            dt.set_drop_effect("copy");
                        }
                    }
                })
            }),
            ("dragleave", {
                let (tx, depth) = (tx.clone(), depth.clone());
                Closure::new(move |e: DragEvent| {
                    if has_files(&e) {
                        depth.set((depth.get() - 1).max(0));
                        if depth.get() == 0 {
                            let _ = tx.unbounded_send(Seen::Over(false));
                        }
                    }
                })
            }),
            ("drop", {
                let (tx, depth) = (tx.clone(), depth.clone());
                Closure::new(move |e: DragEvent| {
                    if !has_files(&e) {
                        return;
                    }
                    e.prevent_default();
                    depth.set(0);
                    let _ = tx.unbounded_send(Seen::Over(false));
                    let Some(files) = e.data_transfer().and_then(|dt| dt.files()) else {
                        return;
                    };
                    for i in 0..files.length() {
                        if let Some(file) = files.get(i) {
                            let _ = tx.unbounded_send(Seen::Dropped(file));
                        }
                    }
                })
            }),
        ];
        for (kind, listener) in &listeners {
            let _ =
                window.add_event_listener_with_callback(kind, listener.as_ref().unchecked_ref());
        }
        // The browser's callbacks only queue; the app hears them from a task.
        let task = Owner::detached().run(|| {
            spawn(async move {
                while let Some(seen) = poll_fn(|cx| Pin::new(&mut rx).poll_next(cx)).await {
                    match seen {
                        Seen::Over(over) => dragging.post(over),
                        Seen::Dropped(file) => match JsFuture::from(file.array_buffer()).await {
                            Ok(buffer) => {
                                let data = Uint8Array::new(&buffer).to_vec();
                                dropped.post((file.name(), file.type_(), data));
                            }
                            Err(e) => log::warn!("cannot read the dropped {}: {e:?}", file.name()),
                        },
                    }
                }
            })
        });
        Stop::new(move || {
            for (kind, listener) in &listeners {
                let _ = window
                    .remove_event_listener_with_callback(kind, listener.as_ref().unchecked_ref());
            }
            task.cancel();
        })
    }
}
