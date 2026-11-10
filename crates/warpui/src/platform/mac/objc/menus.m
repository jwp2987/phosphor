#import "menus.h"

void warp_menu_item_needs_update(NSMenuItem *, void *);
void warp_menu_item_triggered(NSMenuItem *, void *);
void warp_menu_item_deallocated(void *);
// Implemented in Rust (menus.rs): logs `input_order hop=keydown_menu ctrl=<0|1> cmd=<0|1>` (#2041).
void warp_note_menu_key_event(int is_ctrl, int is_cmd);

@implementation WarpCustomMenuItemHandler

- (id)initWithContext:(void *)context {
    self = [super init];
    rustContext = context;
    return self;
}

- (void)itemWasTriggered:(NSMenuItem *)item {
    if (rustContext && ![item hasSubmenu]) {
        // #2041: a key-down the menu took as this item's key equivalent never reaches the view's
        // key dispatch, so it is named here (flags only). A clicked item's current event is a
        // mouse event and logs nothing.
        NSEvent *event = [NSApp currentEvent];
        if (event != nil && [event type] == NSEventTypeKeyDown) {
            NSEventModifierFlags flags = [event modifierFlags];
            warp_note_menu_key_event((flags & NSEventModifierFlagControl) != 0 ? 1 : 0,
                                     (flags & NSEventModifierFlagCommand) != 0 ? 1 : 0);
        }
        warp_menu_item_triggered(item, rustContext);
    }
}

- (void)itemNeedsUpdate:(NSMenuItem *)item {
    if (rustContext) warp_menu_item_needs_update(item, rustContext);
}

- (void)dealloc {
    if (rustContext) warp_menu_item_deallocated(rustContext);
    [super dealloc];
}

/// Our custom menu items set their enabled state in menuNeedsUpdate:, so do nothing here.
- (BOOL)validateMenuItem:(NSMenuItem *)menuItem {
    return menuItem.isEnabled;
}

@end

void set_menu_item_submenu(NSMenuItem *item, NSMenu *submenu) {
    if (submenu == nil) {
        [item setAction:@selector(itemWasTriggered:)];
    } else {
        [item setAction:NULL];
    }
    [item setSubmenu:submenu];
}
