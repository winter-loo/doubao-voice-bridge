use std::ops::Range;

use gpui::{
    App, Bounds, ClipboardItem, Context, CursorStyle, Element, ElementId, ElementInputHandler,
    Entity, EntityInputHandler, FocusHandle, Focusable, GlobalElementId, LayoutId, MouseButton,
    MouseDownEvent, MouseMoveEvent, MouseUpEvent, PaintQuad, Pixels, Point, ScrollWheelEvent,
    SharedString, Style, TextAlign, TextRun, UTF16Selection, UnderlineStyle, Window, WrappedLine,
    actions, div, fill, point, prelude::*, px, relative, rgba, size,
};
use unicode_segmentation::UnicodeSegmentation as _;

actions!(
    transcript_input,
    [
        Backspace,
        Delete,
        Left,
        Right,
        SelectLeft,
        SelectRight,
        SelectAll,
        Home,
        End,
        Paste,
        Cut,
        Copy,
    ]
);

pub struct TranscriptInput {
    focus_handle: Option<FocusHandle>,
    content: SharedString,
    placeholder: SharedString,
    selected_range: Range<usize>,
    selection_reversed: bool,
    marked_range: Option<Range<usize>>,
    last_layout: Option<TranscriptLayout>,
    last_bounds: Option<Bounds<Pixels>>,
    is_selecting: bool,
    scroll_y: Pixels,
    follow_voice_end: bool,
}

impl TranscriptInput {
    pub fn new(cx: &mut Context<Self>) -> Self {
        #[cfg(debug_assertions)]
        let content = std::env::var("DOUBAO_TRANSCRIPT_TEST_TEXT").unwrap_or_default();
        #[cfg(not(debug_assertions))]
        let content = String::new();
        let cursor = content.len();
        Self {
            focus_handle: Some(cx.focus_handle()),
            content: content.into(),
            placeholder: "识别到的文字会显示在这里".into(),
            selected_range: cursor..cursor,
            selection_reversed: false,
            marked_range: None,
            last_layout: None,
            last_bounds: None,
            is_selecting: false,
            scroll_y: px(0.0),
            follow_voice_end: true,
        }
    }

    pub fn content(&self) -> &str {
        &self.content
    }

    pub fn set_voice_text(&mut self, text: &str) {
        #[cfg(debug_assertions)]
        if std::env::var_os("DOUBAO_TRANSCRIPT_TEST_TEXT").is_some() {
            return;
        }
        self.content = text.to_owned().into();
        self.selected_range = text.len()..text.len();
        self.selection_reversed = false;
        self.marked_range = None;
        self.follow_voice_end = true;
    }

    pub fn set_voice_text_and_notify(&mut self, text: &str, cx: &mut Context<Self>) {
        self.set_voice_text(text);
        cx.notify();
    }

    fn left(&mut self, _: &Left, _: &mut Window, cx: &mut Context<Self>) {
        if self.selected_range.is_empty() {
            self.move_to(self.previous_boundary(self.cursor_offset()), cx);
        } else {
            self.move_to(self.selected_range.start, cx);
        }
    }

    fn right(&mut self, _: &Right, _: &mut Window, cx: &mut Context<Self>) {
        if self.selected_range.is_empty() {
            self.move_to(self.next_boundary(self.cursor_offset()), cx);
        } else {
            self.move_to(self.selected_range.end, cx);
        }
    }

    fn select_left(&mut self, _: &SelectLeft, _: &mut Window, cx: &mut Context<Self>) {
        self.select_to(self.previous_boundary(self.cursor_offset()), cx);
    }

    fn select_right(&mut self, _: &SelectRight, _: &mut Window, cx: &mut Context<Self>) {
        self.select_to(self.next_boundary(self.cursor_offset()), cx);
    }

    fn select_all(&mut self, _: &SelectAll, _: &mut Window, cx: &mut Context<Self>) {
        self.move_to(0, cx);
        self.select_to(self.content.len(), cx);
    }

    fn home(&mut self, _: &Home, _: &mut Window, cx: &mut Context<Self>) {
        self.move_to(0, cx);
    }

    fn end(&mut self, _: &End, _: &mut Window, cx: &mut Context<Self>) {
        self.move_to(self.content.len(), cx);
    }

    fn backspace(&mut self, _: &Backspace, window: &mut Window, cx: &mut Context<Self>) {
        if self.selected_range.is_empty() {
            self.select_to(self.previous_boundary(self.cursor_offset()), cx);
        }
        self.replace_text_in_range(None, "", window, cx);
    }

    fn delete(&mut self, _: &Delete, window: &mut Window, cx: &mut Context<Self>) {
        if self.selected_range.is_empty() {
            self.select_to(self.next_boundary(self.cursor_offset()), cx);
        }
        self.replace_text_in_range(None, "", window, cx);
    }

    fn paste(&mut self, _: &Paste, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
            self.replace_text_in_range(None, &text.replace('\n', " "), window, cx);
        }
    }

    fn copy(&mut self, _: &Copy, _: &mut Window, cx: &mut Context<Self>) {
        if !self.selected_range.is_empty() {
            cx.write_to_clipboard(ClipboardItem::new_string(
                self.content[self.selected_range.clone()].to_string(),
            ));
        }
    }

    fn cut(&mut self, _: &Cut, window: &mut Window, cx: &mut Context<Self>) {
        if !self.selected_range.is_empty() {
            cx.write_to_clipboard(ClipboardItem::new_string(
                self.content[self.selected_range.clone()].to_string(),
            ));
            self.replace_text_in_range(None, "", window, cx);
        }
    }

    fn on_mouse_down(
        &mut self,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        window.focus(
            self.focus_handle
                .as_ref()
                .expect("transcript input is missing its focus handle"),
        );
        self.is_selecting = true;
        let offset = self.index_for_mouse_position(event.position);
        if event.modifiers.shift {
            self.select_to(offset, cx);
        } else {
            self.move_to(offset, cx);
        }
    }

    fn on_mouse_up(&mut self, _: &MouseUpEvent, _: &mut Window, _: &mut Context<Self>) {
        self.is_selecting = false;
    }

    fn on_mouse_move(&mut self, event: &MouseMoveEvent, _: &mut Window, cx: &mut Context<Self>) {
        if self.is_selecting {
            self.select_to(self.index_for_mouse_position(event.position), cx);
        }
    }

    fn move_to(&mut self, offset: usize, cx: &mut Context<Self>) {
        self.selected_range = offset..offset;
        self.selection_reversed = false;
        cx.notify();
    }

    fn select_to(&mut self, offset: usize, cx: &mut Context<Self>) {
        if self.selection_reversed {
            self.selected_range.start = offset;
        } else {
            self.selected_range.end = offset;
        }
        if self.selected_range.end < self.selected_range.start {
            self.selection_reversed = !self.selection_reversed;
            self.selected_range = self.selected_range.end..self.selected_range.start;
        }
        cx.notify();
    }

    fn cursor_offset(&self) -> usize {
        if self.selection_reversed {
            self.selected_range.start
        } else {
            self.selected_range.end
        }
    }

    fn index_for_mouse_position(&self, position: Point<Pixels>) -> usize {
        if self.content.is_empty() {
            return 0;
        }
        let (Some(bounds), Some(line)) = (&self.last_bounds, &self.last_layout) else {
            return 0;
        };
        if position.y < bounds.top() {
            return 0;
        }
        if position.y > bounds.bottom() {
            return self.content.len();
        }
        line.index_for_position(position, *bounds, self.scroll_y)
            .min(self.content.len())
    }

    fn on_scroll_wheel(
        &mut self,
        event: &ScrollWheelEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let delta = event.delta.pixel_delta(window.line_height());
        self.scroll_y = clamp_scroll_offset(
            self.scroll_y - delta.y,
            self.last_layout
                .as_ref()
                .map_or(px(0.0), |layout| layout.content_height),
            self.last_bounds
                .map_or(px(0.0), |bounds| bounds.size.height),
        );
        self.follow_voice_end = false;
        cx.notify();
    }

    fn offset_from_utf16(&self, offset: usize) -> usize {
        let mut utf8_offset = 0;
        let mut utf16_count = 0;
        for ch in self.content.chars() {
            if utf16_count >= offset {
                break;
            }
            utf16_count += ch.len_utf16();
            utf8_offset += ch.len_utf8();
        }
        utf8_offset
    }

    fn offset_to_utf16(&self, offset: usize) -> usize {
        let mut utf16_offset = 0;
        let mut utf8_count = 0;
        for ch in self.content.chars() {
            if utf8_count >= offset {
                break;
            }
            utf8_count += ch.len_utf8();
            utf16_offset += ch.len_utf16();
        }
        utf16_offset
    }

    fn range_to_utf16(&self, range: &Range<usize>) -> Range<usize> {
        self.offset_to_utf16(range.start)..self.offset_to_utf16(range.end)
    }

    fn range_from_utf16(&self, range: &Range<usize>) -> Range<usize> {
        self.offset_from_utf16(range.start)..self.offset_from_utf16(range.end)
    }

    fn previous_boundary(&self, offset: usize) -> usize {
        self.content
            .grapheme_indices(true)
            .rev()
            .find_map(|(index, _)| (index < offset).then_some(index))
            .unwrap_or(0)
    }

    fn next_boundary(&self, offset: usize) -> usize {
        self.content
            .grapheme_indices(true)
            .find_map(|(index, _)| (index > offset).then_some(index))
            .unwrap_or(self.content.len())
    }

    fn replace_range(&mut self, range: Range<usize>, text: &str) {
        self.content =
            (self.content[..range.start].to_owned() + text + &self.content[range.end..]).into();
        let cursor = range.start + text.len();
        self.selected_range = cursor..cursor;
        self.selection_reversed = false;
        self.marked_range = None;
    }

    #[cfg(test)]
    fn for_test(content: &str) -> Self {
        Self {
            focus_handle: None,
            content: content.to_owned().into(),
            placeholder: "".into(),
            selected_range: content.len()..content.len(),
            selection_reversed: false,
            marked_range: None,
            last_layout: None,
            last_bounds: None,
            is_selecting: false,
            scroll_y: px(0.0),
            follow_voice_end: true,
        }
    }

    #[cfg(test)]
    fn select_for_test(&mut self, range: Range<usize>) {
        self.selected_range = range;
    }

    #[cfg(test)]
    fn replace_for_test(&mut self, text: &str) {
        self.replace_range(self.selected_range.clone(), text);
    }

    #[cfg(test)]
    fn selection_for_test(&self) -> Range<usize> {
        self.selected_range.clone()
    }

    #[cfg(test)]
    fn previous_boundary_for_test(&self, offset: usize) -> usize {
        self.previous_boundary(offset)
    }

    #[cfg(test)]
    fn next_boundary_for_test(&self, offset: usize) -> usize {
        self.next_boundary(offset)
    }
}

impl EntityInputHandler for TranscriptInput {
    fn text_for_range(
        &mut self,
        range_utf16: Range<usize>,
        actual_range: &mut Option<Range<usize>>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<String> {
        let range = self.range_from_utf16(&range_utf16);
        actual_range.replace(self.range_to_utf16(&range));
        Some(self.content[range].to_string())
    }

    fn selected_text_range(
        &mut self,
        _: bool,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        Some(UTF16Selection {
            range: self.range_to_utf16(&self.selected_range),
            reversed: self.selection_reversed,
        })
    }

    fn marked_text_range(&self, _: &mut Window, _: &mut Context<Self>) -> Option<Range<usize>> {
        self.marked_range
            .as_ref()
            .map(|range| self.range_to_utf16(range))
    }

    fn unmark_text(&mut self, _: &mut Window, _: &mut Context<Self>) {
        self.marked_range = None;
    }

    fn replace_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let range = range_utf16
            .as_ref()
            .map(|range| self.range_from_utf16(range))
            .or(self.marked_range.clone())
            .unwrap_or(self.selected_range.clone());
        self.replace_range(range, new_text);
        cx.notify();
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        new_selected_range_utf16: Option<Range<usize>>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let range = range_utf16
            .as_ref()
            .map(|range| self.range_from_utf16(range))
            .or(self.marked_range.clone())
            .unwrap_or(self.selected_range.clone());
        let start = range.start;
        self.replace_range(range, new_text);
        self.marked_range =
            (!new_text.is_empty()).then_some(start..start.saturating_add(new_text.len()));
        if let Some(selection) = new_selected_range_utf16 {
            let selection = self.range_from_utf16(&selection);
            self.selected_range = start + selection.start..start + selection.end;
        }
        cx.notify();
    }

    fn bounds_for_range(
        &mut self,
        range_utf16: Range<usize>,
        bounds: Bounds<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        let layout = self.last_layout.as_ref()?;
        let range = self.range_from_utf16(&range_utf16);
        let start = layout.position_for_index(range.start, bounds, self.scroll_y)?;
        let end = layout.position_for_index(range.end, bounds, self.scroll_y)?;
        Some(Bounds::from_corners(
            start,
            point(end.x, end.y + layout.line_height),
        ))
    }

    fn character_index_for_point(
        &mut self,
        point: Point<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<usize> {
        let bounds = self.last_bounds?;
        let layout = self.last_layout.as_ref()?;
        let utf8_index = layout.index_for_position(point, bounds, self.scroll_y);
        Some(self.offset_to_utf16(utf8_index))
    }
}

#[derive(Clone)]
struct TranscriptLayout {
    lines: Vec<WrappedLine>,
    line_starts: Vec<usize>,
    line_height: Pixels,
    content_height: Pixels,
}

impl TranscriptLayout {
    fn position_for_index(
        &self,
        index: usize,
        bounds: Bounds<Pixels>,
        scroll_y: Pixels,
    ) -> Option<Point<Pixels>> {
        let mut y = bounds.top() - scroll_y;
        for (line, start) in self.lines.iter().zip(&self.line_starts) {
            let end = *start + line.len();
            if index <= end {
                return line
                    .position_for_index(index.saturating_sub(*start), self.line_height)
                    .map(|position| point(bounds.left() + position.x, y + position.y));
            }
            y += line.size(self.line_height).height;
        }
        Some(point(bounds.left(), y))
    }

    fn index_for_position(
        &self,
        position: Point<Pixels>,
        bounds: Bounds<Pixels>,
        scroll_y: Pixels,
    ) -> usize {
        let mut y = bounds.top() - scroll_y;
        for (line, start) in self.lines.iter().zip(&self.line_starts) {
            let height = line.size(self.line_height).height;
            if position.y <= y + height {
                let local = point(position.x - bounds.left(), position.y - y);
                return *start
                    + line
                        .closest_index_for_position(local, self.line_height)
                        .unwrap_or_else(|index| index);
            }
            y += height;
        }
        self.lines
            .last()
            .zip(self.line_starts.last())
            .map_or(0, |(line, start)| *start + line.len())
    }
}

fn clamp_scroll_offset(offset: Pixels, content_height: Pixels, viewport_height: Pixels) -> Pixels {
    offset
        .max(px(0.0))
        .min((content_height - viewport_height).max(px(0.0)))
}

struct TranscriptTextElement {
    input: Entity<TranscriptInput>,
}

struct PrepaintState {
    layout: TranscriptLayout,
    scroll_y: Pixels,
    cursor: Option<PaintQuad>,
    selection: Vec<PaintQuad>,
}

impl IntoElement for TranscriptTextElement {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for TranscriptTextElement {
    type RequestLayoutState = ();
    type PrepaintState = PrepaintState;

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&gpui::InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, ()) {
        let mut style = Style::default();
        style.size.width = relative(1.0).into();
        style.size.height = window.line_height().into();
        (window.request_layout(style, [], cx), ())
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&gpui::InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) -> PrepaintState {
        let input = self.input.read(cx);
        let content = input.content.clone();
        let display = if content.is_empty() {
            input.placeholder.clone()
        } else {
            content
        };
        let style = window.text_style();
        let color = if input.content.is_empty() {
            rgba(0xffffff66).into()
        } else {
            style.color
        };
        let base_run = TextRun {
            len: display.len(),
            font: style.font(),
            color,
            background_color: None,
            underline: None,
            strikethrough: None,
        };
        let runs = if let Some(marked) = &input.marked_range {
            vec![
                TextRun {
                    len: marked.start,
                    ..base_run.clone()
                },
                TextRun {
                    len: marked.end - marked.start,
                    underline: Some(UnderlineStyle {
                        color: Some(color),
                        thickness: px(1.0),
                        wavy: false,
                    }),
                    ..base_run.clone()
                },
                TextRun {
                    len: display.len() - marked.end,
                    ..base_run
                },
            ]
            .into_iter()
            .filter(|run| run.len > 0)
            .collect()
        } else {
            vec![base_run]
        };
        let font_size = style.font_size.to_pixels(window.rem_size());
        let lines = window
            .text_system()
            .shape_text(display, font_size, &runs, Some(bounds.size.width), None)
            .expect("failed to shape wrapped transcript text");
        let mut line_starts = Vec::with_capacity(lines.len());
        let mut start = 0;
        let mut content_height = px(0.0);
        for line in &lines {
            line_starts.push(start);
            start += line.len() + 1;
            content_height += line.size(window.line_height()).height;
        }
        let layout = TranscriptLayout {
            lines: lines.into_iter().collect(),
            line_starts,
            line_height: window.line_height(),
            content_height,
        };
        let scroll_y = if input.follow_voice_end {
            clamp_scroll_offset(content_height, content_height, bounds.size.height)
        } else {
            clamp_scroll_offset(input.scroll_y, content_height, bounds.size.height)
        };
        let cursor = input.selected_range.is_empty().then(|| {
            let position = layout
                .position_for_index(input.cursor_offset(), bounds, scroll_y)
                .unwrap_or(bounds.origin);
            fill(
                Bounds::new(position, size(px(1.5), window.line_height())),
                rgba(0x7dd3fcff),
            )
        });
        let mut selection = Vec::new();
        if !input.selected_range.is_empty()
            && let (Some(start), Some(end)) = (
                layout.position_for_index(input.selected_range.start, bounds, scroll_y),
                layout.position_for_index(input.selected_range.end, bounds, scroll_y),
            )
        {
            if start.y == end.y {
                selection.push(fill(
                    Bounds::from_corners(start, point(end.x, end.y + window.line_height())),
                    rgba(0x4f8cff55),
                ));
            } else {
                selection.push(fill(
                    Bounds::from_corners(
                        start,
                        point(bounds.right(), start.y + window.line_height()),
                    ),
                    rgba(0x4f8cff55),
                ));
                selection.push(fill(
                    Bounds::from_corners(
                        point(bounds.left(), end.y),
                        point(end.x, end.y + window.line_height()),
                    ),
                    rgba(0x4f8cff55),
                ));
            }
        }
        PrepaintState {
            layout,
            scroll_y,
            cursor,
            selection,
        }
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&gpui::InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut (),
        state: &mut PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        let focus = self
            .input
            .read(cx)
            .focus_handle
            .clone()
            .expect("transcript input is missing its focus handle");
        window.handle_input(
            &focus,
            ElementInputHandler::new(bounds, self.input.clone()),
            cx,
        );
        for selection in state.selection.drain(..) {
            window.paint_quad(selection);
        }
        let mut origin = point(bounds.left(), bounds.top() - state.scroll_y);
        for line in &state.layout.lines {
            line.paint(
                origin,
                window.line_height(),
                TextAlign::Left,
                Some(bounds),
                window,
                cx,
            )
            .expect("failed to paint transcript text");
            origin.y += line.size(window.line_height()).height;
        }
        if focus.is_focused(window)
            && let Some(cursor) = state.cursor.take()
        {
            window.paint_quad(cursor);
        }
        self.input.update(cx, |input, _| {
            input.last_layout = Some(state.layout.clone());
            input.last_bounds = Some(bounds);
            input.scroll_y = state.scroll_y;
        });
    }
}

impl Render for TranscriptInput {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .bg(rgba(0x00000001))
            .key_context("TranscriptInput")
            .track_focus(&self.focus_handle(cx))
            .cursor(CursorStyle::IBeam)
            .on_action(cx.listener(Self::backspace))
            .on_action(cx.listener(Self::delete))
            .on_action(cx.listener(Self::left))
            .on_action(cx.listener(Self::right))
            .on_action(cx.listener(Self::select_left))
            .on_action(cx.listener(Self::select_right))
            .on_action(cx.listener(Self::select_all))
            .on_action(cx.listener(Self::home))
            .on_action(cx.listener(Self::end))
            .on_action(cx.listener(Self::paste))
            .on_action(cx.listener(Self::cut))
            .on_action(cx.listener(Self::copy))
            .on_mouse_down(MouseButton::Left, cx.listener(Self::on_mouse_down))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .on_mouse_up_out(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .on_mouse_move(cx.listener(Self::on_mouse_move))
            .on_scroll_wheel(cx.listener(Self::on_scroll_wheel))
            .line_height(px(24.0))
            .text_size(px(14.0))
            .child(TranscriptTextElement { input: cx.entity() })
    }
}

impl Focusable for TranscriptInput {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle
            .clone()
            .expect("transcript input is missing its focus handle")
    }
}

#[cfg(test)]
mod tests {
    use super::{TranscriptInput, clamp_scroll_offset};
    use gpui::px;

    #[test]
    fn transcript_input_replaces_the_selected_utf8_text() {
        let mut input = TranscriptInput::for_test("你好 world");
        input.select_for_test(7..12);
        input.replace_for_test("世界");

        assert_eq!(input.content(), "你好 世界");
        assert_eq!(input.selection_for_test(), 13..13);
    }

    #[test]
    fn transcript_input_moves_across_whole_graphemes() {
        let input = TranscriptInput::for_test("a👨‍👩‍👧b");
        let family_end = "a👨‍👩‍👧".len();

        assert_eq!(input.next_boundary_for_test(1), family_end);
        assert_eq!(input.previous_boundary_for_test(family_end), 1);
    }

    #[test]
    fn voice_text_replaces_content_and_moves_the_cursor_to_the_end() {
        let mut input = TranscriptInput::for_test("manual edit");
        input.set_voice_text("语音结果");

        assert_eq!(input.content(), "语音结果");
        assert_eq!(
            input.selection_for_test(),
            "语音结果".len().."语音结果".len()
        );
    }

    #[test]
    fn vertical_scroll_is_clamped_to_wrapped_content() {
        assert_eq!(
            clamp_scroll_offset(px(-30.0), px(400.0), px(180.0)),
            px(0.0)
        );
        assert_eq!(
            clamp_scroll_offset(px(80.0), px(400.0), px(180.0)),
            px(80.0)
        );
        assert_eq!(
            clamp_scroll_offset(px(900.0), px(400.0), px(180.0)),
            px(220.0)
        );
    }
}
