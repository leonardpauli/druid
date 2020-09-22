// Copyright 2020 The Druid Authors.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! Text attributes and spans.

use std::ops::Range;

use crate::piet::{Color, FontFamily, FontStyle, FontWeight, TextAttribute as PietTextAttribute};
use crate::{Env, FontDescriptor, KeyOrValue};

/// A collection of spans of attributes of various kinds.
#[derive(Debug, Clone, Default)]
pub struct AttributeSpans {
    family: SpanSet<FontFamily>,
    size: SpanSet<KeyOrValue<f64>>,
    weight: SpanSet<FontWeight>,
    fg_color: SpanSet<KeyOrValue<Color>>,
    style: SpanSet<FontStyle>,
    underline: SpanSet<bool>,
    font_descriptor: SpanSet<KeyOrValue<FontDescriptor>>,
}

/// A set of spans for a given attribute.
///
/// Invariant: the spans are sorted and non-overlapping.
#[derive(Debug, Clone)]
struct SpanSet<T> {
    spans: Vec<Span<T>>,
}

/// An attribute and a range.
///
/// This is used to represent text attributes of various kinds,
/// with the range representing a region of some text buffer.
#[derive(Debug, Clone, PartialEq)]
struct Span<T> {
    range: Range<usize>,
    attr: T,
}

/// Attributes that can be applied to text.
///
/// Where possible, attributes are [`KeyOrValue`] types; this means you
/// can use items defined in the [`theme`] *or* concrete types, where appropriate.
///
/// The easiest way to construct these attributes is via the various constructor
/// methods, such as [`Attribute::size`] or [`Attribute::foregound_color`].
///
/// # Examples
///
/// ```
/// use druid::text::Attribute;
/// use druid::{theme, Color};
///
/// let font = Attribute::font_descriptor(theme::UI_FONT);
/// let font_size = Attribute::size(32.0);
/// let explicit_color = Attribute::foreground_color(Color::BLACK);
/// let theme_color = Attribute::foreground_color(theme::SELECTION_COLOR);
/// ```
///
/// [`KeyOrValue`]: ../enum.KeyOrValue.html
/// [`theme`]: ../theme
/// [`Attribute::size`]: #method.size
/// [`Attribute::foreground_color`]: #method.foreground_color
#[derive(Debug, Clone)]
pub enum Attribute {
    /// The font family.
    FontFamily(FontFamily),
    /// The font size, in points.
    FontSize(KeyOrValue<f64>),
    /// The [`FontWeight`](struct.FontWeight.html).
    Weight(FontWeight),
    /// The foreground color of the text.
    ForegroundColor(KeyOrValue<Color>),
    /// The [`FontStyle`]; either regular or italic.
    ///
    /// [`FontStyle`]: enum.FontStyle.html
    Style(FontStyle),
    /// Underline.
    Underline(bool),
    /// A [`FontDescriptor`](struct.FontDescriptor.html).
    Descriptor(KeyOrValue<FontDescriptor>),
}

impl AttributeSpans {
    /// Add a new [`Attribute`] over the provided [`Range`].
    pub fn add(&mut self, range: Range<usize>, attr: Attribute) {
        match attr {
            Attribute::FontFamily(attr) => self.family.add(Span::new(range, attr)),
            Attribute::FontSize(attr) => self.size.add(Span::new(range, attr)),
            Attribute::Weight(attr) => self.weight.add(Span::new(range, attr)),
            Attribute::ForegroundColor(attr) => self.fg_color.add(Span::new(range, attr)),
            Attribute::Style(attr) => self.style.add(Span::new(range, attr)),
            Attribute::Underline(attr) => self.underline.add(Span::new(range, attr)),
            Attribute::Descriptor(attr) => self.font_descriptor.add(Span::new(range, attr)),
        }
    }

    pub(crate) fn to_piet_attrs(&self, env: &Env) -> Vec<(Range<usize>, PietTextAttribute)> {
        let mut items = Vec::new();
        for Span { range, attr } in self.font_descriptor.iter() {
            let font = attr.resolve(env);
            items.push((range.clone(), PietTextAttribute::FontFamily(font.family)));
            items.push((range.clone(), PietTextAttribute::FontSize(font.size)));
            items.push((range.clone(), PietTextAttribute::Weight(font.weight)));
            items.push((range.clone(), PietTextAttribute::Style(font.style)));
        }

        let other_attrs =
            self.family
                .iter()
                .map(|Span { range, attr }| {
                    (range.clone(), PietTextAttribute::FontFamily(attr.clone()))
                })
                .chain(self.size.iter().map(|Span { range, attr }| {
                    (
                        range.clone(),
                        PietTextAttribute::FontSize(attr.resolve(env)),
                    )
                }))
                .chain(
                    self.weight.iter().map(|Span { range, attr }| {
                        (range.clone(), PietTextAttribute::Weight(*attr))
                    }),
                )
                .chain(self.fg_color.iter().map(|Span { range, attr }| {
                    (
                        range.clone(),
                        PietTextAttribute::ForegroundColor(attr.resolve(env)),
                    )
                }))
                .chain(
                    self.style.iter().map(|Span { range, attr }| {
                        (range.clone(), PietTextAttribute::Style(*attr))
                    }),
                )
                .chain(self.underline.iter().map(|Span { range, attr }| {
                    (range.clone(), PietTextAttribute::Underline(*attr))
                }))
                .chain(self.fg_color.iter().map(|Span { range, attr }| {
                    (
                        range.clone(),
                        PietTextAttribute::ForegroundColor(attr.resolve(env)),
                    )
                }));
        items.extend(other_attrs);
        // sort by ascending start order; this is a stable sort
        // so items that come from FontDescriptor will stay at the front
        items.sort_by(|a, b| a.0.start.cmp(&b.0.start));
        items
    }
}

impl<T: Clone> SpanSet<T> {
    fn iter(&self) -> impl Iterator<Item = &Span<T>> {
        self.spans.iter()
    }

    /// Add a `Span` to this `SpanSet`.
    ///
    /// Spans can be added in any order. existing spans will be updated
    /// as required.
    fn add(&mut self, span: Span<T>) {
        let span_start = span.range.start;
        let span_end = span.range.end;
        let insert_idx = self
            .spans
            .iter()
            .position(|x| x.range.start >= span.range.start)
            .unwrap_or_else(|| self.spans.len());

        // if we are inserting into the middle of an existing span we need
        // to add the trailing portion back afterwards.
        let mut prev_remainder = None;

        if insert_idx > 0 {
            // truncate the preceding item, if necessary
            let before = self.spans.get_mut(insert_idx - 1).unwrap();
            if before.range.end > span_end {
                let mut remainder = before.clone();
                remainder.range.start = span_end;
                prev_remainder = Some(remainder);
            }
            before.range.end = before.range.end.min(span_start);
        }

        self.spans.insert(insert_idx, span);
        if let Some(remainder) = prev_remainder.take() {
            self.spans.insert(insert_idx + 1, remainder);
        }

        // clip any existing spans as needed
        for after in self.spans.iter_mut().skip(insert_idx + 1) {
            after.range.start = after.range.start.max(span_end);
            after.range.end = after.range.end.max(span_end);
        }

        // remove any spans that have been overwritten
        self.spans.retain(|span| !span.is_empty());
    }

    /// Edit the spans, inserting empty space into the changed region if needed.
    ///
    /// This is used to keep the spans up to date as edits occur in the buffer.
    ///
    /// `changed` is the range of the string that has been replaced; this can
    /// be an empty range (eg, 10..10) for the insertion case.
    ///
    /// `new_len` is the length of the inserted text.
    //TODO: we could be smarter here about just extending the existing spans
    //as requred for insertions in the interior of a span.
    fn edit(&mut self, changed: Range<usize>, new_len: usize) {
        let old_len = changed.len();
        let mut to_insert = None;

        for (idx, Span { range, attr }) in self.spans.iter_mut().enumerate() {
            if range.end <= changed.start {
                continue;
            } else if range.start < changed.start {
                // we start before but end inside; truncate end
                if range.end <= changed.end {
                    range.end = changed.start;
                // we start before and end after; this is a special case,
                // we'll need to add a new span
                } else {
                    let new_start = changed.start + new_len;
                    let new_end = range.end - old_len + new_len;
                    let new_span = Span::new(new_start..new_end, attr.clone());
                    to_insert = Some((idx + 1, new_span));
                    range.end = changed.start;
                }
            // start inside
            } else if range.start < changed.end {
                range.start = changed.start + new_len;
                // end inside; collapse
                if range.end <= changed.end {
                    range.end = changed.start + new_len;
                // end outside: adjust by length delta
                } else {
                    range.end -= old_len;
                    range.end += new_len;
                }
            // whole range is after:
            } else {
                range.start -= old_len;
                range.start += new_len;
                range.end -= old_len;
                range.end += new_len;
            }
        }
        if let Some((idx, span)) = to_insert.take() {
            self.spans.insert(idx, span);
        }

        self.spans.retain(|span| !span.is_empty());
    }
}

impl<T> Span<T> {
    fn new(range: Range<usize>, attr: T) -> Self {
        Span { range, attr }
    }

    fn is_empty(&self) -> bool {
        self.range.end <= self.range.start
    }
}

impl Attribute {
    /// Create a new font size attribute.
    pub fn size(size: impl Into<KeyOrValue<f64>>) -> Self {
        Attribute::FontSize(size.into())
    }

    /// Create a new forground color attribute.
    pub fn foreground_color(color: impl Into<KeyOrValue<Color>>) -> Self {
        Attribute::ForegroundColor(color.into())
    }

    /// Create a new font family attribute.
    pub fn font_family(family: FontFamily) -> Self {
        Attribute::FontFamily(family)
    }

    /// Create a new `FontWeight` attribute.
    pub fn weight(weight: FontWeight) -> Self {
        Attribute::Weight(weight)
    }

    /// Create a new `FontStyle` attribute.
    pub fn style(style: FontStyle) -> Self {
        Attribute::Style(style)
    }

    /// Create a new underline attribute.
    pub fn underline(underline: bool) -> Self {
        Attribute::Underline(underline)
    }

    /// Create a new `FontDescriptor` attribute.
    pub fn font_descriptor(font: impl Into<KeyOrValue<FontDescriptor>>) -> Self {
        Attribute::Descriptor(font.into())
    }
}

impl<T> Default for SpanSet<T> {
    fn default() -> Self {
        SpanSet { spans: Vec::new() }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn smoke_test_spans() {
        let mut spans = SpanSet::<u32>::default();
        spans.add(Span::new(2..10, 1));
        spans.add(Span::new(3..6, 2));
        assert_eq!(
            &spans.spans,
            &vec![Span::new(2..3, 1), Span::new(3..6, 2), Span::new(6..10, 1)]
        );

        spans.add(Span::new(0..12, 3));
        assert_eq!(&spans.spans, &vec![Span::new(0..12, 3)]);
        spans.add(Span::new(5..20, 4));
        assert_eq!(&spans.spans, &vec![Span::new(0..5, 3), Span::new(5..20, 4)]);
    }

    #[test]
    fn edit_spans() {
        let mut spans = SpanSet::<u32>::default();
        spans.add(Span::new(0..2, 1));
        spans.add(Span::new(8..12, 2));
        spans.add(Span::new(13..16, 3));
        spans.add(Span::new(20..22, 4));

        let mut deletion = spans.clone();
        deletion.edit(6..14, 0);
        assert_eq!(
            &deletion.spans,
            &vec![Span::new(0..2, 1), Span::new(6..8, 3), Span::new(12..14, 4)]
        );

        let mut insertion = spans.clone();
        insertion.edit(10..10, 2);
        assert_eq!(
            &insertion.spans,
            &vec![
                Span::new(0..2, 1),
                Span::new(8..10, 2),
                Span::new(12..14, 2),
                Span::new(15..18, 3),
                Span::new(22..24, 4),
            ]
        );
    }
}
