//! Where the Mac is, as Maps finds it: CoreLocation at its best accuracy,
//! with no distance filter, asking once for full accuracy when the user
//! granted only an approximate location. The manager lives on the main
//! thread, whose run loop CoreLocation needs; a thread of its own asks the
//! main queue to read it.

use {crate::net::Answer, std::sync::mpsc::Sender};

/// What the location service says.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Located {
    /// A fix, and how far off it may be, in meters.
    Fix { lat: f64, lon: f64, meters: f64 },
    /// The user, or a policy, said no.
    #[cfg_attr(not(target_os = "macos"), allow(dead_code))]
    Denied,
    /// Granted, but only approximately (Precise Location is off).
    Approximate,
    /// Not running from an app bundle: macOS gives no location to a bare
    /// executable (CoreLocation throws), so it is never asked.
    #[cfg_attr(not(target_os = "macos"), allow(dead_code))]
    Unbundled,
}

/// Starts watching; answers arrive until the provider stops listening.
pub fn watch(answers: Sender<Answer>) {
    #[cfg(target_os = "macos")]
    {
        if !mac::bundled() {
            let _ = answers.send(Answer::Location(Located::Unbundled));
            return;
        }
        let spawned = std::thread::Builder::new()
            .name("location".into())
            .spawn(move || mac::run(&answers));
        if let Err(error) = spawned {
            eprintln!("Moonmap: cannot start the location thread: {error}");
        }
    }
    #[cfg(not(target_os = "macos"))]
    drop(answers);
}

#[cfg(target_os = "macos")]
#[allow(unsafe_code)] // CoreLocation is only reachable through Objective-C messages.
mod mac {
    use {
        super::Located,
        crate::net::Answer,
        dispatch2::DispatchQueue,
        objc2::rc::Retained,
        objc2_core_location::{
            CLAccuracyAuthorization, CLAuthorizationStatus, CLLocationManager,
            kCLDistanceFilterNone, kCLLocationAccuracyBest,
        },
        objc2_foundation::{NSBundle, NSString},
        std::{
            cell::RefCell,
            sync::{
                Arc, Mutex,
                atomic::{AtomicBool, Ordering},
                mpsc::Sender,
            },
            time::Duration,
        },
    };

    /// A move smaller than this, at no better accuracy, is not news.
    const MOVED_METERS: f64 = 0.5;
    /// How often the main thread reads the manager.
    const POLL: Duration = Duration::from_millis(700);

    thread_local! {
        /// The one manager, on the main thread: CoreLocation asks for
        /// permission and delivers fixes through the main run loop, which
        /// AppKit runs.
        static MANAGER: RefCell<Option<Retained<CLLocationManager>>> = const { RefCell::new(None) };
    }

    #[derive(Default)]
    struct Poll {
        asked_full: bool,
        said: Option<Located>,
    }

    /// Whether this process runs from a `.app`, the only kind CoreLocation
    /// serves.
    pub fn bundled() -> bool {
        NSBundle::mainBundle()
            .bundlePath()
            .to_string()
            .ends_with(".app")
    }

    /// Asks the main thread to read the manager every [`POLL`] until the
    /// provider stops listening.
    pub fn run(answers: &Sender<Answer>) {
        let poll = Arc::new(Mutex::new(Poll::default()));
        let stopped = Arc::new(AtomicBool::new(false));
        while !stopped.load(Ordering::Relaxed) {
            let (answers, poll, stopped) =
                (answers.clone(), Arc::clone(&poll), Arc::clone(&stopped));
            DispatchQueue::main().exec_async(move || {
                if !read(&answers, &poll) {
                    stopped.store(true, Ordering::Relaxed);
                    MANAGER.with_borrow_mut(|manager| {
                        if let Some(manager) = manager.take() {
                            // SAFETY: on the main thread, which owns it.
                            unsafe { manager.stopUpdatingLocation() };
                        }
                    });
                }
            });
            std::thread::sleep(POLL);
        }
    }

    /// One look at the manager, made on first use; false once nobody listens.
    fn read(answers: &Sender<Answer>, poll: &Mutex<Poll>) -> bool {
        let Ok(mut poll) = poll.lock() else {
            return false;
        };
        let debug = std::env::var_os("MOONMAP_DEBUG_LOCATION").is_some();
        MANAGER.with_borrow_mut(|slot| {
            // SAFETY (every block below): the manager is made, configured
            // and read on the main thread only; the constants are
            // CoreLocation's own exported statics; the rest are property
            // reads.
            let manager = slot.get_or_insert_with(|| unsafe {
                let manager = CLLocationManager::new();
                manager.setDesiredAccuracy(kCLLocationAccuracyBest);
                manager.setDistanceFilter(kCLDistanceFilterNone);
                manager.requestWhenInUseAuthorization();
                manager.startUpdatingLocation();
                manager
            });
            let (status, accuracy, location) = unsafe {
                (
                    manager.authorizationStatus(),
                    manager.accuracyAuthorization(),
                    manager.location(),
                )
            };
            let fix = location.map(|location| {
                let (coordinate, meters) =
                    unsafe { (location.coordinate(), location.horizontalAccuracy()) };
                Located::Fix {
                    lat: coordinate.latitude,
                    lon: coordinate.longitude,
                    meters,
                }
            });
            if debug {
                eprintln!(
                    "location: status {} accuracy {} fix {fix:?}",
                    status.0, accuracy.0
                );
            }
            let mut news = Vec::new();
            if status == CLAuthorizationStatus::Denied
                || status == CLAuthorizationStatus::Restricted
            {
                news.push(Located::Denied);
            } else if let Some(fix @ Located::Fix { meters, .. }) = fix
                && meters >= 0.0
                && is_news(poll.said, fix)
            {
                news.push(fix);
            }
            if accuracy == CLAccuracyAuthorization::ReducedAccuracy
                && status != CLAuthorizationStatus::NotDetermined
                && !poll.asked_full
            {
                poll.asked_full = true;
                let key = NSString::from_str("Directions");
                // SAFETY: the purpose key names an entry of Info.plist's
                // NSLocationTemporaryUsageDescriptionDictionary.
                unsafe { manager.requestTemporaryFullAccuracyAuthorizationWithPurposeKey(&key) };
                news.insert(0, Located::Approximate);
            }
            for located in news {
                if poll.said == Some(located) && located == Located::Denied {
                    continue;
                }
                poll.said = Some(located);
                if answers.send(Answer::Location(located)).is_err() {
                    return false;
                }
            }
            true
        })
    }

    fn is_news(said: Option<Located>, fix: Located) -> bool {
        match (said, fix) {
            (
                Some(Located::Fix { lat, lon, meters }),
                Located::Fix {
                    lat: new_lat,
                    lon: new_lon,
                    meters: new_meters,
                },
            ) => {
                let moved = moonmap_core::distance_meters(
                    moonmap_core::Coordinate::new(lat, lon),
                    moonmap_core::Coordinate::new(new_lat, new_lon),
                );
                moved > MOVED_METERS || new_meters < meters - 1.0
            },
            _ => true,
        }
    }
}
