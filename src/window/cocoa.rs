use cocoa::base::id as CocoaId;
use objc::runtime::{Object, Sel};
use objc::{class, msg_send, sel, sel_impl};
use std::ffi::c_void;

pub struct Window {
    raw: CocoaId,
    view: CocoaId,
}

pub struct EventLoop {
    app: CocoaId,
}

#[derive(Clone)]
pub struct EventLoopWaker;

type EventCallback = Box<dyn FnMut(super::Event)>;

fn default_event_callback(_event: super::Event) {
    unreachable!("uninitialized event handler");
}

thread_local! {
    static HANDLER: EventHandler = EventHandler::new(Box::new(default_event_callback));
}

struct EventHandler {
    callback: std::cell::RefCell<EventCallback>,
}

impl EventHandler {
    pub fn new(callback: EventCallback) -> Self {
        EventHandler {
            callback: std::cell::RefCell::new(callback),
        }
    }
}

trait EventDispatch<'a> {
    fn send(&'a self, event: super::Event);

    fn set_callback(&'a self, callback: EventCallback);
}

impl EventDispatch<'static> for std::thread::LocalKey<EventHandler> {
    fn send(&'static self, event: super::Event) {
        self.with(move |handler| handler.callback.borrow_mut()(event));
    }

    fn set_callback(&'static self, callback: EventCallback) {
        let _ = self.with(move |handler| handler.callback.replace(callback));
    }
}

impl Window {
    pub fn new(event_loop: &EventLoop, config: super::WindowConfig) -> Window {
        use cocoa::appkit::{
            NSBackingStoreType, NSRunningApplication, NSView, NSWindow, NSWindowStyleMask,
        };
        use cocoa::base::{id, nil};
        use cocoa::foundation::{NSAutoreleasePool, NSPoint, NSRect, NSString};

        unsafe {
            let window_class = Self::create_class();

            let window: id = msg_send![window_class, alloc];
            window
                .initWithContentRect_styleMask_backing_defer_(
                    NSRect::new(NSPoint::new(0.0, 0.0), config.size.into()),
                    NSWindowStyleMask::NSClosableWindowMask
                        | NSWindowStyleMask::NSTitledWindowMask
                        | NSWindowStyleMask::NSResizableWindowMask,
                    NSBackingStoreType::NSBackingStoreRetained,
                    i8::from(false),
                )
                .autorelease();

            window.setDelegate_(Self::create_delegate());

            let title = NSString::alloc(nil).init_str("spotty");
            window.setTitle_(title);

            let view = NSView::alloc(nil).init();
            window.setContentView_(view);
            window.setInitialFirstResponder_(view);

            window.cascadeTopLeftFromPoint_(NSPoint::new(20.0, 20.0));
            window.makeKeyAndOrderFront_(event_loop.app);

            let current_app = cocoa::appkit::NSRunningApplication::currentApplication(nil);
            current_app.activateWithOptions_(cocoa::appkit::NSApplicationActivateIgnoringOtherApps);

            Window { raw: window, view }
        }
    }

    unsafe fn create_class() -> &'static objc::runtime::Class {
        let mut window = objc::declare::ClassDecl::new("SpottyWindow", class!(NSWindow)).unwrap();

        window.add_method(
            sel!(keyDown:),
            key_down as extern "C" fn(&Object, Sel, CocoaId),
        );

        window.register()
    }

    unsafe fn create_delegate() -> CocoaId {
        use cocoa::base::id;
        use cocoa::delegate;

        delegate!("SpottyWindowDelegate", {
            (windowDidResize:) => window_did_resize as extern "C" fn(&Object, Sel, CocoaId),
            (windowDidChangeBackingProperties:) => backing_properties_changed as extern "C" fn(&Object, Sel, CocoaId)
        })
    }

    pub fn close(&self) {
        unsafe { cocoa::appkit::NSWindow::close(self.raw) };
    }

    pub fn set_title(&self, title: &str) {
        use cocoa::appkit::NSWindow;
        use cocoa::base::nil;
        use cocoa::foundation::NSString;

        unsafe {
            let new_title = NSString::alloc(nil).init_str(title);
            NSWindow::setTitle_(self.raw, new_title);
        }
    }

    pub fn content_view(&self) -> CocoaId {
        self.view
    }

    pub fn inner_size(&self) -> super::PhysicalSize {
        use cocoa::appkit::NSView;
        unsafe {
            let size = NSView::frame(self.view).size;
            let scale = self.scale_factor();
            super::PhysicalSize {
                width: (scale * size.width) as u32,
                height: (scale * size.height) as u32,
            }
        }
    }

    pub fn scale_factor(&self) -> f64 {
        use cocoa::appkit::NSWindow;
        unsafe { NSWindow::backingScaleFactor(self.raw) }
    }

    pub fn get_clipboard(&self) -> Option<String> {
        use cocoa::appkit::NSPasteboard;
        use cocoa::base::nil;
        use cocoa::foundation::NSString;

        unsafe {
            let pasteboard = NSPasteboard::generalPasteboard(nil);
            let string = pasteboard.stringForType(cocoa::appkit::NSPasteboardTypeString);

            if string == nil {
                None
            } else {
                let text = string.UTF8String();
                let bytes = std::slice::from_raw_parts(text as *const u8, string.len());
                Some(std::str::from_utf8_unchecked(bytes).to_owned())
            }
        }
    }
}

impl EventLoop {
    pub fn new() -> EventLoop {
        use cocoa::appkit::{NSApplication, NSMenu, NSMenuItem};
        use cocoa::base::nil;
        use cocoa::foundation::{NSAutoreleasePool, NSProcessInfo, NSString};

        unsafe {
            let _pool = NSAutoreleasePool::new(nil);

            let app = cocoa::appkit::NSApp();
            app.setActivationPolicy_(
                cocoa::appkit::NSApplicationActivationPolicy::NSApplicationActivationPolicyRegular,
            );

            // create Menu Bar
            let menubar = NSMenu::new(nil).autorelease();
            let app_menu_item = NSMenuItem::new(nil).autorelease();
            menubar.addItem_(app_menu_item);
            app.setMainMenu_(menubar);

            // create Application menu
            let app_menu = NSMenu::new(nil).autorelease();
            let quit_prefix = NSString::alloc(nil).init_str("Quit ");
            let quit_title =
                quit_prefix.stringByAppendingString_(NSProcessInfo::processInfo(nil).processName());
            let quit_action = sel!(terminate:);
            let quit_key = NSString::alloc(nil).init_str("q");
            let quit_item = NSMenuItem::alloc(nil)
                .initWithTitle_action_keyEquivalent_(quit_title, quit_action, quit_key)
                .autorelease();
            app_menu.addItem_(quit_item);
            app_menu_item.setSubmenu_(app_menu);

            EventLoop { app }
        }
    }

    pub fn run(self, event_callback: impl FnMut(super::Event) + 'static) -> ! {
        use cocoa::appkit::{NSApplication, NSWindow};
        use cocoa::base::id;
        use cocoa::delegate;

        unsafe {
            HANDLER.set_callback(Box::new(event_callback));

            let app_delegate = delegate!("SpottyApplicationDelegate", {
                (applicationShouldTerminateAfterLastWindowClosed:) => application_should_terminate_after_last_window_closed as extern fn(&Object, Sel, id) -> bool,
                (applicationWillTerminate:) => application_will_terminate as extern fn(&mut Object, Sel, CocoaId),

                (applicationDidBecomeActive:) => application_did_become_active as extern fn(this: &Object, _cmd: Sel, _notification: id),
                (applicationDidResignActive:) => application_did_resign_active as extern fn(this: &Object, _cmd: Sel, _notification: id)
            });
            self.app.setDelegate_(app_delegate);

            let run_loop = core_foundation::runloop::CFRunLoop::get_main();
            Self::add_observers(&run_loop);

            self.app.run();
        }

        std::process::exit(0);
    }

    fn add_observers(run_loop: &core_foundation::runloop::CFRunLoop) {
        use core_foundation::base::TCFType;

        unsafe {
            let mut context = core_foundation::runloop::CFRunLoopObserverContext {
                version: 0,
                info: std::ptr::null_mut(),
                retain: None,
                release: None,
                copyDescription: None,
            };

            let observer = core_foundation::runloop::CFRunLoopObserver::wrap_under_create_rule(
                core_foundation::runloop::CFRunLoopObserverCreate(
                    std::ptr::null(),
                    core_foundation::runloop::kCFRunLoopBeforeWaiting,
                    cocoa::base::YES as u8,
                    0,
                    events_cleared,
                    &mut context as *mut _,
                ),
            );

            run_loop.add_observer(&observer, core_foundation::runloop::kCFRunLoopCommonModes);
        }
    }
}

impl EventLoop {
    pub fn create_waker(&self) -> EventLoopWaker {
        EventLoopWaker
    }
}

impl EventLoopWaker {
    pub fn wake(&self) {
        unsafe {
            let rl = core_foundation::runloop::CFRunLoopGetMain();
            core_foundation::runloop::CFRunLoopWakeUp(rl);
        }
    }
}

extern "C" fn events_cleared(
    _observer: core_foundation::runloop::CFRunLoopObserverRef,
    _activity: core_foundation::runloop::CFRunLoopActivity,
    _info: *mut c_void,
) {
    HANDLER.send(super::Event::EventsCleared);
}

extern "C" fn application_should_terminate_after_last_window_closed(
    _this: &Object,
    _cmd: Sel,
    _app: CocoaId,
) -> bool {
    true
}

extern "C" fn application_will_terminate(_this: &mut Object, _cmd: Sel, _notification: CocoaId) {
    HANDLER.set_callback(Box::new(default_event_callback));
}

extern "C" fn application_did_become_active(_this: &Object, _cmd: Sel, _notification: CocoaId) {
    HANDLER.send(super::Event::Active);
}

extern "C" fn application_did_resign_active(_this: &Object, _cmd: Sel, _notification: CocoaId) {
    HANDLER.send(super::Event::Inactive);
}

impl From<super::PhysicalSize> for cocoa::foundation::NSSize {
    fn from(size: super::PhysicalSize) -> Self {
        cocoa::foundation::NSSize::new(size.width as f64, size.height as f64)
    }
}

extern "C" fn key_down(_this: &Object, _cmd: Sel, event: CocoaId) {
    use super::{Event::KeyPress, Key};
    use cocoa::appkit::NSEvent;
    use cocoa::foundation::NSString;

    unsafe {
        let modifiers = get_event_modifiers(event);

        match event.keyCode() {
            0x35 => HANDLER.send(KeyPress(Key::Escape, modifiers)),

            0x24 | 0x4c => HANDLER.send(KeyPress(Key::Enter, modifiers)),
            0x33 => HANDLER.send(KeyPress(Key::Backspace, modifiers)),
            0x30 => HANDLER.send(KeyPress(Key::Tab, modifiers)),

            0x7b => HANDLER.send(KeyPress(Key::ArrowLeft, modifiers)),
            0x7c => HANDLER.send(KeyPress(Key::ArrowRight, modifiers)),
            0x7d => HANDLER.send(KeyPress(Key::ArrowDown, modifiers)),
            0x7e => HANDLER.send(KeyPress(Key::ArrowUp, modifiers)),

            _ => {
                let chars = event.charactersIgnoringModifiers();

                let bytes = chars.UTF8String() as *const u8;
                let slice = std::slice::from_raw_parts(bytes, chars.len());

                if let Ok(text) = std::str::from_utf8(slice) {
                    for ch in text.chars() {
                        fn is_private_area(ch: char) -> bool {
                            matches!(u32::from(ch), 0xE000..=0xF8FF | 0xF0000..=0xFFFFD | 0x100000..=0x10FFFD)
                        }

                        if ch.is_control() || is_private_area(ch) {
                            continue;
                        }

                        HANDLER.send(KeyPress(Key::Char(ch), modifiers));
                    }
                }
            }
        }
    }
}

unsafe fn get_event_modifiers(event: CocoaId) -> super::Modifiers {
    use cocoa::appkit::{NSEvent, NSEventModifierFlags};

    let flags = NSEvent::modifierFlags(event);
    let mut modifiers = super::Modifiers::empty();

    if flags.contains(NSEventModifierFlags::NSControlKeyMask) {
        modifiers.insert(super::Modifiers::CONTROL);
    }
    if flags.contains(NSEventModifierFlags::NSShiftKeyMask) {
        modifiers.insert(super::Modifiers::SHIFT);
    }
    if flags.contains(NSEventModifierFlags::NSAlternateKeyMask) {
        modifiers.insert(super::Modifiers::ALT);
    }
    if flags.contains(NSEventModifierFlags::NSCommandKeyMask) {
        modifiers.insert(super::Modifiers::SUPER);
    }

    modifiers
}

extern "C" fn window_did_resize(_this: &Object, _cmd: Sel, notification: CocoaId) {
    use cocoa::appkit::{NSView, NSWindow};

    unsafe {
        let window: CocoaId = objc::msg_send![notification, object];

        let view = NSWindow::contentView(window);
        let size = NSView::frame(view).size;
        let scale_factor = NSWindow::backingScaleFactor(window);
        let width = scale_factor * size.width;
        let height = scale_factor * size.height;

        HANDLER.send(super::Event::Resize(super::PhysicalSize::new(
            width as u32,
            height as u32,
        )));
    }
}

extern "C" fn backing_properties_changed(_this: &Object, _cmd: Sel, _notification: CocoaId) {
    eprintln!("backing properties changed");
    HANDLER.send(super::Event::ScaleFactorChanged);
}
