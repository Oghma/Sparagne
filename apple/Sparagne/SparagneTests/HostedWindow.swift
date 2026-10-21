import AppKit
import SwiftUI
import Testing

/// A SwiftUI view in a window of its own, driven with the keys and clicks a
/// person would give it: a view's key handlers and the mouse monitor it
/// installs can only be reached through events.
///
/// The events are posted to the app's queue and dispatched by its run loop
/// while the test waits, so they reach `NSApp.sendEvent`, the local
/// monitors, the field editor and `onKeyPress` exactly as typed ones do. Two
/// events posted back to back are dispatched before SwiftUI redraws, as a
/// fast typist's keys arrive. The window works without being the key window,
/// so the tests do not need the app in front.
@MainActor
final class HostedWindow {
    let window: NSWindow

    init<Content: View>(_ content: Content, size: CGSize = CGSize(width: 900, height: 420)) {
        window = KeyedWindow(
            contentRect: NSRect(origin: CGPoint(x: 120, y: 120), size: size),
            styleMask: [.titled, .resizable],
            backing: .buffered,
            defer: false
        )
        window.isReleasedWhenClosed = false
        window.contentView = FirstClickHostingView(rootView: content)
        window.makeKeyAndOrderFront(nil)
    }

    /// Takes the view out of the window as well: a view left alive in a
    /// closed window still hears app-wide notifications (⌘D's), and its
    /// fields would take the caret from the next test's.
    func close() {
        window.orderOut(nil)
        window.contentView = nil
        window.close()
    }

    /// The text view typing into a field goes to, while one has the caret.
    var fieldEditor: NSTextView? { window.firstResponder as? NSTextView }

    /// The caret or the selection in the field being typed into, in UTF-16
    /// units as AppKit counts them.
    var fieldSelection: NSRange? { fieldEditor?.selectedRange() }

    // MARK: - Keys

    enum Key {
        case tab, backTab, up, down, left, right, enter, escape, delete, f2
        /// A printable character, as a layout without dead keys sends it.
        case character(Character)

        fileprivate var event: (characters: String, code: UInt16, modifiers: NSEvent.ModifierFlags) {
            switch self {
            case .tab: ("\t", 48, [])
            case .backTab: ("\u{19}", 48, .shift)
            case .up: ("\u{F700}", 126, [.numericPad, .function])
            case .down: ("\u{F701}", 125, [.numericPad, .function])
            case .left: ("\u{F702}", 123, [.numericPad, .function])
            case .right: ("\u{F703}", 124, [.numericPad, .function])
            case .enter: ("\r", 36, [])
            case .escape: ("\u{1B}", 53, [])
            case .delete: ("\u{7F}", 51, [])
            case .f2: ("\u{F705}", 120, .function)
            case .character(let character): (String(character), Self.code(for: character), [])
            }
        }

        /// The US layout's key code, which an input method may look at.
        private static func code(for character: Character) -> UInt16 {
            let codes: [Character: UInt16] = [
                "a": 0, "s": 1, "d": 2, "f": 3, "h": 4, "g": 5, "z": 6, "x": 7, "c": 8, "v": 9,
                "b": 11, "q": 12, "w": 13, "e": 14, "r": 15, "y": 16, "t": 17, "1": 18, "2": 19,
                "3": 20, "4": 21, "6": 22, "5": 23, "9": 25, "7": 26, "8": 28, "0": 29, "o": 31,
                "u": 32, "i": 34, "p": 35, "l": 37, "j": 38, "k": 40, "n": 45, "m": 46, " ": 49,
                ",": 43, ".": 47, "/": 44, "-": 27,
            ]
            return codes[Character(character.lowercased())] ?? 0
        }
    }

    /// Posts `key`, with `modifiers` on top of what the key itself carries.
    func post(_ key: Key, modifiers: NSEvent.ModifierFlags = []) {
        let (characters, code, own) = key.event
        let event = NSEvent.keyEvent(
            with: .keyDown,
            location: .zero,
            modifierFlags: own.union(modifiers),
            timestamp: ProcessInfo.processInfo.systemUptime,
            windowNumber: window.windowNumber,
            context: nil,
            characters: characters,
            charactersIgnoringModifiers: characters,
            isARepeat: false,
            keyCode: code
        )
        NSApp.postEvent(event!, atStart: false)
    }

    /// Each character of `text`, posted back to back.
    func post(typing text: String) {
        for character in text { post(.character(character)) }
    }

    // MARK: - The pointer

    /// A click at `point`, measured from the top left of the window's
    /// content, as SwiftUI measures. `count` 2 is the second click of a
    /// double click, which the first one has to precede.
    func postClick(at point: CGPoint, count: Int = 1, modifiers: NSEvent.ModifierFlags = []) {
        guard let content = window.contentView else { return }
        let location = content.convert(point, to: nil)
        if let hit = content.hitTest(content.superview?.convert(location, from: nil) ?? location) {
            FirstClick.accept(on: type(of: hit))
        }
        for type in [NSEvent.EventType.leftMouseDown, .leftMouseUp] {
            let event = NSEvent.mouseEvent(
                with: type,
                location: location,
                modifierFlags: modifiers,
                timestamp: ProcessInfo.processInfo.systemUptime,
                windowNumber: window.windowNumber,
                context: nil,
                eventNumber: 0,
                clickCount: count,
                pressure: type == .leftMouseDown ? 1 : 0
            )
            NSApp.postEvent(event!, atStart: false)
        }
    }

    func postDoubleClick(at point: CGPoint) {
        postClick(at: point, count: 1)
        postClick(at: point, count: 2)
    }

    /// The pointer over `point`, as the tracking area of the hosting view
    /// hears it: a posted mouse-moved event never reaches it.
    func hover(at point: CGPoint) {
        guard let content = window.contentView else { return }
        let location = content.convert(point, to: nil)
        // Not a tooltip's (`.help`): the one that follows the pointer.
        let area = content.trackingAreas.first { $0.owner === content && $0.options.contains(.mouseMoved) }
        let enter = NSEvent.enterExitEvent(
            with: .mouseEntered,
            location: location,
            modifierFlags: [],
            timestamp: ProcessInfo.processInfo.systemUptime,
            windowNumber: window.windowNumber,
            context: nil,
            eventNumber: 0,
            // What `NSEvent.trackingArea` reads back: the hosting view
            // takes only the events of its own area.
            trackingNumber: area.map { Int(bitPattern: Unmanaged.passUnretained($0).toOpaque()) } ?? 0,
            userData: nil
        )
        if let enter { content.mouseEntered(with: enter) }
        let move = NSEvent.mouseEvent(
            with: .mouseMoved,
            location: location,
            modifierFlags: [],
            timestamp: ProcessInfo.processInfo.systemUptime,
            windowNumber: window.windowNumber,
            context: nil,
            eventNumber: 0,
            clickCount: 0,
            pressure: 0
        )
        if let move { content.mouseMoved(with: move) }
    }

    // MARK: - Waiting

    /// Lets the run loop dispatch what was posted and SwiftUI redraw.
    func settle(for pause: Duration = .milliseconds(150)) async {
        try? await Task.sleep(for: pause)
    }

    /// Lets the run loop go on until `condition` holds or `timeout` runs
    /// out, then a moment more, so what an update started has landed.
    func settle(timeout: Duration = .seconds(2), until condition: () -> Bool) async {
        let clock = ContinuousClock()
        let deadline = clock.now + timeout
        while !condition(), clock.now < deadline {
            try? await Task.sleep(for: .milliseconds(10))
        }
        try? await Task.sleep(for: .milliseconds(30))
    }
}

/// A window that takes itself for the key one: the tests run while another
/// app is in front, where no window of this one is key.
private final class KeyedWindow: NSWindow {
    override var canBecomeKey: Bool { true }
    override var isKeyWindow: Bool { true }
}

/// Takes the first click as a click. In an app that is not in front, AppKit
/// otherwise spends it on bringing the app forward, which the tests' app
/// never comes, and SwiftUI's gestures never see it.
private final class FirstClickHostingView<Content: View>: NSHostingView<Content> {
    override func acceptsFirstMouse(for event: NSEvent?) -> Bool { true }
}

/// The same for the views SwiftUI puts inside the hosting view, a scroll
/// view's content among them: their class is SwiftUI's own, so its
/// `acceptsFirstMouse(for:)` is replaced, for the test process only, by one
/// that says yes.
private enum FirstClick {
    private static var accepted: Set<ObjectIdentifier> = []

    static func accept(on viewClass: AnyClass) {
        guard accepted.insert(ObjectIdentifier(viewClass)).inserted else { return }
        let selector = #selector(NSView.acceptsFirstMouse(for:))
        guard let method = class_getInstanceMethod(viewClass, selector) else { return }
        let yes: @convention(block) (AnyObject, NSEvent?) -> Bool = { _, _ in true }
        class_replaceMethod(viewClass, selector, imp_implementationWithBlock(yes), method_getTypeEncoding(method))
    }
}
