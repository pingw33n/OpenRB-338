//! The executable the bundle runs. On iOS it is [`orb_ios::run`], which does not return: winit's
//! `run_app` calls `UIApplicationMain`.

fn main() {
    #[cfg(target_os = "ios")]
    orb_ios::run();
    #[cfg(not(target_os = "ios"))]
    eprintln!("orb-ios builds the iPad app: scripts/ios-app.sh");
}
