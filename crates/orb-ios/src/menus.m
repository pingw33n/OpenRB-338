// The iPad's menu bar: `UIMenuBuilder` asks the app delegate for the main menu, and
// `orb_ios_menu_rows` (src/menus.rs) answers with the rows built from orb-ui's menu model. The
// delegate exists only for this: winit 0.30 registers none, so the app installs one at start.
#import <UIKit/UIKit.h>
#import <dlfcn.h>

extern void orb_ios_note(const char *line);

// Keep in step with `Row` in src/menus.rs.
typedef struct {
    int menu;
    int kind;
    int depth;
    int enabled;
    int checked;
    int modifiers;
    const char *title;
    const char *id;
    const char *key;
} OrbMenuRow;

extern const OrbMenuRow *orb_ios_menu_rows(int *count);
extern void orb_ios_menu_action(const char *id);

enum {
    OrbKindAction = 0,
    OrbKindCheck = 1,
    OrbKindRadio = 2,
    OrbKindChoice = 3,
    OrbKindSubmenu = 4,
    OrbKindSeparator = 5,
    OrbKindLabel = 6,
    OrbKindMenu = 7,
};

static NSString *const OrbFileMenu = @"orb.file";
static NSString *const OrbModsMenu = @"orb.mods";
static NSString *const OrbOptionsMenu = @"orb.options";
static NSString *const OrbHelpMenu = @"orb.help";

static UIKeyModifierFlags orb_flags(int modifiers) {
    UIKeyModifierFlags flags = 0;
    if (modifiers & 1) {
        flags |= UIKeyModifierCommand;
    }
    if (modifiers & 2) {
        flags |= UIKeyModifierShift;
    }
    if (modifiers & 4) {
        flags |= UIKeyModifierAlternate;
    }
    return flags;
}

static UIAction *orb_action(NSString *title, NSString *identifier, int checked) {
    UIAction *action = [UIAction actionWithTitle:title
                                           image:nil
                                      identifier:identifier
                                         handler:^(UIAction *chosen) {
                                             (void)chosen;
                                             orb_ios_menu_action(identifier.UTF8String);
                                         }];
    if (checked >= 0) {
        action.state = checked > 0 ? UIMenuElementStateOn : UIMenuElementStateOff;
    }
    return action;
}

static UIMenuElement *orb_element(const OrbMenuRow *row) {
    NSString *title = row->title ? @(row->title) : @"";
    NSString *identifier = row->id ? @(row->id) : nil;
    switch (row->kind) {
        case OrbKindSeparator:
            // UIKit draws a separator as an empty inline menu.
            return [UIMenu menuWithTitle:@""
                                   image:nil
                              identifier:nil
                                 options:UIMenuOptionsDisplayInline
                                children:@[]];
        case OrbKindLabel: {
            UIAction *action = orb_action(title, nil, -1);
            action.attributes = UIMenuElementAttributesDisabled;
            return action;
        }
        case OrbKindAction:
            if (row->key && row->key[0] != '\0' && row->enabled) {
                return [UIKeyCommand commandWithTitle:title
                                                image:nil
                                               action:@selector(orbMenuCommand:)
                                                input:@(row->key)
                                        modifierFlags:orb_flags(row->modifiers)
                                         propertyList:identifier];
            }
            if (row->enabled) {
                return orb_action(title, identifier, -1);
            }
            {
                UIAction *action = orb_action(title, identifier, -1);
                action.attributes = UIMenuElementAttributesDisabled;
                return action;
            }
        default:
            return orb_action(title, identifier, row->checked);
    }
}

static UIMenu *orb_menu(int index, NSString *identifier, NSString *title,
                        const OrbMenuRow *rows, int count) {
    NSMutableArray<UIMenuElement *> *elements = [NSMutableArray array];
    for (int i = 0; i < count; i++) {
        const OrbMenuRow *row = &rows[i];
        if (row->menu != index || row->kind == OrbKindMenu || row->depth > 0) {
            continue;
        }
        if (row->kind == OrbKindSubmenu) {
            NSMutableArray<UIMenuElement *> *children = [NSMutableArray array];
            for (int j = i + 1; j < count && rows[j].menu == index && rows[j].depth > 0; j++) {
                [children addObject:orb_element(&rows[j])];
            }
            [elements addObject:[UIMenu menuWithTitle:(row->title ? @(row->title) : @"")
                                                image:nil
                                           identifier:nil
                                              options:0
                                             children:children]];
            continue;
        }
        [elements addObject:orb_element(row)];
    }
    return [UIMenu menuWithTitle:title image:nil identifier:identifier options:0 children:elements];
}

/// Where a menu can go when the system's menus are not where a Mac's are.
static NSString *orb_anchor(id<UIMenuBuilder> builder) {
    for (NSString *identifier in @[ UIMenuEdit, UIMenuView, UIMenuWindow, UIMenuHelp ]) {
        if ([builder menuForIdentifier:identifier] != nil) {
            return identifier;
        }
    }
    return nil;
}

void orb_ios_build_menu_bar(id<UIMenuBuilder> builder) {
    if (builder.system != UIMenuSystem.mainSystem) {
        return;
    }
    int count = 0;
    orb_ios_note("menu bar built");
    const OrbMenuRow *rows = orb_ios_menu_rows(&count);
    if (count == 0) {
        return;
    }
    NSMutableArray<NSString *> *titles = [NSMutableArray array];
    for (int i = 0; i < count; i++) {
        if (rows[i].kind == OrbKindMenu) {
            [titles addObject:(rows[i].title ? @(rows[i].title) : @"")];
        }
    }
    if (titles.count == 0) {
        return;
    }
    NSArray<NSString *> *identifiers = @[ OrbFileMenu, OrbModsMenu, OrbOptionsMenu, OrbHelpMenu ];
    NSString *anchor = orb_anchor(builder);
    if (anchor == nil) {
        return;
    }
    // Our menus go in the bar's order; the system's own File, Edit, View and Window menus then
    // leave, since none of their commands are this app's (Window's Stage Manager ones excepted).
    for (NSUInteger i = 0; i < titles.count; i++) {
        NSString *identifier = i < identifiers.count ? identifiers[i] : [NSString stringWithFormat:@"orb.menu.%lu", (unsigned long)i];
        UIMenu *menu = orb_menu((int)i, identifier, titles[i], rows, count);
        [builder insertSiblingMenu:menu beforeMenuForIdentifier:anchor];
    }
    for (NSString *identifier in @[ UIMenuFile, UIMenuEdit, UIMenuFormat, UIMenuView,
                                    UIMenuWindow, UIMenuHelp ]) {
        if ([builder menuForIdentifier:identifier] != nil) {
            [builder removeMenuForIdentifier:identifier];
        }
    }
}

/// The two places UIKit may ask for the main menu: the app delegate, and (with scenes) the scene
/// delegate. Both build from the same rows.
static void orb_handle_menu_command(UICommand *command) {
    NSString *identifier = command.propertyList;
    if ([identifier isKindOfClass:[NSString class]]) {
        orb_ios_menu_action(identifier.UTF8String);
    }
}

/// The application object, which is the last responder: if UIKit asks the app rather than its
/// delegate for the main menu, this is where it lands.
@interface OrbApplication : UIApplication
@end

@implementation OrbApplication

- (void)orbMenuCommand:(UICommand *)command {
    orb_handle_menu_command(command);
}

- (void)buildMenuWithBuilder:(id<UIMenuBuilder>)builder {
    orb_ios_build_menu_bar(builder);
}

@end

@interface OrbAppDelegate : NSObject <UIApplicationDelegate>
@end

@implementation OrbAppDelegate

- (void)orbMenuCommand:(UICommand *)command {
    orb_handle_menu_command(command);
}

- (void)buildMenuWithBuilder:(id<UIMenuBuilder>)builder {
    orb_ios_build_menu_bar(builder);
}

@end



static OrbAppDelegate *orb_delegate = nil;

/// Makes the delegate that builds the menu bar, once, and asks the system for a rebuild.
void orb_ios_install_menus(void) {
    if (orb_delegate == nil) {
        orb_delegate = [OrbAppDelegate new];
    }
    if (UIApplication.sharedApplication.delegate != orb_delegate) {
        UIApplication.sharedApplication.delegate = orb_delegate;
    }
    orb_ios_note("menu delegate installed");
    [UIMenuSystem.mainSystem setNeedsRebuild];
}

/// The menus changed (a mod appeared, a flag went on): build them again.
void orb_ios_menus_changed(void) {
    [UIMenuSystem.mainSystem setNeedsRebuild];
}

/// UIKit's `UIApplicationMain`, for the interposer below.
static int (*orb_application_main)(int, char *[], NSString *, NSString *);

/// winit calls `UIApplicationMain(argc, argv, nil, nil)`: no delegate class, so there is no app
/// delegate at launch and the menu bar is UIKit's default one, built from nobody. Naming our
/// class here is the only way to have a delegate before launch: setting `UIApplication.delegate`
/// afterwards is too late, and creating the shared application first trips winit's own
/// "EventLoop cannot be run after UIApplicationMain" check.
int UIApplicationMain(int argc, char *argv[], NSString *principalClassName,
                      NSString *delegateClassName) {
    if (orb_application_main == NULL) {
        orb_application_main = (int (*)(int, char *[], NSString *, NSString *))
            dlsym(RTLD_NEXT, "UIApplicationMain");
    }
    if (principalClassName == nil) {
        principalClassName = NSStringFromClass([OrbApplication class]);
    }
    if (delegateClassName == nil) {
        delegateClassName = NSStringFromClass([OrbAppDelegate class]);
        orb_ios_note("menu delegate named at launch");
    }
    return orb_application_main(argc, argv, principalClassName, delegateClassName);
}
