#import <AppKit/AppKit.h>
#import <QuartzCore/QuartzCore.h>

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
