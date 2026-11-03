//! Child windows built on first use.
//!
//! Settings, About, Edit and QR are created by their open handler instead of at
//! startup, so a session that never opens one never pays for its scene graph.

use std::cell::RefCell;
use std::rc::Rc;

use slint::ComponentHandle;

/// UI-thread only holder for a lazily created window.
pub struct LazyWindow<W: ComponentHandle> {
    slot: Rc<RefCell<Option<Rc<W>>>>,
}

impl<W: ComponentHandle> Clone for LazyWindow<W> {
    fn clone(&self) -> Self {
        Self {
            slot: self.slot.clone(),
        }
    }
}

impl<W: ComponentHandle> Default for LazyWindow<W> {
    fn default() -> Self {
        Self::new()
    }
}

impl<W: ComponentHandle> LazyWindow<W> {
    pub fn new() -> Self {
        Self {
            slot: Rc::new(RefCell::new(None)),
        }
    }

    /// The window when it has already been created.
    pub fn get(&self) -> Option<Rc<W>> {
        self.slot.borrow().clone()
    }

    /// Create the window on first use; later calls return the existing one.
    pub fn get_or_create(&self, make: impl FnOnce() -> W) -> Rc<W> {
        if let Some(window) = self.get() {
            return window;
        }
        let window = Rc::new(make());
        *self.slot.borrow_mut() = Some(window.clone());
        window
    }
}
