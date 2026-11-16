//! The two things UIKit needs that winit 0.30 does not do for us: an active audio session, and a
//! scene delegate. iPadOS 26 and later kill an app that has not adopted the UIScene lifecycle
//! (`UIApplicationEvaluateRuntimeIssueForNoSceneLifecycleAdoption`, a brk), and a window that
//! belongs to no scene is never shown; winit creates plain UIWindows.
#import <AVFoundation/AVFoundation.h>
#import <UIKit/UIKit.h>
#import <objc/runtime.h>

static void orb_attach(UIWindowScene *scene) {
    if (scene == nil) {
        return;
    }
#pragma clang diagnostic push
#pragma clang diagnostic ignored "-Wdeprecated-declarations"
    for (UIWindow *window in UIApplication.sharedApplication.windows) {
        if (window.windowScene == nil) {
            window.windowScene = scene;
        }
    }
#pragma clang diagnostic pop
}

static UIWindowScene *orb_scene(void) {
    for (UIScene *scene in UIApplication.sharedApplication.connectedScenes) {
        if ([scene isKindOfClass:[UIWindowScene class]]) {
            return (UIWindowScene *)scene;
        }
    }
    return nil;
}

// winit makes its window with `-[UIWindow makeKeyAndVisible]` and then reads the size of the
// view; a window in no scene has a zero-sized frame in screen space, which wgpu turns into a
// zero-sized surface. The window joins the scene before it is made key.
static void (*orb_make_key_and_visible)(id, SEL);

static void orb_attach_and_make_key(UIWindow *window, SEL selector) {
    if (window.windowScene == nil) {
        window.windowScene = orb_scene();
    }
    orb_make_key_and_visible(window, selector);
}

__attribute__((constructor)) static void orb_install_scene_attachment(void) {
    Method method = class_getInstanceMethod([UIWindow class], @selector(makeKeyAndVisible));
    orb_make_key_and_visible = (void *)method_getImplementation(method);
    method_setImplementation(method, (IMP)orb_attach_and_make_key);
}

void orb_ios_audio_session(void) {
    AVAudioSession *session = [AVAudioSession sharedInstance];
    NSError *error = nil;
    if (![session setCategory:AVAudioSessionCategoryPlayback error:&error]) {
        NSLog(@"orb-ios: audio session category: %@", error);
    }
    error = nil;
    if (![session setPreferredSampleRate:44100 error:&error]) {
        NSLog(@"orb-ios: audio session sample rate: %@", error);
    }
    error = nil;
    if (![session setPreferredIOBufferDuration:0.0232 error:&error]) {
        NSLog(@"orb-ios: audio session buffer: %@", error);
    }
    error = nil;
    if (![session setActive:YES error:&error]) {
        NSLog(@"orb-ios: audio session activation: %@", error);
    }
}

/// Whether the app's window is smaller than its screen: iPadOS shows its own menu bar in the
/// windowed modes, and the panel's bar would be a second one there.
int orb_ios_windowed(void) {
    for (UIScene *scene in UIApplication.sharedApplication.connectedScenes) {
        if (![scene isKindOfClass:[UIWindowScene class]]) {
            continue;
        }
        UIWindowScene *window = (UIWindowScene *)scene;
        if (window.activationState != UISceneActivationStateForegroundActive) {
            continue;
        }
        UIWindow *key = window.keyWindow ?: window.windows.firstObject;
        if (key == nil) {
            return 0;
        }
        CGRect frame = key.frame;
        CGRect screen = window.screen.bounds;
        return (fabs(frame.size.width - screen.size.width) > 1.0 ||
                fabs(frame.size.height - screen.size.height) > 1.0)
                   ? 1
                   : 0;
    }
    return 0;
}

/// Attaches the windows winit has made so far to the app's window scene. Rust calls it once the
/// panel exists; the scene delegate calls it when the scene connects.
void orb_ios_attach_scene(void) {
    for (UIScene *scene in UIApplication.sharedApplication.connectedScenes) {
        if ([scene isKindOfClass:[UIWindowScene class]]) {
            orb_attach((UIWindowScene *)scene);
            return;
        }
    }
}

extern void orb_ios_build_menu_bar(id<UIMenuBuilder> builder);

/// The delegate `Info.plist` names: it adopts the scene lifecycle and asks for the app's menus
/// (with a scene delegate, UIKit asks the scene for the menu bar, not the app delegate).
@interface OrbSceneDelegate : UIResponder <UIWindowSceneDelegate>
@end

@implementation OrbSceneDelegate

- (void)scene:(UIScene *)scene
    willConnectToSession:(UISceneSession *)session
                 options:(UISceneConnectionOptions *)connectionOptions {
    if ([scene isKindOfClass:[UIWindowScene class]]) {
        orb_attach((UIWindowScene *)scene);
    }
}

- (void)sceneDidBecomeActive:(UIScene *)scene {
    if ([scene isKindOfClass:[UIWindowScene class]]) {
        orb_attach((UIWindowScene *)scene);
    }
}

- (void)buildMenuWithBuilder:(id<UIMenuBuilder>)builder {
    orb_ios_build_menu_bar(builder);
}

@end
