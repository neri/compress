pub struct SliceWindow<'a, T> {
    source: &'a [T],
    offset: usize,
    len: usize,
}

impl<'a, T> SliceWindow<'a, T> {
    #[inline]
    pub fn new(source: &'a [T], offset: usize) -> Self {
        Self {
            source,
            offset,
            len: 1,
        }
    }

    /// # Panics
    ///
    /// Panics if `offset + len` exceeds the length of the source slice.
    #[inline]
    pub fn into_slice(self) -> &'a [T] {
        &self.source[self.offset..self.offset + self.len]
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.len
    }
}

impl<T> SliceWindow<'_, T> {
    #[inline]
    pub fn expand(&mut self, delta: usize) {
        self.len += delta;
    }
}
