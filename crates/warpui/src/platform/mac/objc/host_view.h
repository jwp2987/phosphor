#import <AppKit/AppKit.h>
#import <QuartzCore/QuartzCore.h>

// Enter-key tracing (#1482). Implemented in Rust (`warp_log_enter` in window.rs): logs
// `input_order hop=mac_enter stage=<stage> <detail>` at debug level. Only Return (36) and keypad
// Enter (76) key-downs are traced, via WARP_LOG_ENTER; detail never contains typed text.
void warp_log_enter(const char *stage, const char *detail);

static inline BOOL warpIsEnterKeyDown(NSEvent *event) {
    return event != nil && [event type] == NSEventTypeKeyDown &&
           ([event keyCode] == 36 || [event keyCode] == 76);
}

#define WARP_LOG_ENTER(event, stage, ...)                                         \
    do {                                                                          \
        if (warpIsEnterKeyDown(event)) {                                          \
            warp_log_enter(stage, [[NSString stringWithFormat:__VA_ARGS__] UTF8String]); \
        }                                                                         \
    } while (0)

@interface NSPasteboard (Warp)
- (NSArray *)getFilePaths;
@end

/// WarpHostView is the Content view of a Warp window.
// It is backed by a Metal CALayer.
@interface WarpHostView : NSView <CALayerDelegate, NSTextInputClient>
- (WarpHostView *)initWithFrame:(NSRect)frame
                    metalDevice:(id)metalDevice
             enableTitlebarDrag:(BOOL)enableTitlebarDrag
                       testMode:(BOOL)testMode;
- (void)setAsyncCallback:(BOOL)shouldAsync;
- (void)setPresentsWithTransaction:(BOOL)presentsWithTransaction;
- (BOOL)keyDownImpl:(NSEvent *)event;
// Marks `event` (already run through keyDownImpl without being handled) so the keyDown: that
// AppKit then sends for it is skipped instead of delivering the key a second time.
- (void)skipRedeliveryOf:(NSEvent *)event;
@end
