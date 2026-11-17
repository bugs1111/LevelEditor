//! Native counterpart of the source TextEditor dialog.
use super::*;
use egui::text::{CCursor, CCursorRange, LayoutJob};
use std::{path::Path, sync::OnceLock};
use syntect::{
    easy::HighlightLines,
    highlighting::{Color, StyleModifier, Theme, ThemeItem},
    parsing::{SyntaxDefinition, SyntaxSet},
    util::LinesWithEndings,
};

#[derive(Clone)]
struct Editor {
    owner: egui::Id,
    original: String,
    draft: String,
    title: String,
    mode: String,
    read_only: bool,
    focus: u8,
    validate: bool,
}
fn editor_id() -> egui::Id {
    egui::Id::new("source-text-editor")
}
pub(super) fn is_open(ctx: &egui::Context) -> bool {
    ctx.data_mut(|d| d.get_temp::<Editor>(editor_id()))
        .is_some()
}
pub(super) fn request_validate(ctx: &egui::Context) {
    if let Some(mut editor) = ctx.data_mut(|d| d.get_temp::<Editor>(editor_id())) {
        editor.validate = true;
        ctx.data_mut(|d| d.insert_temp(editor_id(), editor));
    }
}
fn open(
    ctx: &egui::Context,
    owner: egui::Id,
    source: &str,
    title: &str,
    mode: &str,
    read_only: bool,
) {
    ctx.data_mut(|d| {
        d.insert_temp(
            editor_id(),
            Editor {
                owner,
                original: source.into(),
                draft: source.into(),
                title: title.into(),
                mode: mode.into(),
                read_only,
                focus: 2,
                validate: false,
            },
        )
    });
}

fn syntax_token(mode: &str) -> &str {
    match mode {
        "LangJson" => "json",
        "LangXml" => "xml",
        "LangMarkdown" => "md",
        "LangRuby" => "rb",
        "LangPython" => "py",
        "LangHaxe" => "hx",
        "LangJS" => "js",
        "LangLua" => "lua",
        "LangC" => "c",
        "LangLog" => "cfg",
        _ => "txt",
    }
}
fn syntax_set() -> &'static SyntaxSet {
    static SET: OnceLock<SyntaxSet> = OnceLock::new();
    SET.get_or_init(|| {
        let mut builder = SyntaxSet::load_defaults_newlines().into_builder();
        for source in [
            include_str!("../../assets/syntax/Haxe.sublime-syntax"),
            include_str!("../../assets/syntax/TTCNConfig.sublime-syntax"),
        ] {
            builder.add(
                SyntaxDefinition::load_from_str(source, true, None)
                    .expect("bundled source language syntax"),
            );
        }
        builder.build()
    })
}
fn lucario() -> &'static Theme {
    static THEME: OnceLock<Theme> = OnceLock::new();
    THEME.get_or_init(|| {
        let color = |rgb: u32| Color {
            r: (rgb >> 16) as u8,
            g: (rgb >> 8) as u8,
            b: rgb as u8,
            a: 255,
        };
        let mut theme = Theme::default();
        theme.settings.foreground = Some(color(0xf8f8f2));
        theme.settings.background = Some(color(0x11202f));
        for (scope, rgb) in [
            ("comment", 0x6fb7f7),
            ("string", 0xe6db74),
            ("constant.numeric", 0xca94ff),
            ("constant.language", 0xbd93f9),
            ("keyword", 0xff6541),
            ("keyword.operator", 0x66d9ef),
            ("entity.name", 0xaac01a),
            ("entity.name.tag", 0xff6541),
            ("entity.other.attribute-name", 0x66d9ef),
            ("support.function", 0xaac01a),
            ("support.type", 0xffb86c),
            ("storage.type", 0xffb86c),
            ("markup.heading", 0xaac01a),
        ] {
            theme.scopes.push(ThemeItem {
                scope: scope.parse().expect("static syntax scope"),
                style: StyleModifier {
                    foreground: Some(color(rgb)),
                    ..Default::default()
                },
            });
        }
        theme
    })
}
fn highlight(source: &str, mode: &str, wrap: f32) -> LayoutJob {
    let syntaxes = syntax_set();
    let syntax = syntaxes
        .find_syntax_by_token(syntax_token(mode))
        .unwrap_or_else(|| syntaxes.find_syntax_plain_text());
    let mut highlighter = HighlightLines::new(syntax, lucario());
    let mut job = LayoutJob::default();
    job.wrap.max_width = wrap;
    for line in LinesWithEndings::from(source) {
        match highlighter.highlight_line(line, syntaxes) {
            Ok(parts) => {
                for (style, text) in parts {
                    job.append(
                        text,
                        0.,
                        egui::TextFormat {
                            font_id: FontId::monospace(13.),
                            color: Color32::from_rgb(
                                style.foreground.r,
                                style.foreground.g,
                                style.foreground.b,
                            ),
                            ..Default::default()
                        },
                    );
                }
            }
            Err(_) => job.append(
                line,
                0.,
                egui::TextFormat::simple(FontId::monospace(13.), Color32::from_rgb(248, 248, 242)),
            ),
        }
    }
    job
}

#[derive(Clone)]
struct HighlightCache {
    source: String,
    mode: String,
    wrap: u32,
    job: LayoutJob,
}
fn cached_highlight(ui: &egui::Ui, id: egui::Id, source: &str, mode: &str, wrap: f32) -> LayoutJob {
    let key = id.with("highlight");
    if let Some(cache) = ui.ctx().data_mut(|d| d.get_temp::<HighlightCache>(key))
        && cache.source == source
        && cache.mode == mode
        && cache.wrap == wrap.to_bits()
    {
        return cache.job;
    }
    let job = highlight(source, mode, wrap);
    ui.ctx().data_mut(|d| {
        d.insert_temp(
            key,
            HighlightCache {
                source: source.into(),
                mode: mode.into(),
                wrap: wrap.to_bits(),
                job: job.clone(),
            },
        )
    });
    job
}

// Source closebrackets/closetag addons: surround selections, skip an existing
// closing delimiter, and keep the caret between newly inserted pairs.
fn paired_input(
    draft: &str,
    range: CCursorRange,
    typed: &str,
    xml: bool,
) -> Option<(String, usize)> {
    let range = range.as_sorted_char_range();
    let chars: Vec<char> = draft.chars().collect();
    let c = typed.chars().next()?;
    if typed.chars().count() != 1 {
        return None;
    }
    let next = chars.get(range.end).copied();
    if range.is_empty() && matches!(c, ')' | ']' | '}' | '\'' | '"') && next == Some(c) {
        return Some((String::new(), range.start + 1));
    }
    if !xml {
        let close = match c {
            '(' => ')',
            '[' => ']',
            '{' => '}',
            '\'' => '\'',
            '"' => '"',
            _ => return None,
        };
        if matches!(c, '\'' | '"')
            && range.is_empty()
            && (chars
                .get(range.start.wrapping_sub(1))
                .is_some_and(|c| c.is_alphanumeric())
                || next.is_some_and(|c| c.is_alphanumeric()))
        {
            return None;
        }
        if !range.is_empty() {
            return Some((
                format!(
                    "{c}{}{close}",
                    chars[range.clone()].iter().collect::<String>()
                ),
                range.end + 1,
            ));
        }
        if next.is_none_or(|c| c.is_whitespace() || ")]},;:".contains(c)) {
            return Some((format!("{c}{close}"), range.start + 1));
        }
    } else if c == '>' && range.is_empty() {
        let prefix: String = chars[..range.start].iter().collect();
        let tag = prefix.rsplit_once('<')?.1;
        if tag.starts_with('/')
            || tag.starts_with('!')
            || tag.starts_with('?')
            || tag.ends_with('/')
        {
            return None;
        }
        let name = tag.split_whitespace().next()?;
        if !name.is_empty()
            && name
                .chars()
                .all(|c| c.is_alphanumeric() || "_:-".contains(c))
        {
            return Some((format!("></{name}>"), range.start + 1));
        }
    }
    None
}

struct CodeBuffer<'a> {
    text: &'a mut String,
    xml: bool,
    read_only: bool,
    selected: Option<(usize, String)>,
    typed: std::collections::VecDeque<bool>,
}
impl egui::TextBuffer for CodeBuffer<'_> {
    fn type_id(&self) -> std::any::TypeId {
        std::any::TypeId::of::<CodeBuffer<'static>>()
    }
    fn is_mutable(&self) -> bool {
        !self.read_only
    }
    fn as_str(&self) -> &str {
        self.text
    }
    fn insert_text(&mut self, text: &str, index: usize) -> usize {
        egui::TextBuffer::insert_text(self.text, text, index)
    }
    fn delete_char_range(&mut self, range: std::ops::Range<usize>) {
        egui::TextBuffer::delete_char_range(self.text, range);
    }
    fn delete_selected(&mut self, range: &CCursorRange) -> CCursor {
        let range = range.as_sorted_char_range();
        self.selected = (!range.is_empty()).then(|| {
            (
                range.start,
                self.text
                    .chars()
                    .skip(range.start)
                    .take(range.len())
                    .collect(),
            )
        });
        egui::TextBuffer::delete_char_range(self.text, range.clone());
        CCursor::new(range.start)
    }
    fn insert_text_at(&mut self, cursor: &mut CCursor, text: &str, limit: usize) {
        let typed = self.typed.pop_front().unwrap_or(false);
        let selected = self.selected.take();
        if typed {
            let range = CCursorRange::one(*cursor);
            if let Some((replacement, next)) = paired_input(self.text, range, text, self.xml) {
                if let Some((start, selected)) = selected
                    && replacement.chars().count() == 2
                {
                    let mut pair = replacement.chars();
                    let replacement = format!(
                        "{}{}{}",
                        pair.next().unwrap(),
                        selected,
                        pair.next().unwrap()
                    );
                    egui::TextBuffer::insert_text(self.text, &replacement, start);
                    cursor.index = start + replacement.chars().count() - 1;
                    return;
                }
                egui::TextBuffer::insert_text(self.text, &replacement, cursor.index);
                cursor.index = next;
                return;
            }
        }
        egui::TextBuffer::insert_text_at(self.text, cursor, text, limit);
    }
    fn replace_with(&mut self, text: &str) {
        self.text.clear();
        self.text.push_str(text);
    }
}
fn insert_kinds(ctx: &egui::Context) -> std::collections::VecDeque<bool> {
    ctx.input(|i| {
        i.events
            .iter()
            .filter_map(|event| match event {
                egui::Event::Text(s) if !s.is_empty() && s != "\n" && s != "\r" => Some(true),
                egui::Event::Paste(s) if !s.is_empty() => Some(false),
                egui::Event::Key {
                    key: Key::Enter | Key::Tab,
                    pressed: true,
                    ..
                } => Some(false),
                egui::Event::Ime(egui::ImeEvent::Preedit(s) | egui::ImeEvent::Commit(s))
                    if !s.is_empty() =>
                {
                    Some(false)
                }
                _ => None,
            })
            .collect()
    })
}
fn show(ui: &mut egui::Ui, owner: egui::Id) -> Option<String> {
    let ctx = ui.ctx().clone();
    let mut editor = ctx.data_mut(|d| d.get_temp::<Editor>(editor_id()))?;
    if editor.owner != owner {
        return None;
    }
    let id = owner.with("code-input");
    let mut close = editor.validate;
    let modal = egui::Modal::new(owner.with("dialog")).show(&ctx, |ui| {
        ui.set_width((ctx.content_rect().width() * 0.8).clamp(320., 1000.));
        ui.heading(&editor.title);
        ui.separator();
        let mut layouter = |ui: &egui::Ui, buffer: &dyn egui::TextBuffer, wrap: f32| {
            let job = cached_highlight(ui, id, buffer.as_str(), &editor.mode, wrap);
            ui.fonts_mut(|f| f.layout_job(job))
        };
        let output = egui::Frame::new()
            .fill(Color32::from_rgb(17, 32, 47))
            .inner_margin(8.)
            .show(ui, |ui| {
                egui::ScrollArea::vertical()
                    .max_height(ctx.content_rect().height() * 0.7)
                    .show(ui, |ui| {
                        ui.visuals_mut().selection.bg_fill = Color32::from_rgb(36, 52, 67);
                        ui.visuals_mut().text_cursor.stroke =
                            Stroke::new(1_f32, Color32::from_rgb(230, 200, 69));
                        let gutter = 34.;
                        let top = ui.cursor().min;
                        ui.add_space(0.);
                        ui.horizontal_top(|ui| {
                            ui.add_space(gutter);
                            let mut buffer = CodeBuffer {
                                text: &mut editor.draft,
                                xml: editor.mode == "LangXml",
                                read_only: editor.read_only,
                                selected: None,
                                typed: insert_kinds(&ctx),
                            };
                            let out = egui::TextEdit::multiline(&mut buffer)
                                .id(id)
                                .font(FontId::monospace(13.))
                                .desired_width(ui.available_width())
                                .desired_rows(15)
                                .frame(false)
                                .lock_focus(true)
                                .layouter(&mut layouter)
                                .show(ui);
                            // Number logical lines, including wrapped rows at their actual galley y.
                            let mut number = 1;
                            for (index, row) in out.galley.rows.iter().enumerate() {
                                if index == 0 || out.galley.rows[index - 1].ends_with_newline {
                                    ui.painter().text(
                                        egui::pos2(
                                            top.x + gutter - 8.,
                                            out.galley_pos.y + row.pos.y,
                                        ),
                                        Align2::RIGHT_TOP,
                                        number.to_string(),
                                        FontId::monospace(13.),
                                        Color32::from_rgb(95, 131, 153),
                                    );
                                    number += 1;
                                }
                            }
                            out
                        })
                        .inner
                    })
                    .inner
            })
            .inner;
        if editor.focus > 0 {
            output.response.request_focus();
            editor.focus -= 1;
            ctx.request_repaint();
        }
        ui.horizontal(|ui| {
            close |= panels::source_controls::button(ui, "Close").clicked();
            if !editor.read_only && panels::source_controls::small_button(ui, "Clear").clicked() {
                editor.draft.clear();
                close = true;
            }
        });
    });
    close |= modal.backdrop_response.is_pointer_button_down_on();
    close |= ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, Key::Escape));
    if close {
        ctx.memory_mut(|m| m.surrender_focus(id));
        ctx.data_mut(|d| d.remove::<Editor>(editor_id()));
        (!editor.read_only && editor.draft != editor.original).then_some(editor.draft)
    } else {
        ctx.data_mut(|d| d.insert_temp(editor_id(), editor));
        None
    }
}
pub(super) fn value_editor(
    ui: &mut egui::Ui,
    key: impl std::hash::Hash,
    source: &str,
    title: &str,
    mode: &str,
    read_only: bool,
) -> Option<String> {
    let owner = ui.make_persistent_id(key);
    let mut preview: String = source.chars().take(256).collect();
    if source.chars().count() > 256 {
        preview.push_str("[...]");
    }
    if preview.is_empty() {
        preview = "(empty string)".into();
    }
    if ui
        .add_sized([240., 50.], egui::Button::new(preview).wrap())
        .clicked()
    {
        open(ui.ctx(), owner, source, title, mode, read_only);
    }
    show(ui, owner)
}
fn file_mode(path: &Path) -> &'static str {
    match path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase()
        .as_str()
    {
        "txt" | "cfg" => "",
        "xml" | "html" | "xhtml" | "jhtml" | "tpl" | "rss" | "svg" => "LangXml",
        "hx" | "hscript" => "LangHaxe",
        "py" => "LangPython",
        "rb" | "rhtml" => "LangRuby",
        "lua" => "LangLua",
        "md" => "LangMarkdown",
        "json" | "ldtk" | "ldtkl" => "LangJson",
        "cs" | "csx" | "c" | "cpp" | "c++" | "cp" | "cc" | "h" => "LangC",
        _ => "LangJS",
    }
}
fn read_external(path: &Path) -> std::io::Result<String> {
    let bytes = std::fs::read(path)?;
    if bytes.iter().take(256).any(|&b| b > 127 || b == 0)
        || bytes.starts_with(b"GIF8")
        || bytes.starts_with(b"BM")
    {
        return Err(std::io::Error::other(
            "This file cannot be opened in the text editor.",
        ));
    }
    String::from_utf8(bytes).map_err(std::io::Error::other)
}
pub(super) fn external_file_editor(ui: &mut egui::Ui, path: &Path) {
    let owner = ui.make_persistent_id(("external-text", path));
    if panels::source_controls::small_button(ui, "Edit file").clicked() {
        match read_external(path) {
            Ok(source) => open(
                ui.ctx(),
                owner,
                &source,
                &path.file_name().unwrap_or_default().to_string_lossy(),
                file_mode(path),
                false,
            ),
            Err(error) => ui
                .ctx()
                .data_mut(|d| d.insert_temp(owner.with("error"), error.to_string())),
        }
    }
    if let Some(content) = show(ui, owner)
        && let Err(error) = std::fs::write(path, content)
    {
        ui.ctx()
            .data_mut(|d| d.insert_temp(owner.with("error"), error.to_string()));
    }
    if let Some(error) = ui
        .ctx()
        .data_mut(|d| d.get_temp::<String>(owner.with("error")))
    {
        ui.colored_label(Color32::LIGHT_RED, error);
    }
}

pub(super) fn log_editor(ui: &mut egui::Ui, path: &Path) {
    let owner = ui.make_persistent_id("app-log-viewer");
    let clicked = ui
        .scope(|ui| {
            ui.spacing_mut().button_padding = egui::vec2(4., 2.);
            panels::source_controls::add_sized_button(
                ui,
                [30., 30.],
                egui::Button::image(
                    egui::Image::new(egui::include_image!("../../assets/icons/edit.svg"))
                        .fit_to_exact_size(Vec2::splat(16.)),
                ),
            )
            .on_hover_text("View logs")
            .clicked()
        })
        .inner;
    if clicked {
        app_log::flush();
        let contents = std::fs::read_to_string(path).unwrap_or_default();
        open(ui.ctx(), owner, &contents, "LDtk logs", "LangLog", true);
    }
    let _ = show(ui, owner);
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn every_source_language_has_a_native_grammar_and_preserves_content() {
        for mode in [
            "LangJson",
            "LangXml",
            "LangMarkdown",
            "LangRuby",
            "LangPython",
            "LangHaxe",
            "LangJS",
            "LangLua",
            "LangC",
            "LangLog",
        ] {
            assert!(
                syntax_set()
                    .find_syntax_by_token(syntax_token(mode))
                    .is_some(),
                "{mode}"
            );
            let source = "hello \"世界\"\n123\n";
            assert_eq!(highlight(source, mode, 300.).text, source);
        }
        let job = highlight("{\"number\":123}", "LangJson", 300.);
        assert!(
            job.sections
                .iter()
                .any(|s| s.format.color == Color32::from_rgb(202, 148, 255))
        );
    }
    #[test]
    fn source_pairs_selection_skip_existing_and_xml_close() {
        let one = |n| CCursorRange::one(CCursor::new(n));
        assert_eq!(paired_input("", one(0), "(", false), Some(("()".into(), 1)));
        assert_eq!(paired_input("()", one(1), ")", false), Some(("".into(), 2)));
        assert_eq!(
            paired_input(
                "猫",
                CCursorRange::two(CCursor::new(0), CCursor::new(1)),
                "[",
                false
            ),
            Some(("[猫]".into(), 2))
        );
        assert_eq!(
            paired_input("<node x=\"v\"", one(11), ">", true),
            Some(("></node>".into(), 12))
        );
        assert!(paired_input("<node/", one(6), ">", true).is_none());
    }
    #[test]
    fn external_editor_rejects_binary_and_guesses_source_extensions() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("hello.json");
        std::fs::write(&file, "{\"text\":\"hello\"}").unwrap();
        assert_eq!(file_mode(&file), "LangJson");
        assert_eq!(read_external(&file).unwrap(), "{\"text\":\"hello\"}");
        std::fs::write(&file, b"\x89PNG\0").unwrap();
        assert!(read_external(&file).is_err());
    }
}
