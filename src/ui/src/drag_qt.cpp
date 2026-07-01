#include <QWidget>
#include <QWindow>
#include <QScreen>
#include <QCursor>

extern "C" void cliptoo_start_window_move(void* widget_ptr) {
    auto* widget = static_cast<QWidget*>(widget_ptr);
    if (auto* win = widget->windowHandle()) {
        win->startSystemMove();
    }
}

extern "C" void cliptoo_cursor_pos(int* x, int* y) {
    QPoint pos = QCursor::pos();
    *x = pos.x();
    *y = pos.y();
}

extern "C" void cliptoo_screen_size(void* widget_ptr, int* w, int* h) {
    auto* widget = static_cast<QWidget*>(widget_ptr);
    if (auto* screen = widget->screen()) {
        QRect geo = screen->availableGeometry();
        *w = geo.width();
        *h = geo.height();
    } else {
        *w = 1920;
        *h = 1080;
    }
}
