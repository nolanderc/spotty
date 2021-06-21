#[derive(Debug, Copy, Clone)]
pub struct Color {
    red: u8,
    green: u8,
    blue: u8,
}

impl Color {
    pub const BLACK: Color = Color::new(0, 0, 0);

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
    
    pub fn into_rgba_f64(self) -> [f64; 4] {
        [
            self.red as f64 / 255.0,
            self.green as f64 / 255.0,
            self.blue as f64 / 255.0,
            1.0,
        ]
    }
}

impl From<[u8; 3]> for Color {
    fn from([red, green, blue]: [u8; 3]) -> Self {
        Color { red, green, blue }
    }
}

pub const DEFAULT_FOREGROUND: Color = DEFAULT_PALETTE[15];
pub const DEFAULT_BACKGROUND: Color = DEFAULT_PALETTE[0];

pub const DEFAULT_CURSOR: Color = DEFAULT_FOREGROUND;
pub const DEFAULT_CURSOR_TEXT: Color = DEFAULT_BACKGROUND;

pub const DEFAULT_PALETTE: [Color; 256] = {
    let mut colors = [Color::new(0, 0, 0); 256];

    // Base 16 colors
    colors[0x0] = Color::from_u32_rgb(0x_282828);
    colors[0x1] = Color::from_u32_rgb(0x_cc241d);
    colors[0x2] = Color::from_u32_rgb(0x_98871a);
    colors[0x3] = Color::from_u32_rgb(0x_d79921);
    colors[0x4] = Color::from_u32_rgb(0x_458588);
    colors[0x5] = Color::from_u32_rgb(0x_b16286);
    colors[0x6] = Color::from_u32_rgb(0x_689d6a);
    colors[0x7] = Color::from_u32_rgb(0x_a89984);

    colors[0x8] = Color::from_u32_rgb(0x_928374);
    colors[0x9] = Color::from_u32_rgb(0x_fb4934);
    colors[0xa] = Color::from_u32_rgb(0x_b8bb26);
    colors[0xb] = Color::from_u32_rgb(0x_fabd2f);
    colors[0xc] = Color::from_u32_rgb(0x_83a598);
    colors[0xd] = Color::from_u32_rgb(0x_d3869b);
    colors[0xe] = Color::from_u32_rgb(0x_8ec07c);
    colors[0xf] = Color::from_u32_rgb(0x_ebdbb2);

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
                    colors[16 + 36 * r + 6 * g + b] = Color {
                        red: (255 * r / 6) as u8,
                        green: (255 * g / 6) as u8,
                        blue: (255 * b / 6) as u8,
                    };
                });
            });
        });
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
