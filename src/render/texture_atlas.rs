use std::convert::TryFrom;

pub struct TextureAtlas {
    size: u16,
    rows: Vec<Vec<FreeRange>>,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
struct FreeRange {
    start: u16,
    end: u16,
}

impl TextureAtlas {
    pub fn new(size: usize) -> TextureAtlas {
        let size = u16::try_from(size).unwrap();
        TextureAtlas {
            size,
            rows: vec![vec![FreeRange::new(0, size)]; size as usize],
        }
    }

    pub fn reserve(&mut self, width: usize, height: usize) -> Option<[u16; 2]> {
        let width = u16::try_from(width).unwrap();
        let height = u16::try_from(height).unwrap();

        for y in 0..self.size.saturating_sub(height) {
            let mut columns = vec![FreeRange::new(0, self.size)];

            for y in y..y + height {
                columns = keep_intersection(&columns, &self.rows[y as usize]);
                columns.retain(|range| range.len() >= width);
                if columns.is_empty() {
                    break;
                }
            }

            let x_range = columns
                .into_iter()
                .filter(|range| range.len() >= width)
                .min_by_key(|range| range.len());

            let x_range = match x_range {
                None => continue,
                Some(range) => range,
            };

            let x = x_range.start;

            for y in y..y + height {
                let ranges = &mut self.rows[y as usize];
                for i in 0..ranges.len() {
                    if let Some((before, rest)) = ranges[i].split(x) {
                        if rest.end == x + width {
                            if before.len() == 0 {
                                ranges.remove(i);
                            } else {
                                ranges[i] = before;
                            }
                        } else {
                            let (_reserved, after) = rest.split(x + width).unwrap();

                            if before.len() == 0 {
                                ranges[i] = after;
                            } else {
                                ranges[i] = before;
                                ranges.insert(i + 1, after);
                            }
                        }

                        break;
                    }
                }
            }

            return Some([x, y]);
        }

        None
    }
}

impl FreeRange {
    pub fn new(start: u16, end: u16) -> FreeRange {
        debug_assert!(start <= end);
        FreeRange { start, end }
    }

    pub fn len(&self) -> u16 {
        self.end - self.start
    }

    pub fn split(self, x: u16) -> Option<(FreeRange, FreeRange)> {
        if self.contains(x) {
            Some((FreeRange::new(self.start, x), FreeRange::new(x, self.end)))
        } else {
            None
        }
    }

    pub fn contains(&self, x: u16) -> bool {
        self.start <= x && x < self.end
    }
}

fn keep_intersection(av: &[FreeRange], bv: &[FreeRange]) -> Vec<FreeRange> {
    let mut intersection = Vec::with_capacity(av.len());

    let mut a_iter = av.iter().copied();
    let mut next_a = a_iter.next();

    'b: for b in bv.iter().copied() {
        'a: loop {
            let a = match next_a {
                None => break 'b,
                Some(a) => a,
            };

            if a.end <= b.start {
                // a:   |------|
                // b:            |------|
                next_a = a_iter.next();
                continue 'a;
            } else if a.start >= b.end {
                // a:            |------|
                // b:  |------|
                continue 'b;
            }

            // From this point on, the two ranges intersect

            let (short, long) = if a.end <= b.end {
                // a:  +----|
                // b:  +------|
                next_a = a_iter.next();
                (a, b)
            } else {
                // a:  +------|
                // b:  +----|
                next_a = Some(FreeRange::new(b.end, a.end));
                (b, a)
            };

            // We may now assume that `short.end <= long.end`

            if short.start <= long.start {
                // short:   |------|
                // long:      |------|
                intersection.push(FreeRange::new(long.start, short.end));
            } else {
                // short:       |--|
                // long:      |------|
                intersection.push(short);
            }
        }
    }

    intersection
}

#[test]
fn range_intersect() {
    assert_eq!(
        keep_intersection(
            &[FreeRange::new(0, 10)],
            &[FreeRange::new(2, 4), FreeRange::new(8, 10)]
        ),
        &[FreeRange::new(2, 4), FreeRange::new(8, 10)]
    )
}
