#include <QApplication>
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

// Turn a top-level widget into a focus-less, input-transparent tooltip window
// anchored to `parent_ptr`, so the hover preview can extend beyond the main
// window without stealing OS focus (which would close the main window to the
// tray) or the pointer hover that opened it. Must run before the first show().
extern "C" void cliptoo_make_tooltip_window(void* child_ptr, void* parent_ptr) {
    auto* child = static_cast<QWidget*>(child_ptr);
    auto* parent = static_cast<QWidget*>(parent_ptr);
    child->setParent(parent, Qt::ToolTip | Qt::FramelessWindowHint
                                 | Qt::WindowStaysOnTopHint
                                 | Qt::WindowDoesNotAcceptFocus);
    child->setAttribute(Qt::WA_TransparentForMouseEvents, true);
    child->setAttribute(Qt::WA_ShowWithoutActivating, true);
}

// True while any of this application's top-level windows still holds OS
// activation. Qt tracks active-window state per process, so this is a single
// static query: it covers the main window, its PopupWindow menus, and the
// Settings/About/Edit child windows alike. The blur-detection poll uses it
// because an open popup steals the main window's keyboard focus, making item
// focus useless as a "still focused" signal while a menu is up.
extern "C" bool cliptoo_app_has_focus() {
    QWidget* active = QApplication::activeWindow();
    // isVisible() filters a stale pointer left pointing at a just-hidden
    // window during activation changes.
    return active != nullptr && active->isVisible();
}
