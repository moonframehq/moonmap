//! `just demo` and the screenshots: a fixed position and a library of
//! public San Francisco places, so a demo never shows where its user is.

use moonmap_core::{Category, Coordinate, Library, Place, Recent};

/// Where a demo says the user is: Union Square.
pub fn here() -> Coordinate {
    Coordinate::new(37.7880, -122.4075)
}

fn place(id: &str, name: &str, subtitle: &str, lat: f64, lon: f64, category: Category) -> Place {
    Place {
        id: format!("demo:{id}"),
        name: name.into(),
        subtitle: subtitle.into(),
        lat,
        lon,
        category,
    }
}

pub fn library() -> Library {
    let mut library = Library::default();
    library.preferences.location = here();
    library.preferences.center = here();
    library.preferences.located = true;
    let now = 1_793_700_000;

    let ferry = place(
        "ferry",
        "Ferry Building",
        "1 Ferry Building, San Francisco",
        37.7955,
        -122.3937,
        Category::Store,
    );
    let bridge = place(
        "golden-gate",
        "Golden Gate Bridge",
        "Golden Gate Bridge, San Francisco",
        37.8078,
        -122.4750,
        Category::Pin,
    );
    library.toggle_pin(ferry, now - 50);
    library.toggle_pin(bridge, now - 40);

    let palace = place(
        "palace",
        "Palace of Fine Arts",
        "3601 Lyon St, San Francisco",
        37.8029,
        -122.4484,
        Category::Pin,
    );
    let coit = place(
        "coit",
        "Coit Tower",
        "1 Telegraph Hill Blvd, San Francisco",
        37.8024,
        -122.4058,
        Category::Pin,
    );
    library.toggle_save(coit, now - 30);
    library.toggle_save(palace.clone(), now - 20);

    let parks = library.add_guide("San Francisco Parks");
    for (id, name, subtitle, lat, lon) in [
        (
            "golden-gate-park",
            "Golden Gate Park",
            "501 Stanyan St, San Francisco",
            37.7694,
            -122.4862,
        ),
        (
            "dolores",
            "Mission Dolores Park",
            "Dolores St & 19th St, San Francisco",
            37.7596,
            -122.4269,
        ),
        (
            "crissy",
            "Crissy Field",
            "1199 East Beach, San Francisco",
            37.8039,
            -122.4640,
        ),
    ] {
        library.toggle_in_guide(
            parks,
            place(id, name, subtitle, lat, lon, Category::Park),
            now - 10,
        );
    }

    let union_square = place(
        "union-square",
        "Union Square",
        "333 Post St, San Francisco",
        here().lat,
        here().lon,
        Category::Pin,
    );
    library.save_route(union_square, palace, "car", now - 5);

    let recents = [
        place(
            "sfo",
            "San Francisco International Airport",
            "San Francisco",
            37.6213,
            -122.3790,
            Category::Airport,
        ),
        place(
            "exploratorium",
            "Exploratorium",
            "Pier 15, San Francisco",
            37.8017,
            -122.3973,
            Category::Pin,
        ),
        place(
            "city-lights",
            "City Lights Bookstore",
            "261 Columbus Ave, San Francisco",
            37.7976,
            -122.4066,
            Category::Store,
        ),
        place(
            "tartine",
            "Tartine Bakery",
            "600 Guerrero St, San Francisco",
            37.7614,
            -122.4241,
            Category::Cafe,
        ),
        place(
            "sfmoma",
            "SFMOMA",
            "151 3rd St, San Francisco",
            37.7857,
            -122.4011,
            Category::Pin,
        ),
        place(
            "oracle-park",
            "Oracle Park",
            "24 Willie Mays Plaza, San Francisco",
            37.7786,
            -122.3893,
            Category::Pin,
        ),
        place(
            "fairmont",
            "Fairmont San Francisco",
            "950 Mason St, San Francisco",
            37.7924,
            -122.4102,
            Category::Hotel,
        ),
    ];
    for (index, place) in recents.into_iter().enumerate() {
        let at = now - 1000 + index as u64 * 10;
        if index == 2 {
            library.remember(Recent::Search {
                query: "coffee".into(),
                near: "North Beach".into(),
                at,
            });
        }
        if index == 4 {
            library.remember(Recent::Search {
                query: "bookstore".into(),
                near: "Mission District".into(),
                at,
            });
        }
        library.visit(place.clone(), at);
        library.remember(Recent::Place { place, at });
    }
    library
}
