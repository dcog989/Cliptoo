#include <QApplication>
#include <QCoreApplication>
#include <QEvent>
#include <QMouseEvent>
#include <QObject>
#include <QPointingDevice>
#include <QSet>
#include <QWheelEvent>
#include <QWidget>

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

namespace {

// Rewrites discrete mouse-wheel events so Slint's Qt backend uses the reliable
// angleDelta (120 units per 15-degree notch) instead of Wayland's pixelDelta,
// which is only a few pixels per notch and makes wheel scrolling crawl
// (i-slint-backend-qt prefers pixelDelta and only falls back to angleDelta when
// it's null). Touchpad gestures leave angleDelta null, so they keep their
// pixelDelta and stay 1:1.
class WheelDeltaFilter : public QObject {
public:
    explicit WheelDeltaFilter(QObject* parent) : QObject(parent) {}

protected:
    bool eventFilter(QObject* receiver, QEvent* event) override {
        if (event->type() != QEvent::Wheel) {
            return false;
        }
        auto* wheel = static_cast<QWheelEvent*>(event);
        if (wheel->pixelDelta().isNull() || wheel->angleDelta().isNull()) {
            return false;
        }
        // Re-dispatch a copy with pixelDelta cleared; the re-sent event passes
        // this filter again (pixelDelta now null) and reaches the widget.
        QWheelEvent normalized(wheel->position(), wheel->globalPosition(),
                               QPoint(), wheel->angleDelta(),
                               wheel->buttons(), wheel->modifiers(),
                               wheel->phase(), wheel->inverted(),
                               wheel->source(), wheel->pointingDevice());
        QCoreApplication::sendEvent(receiver, &normalized);
        return true;
    }
};

// A click that activates an inactive window can reach the app as a release
// with no press: the compositor consumes the press for its activation action,
// and Slint's Qt backend then drops the orphan release (its `isMouseButtonDown`
// guard). Replaying the missing press completes the pair, so the widget under
// the cursor handles the click instead of that click only focusing the window.
// Inert for every click that arrives whole.
class ActivationClickFilter : public QObject {
public:
    explicit ActivationClickFilter(QObject* parent) : QObject(parent) {}

protected:
    bool eventFilter(QObject* receiver, QEvent* event) override {
        switch (event->type()) {
        case QEvent::MouseButtonPress:
        case QEvent::MouseButtonDblClick:
            if (!replaying) {
                pressed.insert(receiver);
            }
            break;
        case QEvent::MouseButtonRelease: {
            auto* release = static_cast<QMouseEvent*>(event);
            // Only a real left-button release whose press never arrived.
            if (release->button() != Qt::LeftButton
                || release->source() != Qt::MouseEventNotSynthesized
                || pressed.remove(receiver)) {
                break;
            }
            replaying = true;
            QMouseEvent press(QEvent::MouseButtonPress, release->position(),
                              release->globalPosition(), Qt::LeftButton,
                              Qt::LeftButton, release->modifiers(),
                              release->pointingDevice());
            QCoreApplication::sendEvent(receiver, &press);
            replaying = false;
            break;
        }
        case QEvent::WindowDeactivate:
        case QEvent::UngrabMouse:
            // A press with no release leaves a stale entry that would make the
            // next orphan release look paired, so drop all of them here.
            pressed.clear();
            break;
        default:
            break;
        }
        return false;
    }

private:
    QSet<QObject*> pressed;
    bool replaying = false;
};

} // namespace

// Install the wheel-delta filter for the whole application. Called once from
// Rust after the first window exists (i.e. after the Qt backend has created
// qApp). See drag.rs normalize_wheel_delta.
extern "C" void cliptoo_normalize_wheel_delta() {
    if (qApp != nullptr) {
        qApp->installEventFilter(new WheelDeltaFilter(qApp));
    }
}

// Install the activation-click repair for the whole application. See
// drag.rs repair_activation_clicks and the filter's comment above.
extern "C" void cliptoo_repair_activation_clicks() {
    if (qApp != nullptr) {
        qApp->installEventFilter(new ActivationClickFilter(qApp));
    }
}
