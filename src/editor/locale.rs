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
            _ => Self::English,
        }
    }

    fn translate(self, text: &str) -> &str {
        if self == Self::SimplifiedChinese
            && let Some(translated) = CHINESE.get(text)
        {
            return translated;
        }
        text
    }
}

static CHINESE: LazyLock<HashMap<String, String>> = LazyLock::new(|| {
    serde_json::from_str(include_str!("../../assets/locales/zh-CN.json"))
        .expect("bundled Simplified Chinese translations")
});

fn locale_id() -> egui::Id {
    egui::Id::new("interface-language")
}

pub(super) fn set(ctx: &egui::Context, language: Language) {
    ctx.data_mut(|data| data.insert_temp(locale_id(), language));
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
    let mut rest = tr_ctx(ctx, text);
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
