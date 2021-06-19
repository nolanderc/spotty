use std::convert::TryInto;

#[derive(Clone, PartialEq, Eq, Hash)]
pub struct InlineStr {
    data: InlineStrData,
}

#[derive(Clone, PartialEq, Eq, Hash)]
enum InlineStrData {
    Inline {
        length: u8,
        bytes: [u8; InlineStr::MAX_INLINE_LEN],
    },
    Boxed(Box<str>),
}

impl InlineStr {
    pub const MAX_INLINE_LEN: usize = std::mem::size_of::<Box<str>>() + std::mem::align_of::<Box<str>>() - 2;

    pub fn new(text: &str) -> InlineStr {
        let data = if text.len() <= Self::MAX_INLINE_LEN {
            let mut bytes = [0u8; Self::MAX_INLINE_LEN];

            bytes[..text.len()].copy_from_slice(text.as_bytes());

            InlineStrData::Inline {
                length: text.len().try_into().unwrap(),
                bytes,
            }
        } else {
            InlineStrData::Boxed(text.to_owned().into_boxed_str())
        };

        InlineStr { data }
    }
}

impl From<&str> for InlineStr {
    fn from(text: &str) -> Self {
        InlineStr::new(text)
    }
}

impl From<char> for InlineStr {
    fn from(ch: char) -> Self {
        let mut buffer = [0u8; 4];
        let encoded = ch.encode_utf8(&mut buffer);
        InlineStr::new(encoded)
    }
}

impl std::ops::Deref for InlineStr {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        match &self.data {
            InlineStrData::Inline { length, bytes } => {
                // SAFETY: we only allow to create an InlineStr from a `&str` so it must contain
                // valid UTF-8
                unsafe { std::str::from_utf8_unchecked(&bytes[..*length as usize]) }
            }
            InlineStrData::Boxed(text) => &text,
        }
    }
}

impl AsRef<str> for InlineStr {
    fn as_ref(&self) -> &str {
        self
    }
}

impl std::borrow::Borrow<str> for InlineStr {
    fn borrow(&self) -> &str {
        self
    }
}

impl std::fmt::Debug for InlineStr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.as_ref().fmt(f)
    }
}

impl std::fmt::Display for InlineStr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.as_ref().fmt(f)
    }
}
