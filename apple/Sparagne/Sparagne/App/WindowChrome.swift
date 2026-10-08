import AppKit
import SwiftUI

/// The AppKit side of the window's own top bar, for a
/// window whose scene has `.windowStyle(.hiddenTitleBar)`: it finds the
/// hosting `NSWindow`, keeps the traffic lights vertically centred in the
/// bar, and makes the bar's empty areas behave like a title bar (drag,
/// double-click).
///
/// Put it behind the bar, as its background: the bar's empty areas then hit
/// this view, while the bar's own controls keep their clicks. A label that
/// should drag too needs `.allowsHitTesting(false)`.
struct WindowChrome: NSViewRepresentable {
    /// The bar's height, from the top edge of the window.
    var barHeight: CGFloat = Metrics.topBar
    /// From the window's leading edge to the close button.
    var buttonsLeading: CGFloat = 14
    /// Called when the window goes in or out of full screen. The traffic
    /// lights are hidden there, so the bar needs no room for them.
    var onFullScreenChange: (Bool) -> Void = { _ in }

    func makeNSView(context: Context) -> WindowChromeView {
        let view = WindowChromeView()
        update(view)
        return view
    }

    func updateNSView(_ view: WindowChromeView, context: Context) {
        update(view)
        view.placeTrafficLights()
    }

    private func update(_ view: WindowChromeView) {
        view.barHeight = barHeight
        view.buttonsLeading = buttonsLeading
        view.onFullScreenChange = onFullScreenChange
    }
}

/// See `WindowChrome`.
final class WindowChromeView: NSView {
    var barHeight: CGFloat = Metrics.topBar
    var buttonsLeading: CGFloat = 14
    var onFullScreenChange: (Bool) -> Void = { _ in }

    private weak var observedWindow: NSWindow?
    /// Main-actor state like the rest, but `deinit` is not isolated and has
    /// to reach the tokens: a view released together with its window never
    /// hears `viewDidMoveToWindow` with no window, the call that detaches.
    /// Written only from the main actor; `deinit` reads them last, once no
    /// other reference to the view is left.
    nonisolated(unsafe) private var observers: [NSObjectProtocol] = []
    /// Between `willEnterFullScreen` and `willExitFullScreen`: AppKit owns
    /// the buttons then, and shows them in the bar that slides down with the
    /// menu bar.
    private var inFullScreen = false
    /// Setting the frames posts the frame notifications this view listens to.
    private var placing = false
    /// AppKit's own distance between two buttons, read before the first move.
    private var buttonSpacing: CGFloat?
    /// The views whose frame changes are observed; AppKit may swap a button.
    private var observedViews: [ObjectIdentifier] = []
    /// See `observers`.
    nonisolated(unsafe) private var frameObservers: [NSObjectProtocol] = []
    private var checkScheduled = false

    /// The notification center keeps a block observer until it is removed;
    /// the blocks hold the view weakly, so a stale one would do nothing, but
    /// it would stay registered for the life of the app.
    deinit {
        for token in observers + frameObservers {
            NotificationCenter.default.removeObserver(token)
        }
    }

    // MARK: Events on the bar's empty areas

    /// A title bar drags its window even while the window is inactive.
    override func acceptsFirstMouse(for event: NSEvent?) -> Bool { true }

    /// The whole bar, not only AppKit's own 32-point title bar inside it: a
    /// drag moves the window, a double-click does what the user chose for
    /// title bars. While the window is inactive, AppKit handles a click in
    /// its own 32 points natively and this is not called; the outcome is the
    /// same (`mouseDownCanMoveWindow` set to false does not change that).
    ///
    /// Leaving the click to AppKit (`mouseDownCanMoveWindow` alone) handles
    /// it natively, but only in the top 32 points; handing it to AppKit's
    /// title bar view recurses, since a transparent title bar gives the
    /// click back to the content view under it, this one.
    override func mouseDown(with event: NSEvent) {
        guard let window else { return super.mouseDown(with: event) }
        if event.clickCount == 2 {
            WindowChromeView.performTitleBarDoubleClick(in: window)
        } else {
            window.performDrag(with: event)
        }
    }

    /// What a double-click on a title bar does, after the user's choice in
    /// System Settings ▸ Desktop & Dock.
    static func performTitleBarDoubleClick(in window: NSWindow) {
        switch UserDefaults.standard.string(forKey: "AppleActionOnDoubleClick") {
        case "Minimize":
            window.performMiniaturize(nil)
        case "None":
            return
        default:
            // "Maximize", which AppKit registers as the default when the
            // user never chose. "Fill" has no public API, so it zooms as
            // well.
            window.performZoom(nil)
        }
    }

    // MARK: Finding the window

    override func viewDidMoveToWindow() {
        super.viewDidMoveToWindow()
        attach(to: window)
    }

    private func attach(to window: NSWindow?) {
        guard window !== observedWindow else { return }
        detach()
        guard let window else { return }
        observedWindow = window
        window.titlebarSeparatorStyle = .none

        let center = NotificationCenter.default
        let names: [Notification.Name] = [
            NSWindow.didResizeNotification,
            NSWindow.didEndLiveResizeNotification,
            NSWindow.didBecomeKeyNotification,
            NSWindow.didResignKeyNotification,
            NSWindow.didBecomeMainNotification,
            NSWindow.didResignMainNotification,
            NSWindow.didChangeScreenNotification,
            NSWindow.willEnterFullScreenNotification,
            NSWindow.didEnterFullScreenNotification,
            NSWindow.willExitFullScreenNotification,
            NSWindow.didExitFullScreenNotification,
        ]
        for name in names {
            observers.append(center.addObserver(forName: name, object: window, queue: .main) { [weak self] _ in
                MainActor.assumeIsolated { self?.windowChanged(name) }
            })
        }
        placeTrafficLights()
    }

    private func detach() {
        observers.forEach(NotificationCenter.default.removeObserver)
        observers.removeAll()
        frameObservers.forEach(NotificationCenter.default.removeObserver)
        frameObservers.removeAll()
        observedViews.removeAll()
        observedWindow = nil
    }

    /// AppKit lays the title bar out again on its own (a resize, a new title,
    /// the key state): the buttons' and the container's frame changes are
    /// where it shows. Observed again whenever AppKit swaps one of them.
    private func observeTitleBarViews(_ views: [NSView]) {
        let identities = views.map(ObjectIdentifier.init)
        guard identities != observedViews else { return }
        frameObservers.forEach(NotificationCenter.default.removeObserver)
        frameObservers.removeAll()
        observedViews = identities
        for view in views {
            view.postsFrameChangedNotifications = true
            frameObservers.append(NotificationCenter.default.addObserver(
                forName: NSView.frameDidChangeNotification,
                object: view,
                queue: .main
            ) { [weak self] _ in
                MainActor.assumeIsolated { self?.placeTrafficLights() }
            })
        }
    }

    private func windowChanged(_ name: Notification.Name) {
        switch name {
        case NSWindow.willEnterFullScreenNotification, NSWindow.didEnterFullScreenNotification:
            inFullScreen = true
            onFullScreenChange(true)
        case NSWindow.willExitFullScreenNotification, NSWindow.didExitFullScreenNotification:
            // From here the buttons come back to the window during the
            // animation; they are placed as soon as they are in it.
            inFullScreen = false
            onFullScreenChange(false)
        default:
            break
        }
        placeTrafficLights()
    }

    // MARK: The traffic lights

    private func buttons(of window: NSWindow) -> [NSButton] {
        [.closeButton, .miniaturizeButton, .zoomButton].compactMap { window.standardWindowButton($0) }
    }

    /// Puts the traffic lights back where the bar wants them. AppKit has its
    /// own layout for the title bar and applies it again on a resize, a new
    /// title or a key change, so this runs after each of those (through the
    /// frame notifications, before anything is drawn), and once more on the
    /// next turn of the run loop: on a new title AppKit puts the zoom button
    /// back without posting one.
    func placeTrafficLights(deferredCheck: Bool = true) {
        guard !placing, !inFullScreen, let window = observedWindow else { return }
        let buttons = buttons(of: window)
        guard buttons.count == 3,
              let titlebar = buttons[0].superview,
              let container = titlebar.superview,
              let frameView = container.superview,
              // On the way in and out of full screen the buttons live for a
              // while in the bar that slides down with the menu bar.
              buttons.allSatisfy({ $0.window === window })
        else { return }
        observeTitleBarViews(buttons + [titlebar, container])
        placing = true
        moveButtons(buttons, frameView: frameView)
        placing = false
        if deferredCheck, !checkScheduled {
            checkScheduled = true
            DispatchQueue.main.async { [weak self] in
                self?.checkScheduled = false
                self?.placeTrafficLights(deferredCheck: false)
            }
        }
    }

    /// Moves each button so its centre is `barHeight / 2` below the window's
    /// top edge, the close button `buttonsLeading` from its leading edge, and
    /// the other two at AppKit's own spacing. Positions are worked out in
    /// window coordinates, whatever frames AppKit gave the container.
    ///
    /// Moving the title bar container instead of the buttons does not work:
    /// on macOS 27 AppKit keeps the buttons at their window position when the
    /// container moves. Nor does making the container and its title bar view
    /// as tall as the bar, after Electron's `trafficLightPosition`: AppKit
    /// sets the title bar view back to its own height on every layout.
    private func moveButtons(_ buttons: [NSButton], frameView: NSView) {
        if buttonSpacing == nil {
            buttonSpacing = buttons[1].frame.minX - buttons[0].frame.minX
        }
        // The title bar is 32 points tall on macOS 27, the app's minimum:
        // room enough for a 14-point button centred 22 points down, so the
        // container keeps the size AppKit gives it.
        let windowHeight = frameView.bounds.height
        let spacing = buttonSpacing ?? 20
        for (index, button) in buttons.enumerated() {
            guard let superview = button.superview else { continue }
            // The wanted frame in window coordinates (origin at the bottom
            // left), then in the coordinates of the button's superview.
            let wanted = NSRect(
                x: buttonsLeading + CGFloat(index) * spacing,
                y: windowHeight - barHeight / 2 - button.frame.height / 2,
                width: button.frame.width,
                height: button.frame.height
            )
            let origin = superview.convert(wanted, from: nil).origin
            let rounded = NSPoint(x: origin.x.rounded(), y: origin.y.rounded())
            if button.frame.origin != rounded {
                button.setFrameOrigin(rounded)
            }
        }
    }
}
