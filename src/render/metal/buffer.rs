pub struct Buffer<T: bytemuck::Pod> {
    raw: metal::Buffer,
    len: usize,
    _phantom: std::marker::PhantomData<T>,
}

impl<T: bytemuck::Pod> Buffer<T> {
    pub fn new(len: usize, device: &metal::Device) -> Buffer<T> {
        let bytes = len * std::mem::size_of::<T>();
        let raw = device.new_buffer(
            bytes as u64,
            metal::MTLResourceOptions::CPUCacheModeDefaultCache
                | metal::MTLResourceOptions::StorageModeManaged,
        );

        Buffer {
            raw,
            len,
            _phantom: Default::default(),
        }
    }

    pub fn with_data(data: &[T], device: &metal::Device) -> Buffer<T> {
        let bytes = bytemuck::cast_slice::<T, u8>(data);
        let raw = device.new_buffer_with_data(
            bytes.as_ptr() as *const _,
            bytes.len() as u64,
            metal::MTLResourceOptions::CPUCacheModeDefaultCache
                | metal::MTLResourceOptions::StorageModeManaged,
        );

        Buffer {
            raw,
            len: data.len(),
            _phantom: Default::default(),
        }
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn write(&mut self, data: &[T], offset: usize) {
        self.modify(offset..offset + data.len(), |contents| {
            contents.copy_from_slice(data);
        });
    }

    pub fn modify(&mut self, range: impl std::ops::RangeBounds<usize>, f: impl FnOnce(&mut [T])) {
        let start_index = match range.start_bound() {
            std::ops::Bound::Included(index) => *index,
            std::ops::Bound::Excluded(index) => *index + 1,
            std::ops::Bound::Unbounded => 0,
        };

        let end_index = match range.end_bound() {
            std::ops::Bound::Included(index) => *index + 1,
            std::ops::Bound::Excluded(index) => *index,
            std::ops::Bound::Unbounded => self.len,
        };

        let ptr = self.raw.contents() as *mut T;
        let contents = unsafe { std::slice::from_raw_parts_mut(ptr, self.len) };
        f(&mut contents[start_index..end_index]);

        let start_byte = start_index * std::mem::size_of::<T>();
        let end_byte = end_index * std::mem::size_of::<T>();
        let length = end_byte - start_byte;
        let modify_range = metal::NSRange::new(start_byte as u64, length as u64);
        self.raw.did_modify_range(modify_range);
    }
}

impl<T: bytemuck::Pod> std::ops::Deref for Buffer<T> {
    type Target = metal::Buffer;

    fn deref(&self) -> &Self::Target {
        &self.raw
    }
}
