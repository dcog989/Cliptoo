#include <QWidget>
#include <QWindow>

extern "C" void cliptoo_start_window_move(void* widget_ptr) {
    auto* widget = static_cast<QWidget*>(widget_ptr);
    if (auto* win = widget->windowHandle()) {
        win->startSystemMove();
    }
}

// Raise and activate the window so the first click isn't consumed just
// focusing it (e.g. the hamburger menu needing two clicks). On Wayland,
// activation is subject to the compositor's focus-stealing policy.
extern "C" void cliptoo_activate_window(void* widget_ptr) {
    auto* widget = static_cast<QWidget*>(widget_ptr);
    widget->raise();
    widget->activateWindow();
}
