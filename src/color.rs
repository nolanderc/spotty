#[derive(Debug, Copy, Clone)]
pub enum Color {
    /// Use a color from the default palette
    Index(u8),
    /// Use a specific RGB color
    Rgb([u8; 3]),
}

pub type Palette = [[u8; 3]; 256];

impl Color {
    pub fn into_rgb(self, palette: &Palette) -> [u8; 3] {
        match self {
            Color::Index(index) => palette[index as usize],
            Color::Rgb(rgb) => rgb,
        }
    }

    pub fn into_rgb_f32(self, palette: &Palette) -> [f32; 3] {
        rgb_u8_to_rgb_f32(self.into_rgb(palette))
    }

    pub fn into_rgb_f64(self, palette: &Palette) -> [f64; 3] {
        rgb_u8_to_rgb_f64(self.into_rgb(palette))
    }

    pub fn into_rgba_f32(self, palette: &Palette) -> [f32; 4] {
        let [r, g, b] = self.into_rgb_f32(palette);
        [r, g, b, 1.0]
    }

    pub fn into_rgba_f64(self, palette: &Palette) -> [f64; 4] {
        let [r, g, b] = self.into_rgb_f64(palette);
        [r, g, b, 1.0]
    }

    pub fn complement(self, palette: &Palette) -> Color {
        let rgb = self.into_rgb_f32(palette);
        let [mut h, s, mut l] = rgb_to_hsl(rgb);
        h = (h + 0.5) % 1.0;
        l = 1.0 - l;
        let rgb = hsl_to_rgb([h, s, l]);
        Color::Rgb(rgb_f32_to_rgb_u8(rgb))
    }
}

pub fn rgb_f32_to_rgb_u8([r, g, b]: [f32; 3]) -> [u8; 3] {
    [(r * 255.0) as u8, (g * 255.0) as u8, (b * 255.0) as u8]
}

pub fn rgb_u8_to_rgb_f32([r, g, b]: [u8; 3]) -> [f32; 3] {
    [r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0]
}

pub fn rgb_u8_to_rgb_f64([r, g, b]: [u8; 3]) -> [f64; 3] {
    [r as f64 / 255.0, g as f64 / 255.0, b as f64 / 255.0]
}

pub fn rgb_to_hsl([r, g, b]: [f32; 3]) -> [f32; 3] {
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);

    let chroma = max - min;

    #[allow(clippy::float_cmp)]
    let hue = if chroma == 0.0 {
        0.0
    } else if max == r {
        (g - b) / chroma
    } else if max == g {
        2.0 + (b - r) / chroma
    } else {
        4.0 + (r - g) / chroma
    };

    // normalize in range [0, 1]
    let hue = (hue / 6.0).rem_euclid(1.0);

    let lightness = (max + min) / 2.0;

    #[allow(clippy::float_cmp)]
    let saturation = if lightness == 0.0 || lightness == 1.0 {
        0.0
    } else {
        (max - lightness) / f32::min(lightness, 1.0 - lightness)
    };

    [hue, saturation, lightness]
}

pub fn hsl_to_rgb([h, s, l]: [f32; 3]) -> [f32; 3] {
    let chroma = (1.0 - f32::abs(2.0 * l - 1.0)) * s;

    let hextant = h * 6.0;
    let x = chroma * (1.0 - f32::abs(hextant.rem_euclid(2.0) - 1.0));

    let [r_base, g_base, b_base] = match hextant.floor() as u8 {
        // 0..1
        0 => [chroma, x, 0.0],
        // 1..2
        1 => [x, chroma, 0.0],
        // 2..3
        2 => [0.0, chroma, x],
        // 3..4
        3 => [0.0, x, chroma],
        // 4..5
        4 => [x, 0.0, chroma],
        // 5..6
        _ => [chroma, 0.0, x],
    };

    let offset = l - chroma / 2.0;

    [r_base + offset, g_base + offset, b_base + offset]
}

impl From<[u8; 3]> for Color {
    fn from(rgb: [u8; 3]) -> Self {
        Color::Rgb(rgb)
    }
}

pub const DEFAULT_FOREGROUND: Color = Color::Index(15);
pub const DEFAULT_BACKGROUND: Color = Color::Index(0);

pub const DEFAULT_CURSOR: Color = DEFAULT_FOREGROUND;

#[allow(clippy::unusual_byte_groupings)]
pub const DEFAULT_PALETTE: Palette = {
    const fn rgb_from_u32(bits: u32) -> [u8; 3] {
        [
            ((bits >> 16) & 0xff) as u8,
            ((bits >> 8) & 0xff) as u8,
            (bits & 0xff) as u8,
        ]
    }

    let mut colors = [[0, 0, 0]; 256];

    // Base 16 colors
    colors[0x0] = rgb_from_u32(0x282828);
    colors[0x1] = rgb_from_u32(0xcc241d);
    colors[0x2] = rgb_from_u32(0x98871a);
    colors[0x3] = rgb_from_u32(0xd79921);
    colors[0x4] = rgb_from_u32(0x458588);
    colors[0x5] = rgb_from_u32(0xb16286);
    colors[0x6] = rgb_from_u32(0x689d6a);
    colors[0x7] = rgb_from_u32(0xa89984);

    colors[0x8] = rgb_from_u32(0x928374);
    colors[0x9] = rgb_from_u32(0xfb4934);
    colors[0xa] = rgb_from_u32(0xb8bb26);
    colors[0xb] = rgb_from_u32(0xfabd2f);
    colors[0xc] = rgb_from_u32(0x83a598);
    colors[0xd] = rgb_from_u32(0xd3869b);
    colors[0xe] = rgb_from_u32(0x8ec07c);
    colors[0xf] = rgb_from_u32(0xebdbb2);

    // 6x6x6 color cube
    {
        macro_rules! const_for {
            ($ident:ident in $low:literal..$high:literal $block:block) => {
                let mut $ident = $low;
                while $ident < $high {
                    $block;
                    $ident += 1;
                }
            };
        }

        const_for!(r in 0..6 {
            const_for!(g in 0..6 {
                const_for!(b in 0..6 {
                    colors[16 + 36 * r + 6 * g + b] = [
                        (255 * r / 6) as u8,
                        (255 * g / 6) as u8,
                        (255 * b / 6) as u8,
                    ];
                });
            });
        });
    }

    // Grayscale in 24 steps
    let mut i = 0;
    while i < 24 {
        let gray = 255 * i / 24;
        colors[232 + i] = [gray as u8; 3];
        i += 1;
    }

    colors
};
