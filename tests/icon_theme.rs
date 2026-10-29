//! Seam 4, icon themes: names are looked up in the user's icon themes under
//! `$XDG_DATA_HOME/icons` as well as the system ones.
//!
//! A suite of its own because it sets `$XDG_DATA_HOME` for the whole process;
//! keep it to one test so nothing reads the environment while it changes.

#[path = "support/icons.rs"]
mod support;

use lucent::icon::Icons;
use lucent::image::Image;
use support::{RED, RED_SVG, SIZE, inspect, write_png};

#[test]
fn an_icon_name_is_found_in_a_user_icon_theme() {
    let data_home = tempfile::tempdir().unwrap();
    let theme = data_home.path().join("icons/Lucent-Test");
    std::fs::create_dir_all(theme.join("48x48/apps")).unwrap();
    std::fs::create_dir_all(theme.join("scalable/apps")).unwrap();
    std::fs::write(
        theme.join("index.theme"),
        "[Icon Theme]\nName=Lucent-Test\nDirectories=48x48/apps,scalable/apps\n\n\
         [48x48/apps]\nSize=48\nType=Fixed\n\n\
         [scalable/apps]\nSize=16\nMinSize=8\nMaxSize=24\nType=Scalable\n",
    )
    .unwrap();
    write_png(&theme.join("48x48/apps/lucent-png.png"));
    std::fs::write(theme.join("scalable/apps/lucent-svg.svg"), RED_SVG).unwrap();
    // SAFETY: the only test in this process, so no other thread reads the
    // environment while it is written.
    unsafe { std::env::set_var("XDG_DATA_HOME", data_home.path()) };
    let icons = Icons::new(SIZE, "Lucent-Test");

    let png = icons.load(&Image::Icon("lucent-png".into()));
    // Only offered for smaller sizes, so found by the any-size fallback.
    let svg = icons.load(&Image::Icon("lucent-svg".into()));

    assert_eq!(inspect(png.unwrap()), ((48, 24), RED));
    assert_eq!(inspect(svg.unwrap()), ((48, 48), RED));
}
