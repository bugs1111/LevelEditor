use eframe::egui::{self, FontData, FontDefinitions, FontFamily};
use std::{
    path::PathBuf,
    sync::{Arc, LazyLock},
};

const BUNDLED_CJK: &[u8] = include_bytes!("../../assets/fonts/LevelEditorCJK-Regular.otf");
const LATIN: [&str; 3] = ["system-ui-regular", "system-ui-bold", "system-ui-light"];
const CHINESE: [&str; 3] = ["system-cjk-regular", "system-cjk-bold", "system-cjk-light"];
const BUNDLED_LATIN: [&str; 3] = ["ldtk-regular", "ldtk-bold", "ldtk-light"];
const REGULAR: usize = 0;
const BOLD: usize = 1;
const LIGHT: usize = 2;

static FONT_DEFINITIONS: LazyLock<[FontDefinitions; 2]> = LazyLock::new(|| {
    let mut fonts = FontDefinitions::default();
    for (name, bytes) in [
        (
            BUNDLED_LATIN[REGULAR],
            &include_bytes!("../../assets/fonts/RobotoCondensed-Regular.ttf")[..],
        ),
        (
            BUNDLED_LATIN[BOLD],
            &include_bytes!("../../assets/fonts/RobotoCondensed-Bold.ttf")[..],
        ),
        (
            BUNDLED_LATIN[LIGHT],
            &include_bytes!("../../assets/fonts/RobotoCondensed-Light.ttf")[..],
        ),
        ("bundled-cjk", BUNDLED_CJK),
    ] {
        fonts
            .font_data
            .insert(name.into(), Arc::new(FontData::from_static(bytes)));
    }
    for (chinese, names) in [(false, LATIN), (true, CHINESE)] {
        for (weight, name) in names.into_iter().enumerate() {
            if let Some(font) = system_font(chinese, weight) {
                fonts.font_data.insert(name.into(), Arc::new(font));
            }
        }
    }
    let symbols = if cfg!(target_os = "macos") {
        PathBuf::from("/System/Library/Fonts/Apple Symbols.ttf")
    } else if cfg!(target_os = "windows") {
        windows_font_directory().join("seguisym.ttf")
    } else {
        PathBuf::from("/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf")
    };
    if let Ok(bytes) = std::fs::read(symbols)
        && ttf_parser::Face::parse(&bytes, 0).is_ok()
    {
        fonts.font_data.insert(
            "system-symbols".into(),
            Arc::new(FontData::from_owned(bytes)),
        );
        for family in fonts.families.values_mut() {
            family.push("system-symbols".into());
        }
    }
    [
        with_priorities(fonts.clone(), false),
        with_priorities(fonts, true),
    ]
});

pub(crate) fn set_font_language(ctx: &egui::Context, chinese: bool) {
    ctx.set_fonts(FONT_DEFINITIONS[usize::from(chinese)].clone());
}

#[cfg(test)]
pub(crate) fn use_reference_layout_fonts(ctx: &egui::Context) {
    // These tests compare exact coordinates measured with the original UI typography.
    // Keep the reference font metrics local to those tests.
    static REFERENCE: LazyLock<FontDefinitions> = LazyLock::new(|| {
        let mut fonts = FONT_DEFINITIONS[0].clone();
        let arial = if cfg!(target_os = "macos") {
            PathBuf::from("/System/Library/Fonts/Supplemental")
        } else {
            windows_font_directory()
        };
        for (weight, file) in [(REGULAR, "Arial.ttf"), (BOLD, "Arial Bold.ttf")] {
            let file = if cfg!(target_os = "windows") && weight == BOLD {
                "arialbd.ttf"
            } else {
                file
            };
            if let Ok(bytes) = std::fs::read(arial.join(file)) {
                fonts
                    .font_data
                    .insert(LATIN[weight].into(), Arc::new(FontData::from_owned(bytes)));
            }
        }
        for (name, weight) in [
            ("heading", BOLD),
            ("panel-title", LIGHT),
            ("panel-header", REGULAR),
        ] {
            let sequence = fonts
                .families
                .get_mut(&FontFamily::Name(name.into()))
                .unwrap();
            sequence.retain(|entry| entry != BUNDLED_LATIN[weight]);
            sequence.insert(0, BUNDLED_LATIN[weight].into());
        }
        fonts
    });
    ctx.set_fonts(REFERENCE.clone());
}

fn with_priorities(mut fonts: FontDefinitions, chinese: bool) -> FontDefinitions {
    let defaults = fonts.families[&FontFamily::Proportional].clone();
    for (family, weight) in [
        (FontFamily::Proportional, REGULAR),
        (FontFamily::Name("heading".into()), BOLD),
        (FontFamily::Name("body-bold".into()), BOLD),
        (FontFamily::Name("panel-title".into()), LIGHT),
        (FontFamily::Name("panel-header".into()), REGULAR),
    ] {
        let preferred = if chinese {
            [
                CHINESE[weight],
                CHINESE[REGULAR],
                "bundled-cjk",
                LATIN[weight],
                LATIN[REGULAR],
            ]
        } else {
            [
                LATIN[weight],
                BUNDLED_LATIN[weight],
                LATIN[REGULAR],
                CHINESE[REGULAR],
                "bundled-cjk",
            ]
        };
        let mut names = Vec::new();
        for name in preferred
            .into_iter()
            .map(str::to_owned)
            .chain(defaults.iter().cloned())
        {
            if fonts.font_data.contains_key(&name) && !names.contains(&name) {
                names.push(name);
            }
        }
        fonts.families.insert(family, names);
    }
    let mono = fonts.families.entry(FontFamily::Monospace).or_default();
    if fonts.font_data.contains_key(CHINESE[REGULAR]) {
        mono.push(CHINESE[REGULAR].into());
    }
    mono.push("bundled-cjk".into());
    fonts
}

#[cfg(any(target_os = "linux", target_os = "windows"))]
fn load_font(
    path: &std::path::Path,
    indices: impl IntoIterator<Item = u32>,
    chinese: bool,
    weight: usize,
) -> Option<FontData> {
    let bytes = std::fs::read(path).ok()?;
    let index = indices.into_iter().find(|&index| {
        ttf_parser::Face::parse(&bytes, index)
            .is_ok_and(|face| suitable_face(&face, chinese, weight))
    })?;
    Some(FontData {
        index,
        ..FontData::from_owned(bytes)
    })
}

fn suitable_face(face: &ttf_parser::Face<'_>, chinese: bool, weight: usize) -> bool {
    let actual = face.weight().to_number();
    let weight_matches = match weight {
        BOLD => actual >= 600,
        LIGHT => actual < 400,
        _ => (350..=500).contains(&actual),
    };
    weight_matches
        && !face.is_italic()
        && (' '..='~').all(|ch| face.glyph_index(ch).is_some())
        && (!chinese
            || "中文了解导出限制"
                .chars()
                .all(|ch| face.glyph_index(ch).is_some()))
}

#[cfg(target_os = "linux")]
fn system_font(chinese: bool, weight: usize) -> Option<FontData> {
    use std::path::Path;
    let lang = if chinese { "zh-cn" } else { "en" };
    let weight_name = ["regular", "bold", "light"][weight];
    if let Ok(output) = std::process::Command::new("fc-match")
        .args([
            "-f",
            "%{file}\t%{index}",
            &format!("sans-serif:lang={lang}:weight={weight_name}"),
        ])
        .output()
        && output.status.success()
        && let Ok(result) = std::str::from_utf8(&output.stdout)
        && let Some((path, index)) = result.rsplit_once('\t')
        && let Ok(index) = index.parse::<u32>()
        && let Some(font) = load_font(Path::new(path), [index & 0xffff], chinese, weight)
    {
        return Some(font);
    }
    let (path, index) = if chinese {
        (
            [
                "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
                "/usr/share/fonts/opentype/noto/NotoSansCJK-Bold.ttc",
                "/usr/share/fonts/opentype/noto/NotoSansCJK-Light.ttc",
            ][weight],
            2,
        )
    } else {
        (
            [
                "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
                "/usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf",
                "/usr/share/fonts/truetype/dejavu/DejaVuSans-ExtraLight.ttf",
            ][weight],
            0,
        )
    };
    load_font(Path::new(path), [index], chinese, weight)
}

fn windows_font_directory() -> PathBuf {
    PathBuf::from(std::env::var_os("WINDIR").unwrap_or_else(|| "C:/Windows".into())).join("Fonts")
}

#[cfg(target_os = "windows")]
fn system_font(chinese: bool, weight: usize) -> Option<FontData> {
    let files = if chinese {
        ["msyh.ttc", "msyhbd.ttc", "msyhl.ttc"]
    } else {
        ["segoeui.ttf", "segoeuib.ttf", "segoeuil.ttf"]
    };
    // Microsoft YaHei UI is the second face in the YaHei collection.
    load_font(
        &windows_font_directory().join(files[weight]),
        [u32::from(chinese), 0],
        chinese,
        weight,
    )
}

#[cfg(target_os = "macos")]
fn system_font(chinese: bool, weight: usize) -> Option<FontData> {
    use core_foundation::{
        base::{CFType, CFTypeRef, TCFType},
        string::{CFString, CFStringRef},
        url::CFURL,
    };

    #[link(name = "CoreText", kind = "framework")]
    unsafe extern "C" {
        fn CTFontCreateWithName(
            name: CFStringRef,
            size: f64,
            matrix: *const std::ffi::c_void,
        ) -> CFTypeRef;
        fn CTFontCreateUIFontForLanguage(kind: u32, size: f64, language: CFStringRef) -> CFTypeRef;
        fn CTFontCopyAttribute(font: CFTypeRef, attribute: CFStringRef) -> CFTypeRef;
        static kCTFontURLAttribute: CFStringRef;
    }

    let from_font = |font: CFTypeRef, name: Option<&str>| {
        if font.is_null() {
            return None;
        }
        let font = unsafe { CFType::wrap_under_create_rule(font) };
        let url = unsafe { CTFontCopyAttribute(font.as_CFTypeRef(), kCTFontURLAttribute) };
        if url.is_null() {
            return None;
        }
        let url = unsafe { CFURL::wrap_under_create_rule(url.cast()) };
        let bytes = std::fs::read(url.to_path()?).ok()?;
        let index = (0..ttf_parser::fonts_in_collection(&bytes).unwrap_or(1)).find(|&index| {
            ttf_parser::Face::parse(&bytes, index).is_ok_and(|face| {
                suitable_face(&face, chinese, weight)
                    && name.is_none_or(|name| {
                        face.names().into_iter().any(|entry| {
                            entry.name_id == ttf_parser::name_id::POST_SCRIPT_NAME
                                && entry.to_string().as_deref() == Some(name)
                        })
                    })
            })
        })?;
        Some(FontData {
            index,
            ..FontData::from_owned(bytes)
        })
    };

    if !chinese && weight != LIGHT {
        const SYSTEM: u32 = 2;
        const EMPHASIZED_SYSTEM: u32 = 3;
        let kind = if weight == BOLD {
            EMPHASIZED_SYSTEM
        } else {
            SYSTEM
        };
        let language = CFString::new("en");
        let font =
            unsafe { CTFontCreateUIFontForLanguage(kind, 0., language.as_concrete_TypeRef()) };
        if let Some(data) = from_font(font, None) {
            return Some(data);
        }
    }

    // Egui reads a font file's default variation. Use a named static face when
    // the native UI font's requested weight cannot be read from that file.
    let name = if chinese {
        [
            "PingFangSC-Regular",
            "PingFangSC-Semibold",
            "PingFangSC-Light",
        ][weight]
    } else {
        ["HelveticaNeue", "HelveticaNeue-Bold", "HelveticaNeue-Light"][weight]
    };
    let font_name = CFString::new(name);
    // Core Text resolves relocated system fonts as well as fonts in /System/Library/Fonts.
    let font =
        unsafe { CTFontCreateWithName(font_name.as_concrete_TypeRef(), 0., std::ptr::null()) };
    from_font(font, Some(name))
}

#[cfg(not(any(target_os = "linux", target_os = "windows", target_os = "macos")))]
fn system_font(_: bool, _: usize) -> Option<FontData> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::FontId;

    #[test]
    fn chinese_roles_use_one_face_for_han_and_latin_and_keep_monospace() {
        let [english, chinese] = &*FONT_DEFINITIONS;
        for (family, weight) in [
            (FontFamily::Proportional, REGULAR),
            (FontFamily::Name("heading".into()), BOLD),
            (FontFamily::Name("body-bold".into()), BOLD),
            (FontFamily::Name("panel-title".into()), LIGHT),
            (FontFamily::Name("panel-header".into()), REGULAR),
        ] {
            let expected = [CHINESE[weight], CHINESE[REGULAR], "bundled-cjk"]
                .into_iter()
                .find(|name| chinese.font_data.contains_key(*name))
                .unwrap();
            assert_eq!(chinese.families[&family][0], expected);
            let data = &chinese.font_data[expected];
            let face = ttf_parser::Face::parse(&data.font, data.index).unwrap();
            assert!(
                "了解 Tiled TMX 导出限制"
                    .chars()
                    .all(|ch| face.glyph_index(ch).is_some()),
                "mixed script must fit in {expected} for {family:?}"
            );
            let expected = if english.font_data.contains_key(LATIN[weight]) {
                LATIN[weight]
            } else {
                BUNDLED_LATIN[weight]
            };
            assert_eq!(english.families[&family][0], expected);
        }
        assert_eq!(
            english.families[&FontFamily::Monospace][0],
            chinese.families[&FontFamily::Monospace][0],
        );
    }

    #[test]
    fn bundled_cjk_covers_the_simplified_chinese_interface_without_system_fonts() {
        let ctx = egui::Context::default();
        let mut fonts = FontDefinitions::empty();
        fonts.font_data.insert(
            "bundled-cjk".into(),
            Arc::new(FontData::from_static(BUNDLED_CJK)),
        );
        fonts
            .families
            .insert(FontFamily::Proportional, vec!["bundled-cjk".into()]);
        ctx.set_fonts(fonts);
        let messages: std::collections::HashMap<String, String> =
            serde_json::from_str(include_str!("../../assets/locales/zh-CN.json")).unwrap();
        let glyphs: std::collections::BTreeSet<_> = messages
            .values()
            .flat_map(|message| message.chars())
            .filter(|ch| matches!(*ch as u32, 0x3000..=0x30ff | 0x3400..=0x9fff | 0xff00..=0xffef))
            .collect();
        let _ = ctx.run(egui::RawInput::default(), |ctx| {
            ctx.fonts_mut(|fonts| {
                for ch in &glyphs {
                    assert!(
                        fonts.has_glyph(&FontId::proportional(14.), *ch),
                        "missing {ch}"
                    );
                }
            });
        });
    }
}
