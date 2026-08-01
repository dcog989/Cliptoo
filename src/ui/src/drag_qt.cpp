#include <QWidget>
#include <QWindow>

extern "C" void cliptoo_start_window_move(void* widget_ptr) {
    auto* widget = static_cast<QWidget*>(widget_ptr);
    if (auto* win = widget->windowHandle()) {
        win->startSystemMove();
    }
}
