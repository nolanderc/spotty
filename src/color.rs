#[derive(Debug, Copy, Clone)]
pub struct Color {
    red: u8,
    green: u8,
    blue: u8,
}

impl Color {
    pub const BLACK: Color = Color::new(0, 0, 0);
    pub const WHITE: Color = Color::new(0xff, 0xff, 0xff);

    pub const fn new(red: u8, green: u8, blue: u8) -> Color {
        Color { red, green, blue }
    }

    pub const fn gray(gray: u8) -> Color {
        Color::new(gray, gray, gray)
    }

    pub const fn from_u32_rgb(bits: u32) -> Color {
        Color {
            red: ((bits >> 16) & 0xff) as u8,
            green: ((bits >> 8) & 0xff) as u8,
            blue: (bits & 0xff) as u8,
        }
    }

    pub fn into_rgba_f32(self) -> [f32; 4] {
        [
            self.red as f32 / 255.0,
            self.green as f32 / 255.0,
            self.blue as f32 / 255.0,
            1.0,
        ]
    }
}

impl From<[u8; 3]> for Color {
    fn from([red, green, blue]: [u8; 3]) -> Self {
        Color { red, green, blue }
    }
}

pub static DEFAULT_FOREGROUND: Color = DEFAULT_PALETTE[7];
pub static DEFAULT_BACKGROUND: Color = DEFAULT_PALETTE[0];

pub static DEFAULT_PALETTE: [Color; 256] = {
    let mut colors = [Color::new(0, 0, 0); 256];

    // Base 16 colors
    colors[0x0] = Color::from_u32_rgb(0x_33_33_33);
    colors[0x1] = Color::from_u32_rgb(0x_aa_33_33);
    colors[0x2] = Color::from_u32_rgb(0x_33_aa_33);
    colors[0x3] = Color::from_u32_rgb(0x_aa_aa_33);
    colors[0x4] = Color::from_u32_rgb(0x_33_33_aa);
    colors[0x5] = Color::from_u32_rgb(0x_aa_33_aa);
    colors[0x6] = Color::from_u32_rgb(0x_33_aa_aa);
    colors[0x7] = Color::from_u32_rgb(0x_aa_aa_aa);

    colors[0x8] = Color::from_u32_rgb(0x_55_55_55);
    colors[0x9] = Color::from_u32_rgb(0x_ff_55_55);
    colors[0xa] = Color::from_u32_rgb(0x_55_ff_55);
    colors[0xb] = Color::from_u32_rgb(0x_ff_ff_55);
    colors[0xc] = Color::from_u32_rgb(0x_55_55_ff);
    colors[0xd] = Color::from_u32_rgb(0x_ff_55_ff);
    colors[0xe] = Color::from_u32_rgb(0x_55_ff_ff);
    colors[0xf] = Color::from_u32_rgb(0x_ff_ff_ff);

    // 6x6x6 color cube
    {
        let mut r = 0;
        let mut g = 0;
        let mut b = 0;

        while r < 6 {
            while g < 6 {
                while b < 6 {
                    colors[16 + 36 * r + 6 * g + b] = Color {
                        red: (255 * r / 6) as u8,
                        green: (255 * g / 6) as u8,
                        blue: (255 * b / 6) as u8,
                    };
                    b += 1;
                }
                g += 1;
            }
            r += 1;
        }
    }

    // Grayscale in 24 steps
    let mut i = 0;
    while i < 24 {
        let gray = 255 * i / 24;
        colors[232 + i] = Color::gray(gray as u8);
        i += 1;
    }

    colors
};
