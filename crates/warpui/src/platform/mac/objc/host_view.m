#import "host_view.h"

#import <Metal/Metal.h>
#import <os/signpost.h>
#import <stdatomic.h>

void warp_view_did_change_backing_properties(WarpHostView *, BOOL);
void warp_view_set_frame_size(WarpHostView *, NSSize, BOOL);
void warp_update_layer(WarpHostView *);
BOOL warp_handle_view_event(WarpHostView *, NSEvent *, BOOL);
BOOL warp_handle_first_mouse_event(WarpHostView *, NSEvent *);
void warp_handle_insert_text(WarpHostView *, id);
void warp_update_ime_state(WarpHostView *, BOOL);
void warp_handle_drag_and_drop(WarpHostView *, NSArray *, NSPoint);
void warp_handle_file_drag(WarpHostView *, NSPoint);
void warp_handle_file_drag_exit(WarpHostView *);
NSRect warp_ime_position(WarpHostView *, NSRect *);
id warp_get_accessibility_contents(WarpHostView *);
void warp_marked_text_updated(WarpHostView *, NSString *, NSRange);
void warp_marked_text_cleared(WarpHostView *);
void warp_frame_presented(uint64_t, uint64_t, BOOL);

// Draw-latency instrumentation (#1148). Intervals/events show up in Instruments under
// "Points of Interest": keyDown -> displayLayer -> nextDrawable -> presented.
static os_log_t warpDrawLog(void) {
    static os_log_t log;
    static dispatch_once_t once;
    dispatch_once(&once, ^{
      log = os_log_create("dev.phosphor.draw", "PointsOfInterest");
    });
    return log;
}

// Same clock as MTLDrawable.presentedTime (mach absolute time, in seconds), as nanoseconds.
static uint64_t warpNowNs(void) {
    return (uint64_t)(CACurrentMediaTime() * 1e9);
}

// Time of the earliest key press that has not yet been attached to a frame (0 = none). Written on
// the main thread; a stale value (no frame followed the key) is overwritten after 500 ms.
static _Atomic uint64_t warpPendingKeyNs = 0;

// Called on the main thread, after nextDrawable and before the drawable is presented. Attaches the
// pending key press (if any) to this frame and reports key -> presented latency from the
// drawable's presented handler. `waitNs` is how long nextDrawable blocked.
void warp_frame_track_drawable(id<MTLDrawable> drawable, uint64_t waitNs) {
    uint64_t keyNs = atomic_exchange(&warpPendingKeyNs, 0);
    os_signpost_event_emit(warpDrawLog(), OS_SIGNPOST_ID_EXCLUSIVE, "nextDrawable", "wait_us=%llu",
                           waitNs / 1000);
    if (@available(macOS 10.15.4, *)) {
        if ([drawable respondsToSelector:@selector(addPresentedHandler:)]) {
            [drawable addPresentedHandler:^(id<MTLDrawable> presented) {
              CFTimeInterval presentedTime = presented.presentedTime;
              uint64_t presentedNs = (uint64_t)(presentedTime * 1e9);
              BOOL wasPresented = presentedTime > 0;
              uint64_t keyToPresentedNs =
                  (wasPresented && keyNs > 0 && presentedNs > keyNs) ? presentedNs - keyNs : 0;
              os_signpost_event_emit(warpDrawLog(), OS_SIGNPOST_ID_EXCLUSIVE, "presented",
                                     "key_to_presented_us=%llu", keyToPresentedNs / 1000);
              warp_frame_presented(keyToPresentedNs, waitNs, wasPresented);
            }];
        }
    }
}

@implementation NSPasteboard (Warp)

- (NSArray *)getFilePaths {
    NSMutableArray *paths = [NSMutableArray array];
    NSArray<NSURL *> *urls = [self readObjectsForClasses:@[ [NSURL class] ] options:0];
    for (NSURL *url in urls) {
        NSString *path = url.path;
        if (path) {
            [paths addObject:path];
        }
    }
    return paths;
}

@end

@implementation WarpHostView {
    // The windowState is managed on the Rust side.
    // Note Rust expects this name even though we are not a window.
    void *windowState;

    // Whether we start a window drag on an unhandled mouseDown event inside the title bar
    BOOL titlebarDragEnabled;

    // Whether we are in test mode, which suppresses drawing.
    BOOL testMode;

    // The metal device for our layer.
    id metalDevice;

    NSMutableAttributedString *markedText;
    NSMutableString *textToInsert;

    // Whether to have resize event callback called asynchronously.
    BOOL asyncCallback;

    // Whether we're in the middle of a call to interpretKeyEvents.
    BOOL interpretingKeyEvents;

    // Whether the IME modified marked text (via setMarkedText: or unmarkText)
    // during the current interpretKeyEvents: pass. Used to avoid wiping a
    // freshly-set marked text in the split-commit scenario where an IME
    // calls insertText: (committing some text) and then setMarkedText: (with
    // new in-progress text) in the same keystroke. Without this, the trailing
    // unmarkText in keyDownImpl would clobber that new marked text.
    BOOL imeTouchedMarkedTextDuringInterpret;
}

- (BOOL)acceptsFirstResponder {
    return YES;
}

- (BOOL)mouseDownCanMoveWindow {
    return !titlebarDragEnabled;
}

- (BOOL)readyForWarp {
    return windowState != NULL;
}

/// Returns the height of the titlebar.
- (CGFloat)titlebarHeight {
    NSButton *closeButton = [self.window standardWindowButton:NSWindowCloseButton];
    NSView *titlebar = [closeButton superview];
    return titlebar.frame.size.height;
}

- (BOOL)mouseInTitleBar:(NSEvent *)event {
    NSPoint windowLoc = [self convertPoint:event.locationInWindow fromView:nil];
    // windowLoc.y is the distance from the bottom of the window to the cursor
    // NSHeight(window.frame) will be the height of the whole window, so
    // NSHeight - titlebarHeight will be the bottom border of the titlebar
    return NSHeight(self.window.frame) - [self titlebarHeight] <= windowLoc.y;
}

// See if the user double clicked in the titlebar. If so, do whatever
// action is given by preferences.
// \return true if handled, false otherwise.
- (BOOL)handleTitleBarDoubleClick:(NSEvent *)event {
    NSWindow *window = self.window;
    NSWindowStyleMask styleMask = window.styleMask;
    // Was this a double click in a full-sized content view, not in full screen?
    if (event.clickCount != 2) return NO;
    if (!(styleMask & NSWindowStyleMaskFullSizeContentView)) return NO;
    if (styleMask & NSWindowStyleMaskFullScreen) return NO;

    // See if our point is in the titlebar of the window.
    if (![self mouseInTitleBar:event]) return NO;

    // Ok, do the action.
    NSString *action =
        [[NSUserDefaults standardUserDefaults] objectForKey:@"AppleActionOnDoubleClick"];

    // When user has not explicitly ticked or unticked the `Double-click the window's
    // title bar to` option in system preferences, the NSUserDefaults will not have the key
    // "AppleActionOnDoubleClick", despite in system preferences the default is to "Zoom".
    // To make the behavior consistent, when the key is nil, we set performZoom as the
    // default behavior here.
    if ([action isEqualToString:@"Minimize"]) {
        [window performMiniaturize:nil];
        return YES;
    } else if (action == nil || [action isEqualToString:@"Maximize"]) {
        [window performZoom:nil];
        return YES;
    }
    return NO;
}

- (void)viewDidChangeBackingProperties {
    if (self.readyForWarp) warp_view_did_change_backing_properties(self, asyncCallback);
    [super viewDidChangeBackingProperties];
}

- (void)setFrameSize:(NSSize)size {
    BOOL changed = !NSEqualSizes(size, self.frame.size);
    // We could receive invalid frame sizes when the window is moved offscreen.
    // Validate the size against the minimum drawable size of the window before
    // passing to the rust side.
    if (size.height >= self.window.minSize.height && size.width >= self.window.minSize.width) {
        [super setFrameSize:size];
        // It's an important optimization to only invoke this if the size changed.
        if (self.readyForWarp && changed) {
            warp_view_set_frame_size(self, size, asyncCallback);
        }
    }
}

- (void)displayLayer:(CALayer *)layer {
    if (!testMode && self.readyForWarp) {
        os_signpost_interval_begin(warpDrawLog(), OS_SIGNPOST_ID_EXCLUSIVE, "displayLayer");
        warp_update_layer(self);
        os_signpost_interval_end(warpDrawLog(), OS_SIGNPOST_ID_EXCLUSIVE, "displayLayer");
    }
}

- (void)setAsyncCallback:(BOOL)shouldAsync {
    asyncCallback = shouldAsync;
}
- (void)setPresentsWithTransaction:(BOOL)presentsWithTransaction {
    CAMetalLayer *layer = (CAMetalLayer *)self.layer;
    layer.presentsWithTransaction = presentsWithTransaction;
}

- (void)keyDown:(NSEvent *)event {
    [self keyDownImpl:event];
}

- (BOOL)keyDownImpl:(NSEvent *)event {
    uint64_t keyNowNs = warpNowNs();
    uint64_t pendingKeyNs = atomic_load(&warpPendingKeyNs);
    if (pendingKeyNs == 0 || keyNowNs - pendingKeyNs > 500000000ull) {
        atomic_store(&warpPendingKeyNs, keyNowNs);
    }
    os_signpost_event_emit(warpDrawLog(), OS_SIGNPOST_ID_EXCLUSIVE, "keyDown");

    BOOL wasComposing = [self hasMarkedText];
    [textToInsert setString:@""];
    imeTouchedMarkedTextDuringInterpret = NO;

    // Interpret the key events here so we could check whether user is composing
    // text within the IME and pass the state down to the KeyDown events.
    interpretingKeyEvents = YES;
    [self interpretKeyEvents:[NSArray arrayWithObject:event]];
    interpretingKeyEvents = NO;

    BOOL handled = NO;
    if (self.readyForWarp) {
        handled = warp_handle_view_event(self, event, wasComposing || [self hasMarkedText]);
    }

    // It's possible to have keybinding conflicts between terminal apps which use the meta key and
    // MacOS "dead keys". Dead keys are used to add diacritical marks to other characters, and they
    // start composing marked text. To detect if a keybinding was triggered in the app, `handled`
    // will be true. If that is the case, we don't want MacOS to also start composing because we
    // already handled that keydown elsewhere. So, if `justStartedComposing` is also true, clear
    // out the marked text.
    // https://support.apple.com/guide/mac-help/enter-characters-with-accent-marks-on-mac-mh27474/mac#mchl45cdda7f
    BOOL justStartedComposing = !wasComposing && [self hasMarkedText];
    if (handled && justStartedComposing) {
        NSTextInputContext *inputContext = [self inputContext];
        [inputContext discardMarkedText];
        [self unmarkText];
    }

    // Dispatch TypedCharacter event after KeyDown has been dispatched.
    if ([textToInsert length] > 0 && !handled) {
        warp_handle_insert_text(self, (NSString *)textToInsert);
        // Only clear marked text if the IME did not touch it during this
        // interpretKeyEvents pass. Otherwise we'd either fire a redundant
        // ClearMarkedText (if IME already cleared) or, worse, wipe the new
        // marked text the IME just set in a split-commit (e.g. Japanese IME
        // committing a phrase and queuing the next character as marked text).
        if (!imeTouchedMarkedTextDuringInterpret) {
            [self unmarkText];
        }
    }

    return handled;
}

- (BOOL)acceptsFirstMouse:(NSEvent *)event {
    // We want to receive mouseDown events even if the window is not key
    // and we explicity fire the event here so that Warp can handle it.
    if (self.readyForWarp) warp_handle_first_mouse_event(self, event);

    // We return NO though so that the event is not fired twice (returning YES
    // would result in the event being passed to the mouseDown handler).
    return NO;
}

- (void)mouseDown:(NSEvent *)event {
    if (self.readyForWarp) {
        BOOL eventHandled = warp_handle_view_event(self, event, NO);
        if (self->titlebarDragEnabled && !eventHandled && [self mouseInTitleBar:event]) {
            // If Warp doesn't do anything with the event, indicated by returning `false`, and
            // if the drag starts in the titlebar, begin dragging the window
            [self.window performWindowDragWithEvent:event];
        }
    }
}

- (void)mouseUp:(NSEvent *)event {
    // Our content view is full-size so we don't get the default behavior
    // on titlebar clicks. Implement it manually.
    BOOL warp_handled = NO;
    if (self.readyForWarp) {
        warp_handled = warp_handle_view_event(self, event, NO);
    }
    if (!warp_handled) {
        [self handleTitleBarDoubleClick:event];
    }
}

- (void)otherMouseDown:(NSEvent *)event {
    if (self.readyForWarp) warp_handle_view_event(self, event, NO);
}

- (void)rightMouseDown:(NSEvent *)event {
    if (self.readyForWarp) warp_handle_view_event(self, event, NO);
}

- (void)mouseDragged:(NSEvent *)event {
    if (self.readyForWarp) warp_handle_view_event(self, event, NO);
}

- (void)scrollWheel:(NSEvent *)event {
    if (self.readyForWarp) warp_handle_view_event(self, event, NO);
}

- (void)mouseMoved:(NSEvent *)event {
    if (self.readyForWarp) warp_handle_view_event(self, event, NO);
}

- (void)flagsChanged:(NSEvent *)event {
    if (self.readyForWarp) warp_handle_view_event(self, event, NO);
}

- (void)dealloc {
    [markedText release];
    [textToInsert release];
    [metalDevice release];
    [super dealloc];
}

- (CALayer *)makeBackingLayer {
    CAMetalLayer *layer = [CAMetalLayer layer];
    layer.pixelFormat = MTLPixelFormatBGRA8Unorm;
    layer.device = metalDevice;
    // Two drawables (default 3): at most one stale frame can queue ahead of the one carrying the
    // latest keystroke. Documented range is 2-3.
    layer.maximumDrawableCount = 2;
    // nextDrawable gives up after ~1 s and returns nil instead of blocking the main thread (and
    // with it all input handling) indefinitely; the renderer skips that frame and re-arms a redraw.
    layer.allowsNextDrawableTimeout = YES;
    layer.autoresizingMask = kCALayerWidthSizable | kCALayerHeightSizable;
    layer.needsDisplayOnBoundsChange = YES;
    layer.presentsWithTransaction = NO;
    layer.delegate = self;
    layer.opaque = NO;
    return layer;
}

- (WarpHostView *)initWithFrame:(NSRect)frame
                    metalDevice:(id)device
             enableTitlebarDrag:(BOOL)enableTitlebarDrag
                       testMode:(BOOL)testModeFlag {
    NSAssert(testModeFlag || device, @"Nil metal device not in test mode");
    [super initWithFrame:frame];

    // Register here so we could receive drag and drop events.
    [self registerForDraggedTypes:@[
        NSPasteboardTypeFileURL,
    ]];
    self->testMode = testModeFlag;
    self->titlebarDragEnabled = enableTitlebarDrag;
    self->metalDevice = [device retain];
    self->markedText = [[NSMutableAttributedString alloc] init];
    self->textToInsert = [[NSMutableString alloc] init];
    self->asyncCallback = YES;
    self.autoresizingMask = NSViewWidthSizable | NSViewHeightSizable;
    self.wantsLayer = YES;
    self.layerContentsRedrawPolicy = NSViewLayerContentsRedrawDuringViewResize;
    return self;
}

// Entry point for drag & drop. Check whether the source is an acceptable type and if so
// pass it down to performDragOperaion.
- (NSDragOperation)draggingEntered:(id<NSDraggingInfo>)sender {
    NSDragOperation sourceMask = [sender draggingSourceOperationMask];

    BOOL pasteOK =
        !![[sender draggingPasteboard] availableTypeFromArray:@[ NSPasteboardTypeFileURL ]];
    if (pasteOK && (sourceMask & NSDragOperationCopy)) {
        return NSDragOperationCopy;
    }
    return NSDragOperationNone;
}

// Called continuously while the drag operation is occurring within the view
- (NSDragOperation)draggingUpdated:(id<NSDraggingInfo>)sender {
    NSPoint dragPoint = [sender draggingLocation];
    NSPoint localPoint = [self convertPoint:dragPoint fromView:nil];

    NSPasteboard *pasteboard = [sender draggingPasteboard];
    if (self.readyForWarp) {
        NSArray *types = [pasteboard types];
        if ([types containsObject:NSPasteboardTypeFileURL]) {
            warp_handle_file_drag(self, localPoint);
            return YES;
        }
    }
    return NSDragOperationNone;
}

- (void)draggingExited:(id<NSDraggingInfo>)sender {
    if (self.readyForWarp) {
        warp_handle_file_drag_exit(self);
    }
}

- (BOOL)performDragOperation:(id<NSDraggingInfo>)sender {
    NSPasteboard *pasteboard = [sender draggingPasteboard];
    NSDragOperation dragOperation = [sender draggingSourceOperationMask];

    NSPoint dragPoint = [sender draggingLocation];
    NSPoint localPoint = [self convertPoint:dragPoint fromView:nil];

    if (self.readyForWarp && (dragOperation & NSDragOperationCopy)) {
        NSArray *types = [pasteboard types];
        if ([types containsObject:NSPasteboardTypeFileURL]) {
            warp_handle_drag_and_drop(self, [pasteboard getFilePaths], localPoint);
            return YES;
        }
    }
    return NO;
}

- (void)closeIMEAsync {
    dispatch_async(dispatch_get_main_queue(), ^{
      NSTextInputContext *inputContext = [self inputContext];
      [inputContext discardMarkedText];

      [self unmarkText];
    });
}

#pragma mark - Accessibility
- (BOOL)isAccessibilityElement {
    return YES;
}

- (NSAccessibilityRole)accessibilityRole {
    return NSAccessibilityTextAreaRole;
}

- (NSString *)accessibilityRoleDescription {
    return NSAccessibilityRoleDescriptionForUIElement(self);
}

- (BOOL)isAccessibilityFocused {
    return YES;
}

- (id)accessibilityValue {
    return warp_get_accessibility_contents(self);
}

- (NSInteger)accessibilityNumberOfCharacters {
    return 0;
}

- (NSInteger)accessibilityInsertionPointLineNumber {
    return 0;
}

- (NSString *)accessibilityDocument {
    return nil;
}

////////////////////////////////////////////////////////////////////////////////
// NSTextInputClient protocol implementation
////////////////////////////////////////////////////////////////////////////////

- (nullable NSAttributedString *)attributedSubstringForProposedRange:(NSRange)range
                                                         actualRange:
                                                             (nullable NSRangePointer)actualRange {
    return nil;
}

- (NSUInteger)characterIndexForPoint:(NSPoint)thePoint {
    return (NSUInteger)0;
}

// This is a no-op as we will be handling control characters in KeyDown events.
- (void)doCommandBySelector:(SEL)selector {
}

- (NSRect)firstRectForCharacterRange:(NSRange)range
                         actualRange:(nullable NSRangePointer)actualRange {
    NSWindow *window = self.window;
    if (self.readyForWarp) {
        NSRect contentRect = [window contentRectForFrameRect:[window frame]];
        NSRect rect = warp_ime_position(self, &contentRect);
        return rect;
    } else {
        return NSZeroRect;
    }
}

- (BOOL)hasMarkedText {
    return [markedText length] > 0;
}

// Referenced glfw for this implementation.
// https://github.com/glfw/glfw/blob/7ef34eb06de54dd9186d3d21a401b2ef819b59e7/src/cocoa_window.m#L814
- (void)insertText:(id)string replacementRange:(NSRange)replacementRange {
    if (self.readyForWarp) {
        NSMutableString *characters = [[NSMutableString alloc] init];

        if ([string isKindOfClass:[NSAttributedString class]]) {
            // We are appending rather than replacing here because sometimes insertText
            // could be fired multiple times in a row. For example, when user types
            // Option-E followed by g, insertText will fire ´ first and then g.
            [characters appendString:[string string]];
        } else {
            [characters appendString:(NSString *)string];
        }

        // If we're in the middle of a call to interpretKeyEvents, batch up all
        // inserted text, as we may handle the event during `keyDown`.  If this
        // call to `insertText` is not in a call stack underneath `keyDown`
        // (e.g.: when inserting an emoji from the emoji composer), just insert
        // the text directly.
        if (interpretingKeyEvents) {
            [textToInsert appendString:characters];
        } else {
            warp_handle_insert_text(self, (NSString *)characters);
        }

        [characters release];
    }
    // When handling the key down Enter, we might need to rely on the IME being open
    // to accept the marked text as-is and so can't call unmarkText.
    if (!interpretingKeyEvents) {
        [self unmarkText];
    }
}

- (NSRange)markedRange {
    if ([markedText length] > 0)
        return NSMakeRange(0, [markedText length]);
    else
        return NSMakeRange(NSNotFound, 0);
}

- (NSRange)selectedRange {
    return NSMakeRange(0, 0);
}

- (void)setMarkedText:(id)string
        selectedRange:(NSRange)selectedRange
     replacementRange:(NSRange)replacementRange {
    if (interpretingKeyEvents) {
        imeTouchedMarkedTextDuringInterpret = YES;
    }

    [markedText release];
    if ([string isKindOfClass:[NSAttributedString class]])
        markedText = [[NSMutableAttributedString alloc] initWithAttributedString:string];
    else
        markedText = [[NSMutableAttributedString alloc] initWithString:string];

    if (self.readyForWarp) {
        warp_marked_text_updated(self, markedText.string, selectedRange);
        if ([markedText length] > 0) {
            warp_update_ime_state(self, YES);
        } else {
            warp_update_ime_state(self, NO);
        }
    }
}

- (void)unmarkText {
    if (interpretingKeyEvents) {
        imeTouchedMarkedTextDuringInterpret = YES;
    }
    [[markedText mutableString] setString:@""];
    if (self.readyForWarp) {
        warp_update_ime_state(self, NO);
        warp_marked_text_cleared(self);
    }
}

- (NSArray<NSString *> *)validAttributesForMarkedText {
    return [NSArray array];
}

@end
