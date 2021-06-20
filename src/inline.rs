/// Optimization over a `Vec<u8>` or `Box<[u8]>` which stores bytes inline with the struct, and
/// only allocates on the heap when necessary.
pub struct InlineBytes(InlineBytesData);

enum InlineBytesData {
    Inline {
        len: u8,
        bytes: [u8; InlineBytes::MAX_INLINE_LEN],
    },
    Boxed(Box<[u8]>),
}

impl InlineBytes {
    pub const MAX_INLINE_LEN: usize =
        std::mem::size_of::<Box<[u8]>>() + std::mem::align_of::<Box<[u8]>>() - 2;

    pub fn new(slice: &[u8]) -> InlineBytes {
        if slice.len() <= Self::MAX_INLINE_LEN {
            let mut bytes = [0u8; Self::MAX_INLINE_LEN];
            bytes[..slice.len()].copy_from_slice(slice);

            InlineBytes(InlineBytesData::Inline {
                len: slice.len() as u8,
                bytes,
            })
        } else {
            InlineBytes(InlineBytesData::Boxed(slice.to_vec().into_boxed_slice()))
        }
    }
}

impl From<u8> for InlineBytes {
    fn from(byte: u8) -> Self {
        Self::new(std::slice::from_ref(&byte))
    }
}

impl From<&[u8]> for InlineBytes {
    fn from(bytes: &[u8]) -> Self {
        Self::new(bytes)
    }
}

impl<const N: usize> From<[u8; N]> for InlineBytes {
    fn from(bytes: [u8; N]) -> Self {
        Self::new(&bytes)
    }
}

impl<const N: usize> From<&[u8; N]> for InlineBytes {
    fn from(bytes: &[u8; N]) -> Self {
        Self::new(bytes)
    }
}

impl std::ops::Deref for InlineBytes {
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        match &self.0 {
            InlineBytesData::Inline { len, bytes } => &bytes[..*len as usize],
            InlineBytesData::Boxed(boxed) => &boxed,
        }
    }
}

impl AsRef<[u8]> for InlineBytes {
    fn as_ref(&self) -> &[u8] {
        self
    }
}

impl std::borrow::Borrow<[u8]> for InlineBytes {
    fn borrow(&self) -> &[u8] {
        self
    }
}

impl std::fmt::Debug for InlineBytes {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.as_ref().fmt(f)
    }
}

/// Same as `InlineBytes` but for strings.
pub struct InlineStr(InlineBytes);

impl InlineStr {
    pub fn new(text: &str) -> InlineStr {
        InlineStr(InlineBytes::new(text.as_bytes()))
    }
}

impl From<char> for InlineStr {
    fn from(ch: char) -> Self {
        let mut buffer = [0u8; 4];
        let encoded = ch.encode_utf8(&mut buffer);
        Self::new(encoded)
    }
}

impl From<&str> for InlineStr {
    fn from(text: &str) -> Self {
        Self::new(text)
    }
}

impl std::ops::Deref for InlineStr {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        // SAFETY: the only way to construct an `InlineStr` is with a valid string slice
        unsafe { std::str::from_utf8_unchecked(&self.0) }
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
