//! A line chart in braille characters for `historikk` (SPEC §7.9). Each character holds
//! 2 × 4 dots, so a chart `width` characters wide has `2 × width` points in time. Prices
//! are drawn as steps: a price holds until it changes, and gaps stay empty.

use jiff::Timestamp;
use owo_colors::{AnsiColors, OwoColorize};

use super::format;
use crate::history::{Interval, oslo_date};
use crate::model::Ore;

/// The colors of the series, in order. With more series than colors, they repeat.
const PALETTE: [AnsiColors; 6] = [
    AnsiColors::Green,
    AnsiColors::Cyan,
    AnsiColors::Yellow,
    AnsiColors::Magenta,
    AnsiColors::Blue,
    AnsiColors::Red,
];

/// The bit of each dot in a braille character, by `[row][column]`.
const DOTS: [[u32; 2]; 4] = [[0x01, 0x08], [0x02, 0x10], [0x04, 0x20], [0x40, 0x80]];

pub struct Series<'a> {
    pub label: String,
    pub intervals: &'a [Interval],
}

/// Draws the series between `start` and `end` in a plot area of `width` × `height`
/// characters, plus a price axis, a date axis and a legend. `None` when no series has a
/// price in the period.
pub fn render(
    series: &[Series<'_>],
    start: Timestamp,
    end: Timestamp,
    width: usize,
    height: usize,
    color: bool,
) -> Option<String> {
    let span = end.as_second() - start.as_second();
    if span <= 0 || width == 0 || height < 2 {
        return None;
    }
    let visible = |i: &&Interval| i.to >= start && i.from <= end;
    let prices = series
        .iter()
        .flat_map(|s| s.intervals.iter().filter(visible))
        .map(|i| i.liter_price.0);
    let (mut low, mut high) = (prices.clone().min()?, prices.max()?);
    if low == high {
        let pad = (high / 20).max(100);
        low -= pad;
        high += pad;
    }

    let mut canvas = Canvas::new(width, height);
    let (px_width, px_height) = (canvas.px_width(), canvas.px_height());
    let y_of = |price: Ore| {
        let from_top = i128::from(high - price.0) * i128::from(px_height - 1);
        let y = crate::pricing::div_round(from_top, i128::from(high - low));
        usize::try_from(y).unwrap_or(0).min(px_height as usize - 1)
    };

    for (index, s) in series.iter().enumerate() {
        let mut previous: Option<usize> = None;
        for px in 0..px_width {
            // The middle of this column of dots.
            let t = start.as_second() + span * (2 * px + 1) / (2 * px_width);
            let price = s
                .intervals
                .iter()
                .find(|i| i.from.as_second() <= t && t <= i.to.as_second())
                .map(|i| i.liter_price);
            let Some(y) = price.map(y_of) else {
                previous = None;
                continue;
            };
            // A price change is a vertical step.
            let (top, bottom) = match previous {
                Some(p) => (p.min(y), p.max(y)),
                None => (y, y),
            };
            for row in top..=bottom {
                canvas.set(px as usize, row, index);
            }
            previous = Some(y);
        }
        // Single observations are too short to be hit by any column.
        for point in s
            .intervals
            .iter()
            .filter(|i| i.from == i.to)
            .filter(visible)
        {
            let px = (point.from.as_second() - start.as_second()) * px_width / span;
            let px = usize::try_from(px).unwrap_or(0).min(px_width as usize - 1);
            canvas.set(px, y_of(point.liter_price), index);
        }
    }

    // The top and bottom labels are the outermost dots; the middle one is the price at
    // the center of its row, which is exactly halfway when `height` is odd.
    let middle = height / 2;
    let middle_dot = 8 * i128::try_from(middle).unwrap_or(0) + 3;
    let middle_price = high
        - crate::pricing::div_round(
            i128::from(high - low) * middle_dot,
            2 * i128::from(px_height - 1),
        );
    let labels = [
        (0, format::kr(Ore(high))),
        (middle, format::kr(Ore(middle_price))),
        (height - 1, format::kr(Ore(low))),
    ];
    let label_width = labels
        .iter()
        .map(|(_, l)| l.chars().count())
        .max()
        .unwrap_or(0);

    let mut out = String::new();
    for row in 0..height {
        let label = labels
            .iter()
            .find(|(r, _)| *r == row)
            .map(|(_, l)| l.as_str());
        let axis = if label.is_some() { '┤' } else { '│' };
        out.push_str(&format!("{:>label_width$} {axis}", label.unwrap_or("")));
        out.push_str(&canvas.row(row, color));
        out.push('\n');
    }
    out.push_str(&format!("{:label_width$} └{}\n", "", "─".repeat(width)));
    out.push_str(&format!(
        "{:label_width$}  {}\n",
        "",
        date_axis(start, end, width)
    ));

    let legend: Vec<String> = series
        .iter()
        .enumerate()
        .map(|(index, s)| {
            let marker = if color {
                "■".color(PALETTE[index % PALETTE.len()]).to_string()
            } else {
                "■".to_owned()
            };
            format!("{marker} {}", s.label)
        })
        .collect();
    out.push_str(&format!("{:label_width$}  {}", "", legend.join("   ")));
    Some(out)
}

/// `2.7.` at the left edge, the middle date in the middle and `30.9.` at the right.
fn date_axis(start: Timestamp, end: Timestamp, width: usize) -> String {
    let label = |t: Timestamp| {
        let d = oslo_date(t);
        format!("{}.{}.", d.day(), d.month())
    };
    let first = label(start);
    let last = label(end);
    let mut line: Vec<char> = vec![' '; width];
    let mut put = |text: &str, at: usize| {
        for (i, c) in text.chars().enumerate() {
            if let Some(slot) = line.get_mut(at + i) {
                *slot = c;
            }
        }
    };
    put(&first, 0);
    if width >= 30 {
        let middle = label(
            Timestamp::from_second((start.as_second() + end.as_second()) / 2).unwrap_or(start),
        );
        put(&middle, width / 2 - middle.chars().count() / 2);
    }
    put(&last, width.saturating_sub(last.chars().count()));
    line.into_iter().collect::<String>().trim_end().to_owned()
}

/// The dots, and which series drew each character last (for its color).
struct Canvas {
    width: usize,
    height: usize,
    bits: Vec<u32>,
    owner: Vec<Option<usize>>,
}

impl Canvas {
    fn new(width: usize, height: usize) -> Self {
        Self {
            width,
            height,
            bits: vec![0; width * height],
            owner: vec![None; width * height],
        }
    }

    fn px_width(&self) -> i64 {
        i64::try_from(self.width * 2).unwrap_or(i64::MAX)
    }

    fn px_height(&self) -> i64 {
        i64::try_from(self.height * 4).unwrap_or(i64::MAX)
    }

    fn set(&mut self, x: usize, y: usize, series: usize) {
        let cell = (y / 4) * self.width + x / 2;
        if let Some(bits) = self.bits.get_mut(cell) {
            *bits |= DOTS[y % 4][x % 2];
            self.owner[cell] = Some(series);
        }
    }

    /// One row of characters, colored in runs by series.
    fn row(&self, row: usize, color: bool) -> String {
        let mut out = String::new();
        let mut run = String::new();
        let mut run_owner = None;
        for col in 0..self.width {
            let cell = row * self.width + col;
            let ch = match self.bits[cell] {
                0 => ' ',
                bits => char::from_u32(0x2800 + bits).unwrap_or(' '),
            };
            let owner = self.owner[cell].filter(|_| color);
            if owner != run_owner && !run.is_empty() {
                out.push_str(&paint(&run, run_owner));
                run.clear();
            }
            run_owner = owner;
            run.push(ch);
        }
        out.push_str(&paint(&run, run_owner));
        out.trim_end().to_owned()
    }
}

fn paint(text: &str, owner: Option<usize>) -> String {
    match owner {
        Some(index) => text.color(PALETTE[index % PALETTE.len()]).to_string(),
        None => text.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use jiff::SignedDuration;

    use super::*;
    use crate::history::start_of_day;

    fn day(n: i64) -> Timestamp {
        start_of_day(jiff::civil::date(2026, 9, 1)) + SignedDuration::from_hours(24 * n + 12)
    }

    fn interval(kr: i64, from: i64, to: i64) -> Interval {
        Interval {
            liter_price: Ore(kr * 100),
            from: day(from),
            to: day(to),
        }
    }

    #[test]
    fn a_price_drop_is_drawn_as_a_step() {
        let meny = [interval(40, 0, 10), interval(30, 10, 20)];
        let chart = render(
            &[Series {
                label: "Meny".into(),
                intervals: &meny,
            }],
            day(0),
            day(20),
            20,
            4,
            false,
        )
        .unwrap();
        insta::assert_snapshot!(chart);
    }

    #[test]
    fn two_series_with_a_gap() {
        let meny = [interval(40, 0, 6), interval(40, 12, 20)];
        let spar = [interval(34, 0, 20)];
        let chart = render(
            &[
                Series {
                    label: "Meny".into(),
                    intervals: &meny,
                },
                Series {
                    label: "Spar".into(),
                    intervals: &spar,
                },
            ],
            day(0),
            day(20),
            40,
            5,
            false,
        )
        .unwrap();
        insta::assert_snapshot!(chart);
    }

    #[test]
    fn colors_only_when_asked() {
        let meny = [interval(40, 0, 20)];
        let series = [Series {
            label: "Meny".into(),
            intervals: &meny,
        }];
        let plain = render(&series, day(0), day(20), 10, 3, false).unwrap();
        let colored = render(&series, day(0), day(20), 10, 3, true).unwrap();
        assert!(!plain.contains('\u{1b}'));
        assert!(colored.contains('\u{1b}'));
    }

    #[test]
    fn nothing_to_draw() {
        let old = [interval(40, 0, 2)];
        let series = [Series {
            label: "Meny".into(),
            intervals: &old,
        }];
        assert_eq!(render(&series, day(10), day(20), 10, 3, false), None);
    }
}
