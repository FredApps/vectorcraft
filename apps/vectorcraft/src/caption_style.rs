//! Linux: which desktop the custom title bar's caption buttons look like with the Classic OG colour
//! scheme, and which buttons go on which side (`vectorcraft_ui_egui::titlebar::CaptionStyle`), as in
//! PhotoCraft (its `caption_style.rs`). GNOME gets libadwaita's buttons in its `button-layout` order
//! (from `gsettings`), KDE (and SteamOS's desktop mode) Breeze's in KWin's order (`kwinrc`); other desktops get plain round buttons, or Adwaita's
//! or Breeze's when their GTK theme is one of those, in GTK's `gtk-decoration-layout` order. Read
//! once, when the app starts.
//!
//! `VECTORCRAFT_CAPTION_STYLE` (`windows`, `adwaita`, `breeze`, `generic`) and
//! `VECTORCRAFT_CAPTION_LAYOUT` (a GTK layout such as `close,minimize:`) override what is found.

#[cfg(target_os = "linux")]
use vectorcraft_ui_egui::titlebar::{CaptionFamily, CaptionStyle};

#[cfg(target_os = "linux")]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Desktop {
    Gnome,
    Kde,
    Other,
}

/// The desktop, from the first of its variables that is set: `ubuntu:GNOME`, `Budgie:GNOME` and
/// the like count as GNOME.
#[cfg(target_os = "linux")]
fn desktop(var: &dyn Fn(&str) -> Option<String>) -> Desktop {
    for key in ["XDG_CURRENT_DESKTOP", "XDG_SESSION_DESKTOP", "DESKTOP_SESSION"] {
        let Some(v) = var(key).map(|v| v.to_ascii_lowercase()).filter(|v| !v.trim().is_empty()) else { continue };
        return if v.contains("gnome") {
            Desktop::Gnome
        } else if v.contains("kde") || v.contains("plasma") {
            Desktop::Kde
        } else {
            Desktop::Other
        };
    }
    if var("KDE_FULL_SESSION").is_some() {
        Desktop::Kde
    } else if var("GNOME_DESKTOP_SESSION_ID").is_some() {
        Desktop::Gnome
    } else {
        Desktop::Other
    }
}

/// GNOME's apps are libadwaita's whatever the GTK theme, and KWin draws Breeze; elsewhere the GTK
/// theme says (Adwaita, adw-gtk3, Yaru, Breeze), and any other theme gets the plain buttons.
#[cfg(target_os = "linux")]
fn family(desktop: Desktop, gtk_theme: Option<&str>) -> CaptionFamily {
    match desktop {
        Desktop::Gnome => CaptionFamily::Adwaita,
        Desktop::Kde => CaptionFamily::Breeze,
        Desktop::Other => {
            let theme = gtk_theme.unwrap_or_default().trim().to_ascii_lowercase();
            if ["adwaita", "adw-gtk3", "yaru"].iter().any(|p| theme.starts_with(p)) {
                CaptionFamily::Adwaita
            } else if theme.starts_with("breeze") {
                CaptionFamily::Breeze
            } else {
                CaptionFamily::Generic
            }
        }
    }
}

/// The value of `key` in `[section]` of an INI-style file (GTK's `settings.ini`, KDE's `kwinrc`).
#[cfg(target_os = "linux")]
fn ini_value(text: &str, section: &str, key: &str) -> Option<String> {
    let mut inside = false;
    for line in text.lines().map(str::trim) {
        if let Some(name) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
            inside = name == section;
        } else if inside
            && let Some((k, v)) = line.split_once('=')
            && k.trim() == key
        {
            return Some(v.trim().to_string());
        }
    }
    None
}

/// KWin's button order (`[org.kde.kdecoration2]` `ButtonsOnLeft` / `ButtonsOnRight`, one letter a
/// button: `I` minimize, `A` maximize, `X` close), as a GTK layout. Missing keys keep KWin's
/// defaults: the window menu on the left, minimize, maximize and close on the right.
#[cfg(target_os = "linux")]
fn kwin_layout(kwinrc: &str) -> String {
    let side = |key: &str, default: &str| {
        let letters = ini_value(kwinrc, "org.kde.kdecoration2", key).unwrap_or_else(|| default.to_string());
        letters
            .chars()
            .filter_map(|c| match c {
                'I' => Some("minimize"),
                'A' => Some("maximize"),
                'X' => Some("close"),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join(",")
    };
    format!("{}:{}", side("ButtonsOnLeft", "MS"), side("ButtonsOnRight", "HIAX"))
}

/// What the style is worked out from.
#[cfg(target_os = "linux")]
#[derive(Clone, Default)]
struct Found {
    gtk_theme: Option<String>,
    /// A GTK decoration layout.
    layout: Option<String>,
}

/// The style from what was found.
#[cfg(target_os = "linux")]
fn resolve(desktop: Desktop, found: &Found, var: &dyn Fn(&str) -> Option<String>) -> CaptionStyle {
    let family =
        var("VECTORCRAFT_CAPTION_STYLE").and_then(|s| CaptionFamily::parse(&s)).unwrap_or_else(|| family(desktop, found.gtk_theme.as_deref()));
    // GNOME's own default shows Close only; GTK's shows all three.
    let default = if desktop == Desktop::Gnome { "appmenu:close" } else { "menu:minimize,maximize,close" };
    let layout = var("VECTORCRAFT_CAPTION_LAYOUT").or_else(|| found.layout.clone()).unwrap_or_else(|| default.to_string());
    CaptionStyle::from_gtk_layout(family, &layout)
}

/// A settings file, if it is there and small (a settings file is a few kilobytes).
#[cfg(target_os = "linux")]
fn read_text(path: &std::path::Path) -> Option<String> {
    use std::io::Read as _;
    const MAX: u64 = 1 << 20;
    let mut text = String::new();
    std::fs::File::open(path).ok()?.take(MAX).read_to_string(&mut text).ok()?;
    Some(text)
}

/// The GTK theme and the button layout from the desktop's files: `kwinrc` on KDE, GTK 3's
/// `settings.ini` elsewhere (on GNOME, its own setting replaces it: [`gnome_button_layout`]).
#[cfg(target_os = "linux")]
fn from_files(desktop: Desktop, var: &dyn Fn(&str) -> Option<String>) -> Found {
    let config = var("XDG_CONFIG_HOME")
        .filter(|d| std::path::Path::new(d).is_absolute())
        .map(std::path::PathBuf::from)
        .or_else(|| var("HOME").map(|h| std::path::Path::new(&h).join(".config")));
    let Some(config) = config else { return Found::default() };
    let gtk = read_text(&config.join("gtk-3.0").join("settings.ini")).unwrap_or_default();
    let gtk_theme = ini_value(&gtk, "Settings", "gtk-theme-name");
    let layout = if desktop == Desktop::Kde {
        Some(kwin_layout(&read_text(&config.join("kwinrc")).unwrap_or_default()))
    } else {
        ini_value(&gtk, "Settings", "gtk-decoration-layout")
    };
    Found { gtk_theme, layout }
}

/// GNOME's `org.gnome.desktop.wm.preferences` `button-layout`, from `gsettings` (none when it is
/// missing, as in some sandboxes, or fails).
#[cfg(target_os = "linux")]
fn gnome_button_layout() -> Option<String> {
    let out = std::process::Command::new("gsettings").args(["get", "org.gnome.desktop.wm.preferences", "button-layout"]).output().ok()?;
    if !out.status.success() {
        return None;
    }
    gsettings_string(&String::from_utf8_lossy(&out.stdout))
}

/// A string as `gsettings get` prints it (`'appmenu:close'`, or `@ms 'appmenu:close'` for a maybe
/// type), without the quotes; none when empty.
#[cfg(target_os = "linux")]
fn gsettings_string(printed: &str) -> Option<String> {
    let s = printed.trim();
    let s = s.strip_prefix("@ms ").unwrap_or(s).trim();
    let s = s.strip_prefix('\'').and_then(|s| s.strip_suffix('\'')).unwrap_or(s).trim();
    (!s.is_empty()).then(|| s.to_string())
}

/// The desktop's caption buttons, for `Services::caption_style`; `None` off Linux (Windows keeps its
/// own buttons, macOS its traffic lights).
pub fn detect() -> Option<vectorcraft_ui_egui::titlebar::CaptionStyle> {
    #[cfg(target_os = "linux")]
    {
        let var = |k: &str| std::env::var(k).ok();
        let desktop = desktop(&var);
        let mut found = from_files(desktop, &var);
        // GNOME keeps its button layout in its settings (what Settings and Tweaks change), not in
        // GTK's settings.ini.
        if desktop == Desktop::Gnome
            && let Some(layout) = gnome_button_layout()
        {
            found.layout = Some(layout);
        }
        Some(resolve(desktop, &found, &var))
    }
    #[cfg(not(target_os = "linux"))]
    {
        None
    }
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;
    use vectorcraft_ui_egui::titlebar::Caption::{Close, Maximize, Minimize};

    fn env(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> + use<> {
        let pairs: Vec<(String, String)> = pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect();
        move |k| pairs.iter().find(|(key, _)| key == k).map(|(_, v)| v.clone())
    }

    #[test]
    fn gsettings_strings_lose_their_quotes() {
        assert_eq!(gsettings_string("'appmenu:minimize,maximize,close'\n").as_deref(), Some("appmenu:minimize,maximize,close"));
        assert_eq!(gsettings_string("@ms 'close,minimize:'").as_deref(), Some("close,minimize:"));
        assert_eq!(gsettings_string("''\n"), None);
        assert_eq!(gsettings_string(""), None);
        // GNOME's layout then sets the buttons (Minimize and Maximize turned on in Tweaks).
        let found = Found { gtk_theme: None, layout: gsettings_string("'appmenu:minimize,maximize,close'") };
        let s = resolve(Desktop::Gnome, &found, &env(&[]));
        assert_eq!((s.family, s.left, s.right), (CaptionFamily::Adwaita, vec![], vec![Minimize, Maximize, Close]));
    }

    #[test]
    fn the_desktop_comes_from_the_first_variable_set() {
        assert_eq!(desktop(&env(&[("XDG_CURRENT_DESKTOP", "ubuntu:GNOME")])), Desktop::Gnome);
        assert_eq!(desktop(&env(&[("XDG_CURRENT_DESKTOP", "KDE")])), Desktop::Kde);
        assert_eq!(desktop(&env(&[("XDG_CURRENT_DESKTOP", "X-Cinnamon"), ("KDE_FULL_SESSION", "true")])), Desktop::Other);
        assert_eq!(desktop(&env(&[("XDG_CURRENT_DESKTOP", " "), ("DESKTOP_SESSION", "plasma")])), Desktop::Kde);
        assert_eq!(desktop(&env(&[("KDE_FULL_SESSION", "true")])), Desktop::Kde);
        assert_eq!(desktop(&env(&[("GNOME_DESKTOP_SESSION_ID", "this-is-deprecated")])), Desktop::Gnome);
        assert_eq!(desktop(&env(&[])), Desktop::Other);
    }

    #[test]
    fn the_family_follows_the_desktop_then_the_gtk_theme() {
        assert_eq!(family(Desktop::Gnome, Some("Arc")), CaptionFamily::Adwaita);
        assert_eq!(family(Desktop::Kde, None), CaptionFamily::Breeze);
        assert_eq!(family(Desktop::Other, Some("adw-gtk3-dark")), CaptionFamily::Adwaita);
        assert_eq!(family(Desktop::Other, Some("Yaru-blue")), CaptionFamily::Adwaita);
        assert_eq!(family(Desktop::Other, Some("Breeze-Dark")), CaptionFamily::Breeze);
        assert_eq!(family(Desktop::Other, Some("Mint-Y")), CaptionFamily::Generic);
        assert_eq!(family(Desktop::Other, None), CaptionFamily::Generic);
    }

    #[test]
    fn kwin_button_letters_become_a_gtk_layout() {
        assert_eq!(kwin_layout(""), ":minimize,maximize,close", "KWin's defaults");
        let rc = "[Desktops]\nButtonsOnLeft=X\n[org.kde.kdecoration2]\nButtonsOnLeft=XIA\nButtonsOnRight=\nlibrary=org.kde.breeze\n";
        assert_eq!(kwin_layout(rc), "close,minimize,maximize:");
        assert_eq!(ini_value(rc, "org.kde.kdecoration2", "library").as_deref(), Some("org.kde.breeze"));
        assert_eq!(ini_value(rc, "nope", "library"), None);
        // Garbage is just no buttons on that side, never a panic.
        assert_eq!(kwin_layout("[org.kde.kdecoration2]\nButtonsOnRight=ÿ€=X\n"), ":close");
    }

    #[test]
    fn layouts_and_overrides_resolve_to_a_style() {
        let none = env(&[]);
        let gnome = resolve(Desktop::Gnome, &Found::default(), &none);
        assert_eq!((gnome.family, gnome.left, gnome.right), (CaptionFamily::Adwaita, vec![], vec![Close]), "GNOME's default layout");
        // KDE, SteamOS's desktop mode among them: Breeze's buttons in KWin's default order.
        let kde = resolve(Desktop::Kde, &Found { layout: Some(kwin_layout("")), ..Found::default() }, &none);
        assert_eq!((kde.family, kde.left, kde.right), (CaptionFamily::Breeze, vec![], vec![Minimize, Maximize, Close]));
        let found = Found { gtk_theme: Some("Mint-Y".into()), layout: Some("close:".into()) };
        let other = resolve(Desktop::Other, &found, &none);
        assert_eq!((other.family, other.left, other.right), (CaptionFamily::Generic, vec![Close], vec![]));
        let forced = env(&[("VECTORCRAFT_CAPTION_STYLE", "Breeze"), ("VECTORCRAFT_CAPTION_LAYOUT", "minimize,close:maximize")]);
        let s = resolve(Desktop::Gnome, &found, &forced);
        assert_eq!((s.family, s.left, s.right), (CaptionFamily::Breeze, vec![Minimize, Close], vec![Maximize]));
        // An unknown override name falls back to what was found.
        assert_eq!(resolve(Desktop::Kde, &found, &env(&[("VECTORCRAFT_CAPTION_STYLE", "aqua")])).family, CaptionFamily::Breeze);
    }

    #[test]
    fn settings_files_are_read_from_the_config_dir() {
        let dir = std::env::temp_dir().join(format!("vectorcraft-caption-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("gtk-3.0")).unwrap();
        std::fs::write(dir.join("gtk-3.0/settings.ini"), "[Settings]\ngtk-theme-name=Yaru\ngtk-decoration-layout=close,maximize:\n").unwrap();
        std::fs::write(dir.join("kwinrc"), "[org.kde.kdecoration2]\nButtonsOnRight=AX\n").unwrap();
        let var = env(&[("XDG_CONFIG_HOME", dir.to_str().unwrap())]);
        let other = from_files(Desktop::Other, &var);
        assert_eq!((other.gtk_theme.as_deref(), other.layout.as_deref()), (Some("Yaru"), Some("close,maximize:")));
        let kde = from_files(Desktop::Kde, &var);
        assert_eq!(kde.layout.as_deref(), Some(":maximize,close"));
        // A relative XDG_CONFIG_HOME is ignored, as the XDG spec says; no HOME: nothing found.
        assert_eq!(from_files(Desktop::Other, &env(&[("XDG_CONFIG_HOME", "rel")])).layout, None);
        let _ = std::fs::remove_dir_all(dir);
    }
}
