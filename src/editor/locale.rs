use eframe::egui;
use std::{collections::HashMap, sync::LazyLock};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum Language {
    #[default]
    English,
    SimplifiedChinese,
}

impl Language {
    pub fn from_code(code: Option<&str>) -> Self {
        match code {
            Some("zh-CN") => Self::SimplifiedChinese,
            Some("system") => *SYSTEM_LANGUAGE,
            _ => Self::English,
        }
    }

    pub(super) fn translate(self, text: &str) -> &str {
        if self == Self::SimplifiedChinese
            && let Some(translated) = CHINESE.get(text)
        {
            return translated;
        }
        text
    }

    pub(super) fn format(self, text: &str, values: &[(&str, &str)]) -> String {
        format_translated(self.translate(text), values)
    }
}

static SYSTEM_LANGUAGE: LazyLock<Language> = LazyLock::new(|| {
    system_locale()
        .as_deref()
        .map_or(Language::English, language_for_locale)
});

fn language_for_locale(locale: &str) -> Language {
    let base = locale.trim().split(['.', '@']).next().unwrap_or_default();
    let mut parts = base.split(['-', '_']);
    if parts
        .next()
        .is_some_and(|part| part.eq_ignore_ascii_case("zh"))
        && parts.next().is_none_or(|part| {
            ["Hans", "CN", "SG", "MY"]
                .iter()
                .any(|code| part.eq_ignore_ascii_case(code))
        })
    {
        Language::SimplifiedChinese
    } else {
        Language::English
    }
}

#[cfg(target_os = "macos")]
fn system_locale() -> Option<String> {
    objc2::rc::autoreleasepool(|_| {
        objc2_foundation::NSLocale::preferredLanguages()
            .firstObject()
            .map(|language| language.to_string())
    })
}

#[cfg(target_os = "windows")]
fn system_locale() -> Option<String> {
    use windows_sys::Win32::Globalization::{GetUserPreferredUILanguages, MUI_LANGUAGE_NAME};
    let mut count = 0;
    let mut len = 0;
    // A null buffer queries the size in UTF-16 code units, including the terminators.
    if unsafe {
        GetUserPreferredUILanguages(
            MUI_LANGUAGE_NAME,
            &mut count,
            std::ptr::null_mut(),
            &mut len,
        )
    } == 0
        || len == 0
    {
        return None;
    }
    let mut buffer = vec![0u16; len as usize];
    // The buffer is writable and has capacity for the length supplied to Windows.
    if unsafe {
        GetUserPreferredUILanguages(MUI_LANGUAGE_NAME, &mut count, buffer.as_mut_ptr(), &mut len)
    } == 0
        || count == 0
    {
        return None;
    }
    let first = buffer.split(|&unit| unit == 0).next()?;
    String::from_utf16(first).ok()
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn system_locale() -> Option<String> {
    locale_from_environment(|key| std::env::var(key).ok())
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn locale_from_environment(read: impl Fn(&str) -> Option<String>) -> Option<String> {
    let locale = ["LC_ALL", "LC_MESSAGES", "LANG"]
        .into_iter()
        .find_map(|key| read(key).filter(|value| !value.trim().is_empty()));
    if matches!(locale.as_deref(), Some("C" | "POSIX")) {
        return locale;
    }
    read("LANGUAGE")
        .as_deref()
        .and_then(|value| {
            value
                .split(':')
                .map(str::trim)
                .find(|part| !part.is_empty())
        })
        .map(str::to_owned)
        .or(locale)
}

static CHINESE: LazyLock<HashMap<String, String>> = LazyLock::new(|| {
    serde_json::from_str(include_str!("../../assets/locales/zh-CN.json"))
        .expect("bundled Simplified Chinese translations")
});

fn locale_id() -> egui::Id {
    egui::Id::new("interface-language")
}

pub(super) fn set(ctx: &egui::Context, language: Language) {
    let changed = ctx.data_mut(|data| {
        let previous = data.get_temp::<Language>(locale_id()).unwrap_or_default();
        data.insert_temp(locale_id(), language);
        previous != language
    });
    if changed {
        crate::theme::set_font_language(ctx, language == Language::SimplifiedChinese);
    }
}

pub(super) fn tr_ctx<'a>(ctx: &egui::Context, text: &'a str) -> &'a str {
    ctx.data(|data| data.get_temp::<Language>(locale_id()))
        .unwrap_or_default()
        .translate(text)
}

pub(super) fn tr<'a>(ui: &egui::Ui, text: &'a str) -> &'a str {
    tr_ctx(ui.ctx(), text)
}

pub(super) fn format_ctx(ctx: &egui::Context, text: &str, values: &[(&str, &str)]) -> String {
    format_translated(tr_ctx(ctx, text), values)
}

fn format_translated(mut rest: &str, values: &[(&str, &str)]) -> String {
    let mut result = String::with_capacity(rest.len());
    while let Some((prefix, placeholder)) = rest.split_once('{') {
        let Some((name, suffix)) = placeholder.split_once('}') else {
            break;
        };
        result.push_str(prefix);
        if let Some((_, value)) = values.iter().find(|(key, _)| *key == name) {
            result.push_str(value);
        } else {
            result.push('{');
            result.push_str(name);
            result.push('}');
        }
        rest = suffix;
    }
    result.push_str(rest);
    result
}

pub(super) fn format(ui: &egui::Ui, text: &str, values: &[(&str, &str)]) -> String {
    format_ctx(ui.ctx(), text, values)
}

pub(super) fn widget(ui: &egui::Ui, text: egui::WidgetText) -> egui::WidgetText {
    if let egui::WidgetText::Text(plain) = &text {
        let translated = tr(ui, plain);
        if translated != plain {
            return translated.into();
        }
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn system_locale_matches_simplified_chinese_and_falls_back_to_english() {
        for locale in ["zh", "zh-Hans-HK", "zh_CN.UTF-8", "ZH-sg", "zh-MY"] {
            assert_eq!(
                language_for_locale(locale),
                Language::SimplifiedChinese,
                "{locale}"
            );
        }
        for locale in [
            "en-US",
            "fr-FR",
            "ja_JP.UTF-8",
            "zh-Hant-CN",
            "zh_TW",
            "zh-HK",
            "",
        ] {
            assert_eq!(language_for_locale(locale), Language::English, "{locale}");
        }
    }

    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    #[test]
    fn environment_uses_the_primary_message_language_not_the_formats_locale() {
        let read = |vars: &[(&str, &str)]| {
            locale_from_environment(|key| {
                vars.iter()
                    .find_map(|(name, value)| (*name == key).then(|| (*value).to_owned()))
            })
        };
        assert_eq!(
            read(&[("LANG", "fr_FR.UTF-8"), ("LC_MESSAGES", "zh_CN.UTF-8")]).as_deref(),
            Some("zh_CN.UTF-8")
        );
        assert_eq!(
            read(&[("LANG", "zh_CN.UTF-8"), ("LANGUAGE", "fr:zh_CN")]).as_deref(),
            Some("fr")
        );
        assert_eq!(
            read(&[("LC_ALL", "C"), ("LANGUAGE", "zh_CN")]).as_deref(),
            Some("C")
        );
        assert_eq!(read(&[]), None);
    }

    #[test]
    fn changing_language_switches_font_priority_on_the_next_frame() {
        let ctx = egui::Context::default();
        crate::theme::configure(&ctx);
        let primary = || {
            let mut name = String::new();
            let _ = ctx.run(egui::RawInput::default(), |ctx| {
                name = ctx.fonts(|fonts| {
                    fonts.definitions().families[&egui::FontFamily::Proportional][0].clone()
                });
            });
            name
        };
        let english = primary();
        set(&ctx, Language::SimplifiedChinese);
        assert!(matches!(
            primary().as_str(),
            "system-cjk-regular" | "bundled-cjk"
        ));
        set(&ctx, Language::English);
        assert_eq!(primary(), english);
    }
}
